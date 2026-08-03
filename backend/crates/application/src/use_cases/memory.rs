use std::collections::HashSet;

use chrono::{DateTime, Utc};
use domain::rules::brief::parse_memory_reference;
use domain::rules::memory_import::{
    import_source_ref, kind_for_metadata_type, parse_memory_file, IMPORT_SOURCE_REF_PREFIX,
};
use domain::rules::memory_lifecycle::{self, MergeOutcome, SupersedeOutcome};
use domain::rules::recall::{
    build_match_query, build_match_query_any, RecallContext, RecallWeights, ScoredMemory,
};
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::{
    MemoryListFilter, MemoryRepository, MEMORY_LIST_DEFAULT_LIMIT, MEMORY_LIST_MAX_LIMIT,
};
use crate::services::{
    MemoryFileSource, MemoryRetriever, RecallQuery, RECALL_DEFAULT_LIMIT, RECALL_MAX_LIMIT,
};

/// How many active memories the near-duplicate gate pulls from FTS5 before the
/// domain similarity rule filters them.
const DUPLICATE_SCAN_LIMIT: u32 = 25;

/// What the caller supplies to `remember`. Mirrors `domain::NewMemory` except for
/// `confirmed`, which the use case turns into the queue status.
#[derive(Debug, Clone)]
pub struct RememberInput {
    pub kind: MemoryKind,
    pub title: String,
    /// The "why": context, alternatives dropped. Never a deadline — that lives
    /// on the task.
    pub body: Option<String>,
    /// When it was decided / promised. Defaults to `now`.
    pub occurred_at: Option<DateTime<Utc>>,
    pub source: MemorySource,
    pub source_ref: Option<String>,
    /// Skip the validation queue and store as `active` — for entries the human
    /// typed themselves.
    pub confirmed: bool,
    pub project_id: Option<ProjectId>,
    pub task_id: Option<TaskId>,
    pub stakeholders: Vec<String>,
}

/// Record a memory (the `aplan remember` path).
pub async fn remember(
    memory_repo: &dyn MemoryRepository,
    user_id: UserId,
    input: RememberInput,
    now: DateTime<Utc>,
) -> Result<Memory, AppError> {
    let memory = Memory::new(
        user_id,
        NewMemory {
            kind: input.kind,
            title: input.title,
            body: input.body,
            occurred_at: input.occurred_at,
            source: input.source,
            source_ref: input.source_ref,
            status: if input.confirmed {
                MemoryStatus::Active
            } else {
                MemoryStatus::Pending
            },
            project_id: input.project_id,
            task_id: input.task_id,
            stakeholders: input.stakeholders,
        },
        now,
    )?;
    memory_repo.create(&memory).await?;
    Ok(memory)
}

/// Deep recall of one memory by id (`aplan recall <id>`). Returns `None` rather
/// than an error so the caller owns the not-found exit code.
pub async fn get_memory(
    memory_repo: &dyn MemoryRepository,
    user_id: UserId,
    id: MemoryId,
) -> Result<Option<Memory>, AppError> {
    Ok(memory_repo.find_by_id(id, user_id).await?)
}

/// How many candidates a reference lookup reports back when it is ambiguous.
const REFERENCE_MATCH_LIMIT: u32 = 10;

/// What a reference resolved to.
#[derive(Debug, Clone, PartialEq)]
pub enum MemoryLookup {
    Found(Memory),
    NotFound,
    /// Several memories share the prefix. Picking one would be a guess, and a
    /// guess here means expanding the wrong memory — so the caller is asked.
    Ambiguous(Vec<Memory>),
}

/// Resolve what a reader typed — a full id, or the short `m:7c1` reference the
/// brief renders — into one memory.
///
/// This is what makes the brief's references usable: the rendering shows a
/// three-character handle, and `aplan recall m:7c1` has to find it again.
pub async fn resolve_memory(
    memory_repo: &dyn MemoryRepository,
    user_id: UserId,
    token: &str,
) -> Result<MemoryLookup, AppError> {
    // Nothing usable in the token is a miss, not an error: the caller already
    // owns the not-found exit code.
    let Some(prefix) = parse_memory_reference(token) else {
        return Ok(MemoryLookup::NotFound);
    };

    if let Ok(id) = Uuid::parse_str(&prefix) {
        return Ok(match memory_repo.find_by_id(id, user_id).await? {
            Some(memory) => MemoryLookup::Found(memory),
            None => MemoryLookup::NotFound,
        });
    }

    let mut matches = memory_repo
        .find_by_id_prefix(user_id, &prefix, REFERENCE_MATCH_LIMIT)
        .await?;
    Ok(match matches.len() {
        0 => MemoryLookup::NotFound,
        1 => MemoryLookup::Found(matches.remove(0)),
        _ => MemoryLookup::Ambiguous(matches),
    })
}

/// How many candidates an ambiguity message names before it stops listing.
const AMBIGUITY_LISTED: usize = 5;

/// Resolve a reference that MUST designate exactly one memory — the form every
/// verb that WRITES needs.
///
/// [`resolve_memory`] reports three outcomes and a mutation can act on only one
/// of them, so the other two become errors here, before any write. Sharing this
/// with the read path is the whole point: the brief prints `[m:7c1]` and the inbox
/// lists candidates, so a short handle is the only id a user ever sees. A verb
/// that accepts one to read but demands a 36-character UUID to act would make
/// every accept, reject, merge and supersede a copy-paste exercise.
pub async fn resolve_memory_id(
    memory_repo: &dyn MemoryRepository,
    user_id: UserId,
    token: &str,
) -> Result<MemoryId, AppError> {
    match resolve_memory(memory_repo, user_id, token).await? {
        MemoryLookup::Found(memory) => Ok(memory.id),
        MemoryLookup::NotFound => Err(AppError::NotFound(format!("memory `{token}`"))),
        MemoryLookup::Ambiguous(candidates) => Err(AppError::Ambiguous(
            describe_ambiguous_memory(token, &candidates),
        )),
    }
}

/// Resolve the two references a verb that touches two memories was given, BEFORE
/// either is used.
///
/// A merge deletes a row and a supersession writes the invalidation link, so
/// resolving lazily — first reference, first write, then discover the second
/// reference is unusable — would leave a half-applied change: a memory hidden
/// with no successor, or a candidate erased into nothing.
pub async fn resolve_memory_id_pair(
    memory_repo: &dyn MemoryRepository,
    user_id: UserId,
    first: &str,
    second: &str,
) -> Result<(MemoryId, MemoryId), AppError> {
    let first_id = resolve_memory_id(memory_repo, user_id, first).await?;
    let second_id = resolve_memory_id(memory_repo, user_id, second).await?;
    Ok((first_id, second_id))
}

