use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Timelike, Utc};
use domain::rules::overlap::find_overlaps;
use domain::rules::workload::half_day_of;
use domain::types::*;

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
    let slot = ActivitySlot::manual(user_id, task_id, now, None, half_day, date, now);

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

/// One flagged double-count: two different tasks' closed slots claiming an
/// overlapping stretch of time, paired back to the two slots the domain rule
/// only referenced by id.
///
/// Nothing here corrects the double count — the user's decision, recorded in
/// the design: several sessions (and the human) can legitimately log time
/// concurrently, each task keeps the time its own entries document, and the
/// user arbitrates at the timesheet review. This only resolves *which* two
/// slots collided, so the GraphQL resolver can turn a `task_id` into a title
/// without `domain` doing any I/O of its own.
#[derive(Debug, Clone)]
pub struct ActivityOverlap {
    pub minutes: i64,
    pub a: ActivitySlot,
    pub b: ActivitySlot,
}

/// Get the day's flagged double-counted stretches: every pair of different-task,
/// closed slots whose times intersect.
///
/// Computed at read time on every call — nothing is stored. Fetches the same
/// slots [`get_activity_journal`] would return for the same date and hands
/// them to the pure domain rule, then pairs each reported
/// [`domain::rules::overlap::Overlap`] back to the two slots it named by id.
pub async fn get_activity_overlaps(
    activity_repo: &dyn ActivitySlotRepository,
    user_id: UserId,
    date: NaiveDate,
) -> Result<Vec<ActivityOverlap>, AppError> {
    let slots = activity_repo.find_by_user_and_date(user_id, date).await?;
    let by_id: HashMap<ActivitySlotId, &ActivitySlot> =
        slots.iter().map(|slot| (slot.id, slot)).collect();

    find_overlaps(&slots)
        .into_iter()
        .map(|overlap| {
            // `find_overlaps` only ever names ids drawn from the slice we just
            // gave it, so a miss here means that invariant broke — fail loudly
            // rather than silently drop a flagged collision.
            let a = by_id.get(&overlap.a).ok_or_else(|| {
                AppError::Validation(format!(
                    "overlap referenced slot {} which was not in the fetched set",
                    overlap.a
                ))
            })?;
            let b = by_id.get(&overlap.b).ok_or_else(|| {
                AppError::Validation(format!(
                    "overlap referenced slot {} which was not in the fetched set",
                    overlap.b
                ))
            })?;
            Ok(ActivityOverlap {
                minutes: overlap.minutes,
                a: (*a).clone(),
                b: (*b).clone(),
            })
        })
        .collect()
}

/// Get the currently active activity slot for a user.
pub async fn get_current_activity(
    activity_repo: &dyn ActivitySlotRepository,
    user_id: UserId,
) -> Result<Option<ActivitySlot>, AppError> {
    activity_repo.find_active(user_id).await.map_err(Into::into)
}

/// Update an existing activity slot.
///
/// A slot whose `source` is `Worklog` is owned by the worklog projection: the
/// flush rebuilds it from the worklog entries on every run and would silently
/// discard a hand edit made here on the next pass. Refuse instead of accepting
/// an edit the next flush would erase, and name the two real remedies so the
/// caller isn't left to guess: fix the worklog entries, or move them to another
/// task with `aplan reattribute`. A `Manual` slot is never rebuilt and keeps
/// editing unchanged.
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

    if slot.source.is_projection() {
        return Err(AppError::Validation(format!(
            "ActivitySlot {} is owned by the worklog projection and is rebuilt from the \
             worklog entries on every flush, so an edit here would be silently overwritten. \
             Correct the underlying worklog entries instead, or move them to a different task \
             with `aplan reattribute --from <task> --to <task>`.",
            slot_id
        )));
    }

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

