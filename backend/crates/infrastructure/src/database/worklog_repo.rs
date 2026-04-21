use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::{WorklogFilter, WorklogRepository};
use domain::types::*;

pub struct SqliteWorklogRepository {
    pool: SqlitePool,
}

impl SqliteWorklogRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RepositoryError::Database(format!("Failed to parse datetime '{s}': {e}")))
}

fn map_row(row: &SqliteRow) -> Result<WorklogEntry, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let user_id_str: String = Row::get(row, "user_id");
    let task_id_str: String = Row::get(row, "task_id");
    let body: String = Row::get(row, "body");
    let logged_at_str: String = Row::get(row, "logged_at");
    let created_at_str: String = Row::get(row, "created_at");
    let updated_at_str: String = Row::get(row, "updated_at");

    Ok(WorklogEntry {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        task_id: Uuid::parse_str(&task_id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        body,
        logged_at: parse_datetime(&logged_at_str)?,
        created_at: parse_datetime(&created_at_str)?,
        updated_at: parse_datetime(&updated_at_str)?,
    })
}

#[async_trait]
impl WorklogRepository for SqliteWorklogRepository {
    async fn create(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO worklog_entries (id, user_id, task_id, body, logged_at, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(entry.id.to_string())
        .bind(entry.user_id.to_string())
        .bind(entry.task_id.to_string())
        .bind(&entry.body)
        .bind(entry.logged_at.to_rfc3339())
        .bind(entry.created_at.to_rfc3339())
        .bind(entry.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn update(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE worklog_entries
                SET body = ?, logged_at = ?, updated_at = ?
              WHERE id = ? AND user_id = ?",
        )
        .bind(&entry.body)
        .bind(entry.logged_at.to_rfc3339())
        .bind(entry.updated_at.to_rfc3339())
        .bind(entry.id.to_string())
        .bind(entry.user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(
        &self,
        id: WorklogEntryId,
        user_id: UserId,
    ) -> Result<bool, RepositoryError> {
        let result = sqlx::query("DELETE FROM worklog_entries WHERE id = ? AND user_id = ?")
            .bind(id.to_string())
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_by_id(
        &self,
        id: WorklogEntryId,
        user_id: UserId,
    ) -> Result<Option<WorklogEntry>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM worklog_entries WHERE id = ? AND user_id = ? LIMIT 1",
        )
        .bind(id.to_string())
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => Ok(Some(map_row(row)?)),
            None => Ok(None),
        }
    }

    async fn list(
        &self,
        user_id: UserId,
        filter: &WorklogFilter,
    ) -> Result<Vec<WorklogEntry>, RepositoryError> {
        let mut sql = String::from("SELECT * FROM worklog_entries WHERE user_id = ?");
        if let Some(ids) = &filter.task_ids {
            if ids.is_empty() {
                return Ok(vec![]);
            }
            sql.push_str(" AND task_id IN (");
            sql.push_str(&vec!["?"; ids.len()].join(","));
            sql.push(')');
        }
        if filter.from.is_some() {
            sql.push_str(" AND logged_at >= ?");
        }
        if filter.to.is_some() {
            sql.push_str(" AND logged_at < ?");
        }
        sql.push_str(" ORDER BY logged_at DESC, created_at DESC LIMIT ? OFFSET ?");

        let mut q = sqlx::query(&sql).bind(user_id.to_string());
        if let Some(ids) = &filter.task_ids {
            for tid in ids {
                q = q.bind(tid.to_string());
            }
        }
        if let Some(from) = filter.from {
            q = q.bind(from.to_rfc3339());
        }
        if let Some(to) = filter.to {
            q = q.bind(to.to_rfc3339());
        }
        let rows = q
            .bind(filter.limit as i64)
            .bind(filter.offset as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(map_row).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::connection::create_sqlite_pool;

    const USER_ID: &str = "00000000-0000-0000-0000-000000000001";
    const TASK_ID: &str = "11111111-1111-1111-1111-111111111111";

    async fn setup() -> SqlitePool {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        sqlx::query(
            "INSERT INTO tasks (id, user_id, title, source, status, impact, urgency, created_at, updated_at, tracking_state)
             VALUES (?, ?, 'T', 'personal', 'todo', 1, 1, ?, ?, 'followed')",
        )
        .bind(TASK_ID)
        .bind(USER_ID)
        .bind("2026-04-21T00:00:00+00:00")
        .bind("2026-04-21T00:00:00+00:00")
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn uid() -> Uuid {
        Uuid::parse_str(USER_ID).unwrap()
    }
    fn tid() -> Uuid {
        Uuid::parse_str(TASK_ID).unwrap()
    }
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-04-21T10:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[tokio::test]
    async fn create_then_find_by_id_roundtrips() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        let entry = WorklogEntry::new(uid(), tid(), "hello".into(), now(), now()).unwrap();
        repo.create(&entry).await.unwrap();
        let found = repo.find_by_id(entry.id, uid()).await.unwrap().unwrap();
        assert_eq!(found.id, entry.id);
        assert_eq!(found.body, "hello");
        assert_eq!(found.logged_at, now());
    }

    #[tokio::test]
    async fn find_by_id_respects_user_scoping() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        let entry = WorklogEntry::new(uid(), tid(), "x".into(), now(), now()).unwrap();
        repo.create(&entry).await.unwrap();
        let other = Uuid::new_v4();
        assert!(repo.find_by_id(entry.id, other).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_orders_by_logged_at_desc() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        let t1 = DateTime::parse_from_rfc3339("2026-04-20T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let t2 = DateTime::parse_from_rfc3339("2026-04-21T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let e1 = WorklogEntry::new(uid(), tid(), "older".into(), t1, t1).unwrap();
        let e2 = WorklogEntry::new(uid(), tid(), "newer".into(), t2, t2).unwrap();
        repo.create(&e1).await.unwrap();
        repo.create(&e2).await.unwrap();

        let out = repo
            .list(
                uid(),
                &WorklogFilter {
                    limit: 10,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].body, "newer");
        assert_eq!(out[1].body, "older");
    }

    #[tokio::test]
    async fn list_filters_by_date_range() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        let t1 = DateTime::parse_from_rfc3339("2026-04-20T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let t2 = DateTime::parse_from_rfc3339("2026-04-21T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        repo.create(&WorklogEntry::new(uid(), tid(), "a".into(), t1, t1).unwrap())
            .await
            .unwrap();
        repo.create(&WorklogEntry::new(uid(), tid(), "b".into(), t2, t2).unwrap())
            .await
            .unwrap();

        let only_21 = repo
            .list(
                uid(),
                &WorklogFilter {
                    from: Some(t2),
                    limit: 10,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(only_21.len(), 1);
        assert_eq!(only_21[0].body, "b");
    }

    #[tokio::test]
    async fn list_respects_limit_and_offset() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        for i in 0..5 {
            let at = DateTime::parse_from_rfc3339(&format!(
                "2026-04-{:02}T09:00:00+00:00",
                20 + i
            ))
            .unwrap()
            .with_timezone(&Utc);
            repo.create(&WorklogEntry::new(uid(), tid(), format!("n{i}"), at, at).unwrap())
                .await
                .unwrap();
        }
        let page1 = repo
            .list(
                uid(),
                &WorklogFilter {
                    limit: 2,
                    offset: 0,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(page1.len(), 2);
        let page2 = repo
            .list(
                uid(),
                &WorklogFilter {
                    limit: 2,
                    offset: 2,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);
        assert_ne!(page1[0].id, page2[0].id);
    }

    #[tokio::test]
    async fn update_persists_new_body() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        let mut entry = WorklogEntry::new(uid(), tid(), "v1".into(), now(), now()).unwrap();
        repo.create(&entry).await.unwrap();
        entry.body = "v2".into();
        entry.updated_at = now() + chrono::Duration::seconds(30);
        repo.update(&entry).await.unwrap();
        let found = repo.find_by_id(entry.id, uid()).await.unwrap().unwrap();
        assert_eq!(found.body, "v2");
    }

    #[tokio::test]
    async fn delete_returns_true_on_hit_false_on_miss() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        let entry = WorklogEntry::new(uid(), tid(), "x".into(), now(), now()).unwrap();
        repo.create(&entry).await.unwrap();
        assert!(repo.delete(entry.id, uid()).await.unwrap());
        assert!(!repo.delete(entry.id, uid()).await.unwrap());
    }

    #[tokio::test]
    async fn deleting_task_cascades_entries() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool.clone());
        let entry = WorklogEntry::new(uid(), tid(), "x".into(), now(), now()).unwrap();
        repo.create(&entry).await.unwrap();
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(tid().to_string())
            .execute(&pool)
            .await
            .unwrap();
        assert!(repo.find_by_id(entry.id, uid()).await.unwrap().is_none());
    }
}
