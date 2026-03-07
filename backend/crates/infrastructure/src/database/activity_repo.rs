use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::ActivitySlotRepository;
use domain::types::*;

use super::conversions::*;

pub struct SqliteActivitySlotRepository {
    pool: SqlitePool,
}

impl SqliteActivitySlotRepository {
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

fn map_activity_slot_row(row: &SqliteRow) -> Result<ActivitySlot, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let user_id_str: String = Row::get(row, "user_id");
    let task_id_str: Option<String> = Row::get(row, "task_id");
    let start_time_str: String = Row::get(row, "start_time");
    let end_time_str: Option<String> = Row::get(row, "end_time");
    let half_day_str: String = Row::get(row, "half_day");
    let date_str: String = Row::get(row, "date");
    let created_at_str: String = Row::get(row, "created_at");

    let task_id = match task_id_str {
        Some(ref s) if !s.is_empty() => Some(
            Uuid::parse_str(s).map_err(|e| RepositoryError::Database(e.to_string()))?,
        ),
        _ => None,
    };

    let end_time = match end_time_str {
        Some(ref s) if !s.is_empty() => Some(parse_datetime(s)?),
        _ => None,
    };

    Ok(ActivitySlot {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        task_id,
        start_time: parse_datetime(&start_time_str)?,
        end_time,
        half_day: half_day_from_str(&half_day_str),
        date: NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").map_err(|e| {
            RepositoryError::Database(format!("Failed to parse date '{}': {}", date_str, e))
        })?,
        created_at: parse_datetime(&created_at_str)?,
    })
}

#[async_trait]
impl ActivitySlotRepository for SqliteActivitySlotRepository {
    async fn find_by_id(&self, id: ActivitySlotId) -> Result<Option<ActivitySlot>, RepositoryError> {
        let rows = sqlx::query("SELECT * FROM activity_slots WHERE id = ?")
            .bind(id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => Ok(Some(map_activity_slot_row(row)?)),
            None => Ok(None),
        }
    }

    async fn find_by_user_and_date(
        &self,
        user_id: UserId,
        date: NaiveDate,
    ) -> Result<Vec<ActivitySlot>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM activity_slots WHERE user_id = ? AND date = ? ORDER BY start_time",
        )
        .bind(user_id.to_string())
        .bind(date.format("%Y-%m-%d").to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(map_activity_slot_row).collect()
    }

    async fn find_active(
        &self,
        user_id: UserId,
    ) -> Result<Option<ActivitySlot>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM activity_slots WHERE user_id = ? AND end_time IS NULL LIMIT 1",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => Ok(Some(map_activity_slot_row(row)?)),
            None => Ok(None),
        }
    }

    async fn save(&self, slot: &ActivitySlot) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO activity_slots (id, user_id, task_id, start_time, end_time, half_day, date, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(slot.id.to_string())
        .bind(slot.user_id.to_string())
        .bind(slot.task_id.map(|id| id.to_string()))
        .bind(slot.start_time.to_rfc3339())
        .bind(slot.end_time.map(|dt| dt.to_rfc3339()))
        .bind(half_day_to_str(slot.half_day))
        .bind(slot.date.format("%Y-%m-%d").to_string())
        .bind(slot.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn update(&self, slot: &ActivitySlot) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE activity_slots SET task_id = ?, start_time = ?, end_time = ?, half_day = ?, date = ? WHERE id = ?",
        )
        .bind(slot.task_id.map(|id| id.to_string()))
        .bind(slot.start_time.to_rfc3339())
        .bind(slot.end_time.map(|dt| dt.to_rfc3339()))
        .bind(half_day_to_str(slot.half_day))
        .bind(slot.date.format("%Y-%m-%d").to_string())
        .bind(slot.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, id: ActivitySlotId) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM activity_slots WHERE id = ?")
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
    use crate::database::connection::create_sqlite_pool;

    async fn setup() -> SqlitePool {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT INTO users (id, name, email, created_at) VALUES (?, ?, ?, ?)")
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

    fn make_slot(half_day: HalfDay, date: &str, active: bool) -> ActivitySlot {
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
        let start = DateTime::parse_from_rfc3339(&format!("{date}T09:00:00+00:00"))
            .unwrap()
            .with_timezone(&Utc);
        let end = if active {
            None
        } else {
            Some(
                DateTime::parse_from_rfc3339(&format!("{date}T12:00:00+00:00"))
                    .unwrap()
                    .with_timezone(&Utc),
            )
        };

        ActivitySlot {
            id: Uuid::new_v4(),
            user_id: user_id(),
            task_id: None,
            start_time: start,
            end_time: end,
            half_day,
            date,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_save_and_find_by_date() {
        let pool = setup().await;
        let repo = SqliteActivitySlotRepository::new(pool);

        let slot = make_slot(HalfDay::Morning, "2024-06-10", false);
        repo.save(&slot).await.unwrap();

        let date = NaiveDate::from_ymd_opt(2024, 6, 10).unwrap();
        let found = repo.find_by_user_and_date(user_id(), date).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].half_day, HalfDay::Morning);
        assert!(found[0].end_time.is_some());
    }

    #[tokio::test]
    async fn test_find_active() {
        let pool = setup().await;
        let repo = SqliteActivitySlotRepository::new(pool);

        let completed = make_slot(HalfDay::Morning, "2024-06-10", false);
        let active = make_slot(HalfDay::Afternoon, "2024-06-10", true);

        repo.save(&completed).await.unwrap();
        repo.save(&active).await.unwrap();

        let found = repo.find_active(user_id()).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.half_day, HalfDay::Afternoon);
        assert!(found.end_time.is_none());
    }

    #[tokio::test]
    async fn test_find_active_none() {
        let pool = setup().await;
        let repo = SqliteActivitySlotRepository::new(pool);

        let slot = make_slot(HalfDay::Morning, "2024-06-10", false);
        repo.save(&slot).await.unwrap();

        let found = repo.find_active(user_id()).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_update() {
        let pool = setup().await;
        let repo = SqliteActivitySlotRepository::new(pool);

        let mut slot = make_slot(HalfDay::Morning, "2024-06-10", true);
        repo.save(&slot).await.unwrap();

        // Stop the activity
        slot.end_time = Some(Utc::now());
        repo.update(&slot).await.unwrap();

        let found = repo.find_active(user_id()).await.unwrap();
        assert!(found.is_none());

        let date = NaiveDate::from_ymd_opt(2024, 6, 10).unwrap();
        let all = repo.find_by_user_and_date(user_id(), date).await.unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].end_time.is_some());
    }

    #[tokio::test]
    async fn test_delete() {
        let pool = setup().await;
        let repo = SqliteActivitySlotRepository::new(pool);

        let slot = make_slot(HalfDay::Morning, "2024-06-10", false);
        repo.save(&slot).await.unwrap();

        repo.delete(slot.id).await.unwrap();

        let date = NaiveDate::from_ymd_opt(2024, 6, 10).unwrap();
        let found = repo.find_by_user_and_date(user_id(), date).await.unwrap();
        assert!(found.is_empty());
    }
}
