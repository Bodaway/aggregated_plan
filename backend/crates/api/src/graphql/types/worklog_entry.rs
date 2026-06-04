use std::sync::Arc;

use async_graphql::{Context, InputObject, Object, ID};
use chrono::{DateTime, NaiveDate, Utc};

use application::repositories::TaskRepository;
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
