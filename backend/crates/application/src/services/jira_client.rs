use async_trait::async_trait;
use chrono::NaiveDate;

use crate::errors::ConnectorError;

/// Represents a task fetched from Jira.
pub struct JiraTask {
    /// Jira issue key, e.g. "PROJ-123".
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    /// Raw Jira status name.
    pub status: String,
    pub assignee: Option<String>,
    pub deadline: Option<NaiveDate>,
    pub priority: Option<String>,
    pub project_key: String,
    pub project_name: String,
}

/// Client trait for fetching tasks from Jira.
#[async_trait]
pub trait JiraClient: Send + Sync {
    /// Fetch tasks from Jira for the given project keys and optional assignee filter.
    async fn fetch_tasks(
        &self,
        project_keys: &[String],
        assignees: Option<&[String]>,
    ) -> Result<Vec<JiraTask>, ConnectorError>;
}
