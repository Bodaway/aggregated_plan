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
    pub time_estimate_seconds: Option<i32>,
    pub time_spent_seconds: Option<i32>,
    pub time_original_estimate_seconds: Option<i32>,
}

/// Client trait for fetching tasks from Jira.
#[async_trait]
pub trait JiraClient: Send + Sync {
    /// Fetch tasks from Jira for the given project keys.
    ///
    /// When `my_tasks_only` is true the JQL is restricted to issues where the
    /// authenticated user is the current assignee or a watcher, using Jira's
    /// `currentUser()` function.  When false and `assignees` is non-empty the
    /// explicit assignee list is used instead; otherwise all project issues are
    /// returned.
    async fn fetch_tasks(
        &self,
        project_keys: &[String],
        assignees: Option<&[String]>,
        my_tasks_only: bool,
    ) -> Result<Vec<JiraTask>, ConnectorError>;
}