/// One wording for "this reference matches several memories", shared by the read
/// and the write paths so a reader never has to learn two.
///
/// One candidate per line, ids in full: the caller is being asked to pick, and it
/// can only pick from something it can copy. Capped at [`AMBIGUITY_LISTED`] so a
/// one-character prefix cannot flood a terminal.
pub fn describe_ambiguous_memory(token: &str, candidates: &[Memory]) -> String {
    let listed = candidates
        .iter()
        .take(AMBIGUITY_LISTED)
        .map(|memory| format!("  - {} {}", memory.id, memory.title))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Ambiguous memory reference `{token}`: {} matches; please add more characters\n{listed}",
        candidates.len()
    )
}

/// Search memories from RAW user input (`aplan recall --q "…"`).
///
/// This is the only place the raw string is turned into an FTS5 expression: a
/// `ValidationError` here means the input held nothing searchable, never that
/// FTS5 choked on it.
pub async fn search_memories(
    retriever: &dyn MemoryRetriever,
    user_id: UserId,
    raw_query: &str,
    context: RecallContext,
    include_history: bool,
    limit: u32,
    now: DateTime<Utc>,
) -> Result<Vec<ScoredMemory>, AppError> {
    let query = RecallQuery {
        match_query: build_match_query(raw_query)?,
        context,
        include_history,
        weights: RecallWeights::default(),
        limit: match limit {
            0 => RECALL_DEFAULT_LIMIT,
            n => n.min(RECALL_MAX_LIMIT),
        },
    };
    Ok(retriever.search(user_id, &query, now).await?)
}

/// The validation queue: candidates awaiting accept / merge / supersede / reject.
pub async fn list_pending_memories(
    memory_repo: &dyn MemoryRepository,
    user_id: UserId,
    limit: u32,
    offset: u32,
) -> Result<Vec<Memory>, AppError> {
    let filter = MemoryListFilter {
        status: Some(vec![MemoryStatus::Pending]),
        include_invalidated: false,
        project_id: None,
        limit: match limit {
            0 => MEMORY_LIST_DEFAULT_LIMIT,
            n => n.min(MEMORY_LIST_MAX_LIMIT),
        },
        offset,
    };
    Ok(memory_repo.list(user_id, &filter).await?)
}

// ─── Import (lot 2) ──────────────────────────────────────────────────────────

/// Why a file was not imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSkipReason {
    /// Already imported by an earlier run — this is what makes import idempotent.
    AlreadyImported,
    /// No frontmatter fence. `MEMORY.md`, the harness index, is this case.
    NoFrontmatter,
    /// Neither `description` nor `name`, so there is no sentence to remember.
    NoTitle,
}

