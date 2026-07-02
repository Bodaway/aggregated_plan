use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::SignalMappingRepository;
use domain::types::*;

pub struct SqliteSignalMappingRepository {
    pool: SqlitePool,
}

impl SqliteSignalMappingRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RepositoryError::Database(format!("bad datetime '{s}': {e}")))
}

fn map_row(row: &SqliteRow) -> Result<SignalMapping, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let user_id_str: String = Row::get(row, "user_id");
    let kind_str: String = Row::get(row, "kind");
    let is_enabled: i64 = Row::get(row, "is_enabled");
    Ok(SignalMapping {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        kind: MappingKind::from_str(&kind_str)
            .ok_or_else(|| RepositoryError::Database(format!("bad kind '{kind_str}'")))?,
        pattern: Row::get(row, "pattern"),
        branch_pattern: Row::get(row, "branch_pattern"),
        gryzzly_project_id: Row::get(row, "gryzzly_project_id"),
        gryzzly_project_name: Row::get(row, "gryzzly_project_name"),
        is_enabled: is_enabled != 0,
        created_at: parse_dt(&Row::get::<String, _>(row, "created_at"))?,
        updated_at: parse_dt(&Row::get::<String, _>(row, "updated_at"))?,
    })
}

#[async_trait]
impl SignalMappingRepository for SqliteSignalMappingRepository {
    async fn list_enabled(&self, user_id: UserId) -> Result<Vec<SignalMapping>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM signal_project_mappings WHERE user_id = ? AND is_enabled = 1",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        rows.iter().map(map_row).collect()
    }

    async fn upsert(&self, m: &SignalMapping) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO signal_project_mappings
                (id, user_id, kind, pattern, branch_pattern, gryzzly_project_id, gryzzly_project_name, is_enabled, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_id, kind, pattern) DO UPDATE SET
                branch_pattern = excluded.branch_pattern,
                gryzzly_project_id = excluded.gryzzly_project_id,
                gryzzly_project_name = excluded.gryzzly_project_name,
                is_enabled = excluded.is_enabled,
                updated_at = excluded.updated_at",
        )
        .bind(m.id.to_string())
        .bind(m.user_id.to_string())
        .bind(m.kind.as_str())
        .bind(&m.pattern)
        .bind(&m.branch_pattern)
        .bind(&m.gryzzly_project_id)
        .bind(&m.gryzzly_project_name)
        .bind(if m.is_enabled { 1 } else { 0 })
        .bind(m.created_at.to_rfc3339())
        .bind(m.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn set_enabled(&self, id: SignalMappingId, enabled: bool) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE signal_project_mappings SET is_enabled = ? WHERE id = ?")
            .bind(if enabled { 1 } else { 0 })
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: SignalMappingId) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM signal_project_mappings WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    async fn pool_with_user() -> (SqlitePool, Uuid) {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../../migrations/sqlite").run(&pool).await.unwrap();
        let uid = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, name, email, created_at) VALUES (?, ?, ?, ?)")
            .bind(uid.to_string())
            .bind("T")
            .bind("t@e.co")
            .bind(Utc::now().to_rfc3339())
            .execute(&pool)
            .await
            .unwrap();
        (pool, uid)
    }

    fn mapping(uid: Uuid) -> SignalMapping {
        SignalMapping {
            id: Uuid::new_v4(),
            user_id: uid,
            kind: MappingKind::RepoPath,
            pattern: "/home/me/repo".into(),
            branch_pattern: None,
            gryzzly_project_id: "p1".into(),
            gryzzly_project_name: Some("Project 1".into()),
            is_enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn upsert_then_list_enabled_returns_it() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteSignalMappingRepository::new(pool);
        repo.upsert(&mapping(uid)).await.unwrap();
        let rows = repo.list_enabled(uid).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].gryzzly_project_id, "p1");
    }

    #[tokio::test]
    async fn upsert_is_idempotent_on_kind_pattern() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteSignalMappingRepository::new(pool);
        let mut m = mapping(uid);
        repo.upsert(&m).await.unwrap();
        m.id = Uuid::new_v4(); // different id, same (kind, pattern)
        m.gryzzly_project_id = "p2".into();
        repo.upsert(&m).await.unwrap();
        let rows = repo.list_enabled(uid).await.unwrap();
        assert_eq!(rows.len(), 1, "same (kind,pattern) must update not duplicate");
        assert_eq!(rows[0].gryzzly_project_id, "p2");
    }

    #[tokio::test]
    async fn disabled_rule_is_excluded() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteSignalMappingRepository::new(pool);
        let m = mapping(uid);
        let id = m.id;
        repo.upsert(&m).await.unwrap();
        repo.set_enabled(id, false).await.unwrap();
        assert!(repo.list_enabled(uid).await.unwrap().is_empty());
    }
}
