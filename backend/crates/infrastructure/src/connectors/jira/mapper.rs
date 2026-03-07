use application::services::jira_client::JiraTask;

use super::types::JiraIssue;

/// Map a Jira API issue to the application-layer JiraTask DTO.
pub fn map_jira_issue(issue: JiraIssue) -> JiraTask {
    JiraTask {
        key: issue.key,
        title: issue.fields.summary,
        description: issue.fields.description,
        status: issue.fields.status.name,
        assignee: issue.fields.assignee.map(|a| a.display_name),
        deadline: issue.fields.duedate.and_then(|d| d.parse().ok()),
        priority: issue.fields.priority.map(|p| p.name),
        project_key: issue.fields.project.key,
        project_name: issue.fields.project.name,
    }
}
