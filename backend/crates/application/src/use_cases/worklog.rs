use chrono::{DateTime, TimeZone, Utc};
use domain::rules::worklog_time::derive_time_blocks;
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::{
    ActivitySlotRepository, ConfigRepository, WorklogFilter, WorklogRepository,
    WORKLOG_FILTER_DEFAULT_LIMIT, WORKLOG_FILTER_MAX_LIMIT,
};

/// Add a new worklog entry. `logged_at` defaults to `now` when `None`.
pub async fn add_worklog_entry(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    task_id: TaskId,
    body: String,
    logged_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<WorklogEntry, AppError> {
    let entry = WorklogEntry::new(user_id, task_id, body, logged_at.unwrap_or(now), now)?;
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

/// Outcome of a flush: how many slots were written and the new watermark.
pub struct FlushOutcome {
    pub slots_written: u32,
    pub active_since: DateTime<Utc>,
}

const DEFAULT_TZ: &str = "Europe/Paris";

/// Materialize worklog entries logged in `[from, now]` for `task_id` into closed
/// activity slots, one per (local day, half-day). Returns new watermark + count.
pub async fn materialize_worklog_time(
    worklog_repo: &dyn WorklogRepository,
    activity_repo: &dyn ActivitySlotRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    task_id: TaskId,
    from: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<FlushOutcome, AppError> {
    let tz: chrono_tz::Tz = config_repo
        .get(user_id, "aplan.timezone")
        .await?
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| DEFAULT_TZ.parse().expect("default tz parses"));

    let filter = WorklogFilter {
        task_ids: Some(vec![task_id]),
        from: Some(from),
        to: Some(now),
        limit: WORKLOG_FILTER_MAX_LIMIT,
        offset: 0,
    };
    let entries = worklog_repo.list(user_id, &filter).await?;

    let mut local_to_utc: std::collections::HashMap<chrono::NaiveDateTime, DateTime<Utc>> =
        std::collections::HashMap::new();
    let mut local_times = Vec::with_capacity(entries.len());
    for e in &entries {
        let local = tz.from_utc_datetime(&e.logged_at.naive_utc()).naive_local();
        local_to_utc.insert(local, e.logged_at);
        local_times.push(local);
    }

    let blocks = derive_time_blocks(&local_times);
    let mut written = 0u32;
    for block in blocks {
        let start_utc = local_to_utc[&block.start];
        let mut end_utc = local_to_utc[&block.end];
        if end_utc <= start_utc {
            end_utc = start_utc + chrono::Duration::minutes(1);
        }
        let slot = ActivitySlot {
            id: Uuid::new_v4(),
            user_id,
            task_id: Some(task_id),
            start_time: start_utc,
            end_time: Some(end_utc),
            half_day: block.half_day,
            date: block.date,
            created_at: Utc::now(),
        };
        activity_repo.save(&slot).await?;
        written += 1;
    }

    Ok(FlushOutcome { slots_written: written, active_since: now })
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
        async fn delete(&self, _id: ActivitySlotId) -> Result<(), RepositoryError> {
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
    async fn materialize_writes_one_local_slot_per_half_day() {
        use chrono::TimeZone;
        let wlog = FakeRepo::default();
        let acts = FakeActivityRepo::default();
        let cfg = FakeConfigRepo::default();
        cfg.set(Uuid::new_v4(), "aplan.timezone", "Europe/Paris").await.unwrap();
        let uid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let from = Utc.with_ymd_and_hms(2026, 6, 8, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2026, 6, 8, 23, 0, 0).unwrap();
        add_worklog_entry(
            &wlog,
            uid,
            tid,
            "a".into(),
            Some(Utc.with_ymd_and_hms(2026, 6, 8, 8, 0, 0).unwrap()),
            from,
        )
        .await
        .unwrap();
        add_worklog_entry(
            &wlog,
            uid,
            tid,
            "b".into(),
            Some(Utc.with_ymd_and_hms(2026, 6, 8, 9, 30, 0).unwrap()),
            from,
        )
        .await
        .unwrap();

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
        let entry = add_worklog_entry(&repo, uid, tid, "x".into(), None, now())
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
        let entry = add_worklog_entry(&repo, uid, tid, "x".into(), Some(earlier), now())
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
        let entry = add_worklog_entry(&repo, a, tid, "orig".into(), None, now())
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
        let entry = add_worklog_entry(&repo, uid, tid, "v1".into(), None, now())
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
        let entry = add_worklog_entry(&repo, a, tid, "x".into(), None, now())
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
        add_worklog_entry(&repo, uid, tid, "one".into(), None, now())
            .await
            .unwrap();
        let out = list_worklog_entries(&repo, uid, WorklogFilter::default())
            .await
            .unwrap();
        assert_eq!(out.len(), 1);
    }
}
