use async_trait::async_trait;
use reqwest::Client;

use application::errors::ConnectorError;
use application::services::jira_client::{JiraClient, JiraTask};

pub struct HttpJiraClient {
    http: Client,
    base_url: String,
    auth_token: String,
}

impl HttpJiraClient {
    pub fn new(base_url: String, auth_token: String) -> Self {
        Self {
            http: Client::new(),
            base_url,
            auth_token,
        }
    }
}

#[async_trait]
impl JiraClient for HttpJiraClient {
    async fn fetch_tasks(
        &self,
        _project_keys: &[String],
        _assignees: Option<&[String]>,
    ) -> Result<Vec<JiraTask>, ConnectorError> {
        // Implemented in Task 26
        todo!("Jira connector not yet implemented")
    }
}
