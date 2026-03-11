use serde::{Deserialize, Serialize};

/// Jira REST API response types.

/// Response from POST /rest/api/3/search/jql (cursor-based pagination).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraSearchResponse {
    pub issues: Vec<JiraIssue>,
    /// Token for the next page; absent when there are no more results.
    pub next_page_token: Option<String>,
}

/// Request body for POST /rest/api/3/search/jql.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraSearchRequest<'a> {
    pub jql: &'a str,
    pub fields: &'a [&'a str],
    pub max_results: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
pub struct JiraIssue {
    pub key: String,
    pub fields: JiraIssueFields,
}

#[derive(Debug, Deserialize)]
pub struct JiraIssueFields {
    pub summary: String,
    /// Jira API v3 returns description as an ADF JSON object; we skip it to avoid parse errors.
    #[serde(default, skip)]
    pub description: Option<String>,
    pub status: JiraStatus,
    pub assignee: Option<JiraUser>,
    pub priority: Option<JiraPriority>,
    pub duedate: Option<String>,
    pub project: JiraProject,
    pub timeestimate: Option<i32>,
    pub timespent: Option<i32>,
    pub timeoriginalestimate: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct JiraStatus {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraUser {
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct JiraPriority {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct JiraProject {
    pub key: String,
    pub name: String,
}
