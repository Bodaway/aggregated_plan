use std::sync::Arc;

use async_graphql::{Context, Object, SimpleObject, ID};
use chrono::{DateTime, Utc};

use application::repositories::TaskRepository;
use domain::types::Session;

use super::enums::SessionModeGql;

/// GraphQL wrapper for the domain Session entity.
pub struct SessionGql(pub Session);

#[Object]
impl SessionGql {
    /// The Claude Code session id. A `String`, not an `ID`: it is minted by another
    /// program and is never a row id of ours to resolve.
    async fn id(&self) -> String {
        self.0.id.clone()
    }

    async fn task_id(&self) -> Option<ID> {
        self.0.task_id.map(|t| ID(t.to_string()))
    }

    async fn task(&self, ctx: &Context<'_>) -> Option<SessionTaskSummaryGql> {
        let task_id = self.0.task_id?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>().ok()?;
        let task = task_repo.find_by_id(task_id).await.ok()??;
        Some(SessionTaskSummaryGql {
            id: ID(task.id.to_string()),
            title: task.title,
        })
    }

    async fn mode(&self) -> SessionModeGql {
        self.0.mode.into()
    }

    async fn label(&self) -> Option<String> {
        self.0.label.clone()
    }

    async fn started_at(&self) -> DateTime<Utc> {
        self.0.started_at
    }

    async fn last_seen_at(&self) -> DateTime<Utc> {
        self.0.last_seen_at
    }

    async fn last_flush_at(&self) -> Option<DateTime<Utc>> {
        self.0.last_flush_at
    }

    async fn ended_at(&self) -> Option<DateTime<Utc>> {
        self.0.ended_at
    }
}

#[derive(SimpleObject)]
pub struct SessionTaskSummaryGql {
    pub id: ID,
    pub title: String,
}

/// A bind, and the task the session was on before it — which the caller flushes.
#[derive(SimpleObject)]
pub struct BindSessionResultGql {
    pub session: SessionGql,
    pub previous_task_id: Option<ID>,
}
