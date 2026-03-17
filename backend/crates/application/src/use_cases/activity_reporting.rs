use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::*;

/// Weekly activity summary with daily totals and per-task breakdown.
#[derive(Debug)]
pub struct WeeklyActivitySummary {
    pub week_start: NaiveDate,
    pub week_end: NaiveDate,
    pub total_hours: f64,
    pub daily_totals: Vec<DailyActivityTotal>,
    pub task_breakdown: Vec<TaskActivitySummary>,
}

/// Total tracked hours for a single day.
#[derive(Debug)]
pub struct DailyActivityTotal {
    pub date: NaiveDate,
    pub total_hours: f64,
}

/// Total tracked hours for a single task across the week.
#[derive(Debug)]
pub struct TaskActivitySummary {
    pub task_id: Option<TaskId>,
    pub task_title: Option<String>,
    pub source_id: Option<String>,
    pub total_hours: f64,
    pub daily_hours: Vec<f64>, // indexed 0=Mon through 6=Sun
}

/// Get a weekly activity summary for the given week (week_start must be a Monday).
pub async fn get_weekly_activity_summary(
    activity_repo: &dyn ActivitySlotRepository,
    task_repo: &dyn TaskRepository,
    user_id: UserId,
    week_start: NaiveDate,
) -> Result<WeeklyActivitySummary, AppError> {
    // Validate week_start is a Monday
    if week_start.weekday() != chrono::Weekday::Mon {
        return Err(AppError::Configuration("week_start must be a Monday".into()));
    }

    let week_end = week_start + Duration::days(6);

    let slots = activity_repo
        .find_by_user_and_date_range(user_id, week_start, week_end)
        .await
        .map_err(AppError::Repository)?;

    // Group slots by task_id
    let mut task_groups: HashMap<Option<TaskId>, Vec<&ActivitySlot>> = HashMap::new();
    for slot in &slots {
        task_groups.entry(slot.task_id).or_default().push(slot);
    }

    // Compute per-task breakdown
    let mut task_breakdown: Vec<TaskActivitySummary> = Vec::new();
    for (task_id, group_slots) in &task_groups {
        let mut daily_hours = vec![0.0_f64; 7];
        let mut total_hours = 0.0;

        for slot in group_slots {
            if let Some(end) = slot.end_time {
                let hours = (end - slot.start_time).num_minutes() as f64 / 60.0;
                let day_index = (slot.date - week_start).num_days() as usize;
                if day_index < 7 {
                    daily_hours[day_index] += hours;
                    total_hours += hours;
                }
            }
        }

        // Look up task details
        let (title, source_id) = if let Some(tid) = task_id {
            match task_repo.find_by_id(*tid).await {
                Ok(Some(task)) => (Some(task.title), task.source_id),
                _ => (None, None),
            }
        } else {
            (None, None)
        };

        task_breakdown.push(TaskActivitySummary {
            task_id: *task_id,
            task_title: title,
            source_id,
            total_hours,
            daily_hours,
        });
    }

    // Sort by total_hours descending
    task_breakdown.sort_by(|a, b| b.total_hours.partial_cmp(&a.total_hours).unwrap_or(std::cmp::Ordering::Equal));

    // Compute daily totals (always 7 entries)
    let mut daily_totals: Vec<DailyActivityTotal> = (0..7)
        .map(|i| DailyActivityTotal {
            date: week_start + Duration::days(i),
            total_hours: 0.0,
        })
        .collect();

    for slot in &slots {
        if let Some(end) = slot.end_time {
            let hours = (end - slot.start_time).num_minutes() as f64 / 60.0;
            let day_index = (slot.date - week_start).num_days() as usize;
            if day_index < 7 {
                daily_totals[day_index].total_hours += hours;
            }
        }
    }

    let total_hours = daily_totals.iter().map(|d| d.total_hours).sum();

    Ok(WeeklyActivitySummary {
        week_start,
        week_end,
        total_hours,
        daily_totals,
        task_breakdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use uuid::Uuid;
    use crate::errors::RepositoryError;

    fn user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn make_slot(task_id: Option<TaskId>, date: &str, start_hour: u32, end_hour: u32) -> ActivitySlot {
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
        let start = DateTime::parse_from_rfc3339(&format!("{date}T{start_hour:02}:00:00+00:00"))
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339(&format!("{date}T{end_hour:02}:00:00+00:00"))
            .unwrap()
            .with_timezone(&Utc);

        ActivitySlot {
            id: Uuid::new_v4(),
            user_id: user_id(),
            task_id,
            start_time: start,
            end_time: Some(end),
            half_day: HalfDay::Morning,
            date,
            created_at: Utc::now(),
        }
    }

    fn make_task(id: TaskId, title: &str, source_id: Option<&str>) -> Task {
        Task {
            id,
            user_id: user_id(),
            title: title.to_string(),
            description: None,
            source: Source::Personal,
            source_id: source_id.map(|s| s.to_string()),
            status: TaskStatus::InProgress,
            jira_status: None,
            urgency: UrgencyLevel::Medium,
            urgency_manual: false,
            impact: ImpactLevel::Medium,
            tags: vec![],
            tracking_state: TrackingState::Followed,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            project_id: None,
            assignee: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    struct MockActivityRepo {
        slots: Vec<ActivitySlot>,
    }

    #[async_trait]
    impl ActivitySlotRepository for MockActivityRepo {
        async fn find_by_id(&self, _id: ActivitySlotId) -> Result<Option<ActivitySlot>, RepositoryError> {
            Ok(None)
        }
        async fn find_by_user_and_date(&self, _user_id: UserId, _date: NaiveDate) -> Result<Vec<ActivitySlot>, RepositoryError> {
            Ok(vec![])
        }
        async fn find_active(&self, _user_id: UserId) -> Result<Option<ActivitySlot>, RepositoryError> {
            Ok(None)
        }
        async fn find_by_user_and_date_range(&self, _user_id: UserId, start: NaiveDate, end: NaiveDate) -> Result<Vec<ActivitySlot>, RepositoryError> {
            Ok(self.slots.iter().filter(|s| s.date >= start && s.date <= end && s.end_time.is_some()).cloned().collect())
        }
        async fn save(&self, _slot: &ActivitySlot) -> Result<(), RepositoryError> { Ok(()) }
        async fn update(&self, _slot: &ActivitySlot) -> Result<(), RepositoryError> { Ok(()) }
        async fn delete(&self, _id: ActivitySlotId) -> Result<(), RepositoryError> { Ok(()) }
    }

    struct MockTaskRepo {
        tasks: Vec<Task>,
    }

    #[async_trait]
    impl TaskRepository for MockTaskRepo {
        async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError> {
            Ok(self.tasks.iter().find(|t| t.id == id).cloned())
        }
        async fn find_by_user(&self, _user_id: UserId, _filter: &crate::repositories::TaskFilter) -> Result<Vec<Task>, RepositoryError> { Ok(vec![]) }
        async fn find_by_source(&self, _user_id: UserId, _source: Source, _source_id: &str) -> Result<Option<Task>, RepositoryError> { Ok(None) }
        async fn find_by_date_range(&self, _user_id: UserId, _start: NaiveDate, _end: NaiveDate) -> Result<Vec<Task>, RepositoryError> { Ok(vec![]) }
        async fn save(&self, _task: &Task) -> Result<(), RepositoryError> { Ok(()) }
        async fn save_batch(&self, _tasks: &[Task]) -> Result<(), RepositoryError> { Ok(()) }
        async fn delete(&self, _id: TaskId) -> Result<(), RepositoryError> { Ok(()) }
        async fn delete_stale_by_source(&self, _user_id: UserId, _source: Source, _keep_ids: &[String]) -> Result<u64, RepositoryError> { Ok(0) }
    }

    #[tokio::test]
    async fn test_empty_week() {
        let activity_repo = MockActivityRepo { slots: vec![] };
        let task_repo = MockTaskRepo { tasks: vec![] };
        let week_start = NaiveDate::from_ymd_opt(2026, 3, 16).unwrap(); // Monday

        let summary = get_weekly_activity_summary(&activity_repo, &task_repo, user_id(), week_start)
            .await
            .unwrap();

        assert_eq!(summary.total_hours, 0.0);
        assert_eq!(summary.daily_totals.len(), 7);
        assert!(summary.task_breakdown.is_empty());
        assert_eq!(summary.daily_totals[0].date, week_start);
    }

    #[tokio::test]
    async fn test_weekly_summary_with_tasks() {
        let task_id = Uuid::new_v4();
        let slots = vec![
            make_slot(Some(task_id), "2026-03-16", 9, 11),  // Monday 2h
            make_slot(Some(task_id), "2026-03-17", 9, 12),  // Tuesday 3h
            make_slot(None, "2026-03-16", 13, 14),           // Monday 1h unassigned
        ];
        let tasks = vec![make_task(task_id, "Fix auth", Some("PROJ-123"))];

        let activity_repo = MockActivityRepo { slots };
        let task_repo = MockTaskRepo { tasks };
        let week_start = NaiveDate::from_ymd_opt(2026, 3, 16).unwrap();

        let summary = get_weekly_activity_summary(&activity_repo, &task_repo, user_id(), week_start)
            .await
            .unwrap();

        assert_eq!(summary.total_hours, 6.0);
        assert_eq!(summary.daily_totals[0].total_hours, 3.0); // Monday: 2h + 1h
        assert_eq!(summary.daily_totals[1].total_hours, 3.0); // Tuesday: 3h
        assert_eq!(summary.daily_totals[2].total_hours, 0.0); // Wednesday: 0h

        assert_eq!(summary.task_breakdown.len(), 2);
        // First task (highest hours) should be the named task
        assert_eq!(summary.task_breakdown[0].task_title.as_deref(), Some("Fix auth"));
        assert_eq!(summary.task_breakdown[0].source_id.as_deref(), Some("PROJ-123"));
        assert_eq!(summary.task_breakdown[0].total_hours, 5.0);
        assert_eq!(summary.task_breakdown[0].daily_hours[0], 2.0); // Monday
        assert_eq!(summary.task_breakdown[0].daily_hours[1], 3.0); // Tuesday
    }

    #[tokio::test]
    async fn test_rejects_non_monday() {
        let activity_repo = MockActivityRepo { slots: vec![] };
        let task_repo = MockTaskRepo { tasks: vec![] };
        let tuesday = NaiveDate::from_ymd_opt(2026, 3, 17).unwrap();

        let result = get_weekly_activity_summary(&activity_repo, &task_repo, user_id(), tuesday).await;
        assert!(result.is_err());
    }
}
