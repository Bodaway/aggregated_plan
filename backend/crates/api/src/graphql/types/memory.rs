use application::use_cases::memory::{AcceptOutcome, MemoryImportOutcome};
use async_graphql::{InputObject, SimpleObject, ID};
use chrono::{DateTime, Utc};
use domain::rules::memory_lifecycle::{MergeOutcome, SupersedeOutcome};
use domain::rules::recall::ScoredMemory;
use domain::types::Memory;

use super::enums::{MemoryKindGql, MemorySourceGql, MemoryStatusGql};

/// A semantic memory: what must be known. Bi-temporal — `occurredAt` is when it
/// became true, `invalidatedAt` when it stopped being true.
#[derive(SimpleObject)]
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
    pub source: MemorySourceGql,
    pub source_ref: Option<String>,
    pub status: MemoryStatusGql,
    pub project_id: Option<ID>,
    pub task_id: Option<ID>,
    /// "Towards whom" / "with whom".
    pub stakeholders: Vec<String>,
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
    pub project_id: Option<ID>,
    pub task_id: Option<ID>,
    pub stakeholders: Option<Vec<String>>,
}
