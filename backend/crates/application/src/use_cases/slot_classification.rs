use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use domain::rules::worklog_time::MIN_BLOCK_MINUTES;
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::{
    ActivitySlotRepository, ConfigRepository, WorklogFilter, WorklogRepository,
    WORKLOG_FILTER_MAX_LIMIT,
};
use crate::use_cases::configuration;

/// Set once the pass has run, so a restart does not redo it.
pub const CLASSIFIED_KEY: &str = "aplan.slot_source_classified";

pub struct ClassificationOutcome {
    pub worklog: u32,
    pub manual: u32,
    /// True when the guard key was already set and nothing was read or written.
    pub skipped: bool,
}

/// Give every pre-014 slot the provenance the data says it has.
///
/// A closed slot with a task came from a flush **iff its `start_time` is some
/// entry's own `logged_at`, and its `end_time` is either another entry's
/// `logged_at` or exactly `start_time + `[`MIN_BLOCK_MINUTES`]** — because the flush
/// copies an entry's `logged_at` verbatim into a slot boundary, and the domain owns
/// the minimum duration a single-timestamp block persists as. This tests boundary
/// *provenance* rather than re-deriving a day's grouping: the gap-splitting rule in
/// `derive_time_blocks` changed which entries become boundaries (`abda52a`,
/// 2026-08-04) without ever changing that a boundary is an entry's timestamp, so
/// comparing against a fresh whole-day recomputation cannot reproduce spans an
/// older, or incrementally-windowed, flush actually wrote — this comparison can.
///
/// Everything else is `Manual`: an open slot (a running timer), a slot with no task,
/// a start that matches no entry, or a start that matches while the end matches
/// neither an entry nor the one-minute minimum — a fragment of a grouping that no
/// longer applies. Erring toward `Manual` errs toward not rebuilding, which loses no
/// time — the opposite error deletes hours.
pub async fn classify_slot_sources(
    activity_repo: &dyn ActivitySlotRepository,
    worklog_repo: &dyn WorklogRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    from: NaiveDate,
    to: NaiveDate,
    now: DateTime<Utc>,
) -> Result<ClassificationOutcome, AppError> {
    if config_repo.get(user_id, CLASSIFIED_KEY).await?.is_some() {
        return Ok(ClassificationOutcome {
            worklog: 0,
            manual: 0,
            skipped: true,
        });
    }

    let slots = activity_repo
        .find_by_user_and_date_range(user_id, from, to)
        .await?;

    // One entry read per task rather than per slot: several slots on one task
    // would otherwise re-fetch the same entries once per slot.
    let mut timestamps_cache: HashMap<TaskId, HashSet<DateTime<Utc>>> = HashMap::new();
    let mut worklog_ids: Vec<ActivitySlotId> = Vec::new();
    let mut manual_ids: Vec<ActivitySlotId> = Vec::new();

    for slot in &slots {
        let (task_id, end_time) = match (slot.task_id, slot.end_time) {
            (Some(task_id), Some(end_time)) => (task_id, end_time),
            _ => {
                manual_ids.push(slot.id);
                continue;
            }
        };

        if !timestamps_cache.contains_key(&task_id) {
            let stamps = entry_timestamps(worklog_repo, user_id, task_id).await?;
            timestamps_cache.insert(task_id, stamps);
        }
        let stamps = &timestamps_cache[&task_id];

        let start_is_an_entry = stamps.contains(&slot.start_time);
        let end_is_an_entry_or_the_minimum = stamps.contains(&end_time)
            || end_time == slot.start_time + Duration::minutes(MIN_BLOCK_MINUTES);

        if start_is_an_entry && end_is_an_entry_or_the_minimum {
            worklog_ids.push(slot.id);
        } else {
            manual_ids.push(slot.id);
        }
    }

    activity_repo
        .set_source(&worklog_ids, SlotSource::Worklog)
        .await?;
    activity_repo
        .set_source(&manual_ids, SlotSource::Manual)
        .await?;

    configuration::set_config(config_repo, user_id, CLASSIFIED_KEY, &now.to_rfc3339()).await?;

    Ok(ClassificationOutcome {
        worklog: worklog_ids.len() as u32,
        manual: manual_ids.len() as u32,
        skipped: false,
    })
}

