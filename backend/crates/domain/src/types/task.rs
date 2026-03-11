use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub user_id: UserId,
    pub title: String,
    pub description: Option<String>,
    pub source: Source,
    pub source_id: Option<String>,
    pub jira_status: Option<String>,
    pub status: TaskStatus,
    pub project_id: Option<ProjectId>,
    pub assignee: Option<String>,
    pub deadline: Option<NaiveDate>,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub estimated_hours: Option<f32>,
    pub urgency: UrgencyLevel,
    pub urgency_manual: bool,
    pub impact: ImpactLevel,
    pub tags: Vec<TagId>,
    pub tracking_state: TrackingState,
    pub jira_remaining_seconds: Option<i32>,
    pub jira_original_estimate_seconds: Option<i32>,
    pub jira_time_spent_seconds: Option<i32>,
    pub remaining_hours_override: Option<f32>,
    pub estimated_hours_override: Option<f32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Task {
    /// Effective remaining hours: local override > Jira remaining > None
    pub fn effective_remaining_hours(&self) -> Option<f32> {
        self.remaining_hours_override
            .or(self.jira_remaining_seconds.map(|s| s as f32 / 3600.0))
    }

    /// Effective estimated hours: local override > Jira estimate > estimated_hours (personal tasks)
    pub fn effective_estimated_hours(&self) -> Option<f32> {
        self.estimated_hours_override
            .or(self.jira_original_estimate_seconds.map(|s| s as f32 / 3600.0))
            .or(self.estimated_hours)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_test_task() -> Task {
        Task {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            title: "Test".to_string(),
            description: None,
            source: Source::Jira,
            source_id: Some("PROJ-1".to_string()),
            jira_status: Some("In Progress".to_string()),
            status: TaskStatus::InProgress,
            project_id: None,
            assignee: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Medium,
            urgency_manual: false,
            impact: ImpactLevel::Medium,
            tags: vec![],
            tracking_state: TrackingState::Followed,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn effective_remaining_hours_override_takes_precedence() {
        let mut task = make_test_task();
        task.jira_remaining_seconds = Some(7200);
        task.remaining_hours_override = Some(5.0);
        assert_eq!(task.effective_remaining_hours(), Some(5.0));
    }

    #[test]
    fn effective_remaining_hours_falls_back_to_jira() {
        let mut task = make_test_task();
        task.jira_remaining_seconds = Some(3600);
        assert_eq!(task.effective_remaining_hours(), Some(1.0));
    }

    #[test]
    fn effective_remaining_hours_none_when_no_data() {
        let task = make_test_task();
        assert_eq!(task.effective_remaining_hours(), None);
    }

    #[test]
    fn effective_estimated_hours_override_takes_precedence() {
        let mut task = make_test_task();
        task.jira_original_estimate_seconds = Some(14400);
        task.estimated_hours_override = Some(8.0);
        assert_eq!(task.effective_estimated_hours(), Some(8.0));
    }

    #[test]
    fn effective_estimated_hours_falls_back_to_jira() {
        let mut task = make_test_task();
        task.jira_original_estimate_seconds = Some(14400);
        assert_eq!(task.effective_estimated_hours(), Some(4.0));
    }

    #[test]
    fn effective_estimated_hours_falls_back_to_estimated_hours() {
        let mut task = make_test_task();
        task.estimated_hours = Some(3.5);
        assert_eq!(task.effective_estimated_hours(), Some(3.5));
    }

    #[test]
    fn effective_estimated_hours_none_when_no_data() {
        let task = make_test_task();
        assert_eq!(task.effective_estimated_hours(), None);
    }
}
