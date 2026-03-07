use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use reqwest::Client;

use application::errors::ConnectorError;
use application::services::jira_client::{JiraClient, JiraTask};

use super::mapper::map_jira_issue;
use super::types::JiraSearchResponse;

/// Maximum number of issues Jira returns per page.
const JIRA_PAGE_SIZE: u32 = 50;

pub struct HttpJiraClient {
    http: Client,
    base_url: String,
    /// Base64-encoded `email:api_token` for Jira Basic auth.
    auth_token: String,
}

impl HttpJiraClient {
    pub fn new(base_url: String, email: String, api_token: String) -> Self {
        let auth_token = BASE64.encode(format!("{}:{}", email, api_token));
        Self {
            http: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_token,
        }
    }

    /// Build the JQL query string for the given project keys and optional assignees.
    fn build_jql(project_keys: &[String], assignees: Option<&[String]>) -> String {
        let keys_csv = project_keys
            .iter()
            .map(|k| format!("\"{}\"", k))
            .collect::<Vec<_>>()
            .join(", ");

        let project_clause = format!("project IN ({})", keys_csv);

        let assignee_clause = match assignees {
            Some(names) if !names.is_empty() => {
                let names_csv = names
                    .iter()
                    .map(|n| format!("\"{}\"", n))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    " AND (assignee IN ({}) OR assignee was currentUser())",
                    names_csv
                )
            }
            _ => String::new(),
        };

        format!("{}{}", project_clause, assignee_clause)
    }

    /// Fetch a single page of issues from Jira.
    async fn fetch_page(&self, jql: &str, start_at: u32) -> Result<JiraSearchResponse, ConnectorError> {
        let url = format!("{}/rest/api/3/search", self.base_url);

        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Basic {}", self.auth_token))
            .header("Accept", "application/json")
            .query(&[
                ("jql", jql),
                (
                    "fields",
                    "summary,description,status,assignee,priority,duedate,project",
                ),
                ("startAt", &start_at.to_string()),
                ("maxResults", &JIRA_PAGE_SIZE.to_string()),
            ])
            .send()
            .await
            .map_err(|e| ConnectorError::NetworkError(e.to_string()))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ConnectorError::AuthFailed {
                service: "Jira".to_string(),
            });
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ConnectorError::Http {
                status: status.as_u16(),
                message: body,
            });
        }

        response
            .json::<JiraSearchResponse>()
            .await
            .map_err(|e| ConnectorError::ParseError(e.to_string()))
    }
}

#[async_trait]
impl JiraClient for HttpJiraClient {
    async fn fetch_tasks(
        &self,
        project_keys: &[String],
        assignees: Option<&[String]>,
    ) -> Result<Vec<JiraTask>, ConnectorError> {
        if project_keys.is_empty() {
            return Ok(Vec::new());
        }

        let jql = Self::build_jql(project_keys, assignees);
        let mut all_tasks: Vec<JiraTask> = Vec::new();
        let mut start_at: u32 = 0;

        loop {
            let page = self.fetch_page(&jql, start_at).await?;
            let fetched_count = page.issues.len() as u32;

            let tasks: Vec<JiraTask> = page.issues.into_iter().map(map_jira_issue).collect();
            all_tasks.extend(tasks);

            start_at += fetched_count;
            if fetched_count < JIRA_PAGE_SIZE || start_at >= page.total {
                break;
            }
        }

        Ok(all_tasks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_jql_single_project_no_assignees() {
        let jql = HttpJiraClient::build_jql(&["PROJ".to_string()], None);
        assert_eq!(jql, "project IN (\"PROJ\")");
    }

    #[test]
    fn build_jql_multiple_projects_no_assignees() {
        let jql = HttpJiraClient::build_jql(
            &["PROJ".to_string(), "OTHER".to_string()],
            None,
        );
        assert_eq!(jql, "project IN (\"PROJ\", \"OTHER\")");
    }

    #[test]
    fn build_jql_with_assignees() {
        let jql = HttpJiraClient::build_jql(
            &["PROJ".to_string()],
            Some(&["alice".to_string(), "bob".to_string()]),
        );
        assert_eq!(
            jql,
            "project IN (\"PROJ\") AND (assignee IN (\"alice\", \"bob\") OR assignee was currentUser())"
        );
    }

    #[test]
    fn build_jql_with_empty_assignees_treated_as_none() {
        let jql = HttpJiraClient::build_jql(&["PROJ".to_string()], Some(&[]));
        assert_eq!(jql, "project IN (\"PROJ\")");
    }

    #[test]
    fn auth_token_is_base64_encoded() {
        let client = HttpJiraClient::new(
            "https://test.atlassian.net".to_string(),
            "user@example.com".to_string(),
            "my-token".to_string(),
        );
        let expected = BASE64.encode("user@example.com:my-token");
        assert_eq!(client.auth_token, expected);
    }

    #[test]
    fn base_url_trailing_slash_stripped() {
        let client = HttpJiraClient::new(
            "https://test.atlassian.net/".to_string(),
            "user@example.com".to_string(),
            "token".to_string(),
        );
        assert_eq!(client.base_url, "https://test.atlassian.net");
    }
}
