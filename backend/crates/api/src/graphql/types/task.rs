use async_graphql::{InputObject, Object, SimpleObject, ID};
use chrono::{DateTime, NaiveDate, Utc};

use domain::rules::priority::determine_quadrant;
use domain::types::Task;

use super::enums::*;
use super::pagination::PageInfo;
use super::tag::TagGql;

/// GraphQL wrapper for the domain Task entity.
pub struct TaskGql(pub Task);

#[Object]
impl TaskGql {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }

    async fn title(&self) -> &str {
        &self.0.title
    }

    async fn description(&self) -> Option<&str> {
        self.0.description.as_deref()
    }

    async fn source(&self) -> SourceGql {
        self.0.source.into()
    }

    async fn source_id(&self) -> Option<&str> {
        self.0.source_id.as_deref()
    }

    async fn jira_status(&self) -> Option<&str> {
        self.0.jira_status.as_deref()
    }

    async fn status(&self) -> TaskStatusGql {
        self.0.status.into()
    }

    async fn project_id(&self) -> Option<ID> {
        self.0.project_id.map(|pid| ID(pid.to_string()))
    }

    /// The associated project. Resolved via data loader in a later task.
    /// Returns None for now.
    async fn project(&self) -> Option<String> {
        // Will be resolved via data loader or separate query
        None
    }

    async fn assignee(&self) -> Option<&str> {
        self.0.assignee.as_deref()
    }

    async fn deadline(&self) -> Option<NaiveDate> {
        self.0.deadline
    }

    async fn planned_start(&self) -> Option<DateTime<Utc>> {
        self.0.planned_start
    }

    async fn planned_end(&self) -> Option<DateTime<Utc>> {
        self.0.planned_end
    }

    async fn estimated_hours(&self) -> Option<f64> {
        self.0.estimated_hours.map(|h| h as f64)
    }

    async fn urgency(&self) -> UrgencyLevelGql {
        self.0.urgency.into()
    }

    async fn urgency_manual(&self) -> bool {
        self.0.urgency_manual
    }

    async fn impact(&self) -> ImpactLevelGql {
        self.0.impact.into()
    }

    /// Computed Eisenhower quadrant based on urgency and impact.
    async fn quadrant(&self) -> QuadrantGql {
        determine_quadrant(self.0.urgency, self.0.impact).into()
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.0.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.0.updated_at
    }

    /// Tags associated with this task. Stub: returns empty vec for now.
    async fn tags(&self) -> Vec<TagGql> {
        // Will be resolved via data loader or batch query
        vec![]
    }

    /// Tag IDs associated with this task.
    async fn tag_ids(&self) -> Vec<ID> {
        self.0.tags.iter().map(|t| ID(t.to_string())).collect()
    }
}

/// Input for creating a new task.
#[derive(InputObject, Debug)]
pub struct CreateTaskInput {
    pub title: String,
    pub description: Option<String>,
    pub project_id: Option<ID>,
    pub deadline: Option<NaiveDate>,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub estimated_hours: Option<f64>,
    pub impact: Option<ImpactLevelGql>,
    pub urgency: Option<UrgencyLevelGql>,
    pub tag_ids: Option<Vec<ID>>,
}

/// Input for updating an existing task.
#[derive(InputObject, Debug)]
pub struct UpdateTaskInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub project_id: Option<ID>,
    pub deadline: Option<NaiveDate>,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub estimated_hours: Option<f64>,
    pub status: Option<TaskStatusGql>,
    pub impact: Option<ImpactLevelGql>,
    pub urgency: Option<UrgencyLevelGql>,
    pub tag_ids: Option<Vec<ID>>,
}

/// Filter input for querying tasks.
#[derive(InputObject, Debug, Default)]
pub struct TaskFilterInput {
    pub status: Option<Vec<TaskStatusGql>>,
    pub source: Option<Vec<SourceGql>>,
    pub project_id: Option<ID>,
    pub assignee: Option<String>,
    pub deadline_before: Option<NaiveDate>,
    pub deadline_after: Option<NaiveDate>,
    pub tag_ids: Option<Vec<ID>>,
}

/// Relay-style edge for task pagination.
pub struct TaskEdge {
    pub node: TaskGql,
    pub cursor: String,
}

#[Object]
impl TaskEdge {
    async fn node(&self) -> &TaskGql {
        &self.node
    }

    async fn cursor(&self) -> &str {
        &self.cursor
    }
}

/// Relay-style connection for paginated task results.
#[derive(SimpleObject)]
pub struct TaskConnection {
    pub edges: Vec<TaskEdge>,
    pub page_info: PageInfo,
    pub total_count: i32,
}
