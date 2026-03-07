use chrono::NaiveDate;
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::*;

/// Aggregated data for the daily dashboard view.
pub struct DailyDashboard {
    pub date: NaiveDate,
    pub tasks: Vec<Task>,
    pub meetings: Vec<Meeting>,
    pub alerts: Vec<Alert>,
    pub sync_statuses: Vec<SyncStatus>,
}

/// Fetch all data needed for the daily dashboard: tasks (sorted by priority),
/// meetings, unresolved alerts, and sync statuses.
pub async fn get_daily_dashboard(
    _task_repo: &dyn TaskRepository,
    _meeting_repo: &dyn MeetingRepository,
    _alert_repo: &dyn AlertRepository,
    _sync_repo: &dyn SyncStatusRepository,
    _user_id: UserId,
    _date: NaiveDate,
) -> Result<DailyDashboard, AppError> {
    todo!()
}
