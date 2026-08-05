use chrono::{DateTime, NaiveDate, TimeZone, Timelike, Utc};
use domain::rules::reattribution::{is_rebuildable, AffectedHalfDay};
use domain::rules::workload::half_day_of;
use domain::rules::worklog_time::{derive_time_blocks, MIN_BLOCK_MINUTES};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::{
    ActivitySlotRepository, ConfigRepository, WorklogFilter, WorklogRepository,
    WORKLOG_FILTER_DEFAULT_LIMIT, WORKLOG_FILTER_MAX_LIMIT,
};
use crate::time::local_window;
use crate::use_cases::reattribution::refuse_a_truncated_page;

/// Add a new worklog entry. `logged_at` defaults to `now` when `None`.
///
/// `session_id` attributes the entry to the session that wrote it, in the same
/// write the entry is created with — `None` is the human, working by hand.
pub async fn add_worklog_entry(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    task_id: TaskId,
    body: String,
    logged_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    session_id: Option<SessionId>,
) -> Result<WorklogEntry, AppError> {
    let entry = WorklogEntry::new(user_id, task_id, body, logged_at.unwrap_or(now), now)?
        .by_session(session_id);
    worklog_repo.create(&entry).await?;
    Ok(entry)
}

/// Partial update. Re-validates body when provided.
pub async fn update_worklog_entry(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    id: WorklogEntryId,
    body: Option<String>,
    logged_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<WorklogEntry, AppError> {
    let mut entry = worklog_repo
        .find_by_id(id, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("worklog entry {id}")))?;

    if let Some(new_body) = body {
        let validated = WorklogEntry::new(
            entry.user_id,
            entry.task_id,
            new_body,
            entry.logged_at,
            now,
        )?;
        entry.body = validated.body;
    }
    if let Some(lat) = logged_at {
        entry.logged_at = lat;
    }
    entry.updated_at = now;
    worklog_repo.update(&entry).await?;
    Ok(entry)
}

pub async fn delete_worklog_entry(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    id: WorklogEntryId,
) -> Result<bool, AppError> {
    Ok(worklog_repo.delete(id, user_id).await?)
}

pub async fn list_worklog_entries(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    mut filter: WorklogFilter,
) -> Result<Vec<WorklogEntry>, AppError> {
    if filter.limit == 0 {
        filter.limit = WORKLOG_FILTER_DEFAULT_LIMIT;
    }
    if filter.limit > WORKLOG_FILTER_MAX_LIMIT {
        filter.limit = WORKLOG_FILTER_MAX_LIMIT;
    }
    Ok(worklog_repo.list(user_id, &filter).await?)
}

/// Outcome of a flush: how many slots were written, and where the next flush's
/// window should start from. `active_since` is always `now`, pairing exactly with
/// the repository's half-open `[from, to)`: an entry logged at precisely `now`
/// falls to the next flush, with no gap and no double-read.
pub struct FlushOutcome {
    pub slots_written: u32,
    pub active_since: DateTime<Utc>,
}

const DEFAULT_TZ: &str = "Europe/Paris";

/// The timezone the half-day projection is expressed in.
///
/// Shared rather than inlined: the flush writes slots and the reattribution repair
/// rebuilds them, and two readings of `aplan.timezone` that could disagree would put
/// the same worklog entry on two different local days.
pub async fn user_timezone(
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
) -> Result<chrono_tz::Tz, AppError> {
    Ok(config_repo
        .get(user_id, "aplan.timezone")
        .await?
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| DEFAULT_TZ.parse().expect("default tz parses")))
}

