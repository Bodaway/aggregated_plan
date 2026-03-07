use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::MeetingRepository;
use domain::types::*;

pub struct SqliteMeetingRepository {
    pool: SqlitePool,
}

impl SqliteMeetingRepository {
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

fn map_meeting_row(row: &SqliteRow) -> Result<Meeting, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let user_id_str: String = Row::get(row, "user_id");
    let start_time_str: String = Row::get(row, "start_time");
    let end_time_str: String = Row::get(row, "end_time");
    let participants_json: String = Row::get(row, "participants");
    let project_id_str: Option<String> = Row::get(row, "project_id");
    let created_at_str: String = Row::get(row, "created_at");

    let project_id = match project_id_str {
        Some(ref s) if !s.is_empty() => Some(
            Uuid::parse_str(s).map_err(|e| RepositoryError::Database(e.to_string()))?,
        ),
        _ => None,
    };

    let participants: Vec<String> = serde_json::from_str(&participants_json)
        .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

    Ok(Meeting {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        title: Row::get(row, "title"),
        start_time: parse_datetime(&start_time_str)?,
        end_time: parse_datetime(&end_time_str)?,
        location: Row::get(row, "location"),
        participants,
        project_id,
        outlook_id: Row::get(row, "outlook_id"),
        created_at: parse_datetime(&created_at_str)?,
    })
}

#[async_trait]
impl MeetingRepository for SqliteMeetingRepository {
    async fn find_by_id(&self, id: MeetingId) -> Result<Option<Meeting>, RepositoryError> {
        let rows = sqlx::query("SELECT * FROM meetings WHERE id = ?")
            .bind(id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => Ok(Some(map_meeting_row(row)?)),
            None => Ok(None),
        }
    }

