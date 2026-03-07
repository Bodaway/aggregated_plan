use domain::types::*;

use crate::errors::AppError;
use crate::repositories::*;

/// A suggested duplicate pair with its similarity score.
pub struct DuplicateSuggestion {
    pub task_a: Task,
    pub task_b: Task,
    pub score: f64,
}

/// Detect potential duplicate tasks for a user.
pub async fn detect_duplicates(
    _task_repo: &dyn TaskRepository,
    _link_repo: &dyn TaskLinkRepository,
    _user_id: UserId,
) -> Result<Vec<DuplicateSuggestion>, AppError> {
    todo!()
}

/// Confirm a duplicate suggestion and merge two tasks.
pub async fn confirm_merge(
    _task_repo: &dyn TaskRepository,
    _link_repo: &dyn TaskLinkRepository,
    _primary_task_id: TaskId,
    _secondary_task_id: TaskId,
) -> Result<TaskLink, AppError> {
    todo!()
}

/// Reject a duplicate suggestion so it won't be shown again.
pub async fn reject_duplicate(
    _link_repo: &dyn TaskLinkRepository,
    _task_id_a: TaskId,
    _task_id_b: TaskId,
) -> Result<TaskLink, AppError> {
    todo!()
}
