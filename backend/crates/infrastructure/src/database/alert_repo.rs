use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::AlertRepository;
use domain::types::*;

use super::conversions::*;

pub struct SqliteAlertRepository {
    pool: SqlitePool,
}

impl SqliteAlertRepository {
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

fn map_alert_row(row: &SqliteRow) -> Result<Alert, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let user_id_str: String = Row::get(row, "user_id");
    let alert_type_str: String = Row::get(row, "alert_type");
    let severity_str: String = Row::get(row, "severity");
    let related_items_json: String = Row::get(row, "related_items");
    let date_str: String = Row::get(row, "date");
    let resolved_val: i32 = Row::get(row, "resolved");
    let created_at_str: String = Row::get(row, "created_at");

    let related_items: Vec<RelatedItem> = serde_json::from_str(&related_items_json)
        .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

    Ok(Alert {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        alert_type: alert_type_from_str(&alert_type_str),
        severity: alert_severity_from_str(&severity_str),
        message: Row::get(row, "message"),
        related_items,
        date: NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").map_err(|e| {
            RepositoryError::Database(format!("Failed to parse date '{}': {}", date_str, e))
        })?,
        resolved: resolved_val != 0,
        created_at: parse_datetime(&created_at_str)?,
    })
}

#[async_trait]
impl AlertRepository for SqliteAlertRepository {
    async fn find_by_id(&self, id: AlertId) -> Result<Option<Alert>, RepositoryError> {
        let rows = sqlx::query("SELECT * FROM alerts WHERE id = ?")
            .bind(id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => Ok(Some(map_alert_row(row)?)),
            None => Ok(None),
        }
    }

