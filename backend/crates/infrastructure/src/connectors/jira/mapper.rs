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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::*;

    fn make_issue(
        key: &str,
        summary: &str,
        status: &str,
        assignee: Option<&str>,
        duedate: Option<&str>,
        priority: Option<&str>,
    ) -> JiraIssue {
        JiraIssue {
            key: key.to_string(),
            fields: JiraIssueFields {
                summary: summary.to_string(),
                description: Some("A description".to_string()),
                status: JiraStatus {
                    name: status.to_string(),
                },
                assignee: assignee.map(|n| JiraUser {
                    display_name: n.to_string(),
                }),
                priority: priority.map(|p| JiraPriority {
                    name: p.to_string(),
                }),
                duedate: duedate.map(|d| d.to_string()),
                project: JiraProject {
                    key: "PROJ".to_string(),
                    name: "My Project".to_string(),
                },
            },
        }
    }

    #[test]
    fn maps_all_fields_correctly() {
        let issue = make_issue("PROJ-42", "Fix bug", "In Progress", Some("Alice"), Some("2026-04-01"), Some("High"));
        let task = map_jira_issue(issue);

        assert_eq!(task.key, "PROJ-42");
        assert_eq!(task.title, "Fix bug");
        assert_eq!(task.description, Some("A description".to_string()));
        assert_eq!(task.status, "In Progress");
        assert_eq!(task.assignee, Some("Alice".to_string()));
        assert_eq!(
            task.deadline,
            Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 1).unwrap())
        );
        assert_eq!(task.priority, Some("High".to_string()));
        assert_eq!(task.project_key, "PROJ");
        assert_eq!(task.project_name, "My Project");
    }

    #[test]
    fn handles_none_assignee_and_priority() {
        let issue = make_issue("PROJ-1", "Simple task", "To Do", None, None, None);
        let task = map_jira_issue(issue);

        assert!(task.assignee.is_none());
        assert!(task.deadline.is_none());
        assert!(task.priority.is_none());
    }

    #[test]
    fn handles_invalid_date_gracefully() {
        let issue = make_issue("PROJ-2", "Bad date", "Open", None, Some("not-a-date"), None);
        let task = map_jira_issue(issue);

        assert!(task.deadline.is_none());
    }
}
