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
