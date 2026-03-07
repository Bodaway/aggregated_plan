use chrono::{DateTime, NaiveDate, Utc};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::*;

/// Start tracking a new activity. Closes the currently active slot (if any).
pub async fn start_activity(
    _activity_repo: &dyn ActivitySlotRepository,
    _user_id: UserId,
    _task_id: Option<TaskId>,
    _now: DateTime<Utc>,
) -> Result<ActivitySlot, AppError> {
    todo!()
}

/// Stop the currently active activity tracking slot.
pub async fn stop_activity(
    _activity_repo: &dyn ActivitySlotRepository,
    _user_id: UserId,
    _now: DateTime<Utc>,
) -> Result<Option<ActivitySlot>, AppError> {
    todo!()
}

/// Update an existing activity slot (e.g., change the associated task).
pub async fn update_activity_slot(
    _activity_repo: &dyn ActivitySlotRepository,
    _slot_id: ActivitySlotId,
    _task_id: Option<TaskId>,
) -> Result<ActivitySlot, AppError> {
    todo!()
}

/// Get the activity journal (all slots) for a user on a specific date.
pub async fn get_activity_journal(
    _activity_repo: &dyn ActivitySlotRepository,
    _user_id: UserId,
    _date: NaiveDate,
) -> Result<Vec<ActivitySlot>, AppError> {
    todo!()
}
