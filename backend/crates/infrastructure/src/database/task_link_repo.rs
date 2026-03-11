use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::TaskLinkRepository;
use domain::types::*;

use super::conversions::*;

pub struct SqliteTaskLinkRepository {
    pool: SqlitePool,
}

impl SqliteTaskLinkRepository {
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

fn map_task_link_row(row: &SqliteRow) -> Result<TaskLink, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let primary_str: String = Row::get(row, "task_id_primary");
    let secondary_str: String = Row::get(row, "task_id_secondary");
    let link_type_str: String = Row::get(row, "link_type");
    let confidence_score: Option<f64> = Row::get(row, "confidence_score");
    let created_at_str: String = Row::get(row, "created_at");

    Ok(TaskLink {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        task_id_primary: Uuid::parse_str(&primary_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        task_id_secondary: Uuid::parse_str(&secondary_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        link_type: task_link_type_from_str(&link_type_str),
        confidence_score,
        created_at: parse_datetime(&created_at_str)?,
    })
}

#[async_trait]
impl TaskLinkRepository for SqliteTaskLinkRepository {
    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<TaskLink>, RepositoryError> {
        // JOIN with tasks to filter by user — a link belongs to a user if either task belongs to that user
        let rows = sqlx::query(
            "SELECT tl.* FROM task_links tl
             INNER JOIN tasks t ON tl.task_id_primary = t.id
             WHERE t.user_id = ?
             ORDER BY tl.created_at DESC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(map_task_link_row).collect()
    }

    async fn find_rejected_pairs(
        &self,
        user_id: UserId,
    ) -> Result<Vec<(TaskId, TaskId)>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT tl.task_id_primary, tl.task_id_secondary FROM task_links tl
             INNER JOIN tasks t ON tl.task_id_primary = t.id
             WHERE t.user_id = ? AND tl.link_type = 'rejected'",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter()
            .map(|row| {
                let primary_str: String = Row::get(row, "task_id_primary");
                let secondary_str: String = Row::get(row, "task_id_secondary");
                let primary = Uuid::parse_str(&primary_str)
                    .map_err(|e| RepositoryError::Database(e.to_string()))?;
                let secondary = Uuid::parse_str(&secondary_str)
                    .map_err(|e| RepositoryError::Database(e.to_string()))?;
                Ok((primary, secondary))
            })
            .collect()
    }

    async fn save(&self, link: &TaskLink) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO task_links (id, task_id_primary, task_id_secondary, link_type, confidence_score, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(link.id.to_string())
        .bind(link.task_id_primary.to_string())
        .bind(link.task_id_secondary.to_string())
        .bind(task_link_type_to_str(link.link_type))
        .bind(link.confidence_score)
        .bind(link.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, id: TaskLinkId) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM task_links WHERE id = ?")
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
    use crate::database::task_repo::SqliteTaskRepository;
    use application::repositories::TaskRepository;

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

    fn make_task(title: &str) -> Task {
        Task {
            id: Uuid::new_v4(),
            user_id: user_id(),
            title: title.to_string(),
            description: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Medium,
            urgency_manual: false,
            impact: ImpactLevel::Medium,
            tags: Vec::new(),
            tracking_state: TrackingState::Inbox,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_link(
        primary: TaskId,
        secondary: TaskId,
        link_type: TaskLinkType,
    ) -> TaskLink {
        TaskLink {
            id: Uuid::new_v4(),
            task_id_primary: primary,
            task_id_secondary: secondary,
            link_type,
            confidence_score: Some(0.85),
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_save_and_find_by_user() {
        let pool = setup().await;
        let task_repo = SqliteTaskRepository::new(pool.clone());
        let link_repo = SqliteTaskLinkRepository::new(pool);

        let t1 = make_task("Task A");
        let t2 = make_task("Task B");
        task_repo.save(&t1).await.unwrap();
        task_repo.save(&t2).await.unwrap();

        let link = make_link(t1.id, t2.id, TaskLinkType::AutoMerged);
        link_repo.save(&link).await.unwrap();

        let found = link_repo.find_by_user(user_id()).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].task_id_primary, t1.id);
        assert_eq!(found[0].task_id_secondary, t2.id);
        assert_eq!(found[0].link_type, TaskLinkType::AutoMerged);
        assert_eq!(found[0].confidence_score, Some(0.85));
    }

    #[tokio::test]
    async fn test_find_rejected_pairs() {
        let pool = setup().await;
        let task_repo = SqliteTaskRepository::new(pool.clone());
        let link_repo = SqliteTaskLinkRepository::new(pool);

        let t1 = make_task("Task A");
        let t2 = make_task("Task B");
        let t3 = make_task("Task C");
        task_repo.save(&t1).await.unwrap();
        task_repo.save(&t2).await.unwrap();
        task_repo.save(&t3).await.unwrap();

        link_repo
            .save(&make_link(t1.id, t2.id, TaskLinkType::Rejected))
            .await
            .unwrap();
        link_repo
            .save(&make_link(t1.id, t3.id, TaskLinkType::AutoMerged))
            .await
            .unwrap();

        let rejected = link_repo.find_rejected_pairs(user_id()).await.unwrap();
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0], (t1.id, t2.id));
    }

    #[tokio::test]
    async fn test_delete() {
        let pool = setup().await;
        let task_repo = SqliteTaskRepository::new(pool.clone());
        let link_repo = SqliteTaskLinkRepository::new(pool);

        let t1 = make_task("Task A");
        let t2 = make_task("Task B");
        task_repo.save(&t1).await.unwrap();
        task_repo.save(&t2).await.unwrap();

        let link = make_link(t1.id, t2.id, TaskLinkType::AutoMerged);
        link_repo.save(&link).await.unwrap();

        link_repo.delete(link.id).await.unwrap();

        let found = link_repo.find_by_user(user_id()).await.unwrap();
        assert!(found.is_empty());
    }
}
