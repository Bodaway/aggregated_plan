use async_trait::async_trait;
use chrono::NaiveDate;
use domain::types::*;

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

    /// Save a new task or update an existing one.
    async fn save(&self, task: &Task) -> Result<(), RepositoryError>;

    /// Save multiple tasks in a single batch operation.
    async fn save_batch(&self, tasks: &[Task]) -> Result<(), RepositoryError>;

    /// Delete a task by its identifier.
    async fn delete(&self, id: TaskId) -> Result<(), RepositoryError>;

    /// Delete all tasks from a given source whose source_id is NOT in `keep_ids`.
    /// Used after a sync to remove tasks that are no longer returned by the source.
    async fn delete_stale_by_source(
        &self,
        user_id: UserId,
        source: Source,
        keep_ids: &[String],
    ) -> Result<u64, RepositoryError>;
}
