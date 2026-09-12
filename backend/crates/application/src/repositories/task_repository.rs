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
    /// When set, a recurring series contributes at most one row: the occurrence
    /// with the greatest `occurrence_date` among those at or before the given day.
    /// Future occurrences of the materialization horizon are hidden, and a series
    /// whose occurrences all lie ahead contributes nothing. `None` disables the
    /// rule and returns every occurrence.
    ///
    /// A date rather than a flag on purpose: the day is the caller's to decide, and
    /// the repository has no business reading a clock -- `find_overdue` already
    /// takes its `today` as an argument for the same reason.
    ///
    /// The collapse picks that row **without looking at status**; the caller's
    /// status filter then applies to the row that survived. The reverse order --
    /// filter first, collapse after -- would resurrect exactly the old occurrences
    /// this exists to hide.
    ///
    /// Tasks with a NULL `recurrence_id` are outside the rule and always returned.
    pub collapse_recurrences: Option<NaiveDate>,
}

impl TaskFilter {
    /// A filter that matches every task, **except** that a recurring series is
    /// collapsed onto its latest due occurrence -- see [`collapse_recurrences`].
    /// Collapsing is the default because the ordinary question a list answers is
    /// "what is to be done now", and a series' older occurrences are noise against
    /// it. A caller that means the whole series sets the field to `None`.
    ///
    /// The day comes from `Utc::now()`, the same convention the GraphQL layer
    /// already uses for "today".
    ///
    /// It also restricts to `tracking_state = Followed`. The tracking state is a
    /// general rule across the app, not a per-view choice: a task the user never
    /// triaged and a task they explicitly dismissed have no business appearing in
    /// the dashboard, the priority matrix, the alerts or the morning brief. Two
    /// kinds of caller opt out by setting the field to `None` -- search, which must
    /// find a task whatever its state, and deduplication, whose whole job is to
    /// match freshly synced `Inbox` tasks against followed ones. The triage views
    /// opt out by naming the states they want.
    ///
    /// [`collapse_recurrences`]: TaskFilter::collapse_recurrences
    pub fn empty() -> Self {
        TaskFilter {
            status: None,
            source: None,
            project_id: None,
            assignee: None,
            deadline_before: None,
            deadline_after: None,
            tag_ids: None,
            source_id: None,
            title_contains: None,
            collapse_recurrences: Some(chrono::Utc::now().date_naive()),
            tracking_state: Some(vec![TrackingState::Followed]),
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

    /// Retrouve une tâche par la clé d'idempotence du client, si elle existe.
    ///
    /// Default implementation returns `Ok(None)` -- most test doubles across the
    /// workspace never exercise offline-capture replay and would otherwise need a
    /// boilerplate override for a feature unrelated to what they test. The one
    /// double that *does* test this behaviour (`task_management::tests`) and the
    /// real `SqliteTaskRepository` both override it.
    async fn find_by_client_request_id(
        &self,
        _user_id: UserId,
        _client_request_id: &str,
    ) -> Result<Option<Task>, RepositoryError> {
        Ok(None)
    }

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
