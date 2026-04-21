use chrono::{DateTime, Utc};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::{
    WorklogFilter, WorklogRepository, WORKLOG_FILTER_DEFAULT_LIMIT, WORKLOG_FILTER_MAX_LIMIT,
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;
    use uuid::Uuid;

    use crate::errors::RepositoryError;

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
