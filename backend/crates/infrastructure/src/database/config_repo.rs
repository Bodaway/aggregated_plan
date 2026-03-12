use async_trait::async_trait;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::ConfigRepository;
use domain::types::*;

pub struct SqliteConfigRepository {
    pool: SqlitePool,
}

impl SqliteConfigRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConfigRepository for SqliteConfigRepository {
    async fn get(&self, user_id: UserId, key: &str) -> Result<Option<String>, RepositoryError> {
        let rows =
            sqlx::query("SELECT value FROM configuration WHERE user_id = ? AND key = ?")
                .bind(user_id.to_string())
                .bind(key)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => {
                let value: String = Row::get(row, "value");
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    async fn get_all(
        &self,
        user_id: UserId,
    ) -> Result<Vec<(String, String)>, RepositoryError> {
        let rows =
            sqlx::query("SELECT key, value FROM configuration WHERE user_id = ? ORDER BY key")
                .bind(user_id.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|row| {
                let key: String = Row::get(row, "key");
                let value: String = Row::get(row, "value");
                (key, value)
            })
            .collect())
    }

    async fn set(
        &self,
        user_id: UserId,
        key: &str,
        value: &str,
    ) -> Result<(), RepositoryError> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO configuration (id, user_id, key, value)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(user_id, key) DO UPDATE SET value = excluded.value",
        )
        .bind(id.to_string())
        .bind(user_id.to_string())
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::connection::create_sqlite_pool;

    async fn setup() -> SqlitePool {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT OR IGNORE INTO users (id, name, email, created_at) VALUES (?, ?, ?, ?)")
            .bind("00000000-0000-0000-0000-000000000001")
            .bind("Test User")
            .bind("test@example.com")
            .bind("2024-01-01T00:00:00+00:00")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    #[tokio::test]
    async fn test_set_and_get() {
        let pool = setup().await;
        let repo = SqliteConfigRepository::new(pool);

        repo.set(user_id(), "theme", "dark").await.unwrap();

        let value = repo.get(user_id(), "theme").await.unwrap();
        assert_eq!(value, Some("dark".to_string()));
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let pool = setup().await;
        let repo = SqliteConfigRepository::new(pool);

        let value = repo.get(user_id(), "nonexistent").await.unwrap();
        assert!(value.is_none());
    }

    #[tokio::test]
    async fn test_upsert_overwrites() {
        let pool = setup().await;
        let repo = SqliteConfigRepository::new(pool);

        repo.set(user_id(), "theme", "light").await.unwrap();
        repo.set(user_id(), "theme", "dark").await.unwrap();

        let value = repo.get(user_id(), "theme").await.unwrap();
        assert_eq!(value, Some("dark".to_string()));
    }

    #[tokio::test]
    async fn test_get_all() {
        let pool = setup().await;
        let repo = SqliteConfigRepository::new(pool);

        repo.set(user_id(), "language", "en").await.unwrap();
        repo.set(user_id(), "theme", "dark").await.unwrap();
        repo.set(user_id(), "auto_sync", "true").await.unwrap();

        let all = repo.get_all(user_id()).await.unwrap();
        assert_eq!(all.len(), 3);
        // Ordered by key
        assert_eq!(all[0], ("auto_sync".to_string(), "true".to_string()));
        assert_eq!(all[1], ("language".to_string(), "en".to_string()));
        assert_eq!(all[2], ("theme".to_string(), "dark".to_string()));
    }
}
