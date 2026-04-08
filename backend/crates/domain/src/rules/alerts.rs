use chrono::{DateTime, NaiveDate, Utc};

use crate::types::*;

use super::urgency::count_business_days;
use super::workload::detect_overload;

/// Alert data produced by business rule checks.
pub struct AlertData {
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub related_items: Vec<RelatedItem>,
    pub date: NaiveDate,
}

/// A scheduled item (task or meeting) with a time range, used for conflict detection.
pub enum ScheduledItem {
    Task {
        id: TaskId,
        title: String,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    Meeting {
        id: MeetingId,
        title: String,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
}

/// R17: Check all tasks for approaching or past deadlines.
/// Returns alerts for tasks that are overdue (Critical) or within the threshold (Warning).
pub fn check_deadline_alerts(
    tasks: &[Task],
    today: NaiveDate,
    threshold_days: i64,
) -> Vec<AlertData> {
    tasks
        .iter()
        .filter_map(|task| {
            let deadline = task.deadline?;
            let days_remaining = count_business_days(today, deadline);
            if days_remaining < 0 {
                Some(AlertData {
                    alert_type: AlertType::Deadline,
                    severity: AlertSeverity::Critical,
                    message: format!(
                        "Task '{}' is overdue by {} day(s)",
                        task.title, -days_remaining
                    ),
                    related_items: vec![RelatedItem::Task(task.id)],
                    date: today,
                })
            } else if days_remaining <= threshold_days {
                Some(AlertData {
                    alert_type: AlertType::Deadline,
                    severity: AlertSeverity::Warning,
                    message: format!(
                        "Task '{}' is due in {} day(s)",
                        task.title, days_remaining
                    ),
                    related_items: vec![RelatedItem::Task(task.id)],
                    date: today,
                })
            } else {
                None
            }
        })
        .collect()
}

/// R18: Check for scheduling conflicts between items.
/// Overlap condition: start_a < end_b AND start_b < end_a.
pub fn check_conflict_alerts(items: &[ScheduledItem], date: NaiveDate) -> Vec<AlertData> {
    let mut alerts = Vec::new();
    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            let (start_a, end_a, title_a, item_a) = extract_schedule_info(&items[i]);
            let (start_b, end_b, title_b, item_b) = extract_schedule_info(&items[j]);
            if start_a < end_b && start_b < end_a {
                alerts.push(AlertData {
                    alert_type: AlertType::Conflict,
                    severity: AlertSeverity::Information,
                    message: format!("Conflict: '{}' overlaps with '{}'", title_a, title_b),
                    related_items: vec![item_a, item_b],
                    date,
                });
            }
        }
    }
    alerts
}

fn extract_schedule_info(
    item: &ScheduledItem,
) -> (DateTime<Utc>, DateTime<Utc>, &str, RelatedItem) {
    match item {
        ScheduledItem::Task {
            id,
            title,
            start,
            end,
        } => (*start, *end, title, RelatedItem::Task(*id)),
        ScheduledItem::Meeting {
            id,
            title,
            start,
            end,
        } => (*start, *end, title, RelatedItem::Meeting(*id)),
    }
}

