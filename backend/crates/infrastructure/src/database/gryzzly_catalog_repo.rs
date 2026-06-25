use application::errors::RepositoryError;
use application::repositories::GryzzlyCatalogRepository;
use async_trait::async_trait;
use domain::types::{GryzzlyCatalogEntry, UserId};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct SqliteGryzzlyCatalogRepository {
    pool: SqlitePool,
}

impl SqliteGryzzlyCatalogRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn map_row(row: &sqlx::sqlite::SqliteRow) -> Result<GryzzlyCatalogEntry, RepositoryError> {
    let id: String = row.try_get("id").map_err(|e| RepositoryError::Database(e.to_string()))?;
    let user_id: String = row.try_get("user_id").map_err(|e| RepositoryError::Database(e.to_string()))?;
    let last_synced_at: String = row.try_get("last_synced_at").map_err(|e| RepositoryError::Database(e.to_string()))?;
    let is_active: i64 = row.try_get("is_active").map_err(|e| RepositoryError::Database(e.to_string()))?;
    Ok(GryzzlyCatalogEntry {
        id: Uuid::parse_str(&id).map_err(|e| RepositoryError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id).map_err(|e| RepositoryError::Database(e.to_string()))?,
        gryzzly_task_id: row.try_get("gryzzly_task_id").map_err(|e| RepositoryError::Database(e.to_string()))?,
        name: row.try_get("name").map_err(|e| RepositoryError::Database(e.to_string()))?,
        gryzzly_project_id: row.try_get("gryzzly_project_id").map_err(|e| RepositoryError::Database(e.to_string()))?,
        project_name: row.try_get("project_name").map_err(|e| RepositoryError::Database(e.to_string()))?,
        customer_name: row.try_get("customer_name").ok(),
        is_active: is_active != 0,
        last_synced_at: chrono::DateTime::parse_from_rfc3339(&last_synced_at)
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .with_timezone(&chrono::Utc),
    })
}

