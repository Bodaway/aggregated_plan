use std::sync::Arc;

use async_graphql::{Context, InputObject, Object, ID};
use chrono::{DateTime, Utc};

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
}

/// Filter input for `worklogEntries`.
#[derive(InputObject, Debug, Default)]
pub struct WorklogEntryFilterInput {
    pub task_ids: Option<Vec<ID>>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}