/// Materialize the worklog time of `task_id` into closed activity slots.
///
/// `from` is a **selector, not a watermark**: it picks which local half-days to
/// rebuild, and every entry of this task in those half-days then decides what the
/// slots are. That inversion is the point of the whole plan. The old
/// implementation appended slots for entries newer than a single global watermark,
/// duplicating whatever it had already written on every re-run — this rebuild fixes
/// that on its own. That watermark was also one key shared by every caller, so
/// flushing one session's task could advance the mark another session's task
/// depended on, losing that task's entries; closing that needed a caller that reads
/// and advances each session's own window instead of the shared key
/// (`SessionRepository::set_last_flush`), which is what plan 2's flush resolver now
/// does.
///
/// Widening the window is therefore free, and re-running is free: the operation is
/// idempotent because it derives everything from the entries and owns only the slots
/// it wrote (`SlotSource::Worklog`).
pub async fn materialize_worklog_time(
    worklog_repo: &dyn WorklogRepository,
    activity_repo: &dyn ActivitySlotRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    task_id: TaskId,
    from: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<FlushOutcome, AppError> {
    let tz = user_timezone(config_repo, user_id).await?;

    // The window's only job: which half-days did this task touch? That needs the
    // distinct half-days, not every entry, so page through `offset` rather than
    // refusing once a page reaches `WORKLOG_FILTER_MAX_LIMIT` — a task can carry
    // far more than that many entries across its whole history and still have a
    // handful of half-days to rebuild.
    let mut half_days: Vec<AffectedHalfDay> = Vec::new();
    let mut offset = 0u32;
    loop {
        let filter = WorklogFilter {
            task_ids: Some(vec![task_id]),
            from: Some(from),
            to: Some(now),
            limit: WORKLOG_FILTER_MAX_LIMIT,
            offset,
        };
        let page = worklog_repo.list(user_id, &filter).await?;
        let page_len = page.len() as u32;
        for entry in &page {
            let local = tz.from_utc_datetime(&entry.logged_at.naive_utc()).naive_local();
            let unit = AffectedHalfDay {
                date: local.date(),
                half_day: half_day_of(local.time().hour()),
            };
            if !half_days.iter().any(|u| u.date == unit.date && u.half_day == unit.half_day) {
                half_days.push(unit);
            }
        }
        if page_len < WORKLOG_FILTER_MAX_LIMIT {
            break;
        }
        offset += WORKLOG_FILTER_MAX_LIMIT;
    }

    let mut written = 0u32;
    for unit in &half_days {
        let plan = plan_task_projection(
            activity_repo, worklog_repo, user_id, task_id,
            std::slice::from_ref(unit), tz, now,
        )
        .await?;
        written += plan.write.len() as u32;
        apply_task_projection(activity_repo, &plan).await?;
    }

    Ok(FlushOutcome { slots_written: written, active_since: now })
}

/// What a rebuild of one task's projection over some half-days would do.
///
/// Separated from the applying so the reattribution preview and the flush share one
/// piece of arithmetic: a preview that computed its figures differently from the
/// write would report numbers nobody could reproduce.
pub struct RebuildPlan {
    pub task_id: TaskId,
    /// Slots the projection owns in these half-days, to be dropped first. Dropping
    /// them is what makes the rewrite exact: without it, a half-day that already
    /// carried a slot would keep it *and* gain the rebuilt one, and the same morning
    /// would be billed twice.
    pub delete: Vec<ActivitySlot>,
    /// What this task's entries in these half-days say the time was.
    pub write: Vec<ActivitySlot>,
}

/// Compute the rebuild of `task_id`'s projection over `half_days`. Reads only.
///
/// `half_days` bounds the blast radius; it never decides truth. Truth is every entry
/// of this task falling in those half-days — which is why naming an extra half-day
/// is harmless, and why an entry logged with a backdated `logged_at` is picked up
/// rather than skipped by a watermark comparison, as long as its own local day
/// falls in the range `half_days` spans. The worklog read is itself scoped to that
/// same span (the earliest to the latest named date) rather than to the task's
/// whole history: the two are equivalent for which entries end up in the plan —
/// membership is decided by half-day match either way — but the scoped read keeps
/// the page cap meaning what it says, "this window may have been cut", instead of
/// firing on how much unrelated history the task happens to carry.
/// An empty `half_days` returns an empty plan without touching either repository.
pub async fn plan_task_projection(
    activity_repo: &dyn ActivitySlotRepository,
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    task_id: TaskId,
    half_days: &[AffectedHalfDay],
    tz: chrono_tz::Tz,
    now: DateTime<Utc>,
) -> Result<RebuildPlan, AppError> {
    let dates: Vec<NaiveDate> = half_days.iter().map(|unit| unit.date).collect();
    let (Some(&first), Some(&last)) = (dates.iter().min(), dates.iter().max()) else {
        // Nothing named: nothing to rebuild, and no reason to read either repository.
        return Ok(RebuildPlan {
            task_id,
            delete: Vec::new(),
            write: Vec::new(),
        });
    };

    let mut delete = Vec::new();
    let mut seen_dates: Vec<NaiveDate> = Vec::new();
    for unit in half_days {
        if seen_dates.contains(&unit.date) {
            continue;
        }
        seen_dates.push(unit.date);
        for slot in activity_repo.find_by_user_and_date(user_id, unit.date).await? {
            let mine = slot.task_id == Some(task_id);
            let named = half_days
                .iter()
                .any(|u| u.date == slot.date && u.half_day == slot.half_day);
            if mine && named && is_rebuildable(&slot) {
                delete.push(slot);
            }
        }
    }

    // Scoped to the local-day window the named half-days actually span: reading
    // this task's whole history and filtering in memory would make the page cap
    // fire on how much history the task has, rather than on how much of the
    // window we could not see.
    let (from, to) = local_window(tz, first, last);
    let filter = WorklogFilter {
        task_ids: Some(vec![task_id]),
        from: Some(from),
        to: Some(to),
        limit: WORKLOG_FILTER_MAX_LIMIT,
        offset: 0,
    };
    let entries = worklog_repo.list(user_id, &filter).await?;
    let entries = refuse_a_truncated_page(entries, "the task's affected half-days")?;

    let mut local_to_utc: std::collections::HashMap<chrono::NaiveDateTime, DateTime<Utc>> =
        std::collections::HashMap::new();
    let mut local_times = Vec::new();
    for entry in &entries {
        let local = tz.from_utc_datetime(&entry.logged_at.naive_utc()).naive_local();
        let in_scope = half_days.iter().any(|u| {
            u.date == local.date() && u.half_day == half_day_of(local.time().hour())
        });
        if !in_scope {
            continue;
        }
        local_to_utc.insert(local, entry.logged_at);
        local_times.push(local);
    }

    let mut write = Vec::new();
    for block in derive_time_blocks(&local_times) {
        // Both ends came out of `local_times`, so both are in the map. A miss would
        // mean the projection invented a timestamp, and writing a slot from an
        // invented instant is worse than writing none.
        let (Some(start), Some(raw_end)) =
            (local_to_utc.get(&block.start), local_to_utc.get(&block.end))
        else {
            continue;
        };
        let mut end = *raw_end;
        if end <= *start {
            end = *start + chrono::Duration::minutes(MIN_BLOCK_MINUTES);
        }
        write.push(ActivitySlot::from_worklog(
            user_id, task_id, None, *start, end, block.half_day, block.date, now,
        ));
    }

    Ok(RebuildPlan { task_id, delete, write })
}

/// Persist a plan: drop the stale projection, then write the fresh one.
///
/// Deletion precedes writing on purpose. The reverse order would leave a window in
/// which the half-day carries both, and a reader landing there sees doubled hours.
pub async fn apply_task_projection(
    activity_repo: &dyn ActivitySlotRepository,
    plan: &RebuildPlan,
) -> Result<(), AppError> {
    for slot in &plan.delete {
        activity_repo.delete(slot.id).await?;
    }
    for slot in &plan.write {
        activity_repo.save(slot).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;
    use uuid::Uuid;

    use crate::errors::RepositoryError;
    use crate::repositories::{ActivitySlotRepository, ConfigRepository};
    use chrono::NaiveDate;
    use domain::types::{ActivitySlot, ActivitySlotId, HalfDay};

    #[derive(Default)]
    struct FakeActivityRepo {
        slots: Mutex<Vec<ActivitySlot>>,
    }

    #[async_trait]
    impl ActivitySlotRepository for FakeActivityRepo {
        async fn find_by_id(
            &self,
            id: ActivitySlotId,
        ) -> Result<Option<ActivitySlot>, RepositoryError> {
            Ok(self.slots.lock().unwrap().iter().find(|s| s.id == id).cloned())
        }
        async fn find_by_user_and_date(
            &self,
            user_id: UserId,
            date: NaiveDate,
        ) -> Result<Vec<ActivitySlot>, RepositoryError> {
            Ok(self
                .slots
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.user_id == user_id && s.date == date)
                .cloned()
                .collect())
        }
        async fn find_active(&self, _user_id: UserId) -> Result<Option<ActivitySlot>, RepositoryError> {
            Ok(None)
        }
        async fn find_by_user_and_date_range(
            &self,
            user_id: UserId,
            start_date: NaiveDate,
            end_date: NaiveDate,
        ) -> Result<Vec<ActivitySlot>, RepositoryError> {
            Ok(self
                .slots
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.user_id == user_id && s.date >= start_date && s.date <= end_date)
                .cloned()
                .collect())
        }
        async fn save(&self, slot: &ActivitySlot) -> Result<(), RepositoryError> {
            self.slots.lock().unwrap().push(slot.clone());
            Ok(())
        }
        async fn update(&self, _slot: &ActivitySlot) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn delete(&self, id: ActivitySlotId) -> Result<(), RepositoryError> {
            self.slots.lock().unwrap().retain(|s| s.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeConfigRepo {
        map: Mutex<std::collections::HashMap<String, String>>,
    }

    #[async_trait]
    impl ConfigRepository for FakeConfigRepo {
        async fn get(&self, _user_id: UserId, key: &str) -> Result<Option<String>, RepositoryError> {
            Ok(self.map.lock().unwrap().get(key).cloned())
        }
        async fn get_all(&self, _user_id: UserId) -> Result<Vec<(String, String)>, RepositoryError> {
            Ok(self
                .map
                .lock()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }
        async fn set(
            &self,
            _user_id: UserId,
            key: &str,
            value: &str,
        ) -> Result<(), RepositoryError> {
            self.map.lock().unwrap().insert(key.to_string(), value.to_string());
            Ok(())
        }
    }

    #[tokio::test]
    async fn materialize_writes_one_slot_per_continuous_stretch() {
        use chrono::TimeZone;
        let wlog = FakeRepo::default();
        let acts = FakeActivityRepo::default();
        let cfg = FakeConfigRepo::default();
        cfg.set(Uuid::new_v4(), "aplan.timezone", "Europe/Paris").await.unwrap();
        let uid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let from = Utc.with_ymd_and_hms(2026, 6, 8, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2026, 6, 8, 23, 0, 0).unwrap();
        // 08:00 and 08:15 UTC = 10:00 and 10:15 Paris: fifteen minutes apart, so one
        // uninterrupted stretch of the same local morning.
        for (body, at) in [("a", (8, 0)), ("b", (8, 15))] {
            add_worklog_entry(
                &wlog,
                uid,
                tid,
                body.into(),
                Some(Utc.with_ymd_and_hms(2026, 6, 8, at.0, at.1, 0).unwrap()),
                from,
                None,
            )
            .await
            .unwrap();
        }

        let result =
            materialize_worklog_time(&wlog, &acts, &cfg, uid, tid, from, to).await.unwrap();

        let slots = acts.slots.lock().unwrap();
        assert_eq!(slots.len(), 1, "one morning block expected");
        assert_eq!(slots[0].half_day, HalfDay::Morning);
        assert_eq!(slots[0].date, NaiveDate::from_ymd_opt(2026, 6, 8).unwrap());
        assert_eq!(slots[0].task_id, Some(tid));
        assert!(slots[0].end_time.unwrap() > slots[0].start_time);
        assert_eq!(result.slots_written, 1);
        assert_eq!(result.active_since, to);
    }

    /// The gap rule reaches the persistence path: a half-day whose entries stop for
    /// more than [`domain::rules::worklog_time::MAX_CONTINUATION_GAP_MINUTES`] becomes
    /// two slots, and the idle stretch between them is charged to nobody.
    #[tokio::test]
    async fn materialize_writes_several_slots_when_the_work_stopped() {
        use chrono::TimeZone;
        let wlog = FakeRepo::default();
        let acts = FakeActivityRepo::default();
        let cfg = FakeConfigRepo::default();
        cfg.set(Uuid::new_v4(), "aplan.timezone", "Europe/Paris").await.unwrap();
        let uid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let from = Utc.with_ymd_and_hms(2026, 6, 8, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2026, 6, 8, 23, 0, 0).unwrap();
        // Local 10:00, 10:15 — then nothing until 11:30.
        for (body, at) in [("a", (8, 0)), ("b", (8, 15)), ("c", (9, 30))] {
            add_worklog_entry(
                &wlog,
                uid,
                tid,
                body.into(),
                Some(Utc.with_ymd_and_hms(2026, 6, 8, at.0, at.1, 0).unwrap()),
                from,
                None,
            )
            .await
            .unwrap();
        }

        let result =
            materialize_worklog_time(&wlog, &acts, &cfg, uid, tid, from, to).await.unwrap();

        let slots = acts.slots.lock().unwrap();
        assert_eq!(result.slots_written, 2);
        assert_eq!(slots.len(), 2, "the 75-minute pause is not worked time");
        assert!(slots.iter().all(|s| s.half_day == HalfDay::Morning));
        let charged: i64 = slots
            .iter()
            .filter_map(|s| s.end_time.map(|end| (end - s.start_time).num_minutes()))
            .sum();
        assert_eq!(charged, 15 + MIN_BLOCK_MINUTES, "15 min worked, then a lone entry");
    }

    #[tokio::test]
    async fn materialize_empty_window_writes_nothing() {
        let wlog = FakeRepo::default();
        let acts = FakeActivityRepo::default();
        let cfg = FakeConfigRepo::default();
        let uid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let from = now();
        let to = now() + chrono::Duration::hours(1);
        let result =
            materialize_worklog_time(&wlog, &acts, &cfg, uid, tid, from, to).await.unwrap();
        assert_eq!(result.slots_written, 0);
        assert!(acts.slots.lock().unwrap().is_empty());
    }

    #[derive(Default)]
    struct FakeRepo {
        entries: Mutex<Vec<WorklogEntry>>,
        /// Every filter `list` was called with, in order — so a test can check the
        /// query was actually bounded, not merely that its result happened to be.
        list_calls: Mutex<Vec<WorklogFilter>>,
    }

    #[async_trait]
    impl WorklogRepository for FakeRepo {
        async fn create(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
            self.entries.lock().unwrap().push(entry.clone());
            Ok(())
        }
        async fn update(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
            let mut v = self.entries.lock().unwrap();
            if let Some(slot) = v.iter_mut().find(|e| e.id == entry.id) {
                *slot = entry.clone();
            }
            Ok(())
        }
        async fn delete(
            &self,
            id: WorklogEntryId,
            user_id: UserId,
        ) -> Result<bool, RepositoryError> {
            let mut v = self.entries.lock().unwrap();
            let before = v.len();
            v.retain(|e| !(e.id == id && e.user_id == user_id));
            Ok(v.len() < before)
        }
        async fn find_by_id(
            &self,
            id: WorklogEntryId,
            user_id: UserId,
        ) -> Result<Option<WorklogEntry>, RepositoryError> {
            Ok(self
                .entries
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.id == id && e.user_id == user_id)
                .cloned())
        }
        async fn find_by_recurrence(
            &self,
            _user_id: UserId,
            _template_id: domain::types::recurrence::RecurrenceTemplateId,
            _limit: u32,
            _offset: u32,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            Ok(vec![])
        }
        async fn list(
            &self,
            user_id: UserId,
            filter: &WorklogFilter,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            self.list_calls.lock().unwrap().push(filter.clone());
            let v = self.entries.lock().unwrap();
            let mut out: Vec<WorklogEntry> = v
                .iter()
                .filter(|e| e.user_id == user_id)
                .filter(|e| match &filter.task_ids {
                    Some(ids) => ids.contains(&e.task_id),
                    None => true,
                })
                .filter(|e| match filter.from {
                    Some(f) => e.logged_at >= f,
                    None => true,
                })
                .filter(|e| match filter.to {
                    Some(t) => e.logged_at < t,
                    None => true,
                })
                .cloned()
                .collect();
            out.sort_by(|a, b| b.logged_at.cmp(&a.logged_at));
            let start = filter.offset as usize;
            let end = (start + filter.limit as usize).min(out.len());
            if start >= out.len() {
                Ok(vec![])
            } else {
                Ok(out[start..end].to_vec())
            }
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-04-21T10:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[tokio::test]
    async fn add_uses_now_when_logged_at_is_none() {
        let repo = FakeRepo::default();
        let uid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let entry = add_worklog_entry(&repo, uid, tid, "x".into(), None, now(), None)
            .await
            .unwrap();
        assert_eq!(entry.logged_at, now());
    }

    #[tokio::test]
    async fn add_uses_override_when_logged_at_is_some() {
        let repo = FakeRepo::default();
        let uid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let earlier = DateTime::parse_from_rfc3339("2026-04-20T08:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let entry = add_worklog_entry(&repo, uid, tid, "x".into(), Some(earlier), now(), None)
            .await
            .unwrap();
        assert_eq!(entry.logged_at, earlier);
    }

    #[tokio::test]
    async fn update_rejects_other_users_entry() {
        let repo = FakeRepo::default();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let entry = add_worklog_entry(&repo, a, tid, "orig".into(), None, now(), None)
            .await
            .unwrap();
        let err = update_worklog_entry(&repo, b, entry.id, Some("hax".into()), None, now())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn update_changes_body_and_touches_updated_at() {
        let repo = FakeRepo::default();
        let uid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let entry = add_worklog_entry(&repo, uid, tid, "v1".into(), None, now(), None)
            .await
            .unwrap();
        let later = now() + chrono::Duration::seconds(30);
        let updated =
            update_worklog_entry(&repo, uid, entry.id, Some("v2".into()), None, later)
                .await
                .unwrap();
        assert_eq!(updated.body, "v2");
        assert_eq!(updated.updated_at, later);
    }

    #[tokio::test]
    async fn delete_removes_owned_entry_and_ignores_others() {
        let repo = FakeRepo::default();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let entry = add_worklog_entry(&repo, a, tid, "x".into(), None, now(), None)
            .await
            .unwrap();
        assert!(!delete_worklog_entry(&repo, b, entry.id).await.unwrap());
        assert!(delete_worklog_entry(&repo, a, entry.id).await.unwrap());
    }

    #[tokio::test]
    async fn list_clamps_limit_to_default_when_zero() {
        let repo = FakeRepo::default();
        let uid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        add_worklog_entry(&repo, uid, tid, "one".into(), None, now(), None)
            .await
            .unwrap();
        let out = list_worklog_entries(&repo, uid, WorklogFilter::default())
            .await
            .unwrap();
        assert_eq!(out.len(), 1);
    }

    // ─── `plan_task_projection` / `apply_task_projection` ───────────────────────
    //
    // Fixed ids and a fixed calendar day, so two calls in the same test refer to
    // the same entity without randomness threading them through — the same
    // convention `slot_classification`'s test module keeps in parallel.

    fn user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn task_id() -> TaskId {
        Uuid::parse_str("00000000-0000-0000-0000-0000000000aa").unwrap()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// August, so `Europe/Paris` (the default when no `aplan.timezone` config is
    /// set) is UTC+2: a UTC hour here reads two hours later locally.
    fn t(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, h, m, 0).unwrap()
    }

    /// The three fakes, with `logged_ats` stored as entries on one task. No
    /// timezone is set, so the plan is computed against the default
    /// `Europe/Paris`.
    async fn fakes_with_entries(
        logged_ats: &[DateTime<Utc>],
    ) -> (FakeActivityRepo, FakeRepo, FakeConfigRepo) {
        let activity = FakeActivityRepo::default();
        let worklog = FakeRepo::default();
        let config = FakeConfigRepo::default();
        for logged_at in logged_ats {
            let entry =
                WorklogEntry::new(user_id(), task_id(), "x".into(), *logged_at, *logged_at)
                    .unwrap();
            worklog.create(&entry).await.unwrap();
        }
        (activity, worklog, config)
    }

    fn half_day(date: &str, hd: HalfDay) -> AffectedHalfDay {
        AffectedHalfDay {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            half_day: hd,
        }
    }

    /// The plan deletes only what the projection owns and rewrites from the entries.
    #[tokio::test]
    async fn the_plan_replaces_a_worklog_slot_and_spares_a_manual_one() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(7, 30)]).await;

        let mine = ActivitySlot::from_worklog(
            user_id(), task_id(), None, t(7, 0), t(7, 30),
            HalfDay::Morning, date(2026, 8, 4), t(9, 0),
        );
        let hand_made = ActivitySlot::manual(
            user_id(), Some(task_id()), t(10, 0), Some(t(11, 0)),
            HalfDay::Morning, date(2026, 8, 4), t(11, 0),
        );
        activity.save(&mine).await.unwrap();
        activity.save(&hand_made).await.unwrap();

        let tz = user_timezone(&config, user_id()).await.unwrap();
        let plan = plan_task_projection(
            &activity, &worklog, user_id(), task_id(),
            &[half_day("2026-08-04", HalfDay::Morning)], tz, t(12, 0),
        ).await.unwrap();

        let deleted: Vec<_> = plan.delete.iter().map(|s| s.id).collect();
        assert!(deleted.contains(&mine.id), "the projection's own slot is replaced");
        assert!(!deleted.contains(&hand_made.id), "a hand-made slot is never deleted");
        assert_eq!(plan.write.len(), 1, "the two entries are one stretch of work");
        assert_eq!(plan.write[0].source, SlotSource::Worklog);
    }

    /// Applying twice leaves the same slots — the property the flush needs.
    #[tokio::test]
    async fn applying_the_plan_twice_is_idempotent() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(7, 30)]).await;
        let tz = user_timezone(&config, user_id()).await.unwrap();
        let units = [half_day("2026-08-04", HalfDay::Morning)];

        for _ in 0..2 {
            let plan = plan_task_projection(
                &activity, &worklog, user_id(), task_id(), &units, tz, t(12, 0),
            ).await.unwrap();
            apply_task_projection(&activity, &plan).await.unwrap();
        }

        let slots = activity
            .find_by_user_and_date(user_id(), date(2026, 8, 4))
            .await
            .unwrap();
        assert_eq!(slots.len(), 1, "the second apply replaced rather than appended");
    }

    /// One task's rebuild never reads or writes another task's slots.
    #[tokio::test]
    async fn the_plan_leaves_another_tasks_slots_alone() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(7, 30)]).await;
        let other = Uuid::new_v4();
        let theirs = ActivitySlot::from_worklog(
            user_id(), other, None, t(7, 0), t(7, 30),
            HalfDay::Morning, date(2026, 8, 4), t(9, 0),
        );
        activity.save(&theirs).await.unwrap();

        let tz = user_timezone(&config, user_id()).await.unwrap();
        let plan = plan_task_projection(
            &activity, &worklog, user_id(), task_id(),
            &[half_day("2026-08-04", HalfDay::Morning)], tz, t(12, 0),
        ).await.unwrap();

        assert!(plan.delete.iter().all(|s| s.task_id == Some(task_id())));
        assert!(plan.write.iter().all(|s| s.task_id == Some(task_id())));
    }

    /// A half-day the caller did not name is not touched, even for the same task.
    #[tokio::test]
    async fn the_plan_is_scoped_to_the_named_half_days() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(14, 0)]).await;
        let morning_slot = ActivitySlot::from_worklog(
            user_id(), task_id(), None, t(14, 0), t(14, 1),
            HalfDay::Afternoon, date(2026, 8, 4), t(15, 0),
        );
        activity.save(&morning_slot).await.unwrap();

        let tz = user_timezone(&config, user_id()).await.unwrap();
        let plan = plan_task_projection(
            &activity, &worklog, user_id(), task_id(),
            &[half_day("2026-08-04", HalfDay::Morning)], tz, t(16, 0),
        ).await.unwrap();

        assert!(
            plan.delete.is_empty(),
            "the afternoon slot is outside the named half-day"
        );
        assert!(plan.write.iter().all(|s| s.half_day == HalfDay::Morning));
    }

    /// The worklog read is scoped to the window the named half-days span, not to
    /// the task's whole history: a full page would otherwise mean "this task has a
    /// long history" rather than "this window may have been cut", and a task with
    /// enough lifetime entries would have every reattribution and every flush
    /// refuse regardless of how small the day actually being rebuilt is.
    #[tokio::test]
    async fn the_plan_reads_a_window_not_the_tasks_whole_history() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(7, 30)]).await;
        // A day the plan never named, months away — the entry an unbounded read
        // would still have to fetch and then discard.
        let stray_at = Utc.with_ymd_and_hms(2026, 9, 3, 7, 0, 0).unwrap();
        let stray = WorklogEntry::new(user_id(), task_id(), "x".into(), stray_at, stray_at)
            .unwrap();
        worklog.create(&stray).await.unwrap();

        let tz = user_timezone(&config, user_id()).await.unwrap();
        let plan = plan_task_projection(
            &activity, &worklog, user_id(), task_id(),
            &[half_day("2026-08-04", HalfDay::Morning)], tz, t(12, 0),
        ).await.unwrap();

        assert!(
            plan.write.iter().all(|s| s.date == date(2026, 8, 4)),
            "an entry from a day the plan never named must not appear in the rebuild"
        );

        let bounded = worklog
            .list_calls
            .lock()
            .unwrap()
            .iter()
            .any(|f| f.from.is_some() && f.to.is_some());
        assert!(
            bounded,
            "the read itself must be scoped to the named half-days' window, not \
             merely filtered afterwards"
        );
    }

    // ─── `materialize_worklog_time` rewired onto the rebuild ─────────────────────
    //
    // `from` is now a selector, not a watermark: these three exercise exactly the
    // properties a watermark-and-append implementation could not have.

    /// Two flushes over the same window produce one set of slots. This is the
    /// property the old append-with-a-watermark implementation could not have.
    #[tokio::test]
    async fn flushing_twice_does_not_double_the_half_day() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(7, 30)]).await;

        for _ in 0..2 {
            materialize_worklog_time(
                &worklog, &activity, &config, user_id(), task_id(), t(6, 0), t(12, 0),
            ).await.unwrap();
        }

        let slots = activity.find_by_user_and_date(user_id(), date(2026, 8, 4)).await.unwrap();
        assert_eq!(slots.len(), 1);
    }

    /// An entry logged with a past `logged_at` — under any watermark the caller might
    /// pass — still reaches the projection, because membership is by half-day.
    #[tokio::test]
    async fn a_backdated_entry_is_still_materialized() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(7, 30)]).await;

        materialize_worklog_time(
            &worklog, &activity, &config, user_id(), task_id(),
            t(7, 15), // a window that starts *after* the first entry
            t(12, 0),
        ).await.unwrap();

        let slots = activity.find_by_user_and_date(user_id(), date(2026, 8, 4)).await.unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].start_time, t(7, 0), "the earlier entry set the boundary");
    }

    /// Flushing one task neither reads nor writes another task's slots — the whole
    /// point of the plan: two sessions on two tasks stop losing each other's hours.
    #[tokio::test]
    async fn flushing_one_task_leaves_another_intact() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(7, 30)]).await;
        let other = Uuid::new_v4();
        let theirs = ActivitySlot::from_worklog(
            user_id(), other, None, t(8, 0), t(8, 30),
            HalfDay::Morning, date(2026, 8, 4), t(9, 0),
        );
        activity.save(&theirs).await.unwrap();

        materialize_worklog_time(
            &worklog, &activity, &config, user_id(), task_id(), t(6, 0), t(12, 0),
        ).await.unwrap();

        let still_there = activity.find_by_id(theirs.id).await.unwrap();
        assert!(still_there.is_some(), "another task's slot survives our flush");
    }

    /// `from` is a selector, not a watermark — but it still bounds *something*: a
    /// half-day entirely before it must be left alone. Without this, relaxing the
    /// selector to `from: None` on "the rebuild is idempotent anyway" reasoning
    /// would make every flush rebuild a task's entire history, and no other test
    /// here would go red.
    #[tokio::test]
    async fn a_half_day_before_the_selector_window_is_left_untouched() {
        // Two half-days back from the current one (2026-08-04 Morning): 2026-08-03
        // Morning, with 2026-08-03 Afternoon in between.
        let old_at = Utc.with_ymd_and_hms(2026, 8, 3, 7, 0, 0).unwrap();
        let (activity, worklog, config) =
            fakes_with_entries(&[old_at, t(7, 0), t(7, 30)]).await;

        let stale = ActivitySlot::from_worklog(
            user_id(), task_id(), None,
            old_at, old_at + chrono::Duration::minutes(MIN_BLOCK_MINUTES),
            HalfDay::Morning, date(2026, 8, 3), old_at,
        );
        activity.save(&stale).await.unwrap();

        // `t(6, 0)` (2026-08-04 06:00 UTC) lands well after `old_at`'s entire local
        // half-day, so the selector must never surface it.
        materialize_worklog_time(
            &worklog, &activity, &config, user_id(), task_id(), t(6, 0), t(12, 0),
        ).await.unwrap();

        let untouched = activity.find_by_user_and_date(user_id(), date(2026, 8, 3)).await.unwrap();
        assert_eq!(untouched.len(), 1, "a half-day before `from` must not be rebuilt");
        assert_eq!(untouched[0].id, stale.id);
        assert_eq!(untouched[0].start_time, stale.start_time);
    }

    /// The selector read only needs the *set of distinct half-days* a task
    /// touched, not every entry — so a task whose window holds more than
    /// `WORKLOG_FILTER_MAX_LIMIT` entries must still flush, by paging through
    /// `offset` instead of refusing. Before this, the read's own remedy
    /// ("narrow the range") named a parameter `flushWorklogTime` has no way to
    /// pass, so such a task could never flush at all.
    ///
    /// Three half-days sized so no single one ever reaches
    /// `plan_task_projection`'s own per-half-day cap (500, 500, 5), yet their sum
    /// exceeds the page cap and the oldest half-day is entirely beyond the
    /// selector's first page.
    #[tokio::test]
    async fn a_task_past_the_selector_page_cap_still_flushes_and_finds_every_half_day() {
        use chrono::Duration;

        let newest = Utc.with_ymd_and_hms(2026, 8, 5, 7, 0, 0).unwrap();
        let middle = Utc.with_ymd_and_hms(2026, 8, 4, 7, 0, 0).unwrap();
        let oldest = Utc.with_ymd_and_hms(2026, 8, 3, 7, 0, 0).unwrap();

        let mut logged_ats = Vec::new();
        for i in 0..500i64 {
            logged_ats.push(newest + Duration::seconds(i));
        }
        for i in 0..500i64 {
            logged_ats.push(middle + Duration::seconds(i));
        }
        for i in 0..5i64 {
            logged_ats.push(oldest + Duration::seconds(i));
        }
        assert!(logged_ats.len() as u32 > WORKLOG_FILTER_MAX_LIMIT);

        let (activity, worklog, config) = fakes_with_entries(&logged_ats).await;

        let from = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 8, 6, 0, 0, 0).unwrap();
        let outcome = materialize_worklog_time(
            &worklog, &activity, &config, user_id(), task_id(), from, now,
        )
        .await
        .unwrap();

        assert_eq!(outcome.slots_written, 3, "each of the three half-days is one stretch");
        for d in [date(2026, 8, 3), date(2026, 8, 4), date(2026, 8, 5)] {
            assert!(
                !activity.find_by_user_and_date(user_id(), d).await.unwrap().is_empty(),
                "half-day {d} must have been found and materialized"
            );
        }
    }
}
