use async_graphql::{Object, ID};
use domain::types::Task;

use super::enums::{SourceGql, TaskStatusGql};

/// Lean task projection for client-side search. Carries pre-resolved project
/// and tag names so the resolver can batch their lookup.
pub struct SearchableTaskGql {
    pub task: Task,
    pub project_name: Option<String>,
    pub tag_names: Vec<String>,
}

#[Object]
impl SearchableTaskGql {
    async fn id(&self) -> ID {
        ID(self.task.id.to_string())
    }

    async fn title(&self) -> &str {
        &self.task.title
    }

    async fn source_id(&self) -> Option<&str> {
        self.task.source_id.as_deref()
    }

    async fn source(&self) -> SourceGql {
        self.task.source.into()
    }

    async fn assignee(&self) -> Option<&str> {
        self.task.assignee.as_deref()
    }

    async fn project_name(&self) -> Option<&str> {
        self.project_name.as_deref()
    }

    async fn tags(&self) -> &[String] {
        &self.tag_names
    }

    async fn description(&self) -> Option<&str> {
        self.task.description.as_deref()
    }

    async fn status(&self) -> TaskStatusGql {
        self.task.status.into()
    }
}
