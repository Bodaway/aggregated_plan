use chrono::NaiveDate;
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::*;

/// A task with its computed quadrant for the Eisenhower matrix.
pub struct TaskWithQuadrant {
    pub task: Task,
    pub quadrant: Quadrant,
}

/// Get all tasks for a user grouped into Eisenhower matrix quadrants.
pub async fn get_priority_matrix(
    _task_repo: &dyn TaskRepository,
    _user_id: UserId,
    _today: NaiveDate,
) -> Result<Vec<TaskWithQuadrant>, AppError> {
    todo!()
}

/// Override the urgency level of a task (manual override).
pub async fn override_urgency(
    _task_repo: &dyn TaskRepository,
    _task_id: TaskId,
    _urgency: UrgencyLevel,
) -> Result<Task, AppError> {
    todo!()
}

/// Override the impact level of a task.
pub async fn override_impact(
    _task_repo: &dyn TaskRepository,
    _task_id: TaskId,
    _impact: ImpactLevel,
) -> Result<Task, AppError> {
    todo!()
}
