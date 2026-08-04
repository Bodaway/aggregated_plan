use std::sync::Arc;

use application::repositories::MemoryRepository;
use application::use_cases::memory::{self as memory_uc, AcceptOutcome, MemoryImportOutcome};
use async_graphql::{ComplexObject, Context, InputObject, Result, SimpleObject, ID};
use chrono::{DateTime, Utc};
use domain::rules::memory_lifecycle::{MergeOutcome, SupersedeOutcome};
use domain::rules::recall::ScoredMemory;
use domain::types::{Memory, UserId};
use uuid::Uuid;

use super::enums::{MemoryKindGql, MemorySourceGql, MemoryStatusGql};

/// A semantic memory: what must be known. Bi-temporal — `occurredAt` is when it
/// became true, `invalidatedAt` when it stopped being true.
#[derive(SimpleObject)]
#[graphql(complex)]
pub struct MemoryGql {
    pub id: ID,
    pub kind: MemoryKindGql,
    pub title: String,
    /// The "why": context and dropped alternatives. Never a deadline.
    pub body: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    /// `null` = still true. Written only by the supersede path.
    pub invalidated_at: Option<DateTime<Utc>>,
    pub superseded_by: Option<ID>,
    /// The memory this candidate CLAIMS to contradict: a supersession *proposed*
    /// and not yet applied. Distinct from `supersededBy`, which records one that
    /// happened.
    ///
    /// Only a `PENDING` candidate carries it; every queue verdict clears it, so a
    /// value here is always a live question. `supersedeMemory` defaults its `old`
    /// argument to it. Use `contradicts` to read the named memory itself.
    pub proposed_supersedes: Option<ID>,
    pub source: MemorySourceGql,
    pub source_ref: Option<String>,
    pub status: MemoryStatusGql,
    pub project_id: Option<ID>,
    pub task_id: Option<ID>,
    /// "Towards whom" / "with whom".
    pub stakeholders: Vec<String>,
}

