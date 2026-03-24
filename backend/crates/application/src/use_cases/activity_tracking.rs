use chrono::{DateTime, NaiveDate, Timelike, Utc};
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
        // Recompute half_day from new start time
        slot.half_day = half_day_of(st.hour());
    }
    if let Some(et) = end_time {
        slot.end_time = Some(et);
    }

    // Validate: end_time must be after start_time (if both are set)
    if let Some(et) = slot.end_time {
        if et <= slot.start_time {
            return Err(AppError::Domain(
                domain::errors::DomainError::ValidationError(
                    "End time must be after start time".to_string(),
                ),
            ));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{TimeZone, Utc};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    use crate::errors::RepositoryError;

    struct InMemoryActivitySlotRepository {
        slots: Mutex<HashMap<ActivitySlotId, ActivitySlot>>,
    }

    impl InMemoryActivitySlotRepository {
        fn new() -> Self {
            Self {
                slots: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl ActivitySlotRepository for InMemoryActivitySlotRepository {
        async fn find_by_id(
            &self,
            id: ActivitySlotId,
        ) -> Result<Option<ActivitySlot>, RepositoryError> {
            let slots = self.slots.lock().unwrap();
            Ok(slots.get(&id).cloned())
        }

        async fn find_by_user_and_date(
            &self,
            user_id: UserId,
            date: NaiveDate,
        ) -> Result<Vec<ActivitySlot>, RepositoryError> {
            let slots = self.slots.lock().unwrap();
            Ok(slots
                .values()
                .filter(|s| s.user_id == user_id && s.date == date)
                .cloned()
                .collect())
        }

        async fn find_active(
            &self,
            user_id: UserId,
        ) -> Result<Option<ActivitySlot>, RepositoryError> {
            let slots = self.slots.lock().unwrap();
            Ok(slots
                .values()
                .find(|s| s.user_id == user_id && s.end_time.is_none())
                .cloned())
        }

        async fn find_by_user_and_date_range(
            &self,
            user_id: UserId,
            start_date: NaiveDate,
            end_date: NaiveDate,
        ) -> Result<Vec<ActivitySlot>, RepositoryError> {
            let slots = self.slots.lock().unwrap();
            Ok(slots
                .values()
                .filter(|s| s.user_id == user_id && s.date >= start_date && s.date <= end_date && s.end_time.is_some())
                .cloned()
                .collect())
        }

        async fn save(&self, slot: &ActivitySlot) -> Result<(), RepositoryError> {
            let mut slots = self.slots.lock().unwrap();
            slots.insert(slot.id, slot.clone());
            Ok(())
        }

        async fn update(&self, slot: &ActivitySlot) -> Result<(), RepositoryError> {
            let mut slots = self.slots.lock().unwrap();
            slots.insert(slot.id, slot.clone());
            Ok(())
        }

        async fn delete(&self, id: ActivitySlotId) -> Result<(), RepositoryError> {
            let mut slots = self.slots.lock().unwrap();
            slots.remove(&id);
            Ok(())
        }
    }

    fn test_user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    #[tokio::test]
    async fn start_activity_creates_slot() {
        let repo = InMemoryActivitySlotRepository::new();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap();
        let task_id = Some(Uuid::new_v4());

        let slot = start_activity(&repo, test_user_id(), task_id, now)
            .await
            .unwrap();

        assert_eq!(slot.user_id, test_user_id());
        assert_eq!(slot.task_id, task_id);
        assert_eq!(slot.start_time, now);
        assert!(slot.end_time.is_none());
        assert_eq!(slot.half_day, HalfDay::Morning);
        assert_eq!(slot.date, now.date_naive());
    }

    #[tokio::test]
    async fn start_activity_afternoon() {
        let repo = InMemoryActivitySlotRepository::new();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 14, 0, 0).unwrap();

        let slot = start_activity(&repo, test_user_id(), None, now)
            .await
            .unwrap();

        assert_eq!(slot.half_day, HalfDay::Afternoon);
        assert!(slot.task_id.is_none());
    }

    #[tokio::test]
    async fn start_activity_stops_previous_active() {
        let repo = InMemoryActivitySlotRepository::new();
        let now1 = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let now2 = Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap();

        let first_slot = start_activity(&repo, test_user_id(), None, now1)
            .await
            .unwrap();
        assert!(first_slot.end_time.is_none());

        let slot2 = start_activity(&repo, test_user_id(), None, now2)
            .await
            .unwrap();

        // first_slot should now be stopped
        let updated_slot1 = repo
            .find_by_id(first_slot.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated_slot1.end_time, Some(now2));

        // slot2 should be active
        assert!(slot2.end_time.is_none());
    }

    #[tokio::test]
    async fn stop_activity_with_active_slot() {
        let repo = InMemoryActivitySlotRepository::new();
        let start = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let stop = Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap();

        start_activity(&repo, test_user_id(), None, start)
            .await
            .unwrap();

        let stopped = stop_activity(&repo, test_user_id(), stop)
            .await
            .unwrap();

        assert!(stopped.is_some());
        let slot = stopped.unwrap();
        assert_eq!(slot.end_time, Some(stop));
    }

    #[tokio::test]
    async fn stop_activity_without_active_returns_none() {
        let repo = InMemoryActivitySlotRepository::new();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap();

        let result = stop_activity(&repo, test_user_id(), now)
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_activity_journal_returns_slots_for_date() {
        let repo = InMemoryActivitySlotRepository::new();
        let date = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let now1 = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let now2 = Utc.with_ymd_and_hms(2026, 3, 9, 14, 0, 0).unwrap();

        // Start and stop two activities on the same date
        let _slot1 = start_activity(&repo, test_user_id(), None, now1)
            .await
            .unwrap();
        stop_activity(&repo, test_user_id(), now1 + chrono::Duration::hours(1))
            .await
            .unwrap();
        start_activity(&repo, test_user_id(), None, now2)
            .await
            .unwrap();

        let journal = get_activity_journal(&repo, test_user_id(), date)
            .await
            .unwrap();

        assert_eq!(journal.len(), 2);
    }

    #[tokio::test]
    async fn get_activity_journal_empty_for_other_date() {
        let repo = InMemoryActivitySlotRepository::new();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();

        start_activity(&repo, test_user_id(), None, now)
            .await
            .unwrap();

        let other_date = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let journal = get_activity_journal(&repo, test_user_id(), other_date)
            .await
            .unwrap();

        assert!(journal.is_empty());
    }

    #[tokio::test]
    async fn get_current_activity_returns_active_slot() {
        let repo = InMemoryActivitySlotRepository::new();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();

        let created = start_activity(&repo, test_user_id(), None, now)
            .await
            .unwrap();

        let current = get_current_activity(&repo, test_user_id())
            .await
            .unwrap();

        assert!(current.is_some());
        assert_eq!(current.unwrap().id, created.id);
    }

    #[tokio::test]
    async fn get_current_activity_returns_none_when_stopped() {
        let repo = InMemoryActivitySlotRepository::new();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let stop_time = Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap();

        start_activity(&repo, test_user_id(), None, now)
            .await
            .unwrap();
        stop_activity(&repo, test_user_id(), stop_time)
            .await
            .unwrap();

        let current = get_current_activity(&repo, test_user_id())
            .await
            .unwrap();

        assert!(current.is_none());
    }

    #[tokio::test]
    async fn update_activity_slot_changes_task_id() {
        let repo = InMemoryActivitySlotRepository::new();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();

        let slot = start_activity(&repo, test_user_id(), None, now)
            .await
            .unwrap();
        assert!(slot.task_id.is_none());

        let new_task_id = Uuid::new_v4();
        let updated = update_activity_slot(&repo, slot.id, Some(Some(new_task_id)), None, None)
            .await
            .unwrap();

        assert_eq!(updated.task_id, Some(new_task_id));
    }

    #[tokio::test]
    async fn update_activity_slot_sets_end_time() {
        let repo = InMemoryActivitySlotRepository::new();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap();

        let slot = start_activity(&repo, test_user_id(), None, now)
            .await
            .unwrap();

        let updated = update_activity_slot(&repo, slot.id, None, None, Some(end))
            .await
            .unwrap();

        assert_eq!(updated.end_time, Some(end));
    }

    #[tokio::test]
    async fn update_activity_slot_not_found() {
        let repo = InMemoryActivitySlotRepository::new();
        let result = update_activity_slot(&repo, Uuid::new_v4(), None, None, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn update_activity_slot_recomputes_half_day_on_start_time_change() {
        let repo = InMemoryActivitySlotRepository::new();
        let morning = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let slot = start_activity(&repo, test_user_id(), None, morning).await.unwrap();
        assert_eq!(slot.half_day, HalfDay::Morning);

        let afternoon = Utc.with_ymd_and_hms(2026, 3, 9, 15, 0, 0).unwrap();
        let updated = update_activity_slot(&repo, slot.id, None, Some(afternoon), None)
            .await
            .unwrap();

        assert_eq!(updated.half_day, HalfDay::Afternoon);
        assert_eq!(updated.start_time, afternoon);
    }

    #[tokio::test]
    async fn update_activity_slot_rejects_end_before_start() {
        let repo = InMemoryActivitySlotRepository::new();
        let start = Utc.with_ymd_and_hms(2026, 3, 9, 14, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 16, 0, 0).unwrap();

        let slot = start_activity(&repo, test_user_id(), None, start).await.unwrap();
        stop_activity(&repo, test_user_id(), end).await.unwrap();

        let bad_end = Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap();
        let result = update_activity_slot(&repo, slot.id, None, None, Some(bad_end)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn update_activity_slot_rejects_start_after_end() {
        let repo = InMemoryActivitySlotRepository::new();
        let start = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap();

        let slot = start_activity(&repo, test_user_id(), None, start).await.unwrap();
        stop_activity(&repo, test_user_id(), end).await.unwrap();

        let bad_start = Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap();
        let result = update_activity_slot(&repo, slot.id, None, Some(bad_start), None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn update_activity_slot_clears_task_id() {
        let repo = InMemoryActivitySlotRepository::new();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let task_id = Some(Uuid::new_v4());

        let slot = start_activity(&repo, test_user_id(), task_id, now).await.unwrap();
        assert!(slot.task_id.is_some());

        let updated = update_activity_slot(&repo, slot.id, Some(None), None, None)
            .await
            .unwrap();

        assert!(updated.task_id.is_none());
    }

    #[tokio::test]
    async fn delete_activity_slot_removes_it() {
        let repo = InMemoryActivitySlotRepository::new();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();

        let slot = start_activity(&repo, test_user_id(), None, now)
            .await
            .unwrap();

        delete_activity_slot(&repo, slot.id).await.unwrap();

        let found = repo.find_by_id(slot.id).await.unwrap();
        assert!(found.is_none());
    }
}
