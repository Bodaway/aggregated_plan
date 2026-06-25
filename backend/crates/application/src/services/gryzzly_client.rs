use async_trait::async_trait;

use crate::errors::ConnectorError;

/// A Gryzzly project (read-only catalog). The project is contextual info only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GryzzlyProject {
    pub id: String,
    pub name: String,
    pub customer_name: Option<String>,
    pub is_active: bool,
}

/// A Gryzzly task — a category of billable work within a project (NOT an aplan task).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GryzzlyTask {
    pub id: String,
    pub name: String,
    pub project_id: String,
    pub is_active: bool,
}

/// Read-only client for the Gryzzly v1 REST API. Named generically (not `…ReadClient`)
/// so a future `push_declaration(...)` write method can be added without renaming.
#[async_trait]
pub trait GryzzlyClient: Send + Sync {
    /// List projects. When `active_only`, archived/closed projects are excluded.
    async fn fetch_projects(&self, active_only: bool) -> Result<Vec<GryzzlyProject>, ConnectorError>;

    /// List tasks belonging to the given project ids.
    async fn fetch_tasks(&self, project_ids: &[String]) -> Result<Vec<GryzzlyTask>, ConnectorError>;
}