#[async_trait]
impl GryzzlyCatalogRepository for SqliteGryzzlyCatalogRepository {
    async fn upsert(&self, entry: &GryzzlyCatalogEntry) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO gryzzly_tasks
                (id, user_id, gryzzly_task_id, name, gryzzly_project_id, project_name, customer_name, is_active, last_synced_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_id, gryzzly_task_id) DO UPDATE SET
                name = excluded.name,
                gryzzly_project_id = excluded.gryzzly_project_id,
                project_name = excluded.project_name,
                customer_name = excluded.customer_name,
                is_active = excluded.is_active,
                last_synced_at = excluded.last_synced_at",
        )
        .bind(entry.id.to_string())
        .bind(entry.user_id.to_string())
        .bind(&entry.gryzzly_task_id)
        .bind(&entry.name)
        .bind(&entry.gryzzly_project_id)
        .bind(&entry.project_name)
        .bind(&entry.customer_name)
        .bind(if entry.is_active { 1i64 } else { 0i64 })
        .bind(entry.last_synced_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn soft_prune_missing(&self, user_id: UserId, keep_ids: &[String]) -> Result<u64, RepositoryError> {
        if keep_ids.is_empty() {
            // Defensive: never mass-disable on an empty keep-list. The sync use case
            // already skips pruning on an empty fetch; this is a second guard.
            return Ok(0);
        }
        let placeholders = keep_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "UPDATE gryzzly_tasks SET is_active = 0
             WHERE user_id = ? AND is_active = 1 AND gryzzly_task_id NOT IN ({placeholders})"
        );
        let mut q = sqlx::query(&sql).bind(user_id.to_string());
        for id in keep_ids {
            q = q.bind(id);
        }
        let res = q.execute(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(res.rows_affected())
    }

    async fn list_active(
        &self,
        user_id: UserId,
        search: Option<&str>,
        project_filter: Option<&str>,
        limit: i64,
    ) -> Result<Vec<GryzzlyCatalogEntry>, RepositoryError> {
        let mut sql = String::from(
            "SELECT * FROM gryzzly_tasks WHERE user_id = ? AND is_active = 1",
        );
        if search.is_some() {
            sql.push_str(" AND (name LIKE ? OR project_name LIKE ?)");
        }
        if project_filter.is_some() {
            sql.push_str(" AND project_name = ?");
        }
        sql.push_str(" ORDER BY project_name, name LIMIT ?");

        let mut q = sqlx::query(&sql).bind(user_id.to_string());
        if let Some(s) = search {
            let pat = format!("%{s}%");
            q = q.bind(pat.clone()).bind(pat);
        }
        if let Some(p) = project_filter {
            q = q.bind(p.to_string());
        }
        q = q.bind(limit);

        let rows = q.fetch_all(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string()))?;
        rows.iter().map(map_row).collect()
    }

    async fn find_by_gryzzly_task_id(
        &self,
        user_id: UserId,
        gryzzly_task_id: &str,
    ) -> Result<Option<GryzzlyCatalogEntry>, RepositoryError> {
        let row = sqlx::query("SELECT * FROM gryzzly_tasks WHERE user_id = ? AND gryzzly_task_id = ?")
            .bind(user_id.to_string())
            .bind(gryzzly_task_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        row.as_ref().map(map_row).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::connection::create_sqlite_pool;
    use chrono::Utc;
    use uuid::Uuid;

    async fn setup_with_user() -> (SqlitePool, Uuid) {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT OR IGNORE INTO users (id, name, email, created_at) VALUES (?, ?, ?, ?)")
            .bind("00000000-0000-0000-0000-000000000001")
            .bind("Test User")
            .bind("test@example.com")
            .bind("2024-01-01T00:00:00+00:00")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        let user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        (pool, user_id)
    }

    fn entry(user_id: Uuid, gid: &str, active: bool) -> GryzzlyCatalogEntry {
        GryzzlyCatalogEntry {
            id: Uuid::new_v4(),
            user_id,
            gryzzly_task_id: gid.to_string(),
            name: format!("Task {gid}"),
            gryzzly_project_id: "proj-1".to_string(),
            project_name: "Website".to_string(),
            customer_name: Some("Acme".to_string()),
            is_active: active,
            last_synced_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn upsert_creates_then_updates() {
        let (pool, user_id) = setup_with_user().await;
        let repo = SqliteGryzzlyCatalogRepository::new(pool);
        repo.upsert(&entry(user_id, "g1", true)).await.unwrap();

        let mut e = entry(user_id, "g1", true);
        e.name = "Renamed".into();
        repo.upsert(&e).await.unwrap();

        let found = repo.find_by_gryzzly_task_id(user_id, "g1").await.unwrap().unwrap();
        assert_eq!(found.name, "Renamed");
        let active = repo.list_active(user_id, None, None, 100).await.unwrap();
        assert_eq!(active.len(), 1, "upsert must not duplicate on (user_id, gryzzly_task_id)");
    }

    #[tokio::test]
    async fn soft_prune_disables_missing_but_keeps_row() {
        let (pool, user_id) = setup_with_user().await;
        let repo = SqliteGryzzlyCatalogRepository::new(pool);
        repo.upsert(&entry(user_id, "g1", true)).await.unwrap();
        repo.upsert(&entry(user_id, "g2", true)).await.unwrap();

        let disabled = repo.soft_prune_missing(user_id, &["g1".to_string()]).await.unwrap();
        assert_eq!(disabled, 1);

        // g2 disabled but still present (so an assignment to it still resolves).
        assert!(repo.list_active(user_id, None, None, 100).await.unwrap().iter().all(|e| e.gryzzly_task_id == "g1"));
        let g2 = repo.find_by_gryzzly_task_id(user_id, "g2").await.unwrap().unwrap();
        assert!(!g2.is_active);
    }

    #[tokio::test]
    async fn list_active_filters_by_search() {
        let (pool, user_id) = setup_with_user().await;
        let repo = SqliteGryzzlyCatalogRepository::new(pool);
        repo.upsert(&entry(user_id, "g1", true)).await.unwrap();
        let hits = repo.list_active(user_id, Some("Task g1"), None, 100).await.unwrap();
        assert_eq!(hits.len(), 1);
        let misses = repo.list_active(user_id, Some("zzz"), None, 100).await.unwrap();
        assert!(misses.is_empty());
    }
}