/// Every `logged_at` a task's worklog entries carry, for exact boundary matching.
async fn entry_timestamps(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    task_id: TaskId,
) -> Result<HashSet<DateTime<Utc>>, AppError> {
    let filter = WorklogFilter {
        task_ids: Some(vec![task_id]),
        from: None,
        to: None,
        limit: WORKLOG_FILTER_MAX_LIMIT,
        offset: 0,
    };
    let entries = worklog_repo.list(user_id, &filter).await?;
    Ok(entries.into_iter().map(|e| e.logged_at).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;
    use uuid::Uuid;

    use crate::errors::RepositoryError;
    use crate::repositories::{
        ActivitySlotRepository, ConfigRepository, WorklogFilter, WorklogRepository,
    };
    use chrono::TimeZone;
    use domain::types::{ActivitySlot, ActivitySlotId, HalfDay, WorklogEntry};

    #[derive(Default)]
    struct FakeActivityRepo {
        slots: Mutex<Vec<ActivitySlot>>,
        sources: Mutex<HashMap<ActivitySlotId, SlotSource>>,
    }

    impl FakeActivityRepo {
        fn recorded_source(&self, id: ActivitySlotId) -> Option<SlotSource> {
            self.sources.lock().unwrap().get(&id).copied()
        }
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
        async fn delete(&self, _id: ActivitySlotId) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn set_source(
            &self,
            ids: &[ActivitySlotId],
            source: SlotSource,
        ) -> Result<u64, RepositoryError> {
            let mut sources = self.sources.lock().unwrap();
            for id in ids {
                sources.insert(*id, source);
            }
            Ok(ids.len() as u64)
        }
    }

    #[derive(Default)]
    struct FakeConfigRepo {
        map: Mutex<HashMap<String, String>>,
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

    #[derive(Default)]
    struct FakeWorklogRepo {
        entries: Mutex<Vec<WorklogEntry>>,
    }

    #[async_trait]
    impl WorklogRepository for FakeWorklogRepo {
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

    fn user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn task_id() -> TaskId {
        Uuid::parse_str("00000000-0000-0000-0000-0000000000aa").unwrap()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn t(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, h, m, 0).unwrap()
    }

    /// The three fakes, with `logged_ats` stored as entries on one task. No
    /// timezone is set: exact-instant matching needs none.
    async fn fakes_with_entries(
        logged_ats: &[DateTime<Utc>],
    ) -> (FakeActivityRepo, FakeWorklogRepo, FakeConfigRepo) {
        let activity = FakeActivityRepo::default();
        let worklog = FakeWorklogRepo::default();
        let config = FakeConfigRepo::default();
        for logged_at in logged_ats {
            let entry =
                WorklogEntry::new(user_id(), task_id(), "x".into(), *logged_at, *logged_at)
                    .unwrap();
            worklog.create(&entry).await.unwrap();
        }
        (activity, worklog, config)
    }

    /// A closed slot on `task_id()`, carrying `SlotSource::Manual` — the NULL reading
    /// migration 014 leaves on every pre-existing row.
    fn closed_slot(start: DateTime<Utc>, end: DateTime<Utc>) -> ActivitySlot {
        ActivitySlot {
            id: Uuid::new_v4(),
            user_id: user_id(),
            task_id: Some(task_id()),
            start_time: start,
            end_time: Some(end),
            half_day: HalfDay::Morning,
            date: start.date_naive(),
            created_at: end,
            session_id: None,
            source: SlotSource::Manual,
        }
    }

    #[tokio::test]
    async fn a_slot_whose_both_ends_are_entries_is_classified_worklog() {
        // The flush copies an entry's `logged_at` verbatim into a slot boundary —
        // both ends being entries of the same task is exactly that signature.
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(9, 0)]).await;
        let slot = closed_slot(t(7, 0), t(9, 0));
        activity.save(&slot).await.unwrap();

        let outcome = classify_slot_sources(
            &activity, &worklog, &config, user_id(),
            date(2026, 8, 1), date(2026, 8, 31), t(12, 0),
        )
        .await
        .unwrap();

        assert_eq!(outcome.worklog, 1);
        assert_eq!(outcome.manual, 0);
        assert_eq!(activity.recorded_source(slot.id), Some(SlotSource::Worklog));
    }

    #[tokio::test]
    async fn a_single_entry_block_matches_its_minimum_duration() {
        // A lone entry's block has start == end, which the flush persists as
        // MIN_BLOCK_MINUTES rather than a zero-length span.
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0)]).await;
        let slot = closed_slot(t(7, 0), t(7, 0) + Duration::minutes(MIN_BLOCK_MINUTES));
        activity.save(&slot).await.unwrap();

        let outcome = classify_slot_sources(
            &activity, &worklog, &config, user_id(),
            date(2026, 8, 1), date(2026, 8, 31), t(12, 0),
        )
        .await
        .unwrap();

        assert_eq!(outcome.worklog, 1);
        assert_eq!(outcome.manual, 0);
    }

    #[tokio::test]
    async fn a_slot_whose_start_matches_no_entry_is_manual() {
        // The only entry is at the slot's end, not its start — a hand-made slot
        // could easily land on a timestamp that happens to equal a real entry.
        let (activity, worklog, config) = fakes_with_entries(&[t(9, 0)]).await;
        let slot = closed_slot(t(7, 0), t(9, 0));
        activity.save(&slot).await.unwrap();

        let outcome = classify_slot_sources(
            &activity, &worklog, &config, user_id(),
            date(2026, 8, 1), date(2026, 8, 31), t(12, 0),
        )
        .await
        .unwrap();

        assert_eq!(outcome.worklog, 0);
        assert_eq!(outcome.manual, 1);
        assert_eq!(activity.recorded_source(slot.id), Some(SlotSource::Manual));
    }

    #[tokio::test]
    async fn a_slot_with_no_task_is_manual_without_consulting_any_entry() {
        let (activity, worklog, config) = fakes_with_entries(&[]).await;
        let mut slot = closed_slot(t(7, 0), t(9, 0));
        slot.task_id = None;
        activity.save(&slot).await.unwrap();

        let outcome = classify_slot_sources(
            &activity, &worklog, &config, user_id(),
            date(2026, 8, 1), date(2026, 8, 31), t(12, 0),
        )
        .await
        .unwrap();

        assert_eq!(outcome.manual, 1);
    }

    #[tokio::test]
    async fn a_fragment_whose_start_matches_but_whose_end_does_not_is_manual() {
        // The signature a pre-`abda52a` grouping (or an incrementally-windowed
        // flush) could leave behind: the start is a real entry, but the end is
        // neither another entry nor the one-minute minimum — a boundary the
        // current rule cannot vouch for. It must not read as worklog.
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(9, 0)]).await;
        let slot = closed_slot(t(7, 0), t(7, 0) + Duration::minutes(10));
        activity.save(&slot).await.unwrap();

        let outcome = classify_slot_sources(
            &activity, &worklog, &config, user_id(),
            date(2026, 8, 1), date(2026, 8, 31), t(12, 0),
        )
        .await
        .unwrap();

        assert_eq!(outcome.worklog, 0);
        assert_eq!(outcome.manual, 1);
        assert_eq!(activity.recorded_source(slot.id), Some(SlotSource::Manual));
    }

    #[tokio::test]
    async fn the_pass_is_skipped_once_the_guard_key_is_set() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(9, 0)]).await;
        activity.save(&closed_slot(t(7, 0), t(9, 0))).await.unwrap();

        let first = classify_slot_sources(
            &activity, &worklog, &config, user_id(),
            date(2026, 8, 1), date(2026, 8, 31), t(12, 0),
        )
        .await
        .unwrap();
        assert!(!first.skipped);

        let second = classify_slot_sources(
            &activity, &worklog, &config, user_id(),
            date(2026, 8, 1), date(2026, 8, 31), t(13, 0),
        )
        .await
        .unwrap();

        assert!(second.skipped, "a restart must not re-classify");
        assert_eq!(second.worklog, 0);
        assert_eq!(second.manual, 0);
    }
}
