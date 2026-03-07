use std::sync::Arc;

use async_graphql::{Context, Object, Result, ID};
use domain::types::UserId;
use uuid::Uuid;

use application::repositories::*;
use application::use_cases::{priority, task_management};

use super::types::*;

/// Root mutation type for the GraphQL schema.
#[derive(Default)]
pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// No-op mutation placeholder. Returns true.
    async fn noop(&self) -> bool {
        true
    }

    /// Create a new personal task.
    async fn create_task(
        &self,
        ctx: &Context<'_>,
        input: CreateTaskInput,
    ) -> Result<TaskGql> {
        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let today = chrono::Utc::now().date_naive();

        let app_input = convert_create_input(input)?;

        let task =
            task_management::create_personal_task(task_repo.as_ref(), *user_id, app_input, today)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Update an existing task.
    async fn update_task(
        &self,
        ctx: &Context<'_>,
        id: ID,
        input: UpdateTaskInput,
    ) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let task_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;
        let today = chrono::Utc::now().date_naive();

        let app_input = convert_update_input(input)?;

        let task = task_management::update_task(task_repo.as_ref(), task_id, app_input, today)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Delete a task by ID. Returns true on success.
    async fn delete_task(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let task_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        task_management::delete_task(task_repo.as_ref(), task_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    /// Mark a task as completed.
    async fn complete_task(&self, ctx: &Context<'_>, id: ID) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let task_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        let task = task_management::complete_task(task_repo.as_ref(), task_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Override the urgency level of a task (manual override).
    async fn update_priority(
        &self,
        ctx: &Context<'_>,
        task_id: ID,
        urgency: Option<UrgencyLevelGql>,
        impact: Option<ImpactLevelGql>,
    ) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let id = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        let mut task: Option<domain::types::Task> = None;

        if let Some(u) = urgency {
            task = Some(
                priority::override_urgency(task_repo.as_ref(), id, u.into())
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?,
            );
        }

        if let Some(i) = impact {
            task = Some(
                priority::override_impact(task_repo.as_ref(), id, i.into())
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?,
            );
        }

        match task {
            Some(t) => Ok(TaskGql(t)),
            None => Err(async_graphql::Error::new(
                "At least one of urgency or impact must be provided",
            )),
        }
    }

    /// Reset urgency to auto-calculated based on deadline.
    async fn reset_urgency(&self, ctx: &Context<'_>, task_id: ID) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let id = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;
        let today = chrono::Utc::now().date_naive();

        let task = priority::reset_urgency(task_repo.as_ref(), id, today)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }
}

/// Convert GraphQL CreateTaskInput to application layer input.
fn convert_create_input(
    input: CreateTaskInput,
) -> Result<task_management::CreateTaskInput> {
    let project_id = match input.project_id {
        Some(id) => Some(
            Uuid::parse_str(&id)
                .map_err(|e| async_graphql::Error::new(format!("Invalid project ID: {}", e)))?,
        ),
        None => None,
    };

    let tag_ids: Vec<Uuid> = match input.tag_ids {
        Some(ids) => ids
            .into_iter()
            .map(|id| {
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid tag ID: {}", e)))
            })
            .collect::<Result<Vec<_>>>()?,
        None => vec![],
    };

    Ok(task_management::CreateTaskInput {
        title: input.title,
        description: input.description,
        project_id,
        deadline: input.deadline,
        planned_start: input.planned_start,
        planned_end: input.planned_end,
        estimated_hours: input.estimated_hours.map(|h| h as f32),
        impact: input.impact.map(|i| i.into()),
        urgency: input.urgency.map(|u| u.into()),
        tags: tag_ids,
    })
}

/// Convert GraphQL UpdateTaskInput to application layer input.
fn convert_update_input(
    input: UpdateTaskInput,
) -> Result<task_management::UpdateTaskInput> {
    let project_id = match input.project_id {
        Some(id) => Some(Some(
            Uuid::parse_str(&id)
                .map_err(|e| async_graphql::Error::new(format!("Invalid project ID: {}", e)))?,
        )),
        None => None,
    };

    let tag_ids = match input.tag_ids {
        Some(ids) => Some(
            ids.into_iter()
                .map(|id| {
                    Uuid::parse_str(&id)
                        .map_err(|e| async_graphql::Error::new(format!("Invalid tag ID: {}", e)))
                })
                .collect::<Result<Vec<_>>>()?,
        ),
        None => None,
    };

    Ok(task_management::UpdateTaskInput {
        title: input.title,
        description: input.description.map(Some),
        project_id,
        deadline: input.deadline.map(Some),
        planned_start: input.planned_start.map(Some),
        planned_end: input.planned_end.map(Some),
        estimated_hours: input.estimated_hours.map(|h| Some(h as f32)),
        status: input.status.map(|s| s.into()),
        impact: input.impact.map(|i| i.into()),
        urgency: input.urgency.map(|u| u.into()),
        tags: tag_ids,
    })
}