/// Create a manual (completed) activity slot with explicit start and end times.
pub async fn create_manual_activity_slot(
    activity_repo: &dyn ActivitySlotRepository,
    user_id: UserId,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    task_id: Option<TaskId>,
) -> Result<ActivitySlot, AppError> {
    // Validate: end_time must be after start_time
    if end_time <= start_time {
        return Err(AppError::Domain(
            domain::errors::DomainError::ValidationError(
                "End time must be after start time".to_string(),
            ),
        ));
    }

    let half_day = half_day_of(start_time.hour());
    let date = start_time.date_naive();

    let slot = ActivitySlot::manual(
        user_id,
        task_id,
        start_time,
        Some(end_time),
        half_day,
        date,
        Utc::now(),
    );

    activity_repo.save(&slot).await?;
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

    /// Two different tasks' overlapping manual slots must come back paired with
    /// both *full* slots, not merely their ids — resolving titles and actors at
    /// the GraphQL layer needs each slot's own `task_id` and `session_id` to
    /// travel with it. Pairing by id, not by position, is the property under
    /// test: a bug that zipped the two slots positionally rather than by the
    /// ids `find_overlaps` returned would still pass a test that only checked
    /// `overlaps.len()`.
    #[tokio::test]
    async fn get_activity_overlaps_pairs_the_two_full_slots() {
        let repo = InMemoryActivitySlotRepository::new();
        let date = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        let start_a = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let end_a = Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap();
        let start_b = Utc.with_ymd_and_hms(2026, 3, 9, 9, 30, 0).unwrap();
        let end_b = Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap();

        let slot_a = create_manual_activity_slot(&repo, test_user_id(), start_a, end_a, Some(task_a))
            .await
            .unwrap();
        let slot_b = create_manual_activity_slot(&repo, test_user_id(), start_b, end_b, Some(task_b))
            .await
            .unwrap();

        let overlaps = get_activity_overlaps(&repo, test_user_id(), date)
            .await
            .unwrap();
        assert_eq!(overlaps.len(), 1);
        let overlap = &overlaps[0];
        assert_eq!(overlap.minutes, 30);

        let (returned_a, returned_b) = if overlap.a.id == slot_a.id {
            (&overlap.a, &overlap.b)
        } else {
            (&overlap.b, &overlap.a)
        };
        assert_eq!(returned_a.id, slot_a.id);
        assert_eq!(returned_a.task_id, Some(task_a));
        assert_eq!(returned_b.id, slot_b.id);
        assert_eq!(returned_b.task_id, Some(task_b));
    }

    /// A worklog-authored slot's `session_id` must come back on the side that
    /// is actually its own slot, not merely be present somewhere in the pair.
    /// This is the field Task 9 renders as `session a1b2 ↔ manuel`: a bug that
    /// dropped `session_id` while pairing (hardcoding `None`) or attached it
    /// to the wrong side would still pass a test that only checked
    /// `.is_some()` on one side and `.is_none()` on the other, since exactly
    /// one side is `None` either way.
    ///
    /// Neither `create_manual_activity_slot` nor `start_activity` can mint a
    /// slot carrying a session id — both always call `ActivitySlot::manual`,
    /// which hardcodes `session_id: None` — so this test builds the
    /// worklog-authored slot directly with `ActivitySlot::from_worklog` and
    /// saves it through the repository, bypassing the use-case layer for
    /// setup only.
    #[tokio::test]
    async fn get_activity_overlaps_carries_session_id_on_its_own_side() {
        let repo = InMemoryActivitySlotRepository::new();
        let date = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let task_session = Uuid::new_v4();
        let task_manual = Uuid::new_v4();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap();

        let session_slot = ActivitySlot::from_worklog(
            test_user_id(),
            task_session,
            Some("sess-a".to_string()),
            Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap(),
            HalfDay::Morning,
            date,
            now,
        );
        repo.save(&session_slot).await.unwrap();

        let manual_slot = create_manual_activity_slot(
            &repo,
            test_user_id(),
            Utc.with_ymd_and_hms(2026, 3, 9, 9, 30, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap(),
            Some(task_manual),
        )
        .await
        .unwrap();

        let overlaps = get_activity_overlaps(&repo, test_user_id(), date)
            .await
            .unwrap();
        assert_eq!(overlaps.len(), 1);
        let overlap = &overlaps[0];

        let (returned_session_side, returned_manual_side) = if overlap.a.id == session_slot.id {
            (&overlap.a, &overlap.b)
        } else {
            (&overlap.b, &overlap.a)
        };
        assert_eq!(returned_session_side.id, session_slot.id);
        assert_eq!(
            returned_session_side.session_id.as_deref(),
            Some("sess-a"),
            "the session-authored slot must keep its own session_id"
        );
        assert_eq!(returned_manual_side.id, manual_slot.id);
        assert!(
            returned_manual_side.session_id.is_none(),
            "the manual slot must not inherit a session_id from the other side"
        );
    }

    /// A day with no colliding slots must report nothing — silence by
    /// construction, not a special case, so the caller never has to filter a
    /// zero-minute or empty-but-present entry back out.
    #[tokio::test]
    async fn get_activity_overlaps_empty_when_no_collision() {
        let repo = InMemoryActivitySlotRepository::new();
        let date = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let start = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap();
        create_manual_activity_slot(&repo, test_user_id(), start, end, Some(Uuid::new_v4()))
            .await
            .unwrap();

        let overlaps = get_activity_overlaps(&repo, test_user_id(), date)
            .await
            .unwrap();
        assert!(overlaps.is_empty());
    }

    /// The same wiring, but on a task's own two stretches: the domain rule
    /// excludes same-task pairs, so a caller must see this reported as
    /// nothing, not as a self-collision.
    #[tokio::test]
    async fn get_activity_overlaps_excludes_same_task() {
        let repo = InMemoryActivitySlotRepository::new();
        let date = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let task = Uuid::new_v4();

        create_manual_activity_slot(
            &repo,
            test_user_id(),
            Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap(),
            Some(task),
        )
        .await
        .unwrap();
        create_manual_activity_slot(
            &repo,
            test_user_id(),
            Utc.with_ymd_and_hms(2026, 3, 9, 9, 30, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 3, 9, 10, 30, 0).unwrap(),
            Some(task),
        )
        .await
        .unwrap();

        let overlaps = get_activity_overlaps(&repo, test_user_id(), date)
            .await
            .unwrap();
        assert!(overlaps.is_empty());
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
    async fn create_manual_activity_slot_success() {
        let repo = InMemoryActivitySlotRepository::new();
        let start = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 11, 30, 0).unwrap();
        let task_id = Some(Uuid::new_v4());

        let slot = create_manual_activity_slot(&repo, test_user_id(), start, end, task_id)
            .await
            .unwrap();

        assert_eq!(slot.user_id, test_user_id());
        assert_eq!(slot.task_id, task_id);
        assert_eq!(slot.start_time, start);
        assert_eq!(slot.end_time, Some(end));
        assert_eq!(slot.half_day, HalfDay::Morning);
        assert_eq!(slot.date, start.date_naive());
    }

    #[tokio::test]
    async fn create_manual_activity_slot_afternoon() {
        let repo = InMemoryActivitySlotRepository::new();
        let start = Utc.with_ymd_and_hms(2026, 3, 9, 14, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 16, 0, 0).unwrap();

        let slot = create_manual_activity_slot(&repo, test_user_id(), start, end, None)
            .await
            .unwrap();

        assert_eq!(slot.half_day, HalfDay::Afternoon);
        assert!(slot.task_id.is_none());
    }

    #[tokio::test]
    async fn create_manual_activity_slot_rejects_end_before_start() {
        let repo = InMemoryActivitySlotRepository::new();
        let start = Utc.with_ymd_and_hms(2026, 3, 9, 14, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap();

        let result = create_manual_activity_slot(&repo, test_user_id(), start, end, None).await;
        assert!(result.is_err());
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

    /// A slot the worklog projection owns is a cache of the worklog entries, not a
    /// fact of its own: the next flush rebuilds its half-day from those entries and
    /// would silently discard a hand edit. `update_activity_slot` must refuse before
    /// it ever reaches the repository.
    #[tokio::test]
    async fn update_activity_slot_refuses_edit_on_worklog_owned_slot() {
        let repo = InMemoryActivitySlotRepository::new();
        let date = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let start = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap();
        let original_task_id = Uuid::new_v4();

        let slot = ActivitySlot::from_worklog(
            test_user_id(),
            original_task_id,
            Some("sess-1".to_string()),
            start,
            end,
            HalfDay::Morning,
            date,
            end,
        );
        repo.save(&slot).await.unwrap();

        let other_task_id = Uuid::new_v4();
        let result = update_activity_slot(&repo, slot.id, Some(Some(other_task_id)), None, None)
            .await;

        assert!(result.is_err(), "expected the edit to be refused");

        // The repository double must show no write at all: same task, same source.
        let stored = repo.find_by_id(slot.id).await.unwrap().unwrap();
        assert_eq!(stored.task_id, Some(original_task_id));
        assert_eq!(stored.source, SlotSource::Worklog);
    }

    /// A `Manual` slot is never rebuilt, so editing it must keep working exactly as
    /// before: the change lands in the repository, not just in the returned value.
    #[tokio::test]
    async fn update_activity_slot_still_succeeds_on_manual_slot() {
        let repo = InMemoryActivitySlotRepository::new();
        let now = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();

        let slot = start_activity(&repo, test_user_id(), None, now)
            .await
            .unwrap();
        assert_eq!(slot.source, SlotSource::Manual);

        let new_task_id = Uuid::new_v4();
        let updated = update_activity_slot(&repo, slot.id, Some(Some(new_task_id)), None, None)
            .await
            .unwrap();

        assert_eq!(updated.task_id, Some(new_task_id));

        // Confirm the double actually mutated its stored state, not just the
        // returned value: re-fetch from the repository.
        let stored = repo.find_by_id(slot.id).await.unwrap().unwrap();
        assert_eq!(stored.task_id, Some(new_task_id));
        assert_eq!(stored.source, SlotSource::Manual);
    }

    /// The refusal is only useful if it tells the caller where to act instead. Pin
    /// both remedies verbatim so a future reword cannot quietly drop one.
    #[tokio::test]
    async fn update_activity_slot_refusal_names_both_remedies() {
        let repo = InMemoryActivitySlotRepository::new();
        let date = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let start = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap();

        let slot = ActivitySlot::from_worklog(
            test_user_id(),
            Uuid::new_v4(),
            None,
            start,
            end,
            HalfDay::Morning,
            date,
            end,
        );
        repo.save(&slot).await.unwrap();

        let err = update_activity_slot(&repo, slot.id, None, None, Some(end))
            .await
            .unwrap_err();
        let message = err.to_string();

        assert!(
            message.contains("Correct the underlying worklog entries"),
            "refusal must name the first remedy (fix the worklog entries): {message}"
        );
        assert!(
            message.contains("aplan reattribute --from <task> --to <task>"),
            "refusal must name the second remedy (move them with aplan reattribute): {message}"
        );
    }
}
