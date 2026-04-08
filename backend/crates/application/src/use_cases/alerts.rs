use chrono::{NaiveDate, Utc};
use domain::rules::alerts::{check_conflict_alerts, check_deadline_alerts, check_overload_alerts, AlertData, ScheduledItem};
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::*;

/// Default deadline alert threshold in business days.
const DEADLINE_THRESHOLD_DAYS: i64 = 3;

/// Default weekly capacity in hours (10 half-days x 4 hours).
const DEFAULT_WEEKLY_CAPACITY: f64 = 40.0;

/// Recompute and persist all alerts for a user based on current data.
pub async fn generate_alerts(
    task_repo: &dyn TaskRepository,
    meeting_repo: &dyn MeetingRepository,
    alert_repo: &dyn AlertRepository,
    user_id: UserId,
    date: NaiveDate,
) -> Result<Vec<Alert>, AppError> {
    // 1. Get active tasks with deadlines
    let filter = TaskFilter {
        status: Some(vec![TaskStatus::Todo, TaskStatus::InProgress]),
        ..TaskFilter::empty()
    };
    let tasks = task_repo.find_by_user(user_id, &filter).await?;

    // 2. Get meetings for the date
    let meetings = meeting_repo.find_by_user_and_date(user_id, date).await?;

    // 3. Check deadline alerts
    let deadline_alerts = check_deadline_alerts(&tasks, date, DEADLINE_THRESHOLD_DAYS);

    // 4. Check conflict alerts by building scheduled items
    let scheduled_items: Vec<ScheduledItem> = tasks
        .iter()
        .filter_map(|t| {
            match (t.planned_start, t.planned_end) {
                (Some(start), Some(end)) if start.date_naive() == date => {
                    Some(ScheduledItem::Task {
                        id: t.id,
                        title: t.title.clone(),
                        start,
                        end,
                    })
                }
                _ => None,
            }
        })
        .chain(meetings.iter().map(|m| ScheduledItem::Meeting {
            id: m.id,
            title: m.title.clone(),
            start: m.start_time,
            end: m.end_time,
        }))
        .collect();

    let conflict_alerts = check_conflict_alerts(&scheduled_items, date);

    // 5. Check overload alerts (simplified: count meeting hours + task estimated hours for the week)
    let total_meeting_hours: f64 = meetings
        .iter()
        .map(|m| domain::rules::workload::meeting_hours(m.start_time, m.end_time))
        .sum();
    let total_task_hours: f64 = tasks
        .iter()
        .filter_map(|t| t.estimated_hours.map(|h| h as f64))
        .sum();
    let overload_alert = check_overload_alerts(
        total_task_hours,
        total_meeting_hours,
        DEFAULT_WEEKLY_CAPACITY,
        date,
    );

    // 6. Convert AlertData to Alert entities and save
    let mut all_alert_data: Vec<AlertData> = Vec::new();
    all_alert_data.extend(deadline_alerts);
    all_alert_data.extend(conflict_alerts);
    if let Some(overload) = overload_alert {
        all_alert_data.push(overload);
    }

    let new_alerts: Vec<Alert> = all_alert_data
        .into_iter()
        .map(|data| Alert {
            id: Uuid::new_v4(),
            user_id,
            alert_type: data.alert_type,
            severity: data.severity,
            message: data.message,
            related_items: data.related_items,
            date: data.date,
            resolved: false,
            created_at: Utc::now(),
        })
        .collect();

    if !new_alerts.is_empty() {
        alert_repo.save_batch(&new_alerts).await?;
    }

    // 7. Return all unresolved alerts
    let all_alerts = alert_repo.find_by_user(user_id, Some(false)).await?;
    Ok(all_alerts)
}

