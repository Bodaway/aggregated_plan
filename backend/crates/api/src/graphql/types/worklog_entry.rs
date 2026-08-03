use std::sync::Arc;

use async_graphql::{Context, InputObject, Object, SimpleObject, ID};
use chrono::{DateTime, NaiveDate, Utc};

use application::repositories::TaskRepository;
use application::use_cases::consolidation::MarkConsolidatedOutcome;
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
}

/// Result of flushing worklog time into activity slots.
pub struct FlushResultGql(pub FlushOutcome);

#[Object]
impl FlushResultGql {
    /// New watermark: entries at/after this instant are not yet materialized.
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