impl ImportSkipReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImportSkipReason::AlreadyImported => "already_imported",
            ImportSkipReason::NoFrontmatter => "no_frontmatter",
            ImportSkipReason::NoTitle => "no_title",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedMemoryFile {
    pub file_name: String,
    pub reason: ImportSkipReason,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemoryImportOutcome {
    pub imported: Vec<Memory>,
    pub skipped: Vec<SkippedMemoryFile>,
}

/// One-shot import of the harness memory files (`aplan memory import <dir>`).
///
/// Idempotent: every imported memory carries a stable `source_ref` derived from
/// the file's frontmatter `name`, and a file whose reference is already stored is
/// skipped. Re-running therefore imports nothing.
///
/// Imported memories land as `active` — they are the user's own curated notes, so
/// there is nothing to validate — with `source = manual`. Whatever is in the
/// directory is imported; nothing is hardcoded, and nothing is written back.
pub async fn import_memories(
    memory_repo: &dyn MemoryRepository,
    file_source: &dyn MemoryFileSource,
    user_id: UserId,
    directory: &str,
    now: DateTime<Utc>,
) -> Result<MemoryImportOutcome, AppError> {
    let files = file_source.list(directory).await?;
    let mut seen: HashSet<String> = memory_repo
        .existing_source_refs(user_id, IMPORT_SOURCE_REF_PREFIX)
        .await?
        .into_iter()
        .collect();

    let mut outcome = MemoryImportOutcome::default();

    for file in files {
        let parsed = match parse_memory_file(&file.contents) {
            Ok(parsed) => parsed,
            Err(_) => {
                outcome.skipped.push(SkippedMemoryFile {
                    file_name: file.file_name,
                    reason: ImportSkipReason::NoFrontmatter,
                });
                continue;
            }
        };

        let front = &parsed.frontmatter;
        let source_ref = import_source_ref(front.name.as_deref(), &file.file_name);
        // `seen` also absorbs this run's own writes, so two files claiming the
        // same name cannot both land.
        if seen.contains(&source_ref) {
            outcome.skipped.push(SkippedMemoryFile {
                file_name: file.file_name,
                reason: ImportSkipReason::AlreadyImported,
            });
            continue;
        }

        let Some(title) = front
            .description
            .clone()
            .or_else(|| front.name.clone())
            .filter(|t| !t.trim().is_empty())
        else {
            outcome.skipped.push(SkippedMemoryFile {
                file_name: file.file_name,
                reason: ImportSkipReason::NoTitle,
            });
            continue;
        };

        let memory = Memory::new(
            user_id,
            NewMemory {
                kind: kind_for_metadata_type(front.metadata_type.as_deref()),
                title,
                body: Some(parsed.body.clone()).filter(|b| !b.is_empty()),
                // The frontmatter date is more precise than the file's mtime,
                // which a checkout or a sync can bump.
                occurred_at: front.modified.or(file.modified_at).or(Some(now)),
                source: MemorySource::Manual,
                source_ref: Some(source_ref.clone()),
                status: MemoryStatus::Active,
                project_id: None,
                task_id: None,
                stakeholders: vec![],
            },
            now,
        )?;
        memory_repo.create(&memory).await?;
        seen.insert(source_ref);
        outcome.imported.push(memory);
    }

    Ok(outcome)
}

// ─── Validation queue and invalidation (lot 3) ───────────────────────────────

/// What `accept_candidate` did, or refused to do.
#[derive(Debug, Clone, PartialEq)]
pub enum AcceptOutcome {
    Accepted(Memory),
    /// The gate of §6.3: never a silent add. The caller must choose `merge` (a
    /// rewording) or `supersede` (a changed fact), or accept explicitly.
    NearDuplicates {
        candidate: Memory,
        duplicates: Vec<Memory>,
    },
}

/// Accept a pending candidate, unless it looks like an existing active memory.
///
/// The near-duplicate gate uses FTS5 to narrow the field (matching ANY word of
/// the title — an AND would miss rewordings) and the pure similarity rule to
/// decide. Whether a duplicate is a rewording or a contradiction is a semantic
/// judgement the backend cannot make, so both options are offered and the human
/// picks. `force` accepts anyway, which keeps the add explicit rather than silent.
pub async fn accept_candidate(
    memory_repo: &dyn MemoryRepository,
    retriever: &dyn MemoryRetriever,
    user_id: UserId,
    id: MemoryId,
    kind_override: Option<MemoryKind>,
    force: bool,
    now: DateTime<Utc>,
) -> Result<AcceptOutcome, AppError> {
    let candidate = load(memory_repo, user_id, id).await?;

    if !force {
        let duplicates = find_near_duplicates(retriever, user_id, &candidate, now).await?;
        if !duplicates.is_empty() {
            return Ok(AcceptOutcome::NearDuplicates {
                candidate,
                duplicates,
            });
        }
    }

    let accepted = memory_lifecycle::accept(&candidate, kind_override)?;
    memory_repo.update(&accepted).await?;
    Ok(AcceptOutcome::Accepted(accepted))
}

/// Active memories that look like `candidate`, most similar first.
async fn find_near_duplicates(
    retriever: &dyn MemoryRetriever,
    user_id: UserId,
    candidate: &Memory,
    now: DateTime<Utc>,
) -> Result<Vec<Memory>, AppError> {
    // A title with nothing searchable in it cannot have a duplicate to find.
    let Ok(match_query) = build_match_query_any(&candidate.title) else {
        return Ok(vec![]);
    };
    let query = RecallQuery {
        match_query,
        context: RecallContext::default(),
        // The gate compares against the CURRENT truths only.
        include_history: false,
        weights: RecallWeights::default(),
        limit: DUPLICATE_SCAN_LIMIT,
    };
    let pool: Vec<Memory> = retriever
        .search(user_id, &query, now)
        .await?
        .into_iter()
        .map(|hit| hit.memory)
        .filter(|m| m.id != candidate.id)
        .collect();

    Ok(memory_lifecycle::near_duplicates(&candidate.title, &pool)
        .into_iter()
        .map(|(memory, _)| memory.clone())
        .collect())
}

/// Reject a pending candidate. The row survives as a tombstone.
pub async fn reject_candidate(
    memory_repo: &dyn MemoryRepository,
    user_id: UserId,
    id: MemoryId,
) -> Result<Memory, AppError> {
    let candidate = load(memory_repo, user_id, id).await?;
    let rejected = memory_lifecycle::reject(&candidate)?;
    memory_repo.update(&rejected).await?;
    Ok(rejected)
}

/// Merge a pending candidate into an active memory: same fact, better wording.
/// One row survives — this ERASES history, and is not the default.
pub async fn merge_candidate(
    memory_repo: &dyn MemoryRepository,
    user_id: UserId,
    candidate_id: MemoryId,
    into_id: MemoryId,
) -> Result<MergeOutcome, AppError> {
    let candidate = load(memory_repo, user_id, candidate_id).await?;
    let target = load(memory_repo, user_id, into_id).await?;
    let outcome = memory_lifecycle::merge(&candidate, &target)?;
    memory_repo
        .apply_merge(&outcome.survivor, outcome.discarded, user_id)
        .await?;
    Ok(outcome)
}

/// Supersede an active memory by another: the fact CHANGED. Both rows survive,
/// and this is the ONLY path that writes `invalidated_at`.
///
/// Serves both `aplan inbox supersede` (successor still pending) and
/// `aplan memory supersede` (successor already active).
pub async fn supersede_memory(
    memory_repo: &dyn MemoryRepository,
    user_id: UserId,
    old_id: MemoryId,
    successor_id: MemoryId,
    now: DateTime<Utc>,
) -> Result<SupersedeOutcome, AppError> {
    let old = load(memory_repo, user_id, old_id).await?;
    let successor = load(memory_repo, user_id, successor_id).await?;
    // Resolved here because walking the chain is I/O; the domain only checks it.
    let chain = memory_repo
        .supersession_chain(user_id, successor_id)
        .await?;
    let outcome = memory_lifecycle::supersede(&old, &successor, &chain, now)?;
    memory_repo
        .apply_supersession(&outcome.invalidated, &outcome.successor)
        .await?;
    Ok(outcome)
}

async fn load(
    memory_repo: &dyn MemoryRepository,
    user_id: UserId,
    id: MemoryId,
) -> Result<Memory, AppError> {
    memory_repo
        .find_by_id(id, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("memory {id}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use domain::rules::recall::{rank, RecallWeights};
    use std::sync::Mutex;
    use uuid::Uuid;

    use crate::errors::RepositoryError;
    use crate::services::MemoryFile;

    #[derive(Default)]
    struct InMemoryMemoryRepository {
        rows: Mutex<Vec<Memory>>,
    }

    #[async_trait]
    impl MemoryRepository for InMemoryMemoryRepository {
        async fn create(&self, memory: &Memory) -> Result<(), RepositoryError> {
            self.rows.lock().expect("lock").push(memory.clone());
            Ok(())
        }

        async fn find_by_id(
            &self,
            id: MemoryId,
            user_id: UserId,
        ) -> Result<Option<Memory>, RepositoryError> {
            Ok(self
                .rows
                .lock()
                .expect("lock")
                .iter()
                .find(|m| m.id == id && m.user_id == user_id)
                .cloned())
        }

        async fn list(
            &self,
            user_id: UserId,
            filter: &MemoryListFilter,
        ) -> Result<Vec<Memory>, RepositoryError> {
            let rows = self.rows.lock().expect("lock");
            Ok(rows
                .iter()
                .filter(|m| m.user_id == user_id)
                .filter(|m| match &filter.status {
                    None => true,
                    Some(wanted) => wanted.contains(&m.status),
                })
                .filter(|m| filter.include_invalidated || m.invalidated_at.is_none())
                .skip(filter.offset as usize)
                .take(filter.limit as usize)
                .cloned()
                .collect())
        }

        async fn update(&self, memory: &Memory) -> Result<(), RepositoryError> {
            let mut rows = self.rows.lock().expect("lock");
            match rows.iter_mut().find(|m| m.id == memory.id) {
                Some(slot) => {
                    *slot = memory.clone();
                    Ok(())
                }
                None => Err(RepositoryError::Database(format!(
                    "memory {} not found",
                    memory.id
                ))),
            }
        }

        async fn apply_merge(
            &self,
            survivor: &Memory,
            discarded: MemoryId,
            _user_id: UserId,
        ) -> Result<(), RepositoryError> {
            let mut rows = self.rows.lock().expect("lock");
            if let Some(slot) = rows.iter_mut().find(|m| m.id == survivor.id) {
                *slot = survivor.clone();
            }
            rows.retain(|m| m.id != discarded);
            Ok(())
        }

        async fn apply_supersession(
            &self,
            invalidated: &Memory,
            successor: &Memory,
        ) -> Result<(), RepositoryError> {
            let mut rows = self.rows.lock().expect("lock");
            for updated in [invalidated, successor] {
                if let Some(slot) = rows.iter_mut().find(|m| m.id == updated.id) {
                    *slot = updated.clone();
                }
            }
            Ok(())
        }

        async fn find_by_id_prefix(
            &self,
            user_id: UserId,
            prefix: &str,
            limit: u32,
        ) -> Result<Vec<Memory>, RepositoryError> {
            Ok(self
                .rows
                .lock()
                .expect("lock")
                .iter()
                .filter(|m| m.user_id == user_id && m.id.to_string().starts_with(prefix))
                .take(limit as usize)
                .cloned()
                .collect())
        }

        async fn existing_source_refs(
            &self,
            user_id: UserId,
            prefix: &str,
        ) -> Result<Vec<String>, RepositoryError> {
            Ok(self
                .rows
                .lock()
                .expect("lock")
                .iter()
                .filter(|m| m.user_id == user_id)
                .filter_map(|m| m.source_ref.clone())
                .filter(|r| r.starts_with(prefix))
                .collect())
        }

        async fn supersession_chain(
            &self,
            user_id: UserId,
            from: MemoryId,
        ) -> Result<Vec<MemoryId>, RepositoryError> {
            let rows = self.rows.lock().expect("lock");
            let mut chain = Vec::new();
            let mut cursor = from;
            while let Some(next) = rows
                .iter()
                .find(|m| m.id == cursor && m.user_id == user_id)
                .and_then(|m| m.superseded_by)
            {
                if chain.contains(&next) {
                    break;
                }
                chain.push(next);
                cursor = next;
            }
            Ok(chain)
        }
    }

    /// The same store also answers searches, so `accept_candidate`'s duplicate
    /// gate can be exercised against the rows the repo holds. Approximates FTS5
    /// by substring-matching the quoted phrases; the real MATCH semantics are
    /// covered in `infrastructure`.
    #[async_trait]
    impl MemoryRetriever for InMemoryMemoryRepository {
        async fn search(
            &self,
            user_id: UserId,
            query: &RecallQuery,
            now: DateTime<Utc>,
        ) -> Result<Vec<ScoredMemory>, RepositoryError> {
            let needles: Vec<String> = query
                .match_query
                .split('"')
                .skip(1)
                .step_by(2)
                .map(|p| p.to_lowercase())
                .collect();
            let rows = self.rows.lock().expect("lock");
            let candidates: Vec<(Memory, f64)> = rows
                .iter()
                .filter(|m| m.user_id == user_id)
                .filter(|m| query.include_history || m.is_recallable())
                .filter(|m| {
                    let hay = format!("{} {}", m.title, m.body.clone().unwrap_or_default())
                        .to_lowercase();
                    needles.iter().any(|n| hay.contains(n))
                })
                .cloned()
                .map(|m| (m, -1.0))
                .collect();
            let mut ranked = rank(candidates, &query.context, now, &query.weights);
            ranked.truncate(query.limit as usize);
            Ok(ranked)
        }
    }

    /// File source double: a fixed list of (name, contents) pairs.
    struct FakeFileSource {
        files: Vec<MemoryFile>,
    }

    #[async_trait]
    impl MemoryFileSource for FakeFileSource {
        async fn list(&self, _directory: &str) -> Result<Vec<MemoryFile>, AppError> {
            Ok(self.files.clone())
        }
    }

    fn file(name: &str, contents: &str) -> MemoryFile {
        MemoryFile {
            file_name: name.into(),
            contents: contents.into(),
            modified_at: None,
        }
    }

    /// Retriever double that records the query it was handed and ranks whatever
    /// rows it was seeded with, so the use case's plumbing is observable.
    struct SpyRetriever {
        rows: Vec<(Memory, f64)>,
        seen: Mutex<Option<RecallQuery>>,
    }

    #[async_trait]
    impl MemoryRetriever for SpyRetriever {
        async fn search(
            &self,
            _user_id: UserId,
            query: &RecallQuery,
            now: DateTime<Utc>,
        ) -> Result<Vec<ScoredMemory>, RepositoryError> {
            *self.seen.lock().expect("lock") = Some(query.clone());
            let ranked = rank(
                self.rows.clone(),
                &query.context,
                now,
                &RecallWeights::default(),
            );
            Ok(ranked.into_iter().take(query.limit as usize).collect())
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-03T09:00:00+00:00")
            .expect("valid fixture")
            .with_timezone(&Utc)
    }

    fn input(kind: MemoryKind, title: &str, confirmed: bool) -> RememberInput {
        RememberInput {
            kind,
            title: title.into(),
            body: None,
            occurred_at: None,
            source: MemorySource::ClaudeSession,
            source_ref: None,
            confirmed,
            project_id: None,
            task_id: None,
            stakeholders: vec![],
        }
    }

    async fn remembered(
        repo: &InMemoryMemoryRepository,
        user_id: UserId,
        kind: MemoryKind,
        title: &str,
        confirmed: bool,
    ) -> Memory {
        remember(repo, user_id, input(kind, title, confirmed), now())
            .await
            .expect("remember succeeds")
    }

    #[tokio::test]
    async fn remember_defaults_to_the_validation_queue() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let m = remembered(&repo, uid, MemoryKind::Decision, "Wave 0 limited", false).await;
        assert_eq!(m.status, MemoryStatus::Pending);
        assert_eq!(repo.find_by_id(m.id, uid).await.unwrap().unwrap().id, m.id);
    }

    #[tokio::test]
    async fn remember_with_confirm_skips_the_queue() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let m = remembered(&repo, uid, MemoryKind::Decision, "Wave 0 limited", true).await;
        assert_eq!(m.status, MemoryStatus::Active);
        assert!(m.is_recallable());
    }

    #[tokio::test]
    async fn remember_propagates_domain_validation() {
        let repo = InMemoryMemoryRepository::default();
        let err = remember(
            &repo,
            Uuid::new_v4(),
            input(MemoryKind::Fact, "   ", true),
            now(),
        )
        .await
        .expect_err("blank title must be rejected");
        assert!(matches!(err, AppError::Domain(_)));
        assert!(repo.rows.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn get_memory_is_scoped_to_its_user() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let m = remembered(&repo, uid, MemoryKind::Fact, "the mcp crate is broken", true).await;
        assert!(get_memory(&repo, uid, m.id).await.unwrap().is_some());
        assert!(
            get_memory(&repo, Uuid::new_v4(), m.id).await.unwrap().is_none(),
            "another user must not read it"
        );
    }

    // ─── Resolving the brief's short references (lot 4) ───────────────────

    /// A memory with a chosen id, so a reference prefix is predictable.
    async fn remembered_with_id(
        repo: &InMemoryMemoryRepository,
        user_id: UserId,
        id: &str,
        title: &str,
    ) -> Memory {
        let mut memory = remembered(repo, user_id, MemoryKind::Decision, title, true).await;
        let wanted = Uuid::parse_str(id).expect("valid fixture uuid");
        {
            let mut rows = repo.rows.lock().expect("lock");
            let slot = rows
                .iter_mut()
                .find(|m| m.id == memory.id)
                .expect("the row just written");
            slot.id = wanted;
        }
        memory.id = wanted;
        memory
    }

    #[tokio::test]
    async fn a_short_reference_from_the_brief_resolves_to_its_memory() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let m = remembered_with_id(
            &repo,
            uid,
            "7c1e4b2a-0000-0000-0000-000000000000",
            "Wave 0 limitée",
        )
        .await;

        for token in ["m:7c1", "[m:7c1]", "7c1", "M:7C1", &m.id.to_string()] {
            match resolve_memory(&repo, uid, token).await.expect("resolves") {
                MemoryLookup::Found(found) => assert_eq!(found.id, m.id, "for token {token}"),
                other => panic!("token {token} gave {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn an_unknown_or_unusable_reference_is_a_miss_not_an_error() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        remembered_with_id(&repo, uid, "7c1e4b2a-0000-0000-0000-000000000000", "un choix").await;
        for token in ["fff", "", "%", "m:", "'; DROP"] {
            assert_eq!(
                resolve_memory(&repo, uid, token).await.expect("resolves"),
                MemoryLookup::NotFound,
                "for token {token:?}"
            );
        }
    }

    /// A three-character reference can collide with a memory the brief never
    /// rendered. Expanding the wrong memory would be worse than saying so.
    #[tokio::test]
    async fn a_colliding_reference_reports_the_ambiguity_instead_of_guessing() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        remembered_with_id(&repo, uid, "7c1a0000-0000-0000-0000-000000000000", "choix A").await;
        remembered_with_id(&repo, uid, "7c1b0000-0000-0000-0000-000000000000", "choix B").await;

        match resolve_memory(&repo, uid, "m:7c1").await.expect("resolves") {
            MemoryLookup::Ambiguous(candidates) => assert_eq!(candidates.len(), 2),
            other => panic!("expected an ambiguity, got {other:?}"),
        }
        // A longer prefix disambiguates.
        assert!(matches!(
            resolve_memory(&repo, uid, "m:7c1b").await.expect("resolves"),
            MemoryLookup::Found(_)
        ));
    }

    #[tokio::test]
    async fn a_reference_is_scoped_to_its_user() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        remembered_with_id(&repo, uid, "7c1e4b2a-0000-0000-0000-000000000000", "un choix").await;
        assert_eq!(
            resolve_memory(&repo, Uuid::new_v4(), "m:7c1")
                .await
                .expect("resolves"),
            MemoryLookup::NotFound
        );
    }

    // ─── Resolving for the verbs that WRITE ───────────────────────────────

    /// Same as `remembered_with_id`, but the row stays in the validation queue.
    async fn pending_with_id(
        repo: &InMemoryMemoryRepository,
        user_id: UserId,
        id: &str,
        title: &str,
    ) -> Memory {
        let mut queued = remembered_with_id(repo, user_id, id, title).await;
        queued.status = MemoryStatus::Pending;
        repo.update(&queued).await.expect("requeues the row");
        queued
    }

    #[tokio::test]
    async fn a_short_reference_resolves_for_a_verb_that_writes() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let m = remembered_with_id(
            &repo,
            uid,
            "7c1e4b2a-0000-0000-0000-000000000000",
            "Wave 0 limitée",
        )
        .await;

        for token in ["m:7c1", "[m:7c1]", "7c1", "M:7C1", &m.id.to_string()] {
            assert_eq!(
                resolve_memory_id(&repo, uid, token)
                    .await
                    .unwrap_or_else(|e| panic!("token {token} failed: {e}")),
                m.id,
                "for token {token}"
            );
        }
    }

    /// The exit-code contract: an id nobody has is "not found" (2), never a
    /// generic failure (1) — and that holds for a garbage token too, which is a
    /// reference that matches nothing rather than a broken command.
    #[tokio::test]
    async fn an_unknown_reference_is_a_not_found_for_a_verb_that_writes() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        remembered_with_id(&repo, uid, "7c1e4b2a-0000-0000-0000-000000000000", "un choix").await;

        for token in ["fff", "", "%", "m:", "'; DROP", &Uuid::new_v4().to_string()] {
            let err = resolve_memory_id(&repo, uid, token)
                .await
                .expect_err("must not resolve");
            assert!(
                matches!(err, AppError::NotFound(_)),
                "token {token:?} gave {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn an_ambiguous_reference_names_its_candidates_instead_of_writing() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let a =
            remembered_with_id(&repo, uid, "7c1a0000-0000-0000-0000-000000000000", "choix A").await;
        let b =
            remembered_with_id(&repo, uid, "7c1b0000-0000-0000-0000-000000000000", "choix B").await;

        let err = resolve_memory_id(&repo, uid, "m:7c1")
            .await
            .expect_err("an ambiguous reference must not resolve");
        let AppError::Ambiguous(message) = &err else {
            panic!("expected an ambiguity, got {err:?}");
        };
        assert!(
            message.contains(&a.id.to_string()) && message.contains(&b.id.to_string()),
            "both candidates must be named so the caller can pick one: {message}"
        );
        assert!(
            message.contains("choix A") && message.contains("choix B"),
            "an id alone does not say which memory it is: {message}"
        );

        // A longer prefix disambiguates, and then the write may proceed.
        assert_eq!(
            resolve_memory_id(&repo, uid, "m:7c1b")
                .await
                .expect("a longer prefix resolves"),
            b.id
        );
    }

    /// Ambiguity is decided against the WHOLE store. Were it decided against the
    /// queue only, a prefix unique among today's candidates would silently start
    /// pointing at a different memory as soon as one was accepted.
    #[tokio::test]
    async fn ambiguity_is_decided_against_the_whole_store_not_just_the_queue() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        pending_with_id(&repo, uid, "ab010000-0000-0000-0000-000000000001", "candidat").await;
        remembered_with_id(&repo, uid, "ab010000-0000-0000-0000-000000000002", "fait actif").await;

        let err = resolve_memory_id(&repo, uid, "ab01")
            .await
            .expect_err("the active memory shares the prefix");
        assert!(
            matches!(err, AppError::Ambiguous(_)),
            "a prefix shared with an already-active memory is ambiguous, got {err:?}"
        );
    }

    /// A verb that touches two memories resolves both first: the second reference
    /// failing must stop the whole thing, not leave a half-applied change.
    #[tokio::test]
    async fn a_pair_of_references_resolves_only_when_both_are_usable() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let old =
            remembered_with_id(&repo, uid, "cc1c0000-0000-0000-0000-000000000000", "juin").await;
        let new =
            pending_with_id(&repo, uid, "dd1d0000-0000-0000-0000-000000000000", "septembre").await;

        assert_eq!(
            resolve_memory_id_pair(&repo, uid, "cc1c", "m:dd1d")
                .await
                .expect("both references resolve"),
            (old.id, new.id)
        );

        let err = resolve_memory_id_pair(&repo, uid, "cc1c", "fff9")
            .await
            .expect_err("an unusable second reference must fail the pair");
        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected a not-found, got {err:?}"
        );
    }

    #[tokio::test]
    async fn list_pending_excludes_active_and_rejected() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        remembered(&repo, uid, MemoryKind::Decision, "pending one", false).await;
        remembered(&repo, uid, MemoryKind::Decision, "active one", true).await;
        let pending = list_pending_memories(&repo, uid, 0, 0).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].title, "pending one");
    }

    #[tokio::test]
    async fn search_turns_raw_input_into_a_safe_match_expression() {
        let retriever = SpyRetriever {
            rows: vec![],
            seen: Mutex::new(None),
        };
        search_memories(
            &retriever,
            Uuid::new_v4(),
            "AP-1234",
            RecallContext::default(),
            false,
            0,
            now(),
        )
        .await
        .expect("search succeeds");
        let seen = retriever.seen.lock().expect("lock").clone().expect("a query");
        assert_eq!(
            seen.match_query, "\"AP-1234\"",
            "one quoted phrase, so adjacency is preserved"
        );
        assert!(!seen.include_history, "the hard filter is on by default");
        assert_eq!(seen.limit, RECALL_DEFAULT_LIMIT);
    }

    #[tokio::test]
    async fn search_rejects_input_with_nothing_searchable() {
        let retriever = SpyRetriever {
            rows: vec![],
            seen: Mutex::new(None),
        };
        for raw in ["", "*", "\"", " :-, "] {
            let err = search_memories(
                &retriever,
                Uuid::new_v4(),
                raw,
                RecallContext::default(),
                false,
                0,
                now(),
            )
            .await
            .expect_err("must not reach the retriever");
            assert!(matches!(err, AppError::Domain(_)), "for input {raw:?}");
        }
        assert!(
            retriever.seen.lock().expect("lock").is_none(),
            "no unsafe query may reach the store"
        );
    }

    #[tokio::test]
    async fn search_caps_the_limit_and_forwards_history_and_context() {
        let retriever = SpyRetriever {
            rows: vec![],
            seen: Mutex::new(None),
        };
        let project = Uuid::new_v4();
        search_memories(
            &retriever,
            Uuid::new_v4(),
            "engagements",
            RecallContext {
                project_id: Some(project),
                ..RecallContext::default()
            },
            true,
            10_000,
            now(),
        )
        .await
        .expect("search succeeds");
        let seen = retriever.seen.lock().expect("lock").clone().expect("a query");
        assert_eq!(seen.limit, RECALL_MAX_LIMIT);
        assert!(seen.include_history);
        assert_eq!(seen.context.project_id, Some(project));
        assert_eq!(
            seen.match_query, "(\"engagements\"* OR \"engagement\"*)",
            "the plural query carries its de-pluralized branch"
        );
    }

    // ─── Import (lot 2) ──────────────────────────────────────────────────

    const FEEDBACK_FILE: &str = "---\nname: aplan-note-cadence\ndescription: atomic notes, never a batched dump\nmetadata:\n  type: feedback\n---\n\nOne entry per finding.\n";
    const PROJECT_FILE: &str = "---\nname: mcp-crate-broken\ndescription: mcp does not compile at HEAD\nmetadata:\n  type: project\n---\n\nUse a scoped cargo test.\n";
    const INDEX_FILE: &str = "- [a note](a.md) — hook\n- [b note](b.md) — hook\n";

    fn source(files: Vec<MemoryFile>) -> FakeFileSource {
        FakeFileSource { files }
    }

    #[tokio::test]
    async fn import_maps_metadata_types_onto_kinds_and_lands_active() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let src = source(vec![
            file("feedback_note.md", FEEDBACK_FILE),
            file("project_mcp.md", PROJECT_FILE),
        ]);

        let outcome = import_memories(&repo, &src, uid, "/some/dir", now())
            .await
            .expect("imports");

        assert_eq!(outcome.imported.len(), 2);
        assert!(outcome.skipped.is_empty());
        let feedback = outcome
            .imported
            .iter()
            .find(|m| m.title.starts_with("atomic"))
            .expect("the feedback note");
        assert_eq!(feedback.kind, MemoryKind::Preference);
        assert_eq!(feedback.source, MemorySource::Manual);
        assert_eq!(feedback.status, MemoryStatus::Active, "curated notes need no validation");
        assert!(feedback.is_recallable());
        assert_eq!(
            feedback.source_ref.as_deref(),
            Some("memory-file:aplan-note-cadence")
        );
        assert_eq!(feedback.body.as_deref(), Some("One entry per finding."));

        let project = outcome
            .imported
            .iter()
            .find(|m| m.title.starts_with("mcp"))
            .expect("the project note");
        assert_eq!(project.kind, MemoryKind::Fact);
    }

    #[tokio::test]
    async fn import_is_idempotent() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let files = vec![
            file("feedback_note.md", FEEDBACK_FILE),
            file("project_mcp.md", PROJECT_FILE),
        ];

        let first = import_memories(&repo, &source(files.clone()), uid, "/d", now())
            .await
            .expect("imports");
        assert_eq!(first.imported.len(), 2);

        let second = import_memories(&repo, &source(files), uid, "/d", now())
            .await
            .expect("imports");
        assert!(second.imported.is_empty(), "a second run imports nothing");
        assert_eq!(second.skipped.len(), 2);
        assert!(second
            .skipped
            .iter()
            .all(|s| s.reason == ImportSkipReason::AlreadyImported));
        assert_eq!(repo.rows.lock().expect("lock").len(), 2, "no duplicate rows");
    }

    #[tokio::test]
    async fn import_skips_the_harness_index_without_failing() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let src = source(vec![
            file("MEMORY.md", INDEX_FILE),
            file("feedback_note.md", FEEDBACK_FILE),
        ]);
        let outcome = import_memories(&repo, &src, uid, "/d", now())
            .await
            .expect("imports");
        assert_eq!(outcome.imported.len(), 1);
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].file_name, "MEMORY.md");
        assert_eq!(outcome.skipped[0].reason, ImportSkipReason::NoFrontmatter);
    }

    #[tokio::test]
    async fn import_takes_whatever_the_directory_holds() {
        // Nothing is hardcoded to four files: the set grows.
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let files: Vec<MemoryFile> = (0..7)
            .map(|i| {
                file(
                    &format!("note_{i}.md"),
                    &format!("---\nname: note-{i}\ndescription: note number {i}\nmetadata:\n  type: reference\n---\nbody {i}\n"),
                )
            })
            .collect();
        let outcome = import_memories(&repo, &source(files), uid, "/d", now())
            .await
            .expect("imports");
        assert_eq!(outcome.imported.len(), 7);
        assert!(outcome.imported.iter().all(|m| m.kind == MemoryKind::Fact));
    }

    #[tokio::test]
    async fn import_prefers_the_frontmatter_date_then_the_file_mtime() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let mtime = now() - chrono::Duration::days(3);
        let dated = "---\nname: dated\ndescription: has a frontmatter date\nmetadata:\n  type: project\n  modified: 2026-06-12T14:00:00.000Z\n---\nbody";
        let undated = "---\nname: undated\ndescription: has none\nmetadata:\n  type: project\n---\nbody";
        let src = source(vec![
            MemoryFile {
                file_name: "dated.md".into(),
                contents: dated.into(),
                modified_at: Some(mtime),
            },
            MemoryFile {
                file_name: "undated.md".into(),
                contents: undated.into(),
                modified_at: Some(mtime),
            },
        ]);

        let outcome = import_memories(&repo, &src, uid, "/d", now())
            .await
            .expect("imports");
        let dated = outcome
            .imported
            .iter()
            .find(|m| m.source_ref.as_deref() == Some("memory-file:dated"))
            .expect("dated");
        assert_eq!(
            dated.occurred_at,
            DateTime::parse_from_rfc3339("2026-06-12T14:00:00.000Z")
                .unwrap()
                .with_timezone(&Utc)
        );
        let undated = outcome
            .imported
            .iter()
            .find(|m| m.source_ref.as_deref() == Some("memory-file:undated"))
            .expect("undated");
        assert_eq!(undated.occurred_at, mtime, "falls back to the file mtime");
    }

    #[tokio::test]
    async fn import_skips_a_file_with_no_title_at_all() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let src = source(vec![file("nameless.md", "---\nmetadata:\n  type: project\n---\nbody")]);
        let outcome = import_memories(&repo, &src, uid, "/d", now())
            .await
            .expect("imports");
        assert!(outcome.imported.is_empty());
        assert_eq!(outcome.skipped[0].reason, ImportSkipReason::NoTitle);
    }

    #[tokio::test]
    async fn imported_memories_are_immediately_recallable() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        import_memories(&repo, &source(vec![file("f.md", PROJECT_FILE)]), uid, "/d", now())
            .await
            .expect("imports");
        let hits = search_memories(
            &repo,
            uid,
            "compile",
            RecallContext::default(),
            false,
            0,
            now(),
        )
        .await
        .expect("search");
        assert_eq!(hits.len(), 1, "the corpus is queryable right after import");
    }

    // ─── Queue and invalidation (lot 3) ──────────────────────────────────

    #[tokio::test]
    async fn accept_promotes_a_candidate_with_no_duplicate() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let candidate = remembered(&repo, uid, MemoryKind::Decision, "Wave 0 limited to AI", false).await;
        let outcome = accept_candidate(&repo, &repo, uid, candidate.id, None, false, now())
            .await
            .expect("accepts");
        match outcome {
            AcceptOutcome::Accepted(m) => {
                assert_eq!(m.status, MemoryStatus::Active);
                let stored = repo.find_by_id(candidate.id, uid).await.unwrap().unwrap();
                assert_eq!(stored.status, MemoryStatus::Active, "the write was persisted");
            }
            other => panic!("expected Accepted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn accept_refuses_a_silent_add_when_a_near_duplicate_is_active() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let existing =
            remembered(&repo, uid, MemoryKind::Decision, "Wave 0 limited to the Microsoft AI scope", true).await;
        let candidate = remembered(
            &repo,
            uid,
            MemoryKind::Decision,
            "Wave 0 scope limited to Microsoft AI",
            false,
        )
        .await;

        let outcome = accept_candidate(&repo, &repo, uid, candidate.id, None, false, now())
            .await
            .expect("returns a decision request");
        match outcome {
            AcceptOutcome::NearDuplicates { duplicates, .. } => {
                assert_eq!(duplicates.len(), 1);
                assert_eq!(duplicates[0].id, existing.id);
            }
            other => panic!("expected NearDuplicates, got {other:?}"),
        }
        let stored = repo.find_by_id(candidate.id, uid).await.unwrap().unwrap();
        assert_eq!(
            stored.status,
            MemoryStatus::Pending,
            "the candidate must stay in the queue"
        );
    }

    #[tokio::test]
    async fn accept_with_force_goes_through_despite_a_duplicate() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        remembered(&repo, uid, MemoryKind::Decision, "Wave 0 limited to the Microsoft AI scope", true).await;
        let candidate = remembered(
            &repo,
            uid,
            MemoryKind::Decision,
            "Wave 0 scope limited to Microsoft AI",
            false,
        )
        .await;
        let outcome = accept_candidate(&repo, &repo, uid, candidate.id, None, true, now())
            .await
            .expect("accepts");
        assert!(matches!(outcome, AcceptOutcome::Accepted(_)));
    }

    #[tokio::test]
    async fn a_pending_lookalike_does_not_block_acceptance() {
        // The gate compares against current truths, not other candidates.
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        remembered(&repo, uid, MemoryKind::Decision, "Wave 0 scope limited to Microsoft AI", false).await;
        let candidate = remembered(
            &repo,
            uid,
            MemoryKind::Decision,
            "Wave 0 limited to the Microsoft AI scope",
            false,
        )
        .await;
        let outcome = accept_candidate(&repo, &repo, uid, candidate.id, None, false, now())
            .await
            .expect("accepts");
        assert!(matches!(outcome, AcceptOutcome::Accepted(_)));
    }

    #[tokio::test]
    async fn accept_and_reject_report_a_missing_id() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let ghost = Uuid::new_v4();
        assert!(matches!(
            accept_candidate(&repo, &repo, uid, ghost, None, false, now())
                .await
                .unwrap_err(),
            AppError::NotFound(_)
        ));
        assert!(matches!(
            reject_candidate(&repo, uid, ghost).await.unwrap_err(),
            AppError::NotFound(_)
        ));
    }

    #[tokio::test]
    async fn reject_leaves_a_tombstone_that_no_longer_recalls() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let candidate = remembered(&repo, uid, MemoryKind::Fact, "not worth keeping", false).await;
        let rejected = reject_candidate(&repo, uid, candidate.id).await.expect("rejects");
        assert_eq!(rejected.status, MemoryStatus::Rejected);
        assert!(
            repo.find_by_id(candidate.id, uid).await.unwrap().is_some(),
            "the tombstone stays so consolidation cannot re-propose it"
        );
        assert!(list_pending_memories(&repo, uid, 0, 0).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn merge_keeps_one_row_and_takes_the_better_wording() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let target = remembered(&repo, uid, MemoryKind::Decision, "wave 0 limited", true).await;
        let candidate = remembered(
            &repo,
            uid,
            MemoryKind::Decision,
            "Wave 0 is limited to the Microsoft AI scope",
            false,
        )
        .await;

        let outcome = merge_candidate(&repo, uid, candidate.id, target.id)
            .await
            .expect("merges");
        assert_eq!(outcome.survivor.id, target.id);
        assert_eq!(outcome.survivor.title, candidate.title);
        assert!(
            repo.find_by_id(candidate.id, uid).await.unwrap().is_none(),
            "the candidate row is gone"
        );
        let stored = repo.find_by_id(target.id, uid).await.unwrap().unwrap();
        assert_eq!(stored.title, candidate.title);
        assert_eq!(stored.invalidated_at, None, "a merge invalidates nothing");
        assert_eq!(repo.rows.lock().expect("lock").len(), 1);
    }

    /// The hard filter of §7.1 is finally triggerable — so prove it moves.
    #[tokio::test]
    async fn a_superseded_memory_leaves_recall_and_returns_under_history() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let old = remembered(&repo, uid, MemoryKind::Decision, "wave scope is Microsoft only", true).await;
        let new = remembered(&repo, uid, MemoryKind::Decision, "wave scope is the whole platform", false).await;

        let before = search_memories(&repo, uid, "wave", RecallContext::default(), false, 0, now())
            .await
            .expect("search");
        assert_eq!(before.len(), 1, "only the active one was recallable");

        let outcome = supersede_memory(&repo, uid, old.id, new.id, now())
            .await
            .expect("supersedes");
        assert_eq!(outcome.invalidated.invalidated_at, Some(now()));
        assert_eq!(outcome.invalidated.superseded_by, Some(new.id));

        let after = search_memories(&repo, uid, "wave", RecallContext::default(), false, 0, now())
            .await
            .expect("search");
        assert_eq!(after.len(), 1, "the old truth left, the new one arrived");
        assert_eq!(after[0].memory.id, new.id);

        let history = search_memories(&repo, uid, "wave", RecallContext::default(), true, 0, now())
            .await
            .expect("search");
        assert_eq!(history.len(), 2, "--history brings the superseded row back");
        assert!(history.iter().any(|h| h.memory.id == old.id));
    }

    #[tokio::test]
    async fn both_rows_survive_a_supersession() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let old = remembered(&repo, uid, MemoryKind::Decision, "scope is X", true).await;
        let new = remembered(&repo, uid, MemoryKind::Decision, "scope is Y", false).await;
        supersede_memory(&repo, uid, old.id, new.id, now()).await.expect("supersedes");
        assert_eq!(
            repo.rows.lock().expect("lock").len(),
            2,
            "supersede preserves history; merge would not"
        );
    }

    #[tokio::test]
    async fn a_chain_is_walked_and_a_cycle_refused() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let a = remembered(&repo, uid, MemoryKind::Decision, "scope is X", true).await;
        let b = remembered(&repo, uid, MemoryKind::Decision, "scope is Y", false).await;
        let c = remembered(&repo, uid, MemoryKind::Decision, "scope is Z", false).await;

        supersede_memory(&repo, uid, a.id, b.id, now()).await.expect("A by B");
        supersede_memory(&repo, uid, b.id, c.id, now()).await.expect("B by C");

        // C is the head. Closing the loop with A must be refused.
        let err = supersede_memory(&repo, uid, c.id, a.id, now())
            .await
            .expect_err("a cycle must be refused");
        assert!(matches!(
            err,
            AppError::Domain(domain::errors::DomainError::MemorySupersessionCycle { .. })
        ));
    }

    #[tokio::test]
    async fn re_superseding_an_invalidated_row_is_refused() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let a = remembered(&repo, uid, MemoryKind::Decision, "scope is X", true).await;
        let b = remembered(&repo, uid, MemoryKind::Decision, "scope is Y", false).await;
        let c = remembered(&repo, uid, MemoryKind::Decision, "scope is Z", false).await;
        supersede_memory(&repo, uid, a.id, b.id, now()).await.expect("A by B");

        let err = supersede_memory(&repo, uid, a.id, c.id, now())
            .await
            .expect_err("A already has a successor");
        assert!(matches!(
            err,
            AppError::Domain(domain::errors::DomainError::MemoryAlreadyInvalidated(_))
        ));
    }

    #[tokio::test]
    async fn supersede_is_scoped_to_its_user() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let old = remembered(&repo, uid, MemoryKind::Decision, "scope is X", true).await;
        let new = remembered(&repo, uid, MemoryKind::Decision, "scope is Y", false).await;
        assert!(matches!(
            supersede_memory(&repo, Uuid::new_v4(), old.id, new.id, now())
                .await
                .unwrap_err(),
            AppError::NotFound(_)
        ));
    }

    #[tokio::test]
    async fn search_returns_results_best_first() {
        let repo = InMemoryMemoryRepository::default();
        let uid = Uuid::new_v4();
        let weak = remembered(&repo, uid, MemoryKind::Decision, "weak match", true).await;
        let strong = remembered(&repo, uid, MemoryKind::Decision, "strong match", true).await;
        let strong_id = strong.id;
        let retriever = SpyRetriever {
            // bm25 is negative: -8.0 beats -0.2.
            rows: vec![(weak, -0.2), (strong, -8.0)],
            seen: Mutex::new(None),
        };
        let hits = search_memories(
            &retriever,
            uid,
            "match",
            RecallContext::default(),
            false,
            0,
            now(),
        )
        .await
        .expect("search succeeds");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].memory.id, strong_id);
    }
}
