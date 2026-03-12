use async_graphql::{Object, ID};
use chrono::{DateTime, Utc};

use domain::types::Project;

use super::enums::*;

/// GraphQL wrapper for the domain Project entity.
pub struct ProjectGql(pub Project);

#[Object]
impl ProjectGql {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }

    async fn name(&self) -> &str {
        &self.0.name
    }

    async fn source(&self) -> SourceGql {
        self.0.source.into()
    }

    async fn source_id(&self) -> Option<&str> {
        self.0.source_id.as_deref()
    }

    async fn status(&self) -> ProjectStatusGql {
        self.0.status.into()
    }

    /// Total number of tasks in this project. Stub: returns 0 for now.
    async fn task_count(&self) -> i32 {
        // Will be resolved via repository lookup
        0
    }

    /// Number of open (non-done) tasks in this project. Stub: returns 0 for now.
    async fn open_task_count(&self) -> i32 {
        // Will be resolved via repository lookup
        0
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.0.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.0.updated_at
    }
}