    async fn find_unresolved(&self, user_id: UserId) -> Result<Vec<Alert>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM alerts WHERE user_id = ? AND resolved = 0 ORDER BY created_at DESC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(map_alert_row).collect()
    }

    async fn find_by_user(
        &self,
        user_id: UserId,
        resolved: Option<bool>,
    ) -> Result<Vec<Alert>, RepositoryError> {
        let rows = match resolved {
            Some(r) => {
                sqlx::query(
                    "SELECT * FROM alerts WHERE user_id = ? AND resolved = ? ORDER BY created_at DESC",
                )
                .bind(user_id.to_string())
                .bind(if r { 1i32 } else { 0i32 })
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query(
                    "SELECT * FROM alerts WHERE user_id = ? ORDER BY created_at DESC",
                )
                .bind(user_id.to_string())
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(map_alert_row).collect()
    }

    async fn save(&self, alert: &Alert) -> Result<(), RepositoryError> {
        let related_items_json = serde_json::to_string(&alert.related_items)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

        sqlx::query(
            "INSERT INTO alerts (id, user_id, alert_type, severity, message, related_items, date, resolved, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(alert.id.to_string())
        .bind(alert.user_id.to_string())
        .bind(alert_type_to_str(alert.alert_type))
        .bind(alert_severity_to_str(alert.severity))
        .bind(&alert.message)
        .bind(&related_items_json)
        .bind(alert.date.format("%Y-%m-%d").to_string())
        .bind(if alert.resolved { 1i32 } else { 0i32 })
        .bind(alert.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn save_batch(&self, alerts: &[Alert]) -> Result<(), RepositoryError> {
        for alert in alerts {
            self.save(alert).await?;
        }
        Ok(())
    }

    async fn update(&self, alert: &Alert) -> Result<(), RepositoryError> {
        let related_items_json = serde_json::to_string(&alert.related_items)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

        sqlx::query(
            "UPDATE alerts SET alert_type = ?, severity = ?, message = ?, related_items = ?, date = ?, resolved = ? WHERE id = ?",
        )
        .bind(alert_type_to_str(alert.alert_type))
        .bind(alert_severity_to_str(alert.severity))
        .bind(&alert.message)
        .bind(&related_items_json)
        .bind(alert.date.format("%Y-%m-%d").to_string())
        .bind(if alert.resolved { 1i32 } else { 0i32 })
        .bind(alert.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn delete_resolved(&self, user_id: UserId) -> Result<u64, RepositoryError> {
        let result = sqlx::query("DELETE FROM alerts WHERE user_id = ? AND resolved = 1")
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(result.rows_affected())
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

    fn make_alert(message: &str, resolved: bool) -> Alert {
        let task_id = Uuid::new_v4();
        Alert {
            id: Uuid::new_v4(),
            user_id: user_id(),
            alert_type: AlertType::Deadline,
            severity: AlertSeverity::Warning,
            message: message.to_string(),
            related_items: vec![RelatedItem::Task(task_id)],
            date: NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(),
            resolved,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_save_and_find_unresolved() {
        let pool = setup().await;
        let repo = SqliteAlertRepository::new(pool);

        repo.save(&make_alert("Unresolved", false)).await.unwrap();
        repo.save(&make_alert("Resolved", true)).await.unwrap();

        let unresolved = repo.find_unresolved(user_id()).await.unwrap();
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].message, "Unresolved");
    }

    #[tokio::test]
    async fn test_find_by_user_all() {
        let pool = setup().await;
        let repo = SqliteAlertRepository::new(pool);

        repo.save(&make_alert("A1", false)).await.unwrap();
        repo.save(&make_alert("A2", true)).await.unwrap();

        let all = repo.find_by_user(user_id(), None).await.unwrap();
        assert_eq!(all.len(), 2);

        let resolved = repo.find_by_user(user_id(), Some(true)).await.unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].message, "A2");

        let unresolved = repo.find_by_user(user_id(), Some(false)).await.unwrap();
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].message, "A1");
    }

    #[tokio::test]
    async fn test_save_batch() {
        let pool = setup().await;
        let repo = SqliteAlertRepository::new(pool);

        let alerts = vec![
            make_alert("Batch 1", false),
            make_alert("Batch 2", false),
        ];
        repo.save_batch(&alerts).await.unwrap();

        let found = repo.find_unresolved(user_id()).await.unwrap();
        assert_eq!(found.len(), 2);
    }

    #[tokio::test]
    async fn test_update() {
        let pool = setup().await;
        let repo = SqliteAlertRepository::new(pool);

        let mut alert = make_alert("Original", false);
        repo.save(&alert).await.unwrap();

        alert.resolved = true;
        alert.message = "Updated".to_string();
        repo.update(&alert).await.unwrap();

        let unresolved = repo.find_unresolved(user_id()).await.unwrap();
        assert!(unresolved.is_empty());

        let all = repo.find_by_user(user_id(), None).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].message, "Updated");
        assert!(all[0].resolved);
    }

    #[tokio::test]
    async fn test_delete_resolved() {
        let pool = setup().await;
        let repo = SqliteAlertRepository::new(pool);

        repo.save(&make_alert("Keep", false)).await.unwrap();
        repo.save(&make_alert("Delete 1", true)).await.unwrap();
        repo.save(&make_alert("Delete 2", true)).await.unwrap();

        let deleted = repo.delete_resolved(user_id()).await.unwrap();
        assert_eq!(deleted, 2);

        let remaining = repo.find_by_user(user_id(), None).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].message, "Keep");
    }

    #[tokio::test]
    async fn test_related_items_serialization() {
        let pool = setup().await;
        let repo = SqliteAlertRepository::new(pool);

        let task_id = Uuid::new_v4();
        let meeting_id = Uuid::new_v4();
        let mut alert = make_alert("Mixed", false);
        alert.related_items = vec![
            RelatedItem::Task(task_id),
            RelatedItem::Meeting(meeting_id),
        ];

        repo.save(&alert).await.unwrap();

        let found = repo.find_unresolved(user_id()).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].related_items.len(), 2);
    }
}
