use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::ProjectRepository;
use domain::types::*;

use super::conversions::*;

pub struct SqliteProjectRepository {
    pool: SqlitePool,
}

impl SqliteProjectRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn map_project_row(row: &SqliteRow) -> Result<Project, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let user_id_str: String = Row::get(row, "user_id");
    let source_str: String = Row::get(row, "source");
    let status_str: String = Row::get(row, "status");
    let created_at_str: String = Row::get(row, "created_at");
    let updated_at_str: String = Row::get(row, "updated_at");

    Ok(Project {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        name: Row::get(row, "name"),
        source: source_from_str(&source_str),
        source_id: Row::get(row, "source_id"),
        status: project_status_from_str(&status_str),
        created_at: parse_datetime(&created_at_str)?,
        updated_at: parse_datetime(&updated_at_str)?,
    })
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    // Try RFC 3339 first, then other common formats
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

#[async_trait]
impl ProjectRepository for SqliteProjectRepository {
    async fn find_by_id(&self, id: ProjectId) -> Result<Option<Project>, RepositoryError> {
        let rows = sqlx::query("SELECT * FROM projects WHERE id = ?")
            .bind(id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => Ok(Some(map_project_row(row)?)),
            None => Ok(None),
        }
    }

    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<Project>, RepositoryError> {
        let rows = sqlx::query("SELECT * FROM projects WHERE user_id = ? ORDER BY name")
            .bind(user_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(map_project_row).collect()
    }

    async fn find_by_source(
        &self,
        user_id: UserId,
        source: Source,
        source_id: &str,
    ) -> Result<Option<Project>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM projects WHERE user_id = ? AND source = ? AND source_id = ?",
        )
        .bind(user_id.to_string())
        .bind(source_to_str(source))
        .bind(source_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => Ok(Some(map_project_row(row)?)),
            None => Ok(None),
        }
    }

    async fn save(&self, project: &Project) -> Result<(), RepositoryError> {
        // A true upsert, NOT `INSERT OR REPLACE`: SQLite resolves a REPLACE conflict by
        // DELETING the conflicting row first, which fires the `ON DELETE SET NULL` of
        // `tasks.project_id`, `meetings.project_id`, `task_recurrences.project_id` and
        // `memories.project_id`. Merely renaming a project used to unassign everything
        // that belonged to it. `ON CONFLICT(id) DO UPDATE` updates the row in place.
        sqlx::query(
            "INSERT INTO projects (id, user_id, name, source, source_id, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                user_id = excluded.user_id,
                name = excluded.name,
                source = excluded.source,
                source_id = excluded.source_id,
                status = excluded.status,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
        )
        .bind(project.id.to_string())
        .bind(project.user_id.to_string())
        .bind(&project.name)
        .bind(source_to_str(project.source))
        .bind(&project.source_id)
        .bind(project_status_to_str(project.status))
        .bind(project.created_at.to_rfc3339())
        .bind(project.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, id: ProjectId) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM projects WHERE id = ?")
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

    fn make_project(name: &str) -> Project {
        Project {
            id: Uuid::new_v4(),
            user_id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            name: name.to_string(),
            source: Source::Jira,
            source_id: Some("PROJ-1".to_string()),
            status: ProjectStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_save_and_find_by_id() {
        let pool = setup().await;
        let repo = SqliteProjectRepository::new(pool);
        let project = make_project("Test Project");

        repo.save(&project).await.unwrap();
        let found = repo.find_by_id(project.id).await.unwrap();

        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, project.id);
        assert_eq!(found.name, "Test Project");
        assert_eq!(found.source, Source::Jira);
        assert_eq!(found.status, ProjectStatus::Active);
    }

    #[tokio::test]
    async fn test_find_by_id_not_found() {
        let pool = setup().await;
        let repo = SqliteProjectRepository::new(pool);
        let found = repo.find_by_id(Uuid::new_v4()).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_by_user() {
        let pool = setup().await;
        let repo = SqliteProjectRepository::new(pool);

        let p1 = make_project("Alpha");
        let mut p2 = make_project("Beta");
        p2.source_id = Some("PROJ-2".to_string());

        repo.save(&p1).await.unwrap();
        repo.save(&p2).await.unwrap();

        let user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let projects = repo.find_by_user(user_id).await.unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "Alpha");
        assert_eq!(projects[1].name, "Beta");
    }

    #[tokio::test]
    async fn test_find_by_source() {
        let pool = setup().await;
        let repo = SqliteProjectRepository::new(pool);
        let project = make_project("Source Project");

        repo.save(&project).await.unwrap();

        let user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let found = repo
            .find_by_source(user_id, Source::Jira, "PROJ-1")
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Source Project");

        let not_found = repo
            .find_by_source(user_id, Source::Excel, "PROJ-1")
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_update_via_save() {
        let pool = setup().await;
        let repo = SqliteProjectRepository::new(pool);
        let mut project = make_project("Original");

        repo.save(&project).await.unwrap();

        project.name = "Updated".to_string();
        project.status = ProjectStatus::Completed;
        repo.save(&project).await.unwrap();

        let found = repo.find_by_id(project.id).await.unwrap().unwrap();
        assert_eq!(found.name, "Updated");
        assert_eq!(found.status, ProjectStatus::Completed);
    }

    #[tokio::test]
    async fn test_delete() {
        let pool = setup().await;
        let repo = SqliteProjectRepository::new(pool);
        let project = make_project("To Delete");

        repo.save(&project).await.unwrap();
        assert!(repo.find_by_id(project.id).await.unwrap().is_some());

        repo.delete(project.id).await.unwrap();
        assert!(repo.find_by_id(project.id).await.unwrap().is_none());
    }

    /// `tasks.project_id REFERENCES projects(id) ON DELETE SET NULL` (migrations 001/007).
    ///
    /// SQLite resolves a REPLACE conflict by DELETING the conflicting row first, which
    /// fires that action: an `INSERT OR REPLACE INTO projects` silently unassigned every
    /// task of the project it was merely updating. `save` must UPDATE in place.
    /// Foreign keys are enforced here exactly as in production — `create_sqlite_pool`
    /// sets `.foreign_keys(true)` on the connect options.
    #[tokio::test]
    async fn save_preserves_task_project_assignment() {
        let pool = setup().await;
        let repo = SqliteProjectRepository::new(pool.clone());

        let mut project = make_project("Original");
        repo.save(&project).await.unwrap();

        let task_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tasks \
             (id, user_id, title, source, status, urgency, impact, project_id, created_at, updated_at) \
             VALUES (?, ?, 'A task on the project', 'personal', 'todo', 2, 2, ?, ?, ?)",
        )
        .bind(task_id.to_string())
        .bind("00000000-0000-0000-0000-000000000001")
        .bind(project.id.to_string())
        .bind("2026-08-11T09:00:00+00:00")
        .bind("2026-08-11T09:00:00+00:00")
        .execute(&pool)
        .await
        .unwrap();

        project.name = "Renamed".to_string();
        project.status = ProjectStatus::Paused;
        repo.save(&project).await.unwrap();

        let stored: (Option<String>,) = sqlx::query_as("SELECT project_id FROM tasks WHERE id = ?")
            .bind(task_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            stored.0,
            Some(project.id.to_string()),
            "saving a project must not null out its tasks' project_id"
        );

        let found = repo.find_by_id(project.id).await.unwrap().unwrap();
        assert_eq!(found.name, "Renamed");
        assert_eq!(found.status, ProjectStatus::Paused);
    }
}
