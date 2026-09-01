use async_trait::async_trait;
use chrono::NaiveDate;
use domain::types::*;
use domain::types::recurrence::RecurrenceTemplateId;

use crate::errors::RepositoryError;

/// Filter criteria for querying tasks.
pub struct TaskFilter {
    pub status: Option<Vec<TaskStatus>>,
    pub source: Option<Vec<Source>>,
    pub project_id: Option<ProjectId>,
    pub assignee: Option<String>,
    pub deadline_before: Option<NaiveDate>,
    pub deadline_after: Option<NaiveDate>,
    pub tag_ids: Option<Vec<TagId>>,
    pub tracking_state: Option<Vec<TrackingState>>,
    /// Exact match against `tasks.source_id` (e.g. a Jira key like "AP-123").
    pub source_id: Option<String>,
    /// Case-insensitive substring match against `tasks.title`.
    pub title_contains: Option<String>,
}

impl TaskFilter {
    /// Create an empty filter that matches all tasks.
    pub fn empty() -> Self {
        TaskFilter {
            status: None,
            source: None,
            project_id: None,
            assignee: None,
            deadline_before: None,
            deadline_after: None,
            tag_ids: None,
            tracking_state: None,
            source_id: None,
            title_contains: None,
        }
    }
}

/// Repository trait for persisting and querying tasks.
#[async_trait]
pub trait TaskRepository: Send + Sync {
    /// Find a task by its unique identifier.
    async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError>;

    /// Find all tasks for a user, optionally filtered.
    async fn find_by_user(
        &self,
        user_id: UserId,
        filter: &TaskFilter,
    ) -> Result<Vec<Task>, RepositoryError>;

    /// Find a task by its external source and source-specific identifier.
    async fn find_by_source(
        &self,
        user_id: UserId,
        source: Source,
        source_id: &str,
    ) -> Result<Option<Task>, RepositoryError>;

    /// Find tasks within a date range (based on deadline or planned dates).
    async fn find_by_date_range(
        &self,
        user_id: UserId,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Task>, RepositoryError>;

    /// R73/R74: find the active tasks that are overdue as of `today` — either their
    /// `planned_start` or their `deadline` is strictly before it. Losers of a merge are
    /// excluded: they no longer exist for the user, so they cannot be late.
    async fn find_overdue(
        &self,
        user_id: UserId,
        today: NaiveDate,
    ) -> Result<Vec<Task>, RepositoryError>;

    /// Save a new task or update an existing one.
    async fn save(&self, task: &Task) -> Result<(), RepositoryError>;

    /// Save multiple tasks in a single batch operation.
    async fn save_batch(&self, tasks: &[Task]) -> Result<(), RepositoryError>;

    /// Delete a task by its identifier.
    async fn delete(&self, id: TaskId) -> Result<(), RepositoryError>;

    /// Delete the tasks of `source` whose `source_id` is NOT in `keep_ids`.
    /// Used after a sync to drop tasks the source no longer returns.
    ///
    /// Two refusals are part of the contract, not implementation details:
    /// - an **empty** `keep_ids` deletes NOTHING and returns `Ok(0)`. It carries no
    ///   information about staleness (a successful fetch returns zero rows for a
    ///   mistyped project key or a revoked permission just as readily as for a
    ///   genuinely empty source), so reading it as "everything is stale" is a
    ///   silent bulk delete. Callers must still avoid calling it in that case.
    /// - a task carrying **logged work** (worklog entries or activity slots) is
    ///   never deleted. It stops being refreshed but survives locally; only an
    ///   explicit `delete` removes it. Logged work is user data, not synced data.
    async fn delete_stale_by_source(
        &self,
        user_id: UserId,
        source: Source,
        keep_ids: &[String],
    ) -> Result<u64, RepositoryError>;

    /// Find an existing task instance for a specific recurrence template and occurrence date.
    ///
    /// Returns `None` if no instance exists yet. Default implementation returns `Ok(None)`;
    /// concrete repositories override this when Wave 3A is implemented.
    async fn find_by_recurrence_slot(
        &self,
        template_id: RecurrenceTemplateId,
        occurrence_date: NaiveDate,
    ) -> Result<Option<Task>, RepositoryError> {
        let _ = (template_id, occurrence_date);
        Ok(None)
    }

    /// Find all task instances for a given recurrence template.
    ///
    /// Default implementation returns an empty vec; concrete repositories override in Wave 3A.
    async fn find_by_recurrence(
        &self,
        template_id: RecurrenceTemplateId,
    ) -> Result<Vec<Task>, RepositoryError> {
        let _ = template_id;
        Ok(vec![])
    }

    /// Distinct, sorted delegate names previously used on the user's tasks.
    /// Backs the auto-learned suggestion list for the delegation field.
    /// Default implementation returns an empty list; concrete repositories override.
    async fn list_delegates(&self, user_id: UserId) -> Result<Vec<String>, RepositoryError> {
        let _ = user_id;
        Ok(vec![])
    }
}
