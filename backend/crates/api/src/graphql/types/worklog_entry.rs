use std::sync::Arc;

use async_graphql::{Context, InputObject, Object, SimpleObject, ID};
use chrono::{DateTime, NaiveDate, Utc};

use application::repositories::TaskRepository;
use application::use_cases::consolidation::MarkConsolidatedOutcome;
use application::use_cases::reattribution::{ReattributionOutcome, TaskTimeChange};
use application::use_cases::worklog::FlushOutcome;
use domain::types::WorklogEntry;

use super::task::TaskGql;

/// GraphQL wrapper for the domain WorklogEntry entity.
pub struct WorklogEntryGql(pub WorklogEntry);

#[Object]
impl WorklogEntryGql {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }

    async fn task_id(&self) -> ID {
        ID(self.0.task_id.to_string())
    }

    /// Hydrated task. Null only if the task was deleted between list and resolve
    /// (shouldn't happen under normal conditions thanks to the FK cascade).
    async fn task(&self, ctx: &Context<'_>) -> Option<TaskGql> {
        let repo = ctx.data::<Arc<dyn TaskRepository>>().ok()?;
        let task = repo.find_by_id(self.0.task_id).await.ok()??;
        Some(TaskGql(task))
    }

    async fn body(&self) -> &str {
        &self.0.body
    }

    async fn logged_at(&self) -> DateTime<Utc> {
        self.0.logged_at
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.0.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.0.updated_at
    }

    /// The occurrence date of the task this entry belongs to.
    /// `Some` for recurring instances, `None` for one-shot tasks.
    async fn occurrence_date(&self, ctx: &Context<'_>) -> Option<NaiveDate> {
        let repo = ctx.data::<Arc<dyn TaskRepository>>().ok()?;
        let task = repo.find_by_id(self.0.task_id).await.ok()??;
        task.occurrence_date
    }

    /// The session that wrote this entry. Null is the human, working by hand.
    async fn session_id(&self) -> Option<String> {
        self.0.session_id.clone()
    }
}

/// Result of flushing worklog time into activity slots.
pub struct FlushResultGql(pub FlushOutcome);

#[Object]
impl FlushResultGql {
    /// Start of the next flush's selector window (not a watermark): entries at/after
    /// this instant are left for that flush, which picks them up by half-day rather
    /// than by comparing timestamps against this value.
    async fn active_since(&self) -> chrono::DateTime<chrono::Utc> {
        self.0.active_since
    }
    /// Number of activity slots written by this flush.
    async fn slots_written(&self) -> i32 {
        self.0.slots_written as i32
    }
}

/// Result of `markWorklogEntriesConsolidated` — the write side of the
/// per-entry consolidation watermark (§6.2).
///
/// `marked` is deliberately allowed to be lower than `requested`: an id already
/// consolidated, or belonging to another user, is a no-op rather than an error, so
/// a job that retries after a crash converges instead of failing.
#[derive(SimpleObject)]
pub struct MarkConsolidatedResultGql {
    /// How many ids the caller submitted.
    pub requested: i32,
    /// How many rows actually moved from unmarked to marked.
    pub marked: i32,
    /// The timestamp written into `worklog_entries.consolidated_at`.
    pub consolidated_at: DateTime<Utc>,
}

impl From<MarkConsolidatedOutcome> for MarkConsolidatedResultGql {
    fn from(outcome: MarkConsolidatedOutcome) -> Self {
        Self {
            requested: outcome.requested as i32,
            marked: outcome.marked as i32,
            consolidated_at: outcome.consolidated_at,
        }
    }
}

/// Result of `recordConsolidationRun`: the timestamp now stored under
/// `memory.consolidation.last_run` in `configuration`, and the key it went to — so
/// a caller can verify it landed where `aplan brief` reads it.
#[derive(SimpleObject)]
pub struct ConsolidationRunGql {
    pub key: String,
    pub ran_at: DateTime<Utc>,
}

/// What `reattributeWorklogEntries` is asked to move.
///
/// Two selections, one at a time: the explicit entries, or the source task's
/// entries over a local date window. `confirm` defaults to `false`, so the default
/// call previews and writes nothing — this operation rewrites billing-relevant
/// history and an accidental invocation must not be able to move a month of work.
#[derive(InputObject, Debug)]
pub struct ReattributeWorklogInput {
    /// The task the time is currently attributed to.
    pub from_task: ID,
    /// The task it belongs to.
    pub to_task: ID,
    /// Entry references: full UUIDs or id prefixes. Resolved server-side, where an
    /// ambiguous prefix is reported rather than guessed.
    pub entry_refs: Option<Vec<String>>,
    /// First local day of the window (inclusive).
    pub since: Option<NaiveDate>,
    /// Last local day of the window (inclusive). Defaults to `since`.
    pub until: Option<NaiveDate>,
    /// Write. Absent or `false` previews.
    pub confirm: Option<bool>,
}

/// One task's hours on the affected days, before and after the correction.
#[derive(SimpleObject)]
pub struct TaskTimeChangeGql {
    pub task_id: ID,
    pub hours_before: f64,
    pub hours_after: f64,
}

impl From<TaskTimeChange> for TaskTimeChangeGql {
    fn from(change: TaskTimeChange) -> Self {
        Self {
            task_id: ID(change.task_id.to_string()),
            hours_before: change.hours_before,
            hours_after: change.hours_after,
        }
    }
}

/// Result of `reattributeWorklogEntries`.
///
/// Everything a reader needs to CHECK the correction rather than trust it: what was
/// selected, what actually moved, which local days had their slots rebuilt, and the
/// hours on both sides before and after. `applied: false` means nothing was written.
#[derive(SimpleObject)]
pub struct ReattributionResultGql {
    pub applied: bool,
    pub selected_entries: Vec<ID>,
    /// Rows that actually moved. `0` on a preview; below `selectedEntries` only if a
    /// row left the source task concurrently.
    pub moved_entries: i32,
    pub affected_dates: Vec<NaiveDate>,
    /// Closed slots of the two tasks dropped from those days.
    pub slots_discarded: i32,
    /// Slots written back from the entries.
    pub slots_rebuilt: i32,
    pub source: TaskTimeChangeGql,
    pub destination: TaskTimeChangeGql,
}

impl From<ReattributionOutcome> for ReattributionResultGql {
    fn from(outcome: ReattributionOutcome) -> Self {
        Self {
            applied: outcome.applied,
            selected_entries: outcome
                .selected_entries
                .iter()
                .map(|id| ID(id.to_string()))
                .collect(),
            moved_entries: outcome.moved_entries as i32,
            affected_dates: outcome.affected_dates,
            slots_discarded: outcome.slots_discarded as i32,
            slots_rebuilt: outcome.slots_rebuilt as i32,
            source: outcome.source.into(),
            destination: outcome.destination.into(),
        }
    }
}

/// Filter input for `worklogEntries`.
/// When `recurrence_id` is provided, it wins over `task_ids` and returns all entries
/// whose task belongs to the given recurrence template.
#[derive(InputObject, Debug, Default)]
pub struct WorklogEntryFilterInput {
    pub task_ids: Option<Vec<ID>>,
    pub recurrence_id: Option<ID>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}
