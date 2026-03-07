use chrono::{DateTime, NaiveDate, Utc};
use domain::rules::workload::half_day_of;
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::*;

/// Start tracking a new activity. Closes the currently active slot (if any).
pub async fn start_activity(
    activity_repo: &dyn ActivitySlotRepository,
    user_id: UserId,
    task_id: Option<TaskId>,
    now: DateTime<Utc>,
) -> Result<ActivitySlot, AppError> {
    // 1. Check if there's already an active slot
    if let Some(mut active) = activity_repo.find_active(user_id).await? {
        // Stop the active slot
        active.end_time = Some(now);
        activity_repo.update(&active).await?;
    }

    // 2. Determine half-day from current hour
    let half_day = half_day_of(now.time().format("%H").to_string().parse::<u32>().unwrap_or(12));
    let date = now.date_naive();

    // 3. Create new slot
    let slot = ActivitySlot {
        id: Uuid::new_v4(),
        user_id,
        task_id,
        start_time: now,
        end_time: None,
        half_day,
        date,
        created_at: now,
    };

    activity_repo.save(&slot).await?;
    Ok(slot)
}

/// Stop the currently active activity tracking slot.
pub async fn stop_activity(
    activity_repo: &dyn ActivitySlotRepository,
    user_id: UserId,
    now: DateTime<Utc>,
) -> Result<Option<ActivitySlot>, AppError> {
    match activity_repo.find_active(user_id).await? {
        Some(mut slot) => {
            slot.end_time = Some(now);
            activity_repo.update(&slot).await?;
            Ok(Some(slot))
        }
        None => Ok(None),
    }
}

/// Get the activity journal (all slots) for a user on a specific date.
pub async fn get_activity_journal(
    activity_repo: &dyn ActivitySlotRepository,
    user_id: UserId,
    date: NaiveDate,
) -> Result<Vec<ActivitySlot>, AppError> {
    activity_repo
        .find_by_user_and_date(user_id, date)
        .await
        .map_err(Into::into)
}

/// Get the currently active activity slot for a user.
pub async fn get_current_activity(
    activity_repo: &dyn ActivitySlotRepository,
    user_id: UserId,
) -> Result<Option<ActivitySlot>, AppError> {
    activity_repo.find_active(user_id).await.map_err(Into::into)
}

/// Update an existing activity slot.
pub async fn update_activity_slot(
    activity_repo: &dyn ActivitySlotRepository,
    slot_id: ActivitySlotId,
    task_id: Option<Option<TaskId>>,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
) -> Result<ActivitySlot, AppError> {
    let mut slot = activity_repo
        .find_by_id(slot_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ActivitySlot {}", slot_id)))?;

    if let Some(tid) = task_id {
        slot.task_id = tid;
    }
    if let Some(st) = start_time {
        slot.start_time = st;
    }
    if let Some(et) = end_time {
        slot.end_time = Some(et);
    }

    activity_repo.update(&slot).await?;
    Ok(slot)
}

/// Delete an activity slot.
pub async fn delete_activity_slot(
    activity_repo: &dyn ActivitySlotRepository,
    slot_id: ActivitySlotId,
) -> Result<(), AppError> {
    activity_repo.delete(slot_id).await.map_err(Into::into)
}
