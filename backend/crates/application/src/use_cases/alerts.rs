use chrono::NaiveDate;
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::*;

/// Recompute and persist all alerts for a user based on current data.
pub async fn refresh_alerts(
    _task_repo: &dyn TaskRepository,
    _meeting_repo: &dyn MeetingRepository,
    _alert_repo: &dyn AlertRepository,
    _user_id: UserId,
    _today: NaiveDate,
) -> Result<Vec<Alert>, AppError> {
    todo!()
}

/// Mark an alert as resolved.
pub async fn resolve_alert(
    _alert_repo: &dyn AlertRepository,
    _alert_id: AlertId,
) -> Result<Alert, AppError> {
    todo!()
}

/// Get all alerts for a user, optionally filtered by resolved status.
pub async fn get_alerts(
    _alert_repo: &dyn AlertRepository,
    _user_id: UserId,
    _resolved: Option<bool>,
) -> Result<Vec<Alert>, AppError> {
    todo!()
}

/// Clean up all resolved alerts for a user. Returns the number of deleted alerts.
pub async fn cleanup_resolved_alerts(
    _alert_repo: &dyn AlertRepository,
    _user_id: UserId,
) -> Result<u64, AppError> {
    todo!()
}
