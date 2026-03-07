use chrono::{DateTime, NaiveDate, Utc};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::*;

/// Input data for creating a new personal task.
pub struct CreateTaskInput {
    pub title: String,
    pub description: Option<String>,
    pub project_id: Option<ProjectId>,
    pub deadline: Option<NaiveDate>,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub estimated_hours: Option<f32>,
    pub impact: Option<ImpactLevel>,
    pub urgency: Option<UrgencyLevel>,
    pub tags: Vec<TagId>,
}

/// Input data for updating an existing task.
pub struct UpdateTaskInput {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub project_id: Option<Option<ProjectId>>,
    pub deadline: Option<Option<NaiveDate>>,
    pub planned_start: Option<Option<DateTime<Utc>>>,
    pub planned_end: Option<Option<DateTime<Utc>>>,
    pub estimated_hours: Option<Option<f32>>,
    pub status: Option<TaskStatus>,
    pub impact: Option<ImpactLevel>,
    pub urgency: Option<UrgencyLevel>,
    pub tags: Option<Vec<TagId>>,
}

/// Create a new personal task with auto-calculated urgency if not provided.
pub async fn create_personal_task(
    _task_repo: &dyn TaskRepository,
    _user_id: UserId,
    _input: CreateTaskInput,
    _today: NaiveDate,
) -> Result<Task, AppError> {
    todo!()
}

/// Update an existing task. Returns the updated task.
pub async fn update_task(
    _task_repo: &dyn TaskRepository,
    _task_id: TaskId,
    _input: UpdateTaskInput,
    _today: NaiveDate,
) -> Result<Task, AppError> {
    todo!()
}

/// Delete a task by its identifier.
pub async fn delete_task(
    _task_repo: &dyn TaskRepository,
    _task_id: TaskId,
) -> Result<(), AppError> {
    todo!()
}

/// Mark a task as completed.
pub async fn complete_task(
    _task_repo: &dyn TaskRepository,
    _task_id: TaskId,
) -> Result<Task, AppError> {
    todo!()
}
