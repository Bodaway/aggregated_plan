use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::{SyncStatus, SyncStatusRepository};
use domain::types::*;

use super::conversions::*;

pub struct SqliteSyncStatusRepository {
    pool: SqlitePool,
}

impl SqliteSyncStatusRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| ndt.and_utc())
        })
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
                .map(|ndt| ndt.and_utc())
        })
        .map_err(|e| RepositoryError::Database(format!("Failed to parse datetime '{}': {}", s, e)))
}

fn map_sync_status_row(row: &SqliteRow) -> Result<SyncStatus, RepositoryError> {
    let user_id_str: String = Row::get(row, "user_id");
    let source_str: String = Row::get(row, "source");
    let last_sync_at_str: Option<String> = Row::get(row, "last_sync_at");
    let status_str: String = Row::get(row, "status");

    let last_sync_at = match last_sync_at_str {
        Some(ref s) if !s.is_empty() => Some(parse_datetime(s)?),
        _ => None,
    };

    Ok(SyncStatus {
        user_id: Uuid::parse_str(&user_id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        source: source_from_str(&source_str),
        last_sync_at,
        status: sync_status_from_str(&status_str),
        error_message: Row::get(row, "error_message"),
    })
}

#[async_trait]
impl SyncStatusRepository for SqliteSyncStatusRepository {
    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<SyncStatus>, RepositoryError> {
        let rows = sqlx::query("SELECT * FROM sync_status WHERE user_id = ?")
            .bind(user_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(map_sync_status_row).collect()
    }

    async fn upsert(&self, status: &SyncStatus) -> Result<(), RepositoryError> {
        // Use INSERT OR REPLACE with (user_id, source) uniqueness
        // We need to generate an id for the row
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO sync_status (id, user_id, source, last_sync_at, status, error_message)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_id, source) DO UPDATE SET
                last_sync_at = excluded.last_sync_at,
                status = excluded.status,
                error_message = excluded.error_message",
        )
        .bind(id.to_string())
        .bind(status.user_id.to_string())
        .bind(source_to_str(status.source))
        .bind(status.last_sync_at.map(|dt| dt.to_rfc3339()))
        .bind(sync_status_to_str(status.status))
        .bind(&status.error_message)
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
    async fn test_upsert_and_find_by_user() {
        let pool = setup().await;
        let repo = SqliteSyncStatusRepository::new(pool);

        let status = SyncStatus {
            user_id: user_id(),
            source: Source::Jira,
            last_sync_at: Some(Utc::now()),
            status: SyncSourceStatus::Success,
            error_message: None,
        };
        repo.upsert(&status).await.unwrap();

        let found = repo.find_by_user(user_id()).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, Source::Jira);
        assert_eq!(found[0].status, SyncSourceStatus::Success);
        assert!(found[0].last_sync_at.is_some());
        assert!(found[0].error_message.is_none());
    }

    #[tokio::test]
    async fn test_upsert_updates_existing() {
        let pool = setup().await;
        let repo = SqliteSyncStatusRepository::new(pool);

        let status1 = SyncStatus {
            user_id: user_id(),
            source: Source::Jira,
            last_sync_at: None,
            status: SyncSourceStatus::Syncing,
            error_message: None,
        };
        repo.upsert(&status1).await.unwrap();

        let status2 = SyncStatus {
            user_id: user_id(),
            source: Source::Jira,
            last_sync_at: Some(Utc::now()),
            status: SyncSourceStatus::Error,
            error_message: Some("Connection timeout".to_string()),
        };
        repo.upsert(&status2).await.unwrap();

        let found = repo.find_by_user(user_id()).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].status, SyncSourceStatus::Error);
        assert_eq!(
            found[0].error_message,
            Some("Connection timeout".to_string())
        );
    }

    #[tokio::test]
    async fn test_multiple_sources() {
        let pool = setup().await;
        let repo = SqliteSyncStatusRepository::new(pool);

        repo.upsert(&SyncStatus {
            user_id: user_id(),
            source: Source::Jira,
            last_sync_at: None,
            status: SyncSourceStatus::Idle,
            error_message: None,
        })
        .await
        .unwrap();

        repo.upsert(&SyncStatus {
            user_id: user_id(),
            source: Source::Excel,
            last_sync_at: None,
            status: SyncSourceStatus::Idle,
            error_message: None,
        })
        .await
        .unwrap();

        let found = repo.find_by_user(user_id()).await.unwrap();
        assert_eq!(found.len(), 2);
    }
}