#[ComplexObject]
impl MemoryGql {
    /// The memory named by `proposedSupersedes`, resolved.
    ///
    /// A triage screen has to *name* the conflict — an id alone says nothing about
    /// which decision is being contradicted, which is exactly why the prose
    /// workaround this replaces spelled the old title out by hand. Resolved here
    /// rather than joined into the list query so `pendingMemories` keeps its shape
    /// and the lookup only happens for the callers that ask for it.
    ///
    /// `null` when the candidate proposes nothing, or when the memory it named has
    /// since been deleted (the column is `ON DELETE SET NULL`).
    async fn contradicts(&self, ctx: &Context<'_>) -> Result<Option<MemoryGql>> {
        let Some(proposed) = &self.proposed_supersedes else {
            return Ok(None);
        };
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn MemoryRepository>>()?;
        let id = Uuid::parse_str(proposed)
            .map_err(|e| async_graphql::Error::new(format!("Invalid proposedSupersedes: {e}")))?;
        let found = memory_uc::get_memory(repo.as_ref(), user_id, id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(found.map(MemoryGql::from))
    }
}

impl From<Memory> for MemoryGql {
    fn from(m: Memory) -> Self {
        Self {
            id: ID(m.id.to_string()),
            kind: m.kind.into(),
            title: m.title,
            body: m.body,
            occurred_at: m.occurred_at,
            recorded_at: m.recorded_at,
            invalidated_at: m.invalidated_at,
            superseded_by: m.superseded_by.map(|id| ID(id.to_string())),
            proposed_supersedes: m.proposed_supersedes.map(|id| ID(id.to_string())),
            source: m.source.into(),
            source_ref: m.source_ref,
            status: m.status.into(),
            project_id: m.project_id.map(|id| ID(id.to_string())),
            task_id: m.task_id.map(|id| ID(id.to_string())),
            stakeholders: m.stakeholders,
        }
    }
}

/// A recall hit: the memory plus the score it obtained. Results come back
/// best-first, so `score` is for debugging the ranking, not for re-sorting.
#[derive(SimpleObject)]
pub struct ScoredMemoryGql {
    pub memory: MemoryGql,
    pub score: f64,
}

impl From<ScoredMemory> for ScoredMemoryGql {
    fn from(s: ScoredMemory) -> Self {
        Self {
            memory: MemoryGql::from(s.memory),
            score: s.score,
        }
    }
}

/// Result of `acceptMemory`.
///
/// When `accepted` is null, `nearDuplicates` is non-empty: the candidate looks
/// like an existing active memory and was NOT added. The caller must choose
/// `mergeMemory` (a rewording) or `supersedeMemory` (the fact changed), or retry
/// with `force: true` — an add is never silent.
#[derive(SimpleObject)]
pub struct AcceptMemoryResultGql {
    pub accepted: Option<MemoryGql>,
    pub near_duplicates: Vec<MemoryGql>,
}

impl From<AcceptOutcome> for AcceptMemoryResultGql {
    fn from(outcome: AcceptOutcome) -> Self {
        match outcome {
            AcceptOutcome::Accepted(memory) => Self {
                accepted: Some(MemoryGql::from(memory)),
                near_duplicates: vec![],
            },
            AcceptOutcome::NearDuplicates { duplicates, .. } => Self {
                accepted: None,
                near_duplicates: duplicates.into_iter().map(MemoryGql::from).collect(),
            },
        }
    }
}

/// Result of `mergeMemory`: ONE row survives, the other is gone. History is not
/// preserved — that is what distinguishes a merge from a supersession.
#[derive(SimpleObject)]
pub struct MergeMemoryResultGql {
    pub survivor: MemoryGql,
    pub discarded_id: ID,
}

impl From<MergeOutcome> for MergeMemoryResultGql {
    fn from(outcome: MergeOutcome) -> Self {
        Self {
            survivor: MemoryGql::from(outcome.survivor),
            discarded_id: ID(outcome.discarded.to_string()),
        }
    }
}

/// Result of `supersedeMemory`: BOTH rows survive. `invalidated` carries the end
/// of its validity and a pointer to `successor`.
#[derive(SimpleObject)]
pub struct SupersedeMemoryResultGql {
    pub invalidated: MemoryGql,
    pub successor: MemoryGql,
}

impl From<SupersedeOutcome> for SupersedeMemoryResultGql {
    fn from(outcome: SupersedeOutcome) -> Self {
        Self {
            invalidated: MemoryGql::from(outcome.invalidated),
            successor: MemoryGql::from(outcome.successor),
        }
    }
}

/// A file the import did not turn into a memory, and why.
#[derive(SimpleObject)]
pub struct SkippedMemoryFileGql {
    pub file_name: String,
    /// `already_imported` | `no_frontmatter` | `no_title`.
    pub reason: String,
}

/// Result of `importMemories`.
#[derive(SimpleObject)]
pub struct MemoryImportResultGql {
    pub imported: Vec<MemoryGql>,
    pub skipped: Vec<SkippedMemoryFileGql>,
    pub imported_count: i32,
    pub skipped_count: i32,
}

impl From<MemoryImportOutcome> for MemoryImportResultGql {
    fn from(outcome: MemoryImportOutcome) -> Self {
        Self {
            imported_count: outcome.imported.len() as i32,
            skipped_count: outcome.skipped.len() as i32,
            imported: outcome.imported.into_iter().map(MemoryGql::from).collect(),
            skipped: outcome
                .skipped
                .into_iter()
                .map(|s| SkippedMemoryFileGql {
                    file_name: s.file_name,
                    reason: s.reason.as_str().to_string(),
                })
                .collect(),
        }
    }
}

/// Input for the `remember` mutation.
#[derive(InputObject, Debug)]
pub struct RememberInputGql {
    pub kind: MemoryKindGql,
    /// One sentence: what is retained.
    pub title: String,
    /// The context: why, alternatives dropped (`--why` on the CLI).
    pub body: Option<String>,
    /// When it was decided / promised. Defaults to now.
    pub occurred_at: Option<DateTime<Utc>>,
    /// Defaults to `CLAUDE_SESSION`.
    pub source: Option<MemorySourceGql>,
    /// Worklog entry id, session id — free-form, no foreign key.
    pub source_ref: Option<String>,
    /// Skip the validation queue and store as `ACTIVE`.
    pub confirmed: Option<bool>,
    /// The active memory this candidate contradicts, so the triage sees the
    /// conflict and `supersedeMemory` can default to it. Takes a full UUID **or**
    /// the short reference the brief renders (`m:7c1`), like every other memory
    /// argument.
    ///
    /// A proposal is a question for the validation queue, so it is incompatible
    /// with `confirmed: true` — revise an established memory with `supersedeMemory`
    /// instead.
    pub proposed_supersedes: Option<ID>,
    pub project_id: Option<ID>,
    pub task_id: Option<ID>,
    pub stakeholders: Option<Vec<String>>,
}