/// Mark an alert as resolved.
pub async fn resolve_alert(
    alert_repo: &dyn AlertRepository,
    alert_id: AlertId,
) -> Result<Alert, AppError> {
    let mut alert = alert_repo
        .find_by_id(alert_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Alert {}", alert_id)))?;

    alert.resolved = true;
    alert_repo.update(&alert).await?;
    Ok(alert)
}

/// Get all alerts for a user, optionally filtered by resolved status.
pub async fn get_alerts(
    alert_repo: &dyn AlertRepository,
    user_id: UserId,
    resolved: Option<bool>,
) -> Result<Vec<Alert>, AppError> {
    alert_repo
        .find_by_user(user_id, resolved)
        .await
        .map_err(Into::into)
}

/// Clean up all resolved alerts for a user. Returns the number of deleted alerts.
pub async fn cleanup_resolved_alerts(
    alert_repo: &dyn AlertRepository,
    user_id: UserId,
) -> Result<u64, AppError> {
    alert_repo
        .delete_resolved(user_id)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{Duration, Utc};
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::errors::RepositoryError;

    // ---- In-memory repos ----

    struct InMemoryAlertRepository {
        alerts: Mutex<HashMap<AlertId, Alert>>,
    }

    impl InMemoryAlertRepository {
        fn new() -> Self {
            Self {
                alerts: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl AlertRepository for InMemoryAlertRepository {
        async fn find_by_id(&self, id: AlertId) -> Result<Option<Alert>, RepositoryError> {
            let alerts = self.alerts.lock().unwrap();
            Ok(alerts.get(&id).cloned())
        }

        async fn find_unresolved(&self, user_id: UserId) -> Result<Vec<Alert>, RepositoryError> {
            let alerts = self.alerts.lock().unwrap();
            Ok(alerts
                .values()
                .filter(|a| a.user_id == user_id && !a.resolved)
                .cloned()
                .collect())
        }

        async fn find_by_user(
            &self,
            user_id: UserId,
            resolved: Option<bool>,
        ) -> Result<Vec<Alert>, RepositoryError> {
            let alerts = self.alerts.lock().unwrap();
            Ok(alerts
                .values()
                .filter(|a| a.user_id == user_id)
                .filter(|a| match resolved {
                    Some(r) => a.resolved == r,
                    None => true,
                })
                .cloned()
                .collect())
        }

        async fn save(&self, alert: &Alert) -> Result<(), RepositoryError> {
            let mut alerts = self.alerts.lock().unwrap();
            alerts.insert(alert.id, alert.clone());
            Ok(())
        }

        async fn save_batch(&self, batch: &[Alert]) -> Result<(), RepositoryError> {
            let mut alerts = self.alerts.lock().unwrap();
            for alert in batch {
                alerts.insert(alert.id, alert.clone());
            }
            Ok(())
        }

        async fn update(&self, alert: &Alert) -> Result<(), RepositoryError> {
            let mut alerts = self.alerts.lock().unwrap();
            alerts.insert(alert.id, alert.clone());
            Ok(())
        }

        async fn delete_resolved(&self, user_id: UserId) -> Result<u64, RepositoryError> {
            let mut alerts = self.alerts.lock().unwrap();
            let before = alerts.len();
            alerts.retain(|_, a| !(a.user_id == user_id && a.resolved));
            Ok((before - alerts.len()) as u64)
        }
    }

    struct InMemoryTaskRepository {
        tasks: Mutex<HashMap<TaskId, Task>>,
    }

    impl InMemoryTaskRepository {
        fn new() -> Self {
            Self {
                tasks: Mutex::new(HashMap::new()),
            }
        }

        fn insert(&self, task: Task) {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.insert(task.id, task);
        }
    }

    #[async_trait]
    impl TaskRepository for InMemoryTaskRepository {
        async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError> {
            let tasks = self.tasks.lock().unwrap();
            Ok(tasks.get(&id).cloned())
        }

        async fn find_by_user(
            &self,
            user_id: UserId,
            filter: &TaskFilter,
        ) -> Result<Vec<Task>, RepositoryError> {
            let tasks = self.tasks.lock().unwrap();
            Ok(tasks
                .values()
                .filter(|t| t.user_id == user_id)
                .filter(|t| {
                    if let Some(ref statuses) = filter.status {
                        statuses.contains(&t.status)
                    } else {
                        true
                    }
                })
                .cloned()
                .collect())
        }

        async fn find_by_source(
            &self,
            _user_id: UserId,
            _source: Source,
            _source_id: &str,
        ) -> Result<Option<Task>, RepositoryError> {
            Ok(None)
        }

        async fn find_by_date_range(
            &self,
            _user_id: UserId,
            _start: NaiveDate,
            _end: NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(vec![])
        }

        async fn save(&self, task: &Task) -> Result<(), RepositoryError> {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.insert(task.id, task.clone());
            Ok(())
        }

        async fn save_batch(&self, tasks: &[Task]) -> Result<(), RepositoryError> {
            let mut store = self.tasks.lock().unwrap();
            for task in tasks {
                store.insert(task.id, task.clone());
            }
            Ok(())
        }

        async fn delete(&self, id: TaskId) -> Result<(), RepositoryError> {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.remove(&id);
            Ok(())
        }

        async fn delete_stale_by_source(&self, _user_id: UserId, _source: Source, _keep_ids: &[String]) -> Result<u64, RepositoryError> {
            Ok(0)
        }
    }

    struct InMemoryMeetingRepository {
        meetings: Mutex<Vec<Meeting>>,
    }

    impl InMemoryMeetingRepository {
        fn new() -> Self {
            Self {
                meetings: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl MeetingRepository for InMemoryMeetingRepository {
        async fn find_by_id(&self, _id: MeetingId) -> Result<Option<Meeting>, RepositoryError> {
            Ok(None)
        }
        async fn update(&self, _meeting: &Meeting) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn find_by_user_and_date(
            &self,
            user_id: UserId,
            date: NaiveDate,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            let meetings = self.meetings.lock().unwrap();
            Ok(meetings
                .iter()
                .filter(|m| m.user_id == user_id && m.start_time.date_naive() == date)
                .cloned()
                .collect())
        }
        async fn find_by_user_and_range(
            &self,
            _user_id: UserId,
            _start: NaiveDate,
            _end: NaiveDate,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            Ok(vec![])
        }
        async fn find_by_project(
            &self,
            _user_id: UserId,
            _project_id: ProjectId,
        ) -> Result<Vec<Meeting>, RepositoryError> {
            Ok(vec![])
        }
        async fn upsert_batch(&self, _meetings: &[Meeting]) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn delete_stale(
            &self,
            _user_id: UserId,
            _current_ids: &[String],
        ) -> Result<u64, RepositoryError> {
            Ok(0)
        }
    }

    fn test_user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn make_task_with_deadline(title: &str, deadline: NaiveDate) -> Task {
        Task {
            id: Uuid::new_v4(),
            user_id: test_user_id(),
            title: title.to_string(),
            description: None,
            notes: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::InProgress,
            project_id: None,
            assignee: None,
            deadline: Some(deadline),
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Medium,
            urgency_manual: false,
            impact: ImpactLevel::Medium,
            tags: vec![],
            tracking_state: TrackingState::Inbox,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn generate_alerts_with_no_tasks_returns_empty() {
        let task_repo = InMemoryTaskRepository::new();
        let meeting_repo = InMemoryMeetingRepository::new();
        let alert_repo = InMemoryAlertRepository::new();
        let date = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();

        let alerts = generate_alerts(
            &task_repo,
            &meeting_repo,
            &alert_repo,
            test_user_id(),
            date,
        )
        .await
        .unwrap();

        assert!(alerts.is_empty());
    }

    #[tokio::test]
    async fn generate_alerts_detects_deadline() {
        let task_repo = InMemoryTaskRepository::new();
        let meeting_repo = InMemoryMeetingRepository::new();
        let alert_repo = InMemoryAlertRepository::new();
        let date = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();

        // Task with deadline tomorrow (within 3-day threshold)
        let task = make_task_with_deadline("Urgent Task", date + Duration::days(1));
        task_repo.insert(task);

        let alerts = generate_alerts(
            &task_repo,
            &meeting_repo,
            &alert_repo,
            test_user_id(),
            date,
        )
        .await
        .unwrap();

        assert!(!alerts.is_empty());
        // Should contain at least one deadline alert
        assert!(
            alerts.iter().any(|a| a.alert_type == AlertType::Deadline),
            "Expected a deadline alert"
        );
    }

    #[tokio::test]
    async fn resolve_alert_marks_resolved() {
        let alert_repo = InMemoryAlertRepository::new();
        let alert = Alert {
            id: Uuid::new_v4(),
            user_id: test_user_id(),
            alert_type: AlertType::Deadline,
            severity: AlertSeverity::Warning,
            message: "Deadline approaching".to_string(),
            related_items: vec![],
            date: NaiveDate::from_ymd_opt(2026, 3, 9).unwrap(),
            resolved: false,
            created_at: Utc::now(),
        };

        alert_repo.save(&alert).await.unwrap();
        assert!(!alert.resolved);

        let resolved = resolve_alert(&alert_repo, alert.id).await.unwrap();
        assert!(resolved.resolved);
    }

    #[tokio::test]
    async fn resolve_alert_not_found() {
        let alert_repo = InMemoryAlertRepository::new();
        let result = resolve_alert(&alert_repo, Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_alerts_returns_all_for_user() {
        let alert_repo = InMemoryAlertRepository::new();
        let user = test_user_id();

        for i in 0..3 {
            let alert = Alert {
                id: Uuid::new_v4(),
                user_id: user,
                alert_type: AlertType::Deadline,
                severity: AlertSeverity::Warning,
                message: format!("Alert {}", i),
                related_items: vec![],
                date: NaiveDate::from_ymd_opt(2026, 3, 9).unwrap(),
                resolved: i == 2, // last one is resolved
                created_at: Utc::now(),
            };
            alert_repo.save(&alert).await.unwrap();
        }

        // All alerts
        let all = get_alerts(&alert_repo, user, None).await.unwrap();
        assert_eq!(all.len(), 3);

        // Only unresolved
        let unresolved = get_alerts(&alert_repo, user, Some(false)).await.unwrap();
        assert_eq!(unresolved.len(), 2);

        // Only resolved
        let resolved = get_alerts(&alert_repo, user, Some(true)).await.unwrap();
        assert_eq!(resolved.len(), 1);
    }

    #[tokio::test]
    async fn cleanup_resolved_alerts_deletes_resolved() {
        let alert_repo = InMemoryAlertRepository::new();
        let user = test_user_id();

        // Create 2 resolved, 1 unresolved
        for i in 0..3 {
            let alert = Alert {
                id: Uuid::new_v4(),
                user_id: user,
                alert_type: AlertType::Overload,
                severity: AlertSeverity::Information,
                message: format!("Alert {}", i),
                related_items: vec![],
                date: NaiveDate::from_ymd_opt(2026, 3, 9).unwrap(),
                resolved: i < 2, // first two are resolved
                created_at: Utc::now(),
            };
            alert_repo.save(&alert).await.unwrap();
        }

        let deleted = cleanup_resolved_alerts(&alert_repo, user).await.unwrap();
        assert_eq!(deleted, 2);

        let remaining = get_alerts(&alert_repo, user, None).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(!remaining[0].resolved);
    }

    #[tokio::test]
    async fn generate_alerts_no_deadline_no_alert() {
        let task_repo = InMemoryTaskRepository::new();
        let meeting_repo = InMemoryMeetingRepository::new();
        let alert_repo = InMemoryAlertRepository::new();
        let date = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();

        // Task with deadline far in the future (beyond 3-day threshold)
        let task = make_task_with_deadline("Future Task", date + Duration::days(30));
        task_repo.insert(task);

        let alerts = generate_alerts(
            &task_repo,
            &meeting_repo,
            &alert_repo,
            test_user_id(),
            date,
        )
        .await
        .unwrap();

        // Should have no deadline alerts (30 days out > 3 day threshold)
        let deadline_alerts: Vec<_> = alerts
            .iter()
            .filter(|a| a.alert_type == AlertType::Deadline)
            .collect();
        assert!(
            deadline_alerts.is_empty(),
            "No deadline alerts expected for task 30 days out"
        );
    }
}