/// R16: Check overload for the week.
/// Returns an alert if total planned + meeting half-days exceed weekly capacity.
/// Severity is Critical if excess > 2.0, Warning otherwise.
pub fn check_overload_alerts(
    planned_half_days: f64,
    meeting_half_days: f64,
    weekly_capacity: f64,
    week_start: NaiveDate,
) -> Option<AlertData> {
    detect_overload(planned_half_days, meeting_half_days, weekly_capacity).map(|excess| {
        let severity = if excess > 2.0 {
            AlertSeverity::Critical
        } else {
            AlertSeverity::Warning
        };
        AlertData {
            alert_type: AlertType::Overload,
            severity,
            message: format!("Overloaded by {:.1} half-day(s) this week", excess),
            related_items: vec![],
            date: week_start,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone, Utc};
    use uuid::Uuid;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn make_task_with_deadline(title: &str, deadline: Option<NaiveDate>) -> Task {
        Task {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            title: title.to_string(),
            description: None,
            notes: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            deadline,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Low,
            urgency_manual: false,
            impact: ImpactLevel::Low,
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

    // ─── check_deadline_alerts ───

    #[test]
    fn deadline_overdue_produces_critical_alert() {
        let today = date(2026, 3, 11); // Wednesday
        let tasks = vec![make_task_with_deadline("Overdue task", Some(date(2026, 3, 9)))];
        let alerts = check_deadline_alerts(&tasks, today, 3);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, AlertSeverity::Critical);
        assert_eq!(alerts[0].alert_type, AlertType::Deadline);
        assert!(alerts[0].message.contains("overdue"));
    }

    #[test]
    fn deadline_close_produces_warning_alert() {
        let today = date(2026, 3, 9); // Monday
        // Deadline is Wednesday = 2 business days away, threshold = 3
        let tasks = vec![make_task_with_deadline("Close task", Some(date(2026, 3, 11)))];
        let alerts = check_deadline_alerts(&tasks, today, 3);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, AlertSeverity::Warning);
        assert!(alerts[0].message.contains("due in"));
    }

    #[test]
    fn deadline_far_away_produces_no_alert() {
        let today = date(2026, 3, 9);
        let tasks = vec![make_task_with_deadline("Far task", Some(date(2026, 4, 1)))];
        let alerts = check_deadline_alerts(&tasks, today, 3);
        assert!(alerts.is_empty());
    }

    #[test]
    fn no_deadline_produces_no_alert() {
        let today = date(2026, 3, 9);
        let tasks = vec![make_task_with_deadline("No deadline", None)];
        let alerts = check_deadline_alerts(&tasks, today, 3);
        assert!(alerts.is_empty());
    }

    // ─── check_conflict_alerts ───

    #[test]
    fn overlapping_items_produce_conflict_alert() {
        let items = vec![
            ScheduledItem::Task {
                id: Uuid::new_v4(),
                title: "Task A".to_string(),
                start: Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap(),
            },
            ScheduledItem::Meeting {
                id: Uuid::new_v4(),
                title: "Meeting B".to_string(),
                start: Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap(),
            },
        ];
        let alerts = check_conflict_alerts(&items, date(2026, 3, 9));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, AlertType::Conflict);
        assert_eq!(alerts[0].severity, AlertSeverity::Information);
        assert!(alerts[0].message.contains("Task A"));
        assert!(alerts[0].message.contains("Meeting B"));
    }

    #[test]
    fn non_overlapping_items_produce_no_alert() {
        let items = vec![
            ScheduledItem::Task {
                id: Uuid::new_v4(),
                title: "Task A".to_string(),
                start: Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap(),
            },
            ScheduledItem::Meeting {
                id: Uuid::new_v4(),
                title: "Meeting B".to_string(),
                start: Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap(),
            },
        ];
        let alerts = check_conflict_alerts(&items, date(2026, 3, 9));
        assert!(alerts.is_empty());
    }

    #[test]
    fn adjacent_items_no_conflict() {
        // End of one == start of next: no overlap
        let items = vec![
            ScheduledItem::Task {
                id: Uuid::new_v4(),
                title: "A".to_string(),
                start: Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap(),
            },
            ScheduledItem::Task {
                id: Uuid::new_v4(),
                title: "B".to_string(),
                start: Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap(),
            },
        ];
        let alerts = check_conflict_alerts(&items, date(2026, 3, 9));
        assert!(alerts.is_empty());
    }

    // ─── check_overload_alerts ───

    #[test]
    fn overload_produces_warning_alert() {
        let alert = check_overload_alerts(8.0, 3.0, 10.0, date(2026, 3, 9));
        assert!(alert.is_some());
        let alert = alert.unwrap();
        assert_eq!(alert.alert_type, AlertType::Overload);
        assert_eq!(alert.severity, AlertSeverity::Warning);
        assert!(alert.message.contains("1.0"));
    }

    #[test]
    fn overload_large_excess_produces_critical_alert() {
        let alert = check_overload_alerts(8.0, 5.0, 10.0, date(2026, 3, 9));
        assert!(alert.is_some());
        let alert = alert.unwrap();
        assert_eq!(alert.severity, AlertSeverity::Critical);
    }

    #[test]
    fn no_overload_produces_no_alert() {
        let alert = check_overload_alerts(5.0, 3.0, 10.0, date(2026, 3, 9));
        assert!(alert.is_none());
    }
}