    async fn update(&self, meeting: &Meeting) -> Result<(), RepositoryError> {
        let participants_json = serde_json::to_string(&meeting.participants)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

        sqlx::query(
            "UPDATE meetings SET title = ?, start_time = ?, end_time = ?, location = ?, participants = ?, project_id = ? WHERE id = ?",
        )
        .bind(&meeting.title)
        .bind(meeting.start_time.to_rfc3339())
        .bind(meeting.end_time.to_rfc3339())
        .bind(&meeting.location)
        .bind(&participants_json)
        .bind(meeting.project_id.map(|id| id.to_string()))
        .bind(meeting.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn find_by_user_and_date(
        &self,
        user_id: UserId,
        date: NaiveDate,
    ) -> Result<Vec<Meeting>, RepositoryError> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let rows = sqlx::query(
            "SELECT * FROM meetings WHERE user_id = ? AND date(start_time) = ? ORDER BY start_time",
        )
        .bind(user_id.to_string())
        .bind(&date_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(map_meeting_row).collect()
    }

    async fn find_by_user_and_range(
        &self,
        user_id: UserId,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Meeting>, RepositoryError> {
        let start_str = start.format("%Y-%m-%d").to_string();
        // end is inclusive — include all of the end day
        let end_str = format!("{}T23:59:59", end.format("%Y-%m-%d"));
        let rows = sqlx::query(
            "SELECT * FROM meetings WHERE user_id = ? AND start_time >= ? AND start_time <= ? ORDER BY start_time",
        )
        .bind(user_id.to_string())
        .bind(&start_str)
        .bind(&end_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(map_meeting_row).collect()
    }

    async fn upsert_batch(&self, meetings: &[Meeting]) -> Result<(), RepositoryError> {
        for meeting in meetings {
            let participants_json = serde_json::to_string(&meeting.participants)
                .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

            sqlx::query(
                "INSERT OR REPLACE INTO meetings (id, user_id, title, start_time, end_time, location, participants, project_id, outlook_id, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(meeting.id.to_string())
            .bind(meeting.user_id.to_string())
            .bind(&meeting.title)
            .bind(meeting.start_time.to_rfc3339())
            .bind(meeting.end_time.to_rfc3339())
            .bind(&meeting.location)
            .bind(&participants_json)
            .bind(meeting.project_id.map(|id| id.to_string()))
            .bind(&meeting.outlook_id)
            .bind(meeting.created_at.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        }

        Ok(())
    }

    async fn delete_stale(
        &self,
        user_id: UserId,
        current_outlook_ids: &[String],
    ) -> Result<u64, RepositoryError> {
        if current_outlook_ids.is_empty() {
            // Delete all meetings for the user
            let result = sqlx::query("DELETE FROM meetings WHERE user_id = ?")
                .bind(user_id.to_string())
                .execute(&self.pool)
                .await
                .map_err(|e| RepositoryError::Database(e.to_string()))?;
            return Ok(result.rows_affected());
        }

        let placeholders: Vec<&str> = current_outlook_ids.iter().map(|_| "?").collect();
        let sql = format!(
            "DELETE FROM meetings WHERE user_id = ? AND outlook_id NOT IN ({})",
            placeholders.join(",")
        );

        let mut query = sqlx::query(&sql).bind(user_id.to_string());
        for oid in current_outlook_ids {
            query = query.bind(oid);
        }

        let result = query
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(result.rows_affected())
    }

    async fn find_by_project(
        &self,
        user_id: UserId,
        project_id: ProjectId,
    ) -> Result<Vec<Meeting>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM meetings WHERE user_id = ? AND project_id = ? ORDER BY start_time",
        )
        .bind(user_id.to_string())
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(map_meeting_row).collect()
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

    fn make_meeting(title: &str, outlook_id: &str, date: &str) -> Meeting {
        Meeting {
            id: Uuid::new_v4(),
            user_id: user_id(),
            title: title.to_string(),
            start_time: DateTime::parse_from_rfc3339(&format!("{date}T09:00:00+00:00"))
                .unwrap()
                .with_timezone(&Utc),
            end_time: DateTime::parse_from_rfc3339(&format!("{date}T10:00:00+00:00"))
                .unwrap()
                .with_timezone(&Utc),
            location: Some("Room A".to_string()),
            participants: vec!["alice@test.com".to_string(), "bob@test.com".to_string()],
            project_id: None,
            outlook_id: outlook_id.to_string(),
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_upsert_batch_and_find_by_date() {
        let pool = setup().await;
        let repo = SqliteMeetingRepository::new(pool);

        let meetings = vec![
            make_meeting("Standup", "out-1", "2024-06-10"),
            make_meeting("Review", "out-2", "2024-06-10"),
            make_meeting("Planning", "out-3", "2024-06-11"),
        ];

        repo.upsert_batch(&meetings).await.unwrap();

        let date = NaiveDate::from_ymd_opt(2024, 6, 10).unwrap();
        let found = repo.find_by_user_and_date(user_id(), date).await.unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].participants.len(), 2);
    }

    #[tokio::test]
    async fn test_find_by_user_and_range() {
        let pool = setup().await;
        let repo = SqliteMeetingRepository::new(pool);

        let meetings = vec![
            make_meeting("Mon", "out-1", "2024-06-10"),
            make_meeting("Wed", "out-2", "2024-06-12"),
            make_meeting("Fri", "out-3", "2024-06-14"),
        ];
        repo.upsert_batch(&meetings).await.unwrap();

        let start = NaiveDate::from_ymd_opt(2024, 6, 10).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 6, 12).unwrap();

        let found = repo
            .find_by_user_and_range(user_id(), start, end)
            .await
            .unwrap();
        assert_eq!(found.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_stale() {
        let pool = setup().await;
        let repo = SqliteMeetingRepository::new(pool);

        let meetings = vec![
            make_meeting("Keep", "keep-1", "2024-06-10"),
            make_meeting("Delete", "stale-1", "2024-06-10"),
            make_meeting("Also Delete", "stale-2", "2024-06-11"),
        ];
        repo.upsert_batch(&meetings).await.unwrap();

        let deleted = repo
            .delete_stale(user_id(), &["keep-1".to_string()])
            .await
            .unwrap();
        assert_eq!(deleted, 2);

        let date = NaiveDate::from_ymd_opt(2024, 6, 10).unwrap();
        let remaining = repo.find_by_user_and_date(user_id(), date).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].title, "Keep");
    }

    #[tokio::test]
    async fn test_find_by_project() {
        let pool = setup().await;
        let repo = SqliteMeetingRepository::new(pool.clone());

        // Create a project first
        let project_id = Uuid::new_v4();
        sqlx::query("INSERT INTO projects (id, user_id, name, source, status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(project_id.to_string())
            .bind(user_id().to_string())
            .bind("Test Project")
            .bind("personal")
            .bind("active")
            .bind("2024-01-01T00:00:00+00:00")
            .bind("2024-01-01T00:00:00+00:00")
            .execute(&pool)
            .await
            .unwrap();

        let mut m1 = make_meeting("Project Meeting", "out-1", "2024-06-10");
        m1.project_id = Some(project_id);
        let m2 = make_meeting("Other Meeting", "out-2", "2024-06-10");

        repo.upsert_batch(&[m1, m2]).await.unwrap();

        let found = repo
            .find_by_project(user_id(), project_id)
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Project Meeting");
    }

    #[tokio::test]
    async fn test_upsert_updates_existing() {
        let pool = setup().await;
        let repo = SqliteMeetingRepository::new(pool);

        let mut meeting = make_meeting("Original", "out-1", "2024-06-10");
        repo.upsert_batch(&[meeting.clone()]).await.unwrap();

        meeting.title = "Updated".to_string();
        repo.upsert_batch(&[meeting]).await.unwrap();

        let date = NaiveDate::from_ymd_opt(2024, 6, 10).unwrap();
        let found = repo.find_by_user_and_date(user_id(), date).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Updated");
    }
}
