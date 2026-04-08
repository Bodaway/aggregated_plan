use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::{TaskFilter, TaskRepository};
use domain::types::*;

use super::conversions::*;

pub struct SqliteTaskRepository {
    pool: SqlitePool,
}

impl SqliteTaskRepository {
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

fn parse_optional_datetime(s: Option<String>) -> Result<Option<DateTime<Utc>>, RepositoryError> {
    match s {
        Some(ref val) if !val.is_empty() => Ok(Some(parse_datetime(val)?)),
        _ => Ok(None),
    }
}

fn parse_optional_date(s: Option<String>) -> Result<Option<NaiveDate>, RepositoryError> {
    match s {
        Some(ref val) if !val.is_empty() => NaiveDate::parse_from_str(val, "%Y-%m-%d")
            .map(Some)
            .map_err(|e| {
                RepositoryError::Database(format!("Failed to parse date '{}': {}", val, e))
            }),
        _ => Ok(None),
    }
}

fn map_task_row(row: &SqliteRow) -> Result<Task, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let user_id_str: String = Row::get(row, "user_id");
    let source_str: String = Row::get(row, "source");
    let status_str: String = Row::get(row, "status");
    let created_at_str: String = Row::get(row, "created_at");
    let updated_at_str: String = Row::get(row, "updated_at");
    let project_id_str: Option<String> = Row::get(row, "project_id");
    let deadline_str: Option<String> = Row::get(row, "deadline");
    let planned_start_str: Option<String> = Row::get(row, "planned_start");
    let planned_end_str: Option<String> = Row::get(row, "planned_end");
    let urgency_val: i32 = Row::get(row, "urgency");
    let urgency_manual_val: i32 = Row::get(row, "urgency_manual");
    let impact_val: i32 = Row::get(row, "impact");
    let estimated_hours: Option<f64> = Row::get(row, "estimated_hours");

    let project_id = match project_id_str {
        Some(ref s) if !s.is_empty() => Some(
            Uuid::parse_str(s).map_err(|e| RepositoryError::Database(e.to_string()))?,
        ),
        _ => None,
    };

    let tracking_state_str: Option<String> = Row::try_get(row, "tracking_state").ok();
    let tracking_state = tracking_state_str
        .as_deref()
        .and_then(|s| s.parse().ok())
        .unwrap_or_default();

    Ok(Task {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        title: Row::get(row, "title"),
        description: Row::get(row, "description"),
        notes: Row::try_get(row, "notes").ok().flatten(),
        source: source_from_str(&source_str),
        source_id: Row::get(row, "source_id"),
        jira_status: Row::get(row, "jira_status"),
        status: task_status_from_str(&status_str),
        project_id,
        assignee: Row::get(row, "assignee"),
        deadline: parse_optional_date(deadline_str)?,
        planned_start: parse_optional_datetime(planned_start_str)?,
        planned_end: parse_optional_datetime(planned_end_str)?,
        estimated_hours: estimated_hours.map(|v| v as f32),
        urgency: urgency_from_i32(urgency_val),
        urgency_manual: urgency_manual_val != 0,
        impact: impact_from_i32(impact_val),
        tags: Vec::new(), // Tags are loaded separately
        tracking_state,
        jira_remaining_seconds: Row::try_get(row, "jira_remaining_seconds").ok().flatten(),
        jira_original_estimate_seconds: Row::try_get(row, "jira_original_estimate_seconds").ok().flatten(),
        jira_time_spent_seconds: Row::try_get(row, "jira_time_spent_seconds").ok().flatten(),
        remaining_hours_override: {
            let v: Option<f64> = Row::try_get(row, "remaining_hours_override").ok().flatten();
            v.map(|x| x as f32)
        },
        estimated_hours_override: {
            let v: Option<f64> = Row::try_get(row, "estimated_hours_override").ok().flatten();
            v.map(|x| x as f32)
        },
        created_at: parse_datetime(&created_at_str)?,
        updated_at: parse_datetime(&updated_at_str)?,
    })
}

/// Load tag IDs for a task from the junction table.
async fn load_tags_for_task(
    pool: &SqlitePool,
    task_id: &TaskId,
) -> Result<Vec<TagId>, RepositoryError> {
    let rows = sqlx::query("SELECT tag_id FROM task_tags WHERE task_id = ?")
        .bind(task_id.to_string())
        .fetch_all(pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

    rows.iter()
        .map(|row| {
            let tag_id_str: String = Row::get(row, "tag_id");
            Uuid::parse_str(&tag_id_str).map_err(|e| RepositoryError::Database(e.to_string()))
        })
        .collect()
}

/// Load tags for multiple tasks at once and assign them.
async fn load_tags_for_tasks(
    pool: &SqlitePool,
    tasks: &mut [Task],
) -> Result<(), RepositoryError> {
    for task in tasks.iter_mut() {
        task.tags = load_tags_for_task(pool, &task.id).await?;
    }
    Ok(())
}

/// Save tags for a task: delete existing, insert new.
async fn save_task_tags(
    pool: &SqlitePool,
    task_id: &TaskId,
    tags: &[TagId],
) -> Result<(), RepositoryError> {
    sqlx::query("DELETE FROM task_tags WHERE task_id = ?")
        .bind(task_id.to_string())
        .execute(pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

    for tag_id in tags {
        sqlx::query("INSERT INTO task_tags (task_id, tag_id) VALUES (?, ?)")
            .bind(task_id.to_string())
            .bind(tag_id.to_string())
            .execute(pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
    }

    Ok(())
}

#[async_trait]
impl TaskRepository for SqliteTaskRepository {
    async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError> {
        let rows = sqlx::query("SELECT * FROM tasks WHERE id = ?")
            .bind(id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => {
                let mut task = map_task_row(row)?;
                task.tags = load_tags_for_task(&self.pool, &task.id).await?;
                Ok(Some(task))
            }
            None => Ok(None),
        }
    }

    async fn find_by_user(
        &self,
        user_id: UserId,
        filter: &TaskFilter,
    ) -> Result<Vec<Task>, RepositoryError> {
        let mut sql = String::from("SELECT * FROM tasks WHERE user_id = ?");
        let mut bind_values: Vec<String> = vec![user_id.to_string()];

        if let Some(ref statuses) = filter.status {
            if !statuses.is_empty() {
                let placeholders: Vec<&str> = statuses.iter().map(|_| "?").collect();
                sql.push_str(&format!(" AND status IN ({})", placeholders.join(",")));
                for s in statuses {
                    bind_values.push(task_status_to_str(*s).to_string());
                }
            }
        }

        if let Some(ref sources) = filter.source {
            if !sources.is_empty() {
                let placeholders: Vec<&str> = sources.iter().map(|_| "?").collect();
                sql.push_str(&format!(" AND source IN ({})", placeholders.join(",")));
                for s in sources {
                    bind_values.push(source_to_str(*s).to_string());
                }
            }
        }

        if let Some(ref pid) = filter.project_id {
            sql.push_str(" AND project_id = ?");
            bind_values.push(pid.to_string());
        }

        if let Some(ref assignee) = filter.assignee {
            sql.push_str(" AND assignee = ?");
            bind_values.push(assignee.clone());
        }

        if let Some(ref before) = filter.deadline_before {
            sql.push_str(" AND deadline IS NOT NULL AND deadline <= ?");
            bind_values.push(before.format("%Y-%m-%d").to_string());
        }

        if let Some(ref after) = filter.deadline_after {
            sql.push_str(" AND deadline IS NOT NULL AND deadline >= ?");
            bind_values.push(after.format("%Y-%m-%d").to_string());
        }

        if let Some(ref tag_ids) = filter.tag_ids {
            if !tag_ids.is_empty() {
                let placeholders: Vec<&str> = tag_ids.iter().map(|_| "?").collect();
                sql.push_str(&format!(
                    " AND id IN (SELECT task_id FROM task_tags WHERE tag_id IN ({}))",
                    placeholders.join(",")
                ));
                for tid in tag_ids {
                    bind_values.push(tid.to_string());
                }
            }
        }

        if let Some(ref states) = filter.tracking_state {
            if !states.is_empty() {
                let placeholders: Vec<&str> = states.iter().map(|_| "?").collect();
                sql.push_str(&format!(" AND tracking_state IN ({})", placeholders.join(",")));
                for s in states {
                    bind_values.push(s.to_string());
                }
            }
        }

        sql.push_str(" ORDER BY created_at DESC");

        let mut query = sqlx::query(&sql);
        for val in &bind_values {
            query = query.bind(val);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        let mut tasks: Vec<Task> = rows.iter().map(map_task_row).collect::<Result<_, _>>()?;
        load_tags_for_tasks(&self.pool, &mut tasks).await?;

        Ok(tasks)
    }

    async fn find_by_source(
        &self,
        user_id: UserId,
        source: Source,
        source_id: &str,
    ) -> Result<Option<Task>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM tasks WHERE user_id = ? AND source = ? AND source_id = ?",
        )
        .bind(user_id.to_string())
        .bind(source_to_str(source))
        .bind(source_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => {
                let mut task = map_task_row(row)?;
                task.tags = load_tags_for_task(&self.pool, &task.id).await?;
                Ok(Some(task))
            }
            None => Ok(None),
        }
    }

    async fn find_by_date_range(
        &self,
        user_id: UserId,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Task>, RepositoryError> {
        let start_str = start.format("%Y-%m-%d").to_string();
        let end_str = end.format("%Y-%m-%d").to_string();
        let rows = sqlx::query(
            "SELECT * FROM tasks WHERE user_id = ? AND (
                (deadline IS NOT NULL AND deadline >= ? AND deadline <= ?)
                OR (planned_start IS NOT NULL AND date(planned_start) >= ? AND date(planned_start) <= ?)
            ) ORDER BY COALESCE(date(planned_start), deadline)",
        )
        .bind(user_id.to_string())
        .bind(&start_str)
        .bind(&end_str)
        .bind(&start_str)
        .bind(&end_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        let mut tasks: Vec<Task> = rows.iter().map(map_task_row).collect::<Result<_, _>>()?;
        load_tags_for_tasks(&self.pool, &mut tasks).await?;

        Ok(tasks)
    }

    async fn save(&self, task: &Task) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT OR REPLACE INTO tasks (id, user_id, title, description, notes, source, source_id, jira_status, status, project_id, assignee, deadline, planned_start, planned_end, estimated_hours, urgency, urgency_manual, impact, tracking_state, jira_remaining_seconds, jira_original_estimate_seconds, jira_time_spent_seconds, remaining_hours_override, estimated_hours_override, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(task.id.to_string())
        .bind(task.user_id.to_string())
        .bind(&task.title)
        .bind(&task.description)
        .bind(&task.notes)
        .bind(source_to_str(task.source))
        .bind(&task.source_id)
        .bind(&task.jira_status)
        .bind(task_status_to_str(task.status))
        .bind(task.project_id.map(|id| id.to_string()))
        .bind(&task.assignee)
        .bind(task.deadline.map(|d| d.format("%Y-%m-%d").to_string()))
        .bind(task.planned_start.map(|dt| dt.to_rfc3339()))
        .bind(task.planned_end.map(|dt| dt.to_rfc3339()))
        .bind(task.estimated_hours.map(|h| h as f64))
        .bind(urgency_to_i32(task.urgency))
        .bind(if task.urgency_manual { 1i32 } else { 0i32 })
        .bind(impact_to_i32(task.impact))
        .bind(task.tracking_state.to_string())
        .bind(task.jira_remaining_seconds)
        .bind(task.jira_original_estimate_seconds)
        .bind(task.jira_time_spent_seconds)
        .bind(task.remaining_hours_override.map(|h| h as f64))
        .bind(task.estimated_hours_override.map(|h| h as f64))
        .bind(task.created_at.to_rfc3339())
        .bind(task.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        save_task_tags(&self.pool, &task.id, &task.tags).await?;

        Ok(())
    }

    async fn save_batch(&self, tasks: &[Task]) -> Result<(), RepositoryError> {
        for task in tasks {
            self.save(task).await?;
        }
        Ok(())
    }

    async fn delete(&self, id: TaskId) -> Result<(), RepositoryError> {
        // task_tags will be deleted by CASCADE
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn delete_stale_by_source(
        &self,
        user_id: UserId,
        source: Source,
        keep_ids: &[String],
    ) -> Result<u64, RepositoryError> {
        let source_str = source_to_str(source);
        if keep_ids.is_empty() {
            let result = sqlx::query(
                "DELETE FROM tasks WHERE user_id = ? AND source = ?",
            )
            .bind(user_id.to_string())
            .bind(source_str)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
            return Ok(result.rows_affected());
        }

        let placeholders = keep_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "DELETE FROM tasks WHERE user_id = ? AND source = ? AND source_id NOT IN ({})",
            placeholders
        );
        let mut q = sqlx::query(&sql)
            .bind(user_id.to_string())
            .bind(source_str);
        for id in keep_ids {
            q = q.bind(id);
        }
        let result = q
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
    use crate::database::tag_repo::SqliteTagRepository;
    use application::repositories::TagRepository;

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
        // Enable foreign key enforcement for in-memory databases
        sqlx::query("PRAGMA foreign_keys = ON")
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
            description: Some("A test task".to_string()),
            notes: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::Todo,
            project_id: None,
            assignee: Some("dev@test.com".to_string()),
            deadline: Some(NaiveDate::from_ymd_opt(2024, 6, 15).unwrap()),
            planned_start: None,
            planned_end: None,
            estimated_hours: Some(4.0),
            urgency: UrgencyLevel::Medium,
            urgency_manual: false,
            impact: ImpactLevel::High,
            tags: Vec::new(),
            tracking_state: TrackingState::Inbox,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_save_and_find_by_id() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);
        let task = make_task("Test Task");

        repo.save(&task).await.unwrap();
        let found = repo.find_by_id(task.id).await.unwrap();

        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, task.id);
        assert_eq!(found.title, "Test Task");
        assert_eq!(found.source, Source::Personal);
        assert_eq!(found.status, TaskStatus::Todo);
        assert_eq!(found.urgency, UrgencyLevel::Medium);
        assert_eq!(found.impact, ImpactLevel::High);
        assert_eq!(found.estimated_hours, Some(4.0));
        assert!(!found.urgency_manual);
    }

    #[tokio::test]
    async fn test_find_by_id_not_found() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);
        let found = repo.find_by_id(Uuid::new_v4()).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_by_user_no_filter() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        repo.save(&make_task("Task 1")).await.unwrap();
        repo.save(&make_task("Task 2")).await.unwrap();

        let tasks = repo
            .find_by_user(user_id(), &TaskFilter::empty())
            .await
            .unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[tokio::test]
    async fn test_find_by_user_with_status_filter() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let mut t1 = make_task("Todo Task");
        t1.status = TaskStatus::Todo;

        let mut t2 = make_task("Done Task");
        t2.status = TaskStatus::Done;

        repo.save(&t1).await.unwrap();
        repo.save(&t2).await.unwrap();

        let filter = TaskFilter {
            status: Some(vec![TaskStatus::Done]),
            ..TaskFilter::empty()
        };
        let tasks = repo.find_by_user(user_id(), &filter).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Done Task");
    }

    #[tokio::test]
    async fn test_find_by_user_with_source_filter() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let mut t1 = make_task("Jira Task");
        t1.source = Source::Jira;
        t1.source_id = Some("JIRA-1".to_string());

        let t2 = make_task("Personal Task");

        repo.save(&t1).await.unwrap();
        repo.save(&t2).await.unwrap();

        let filter = TaskFilter {
            source: Some(vec![Source::Jira]),
            ..TaskFilter::empty()
        };
        let tasks = repo.find_by_user(user_id(), &filter).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Jira Task");
    }

    #[tokio::test]
    async fn test_find_by_source() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let mut task = make_task("Jira Task");
        task.source = Source::Jira;
        task.source_id = Some("PROJ-123".to_string());

        repo.save(&task).await.unwrap();

        let found = repo
            .find_by_source(user_id(), Source::Jira, "PROJ-123")
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().title, "Jira Task");

        let not_found = repo
            .find_by_source(user_id(), Source::Jira, "PROJ-999")
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_find_by_date_range() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let mut t1 = make_task("Early Task");
        t1.deadline = Some(NaiveDate::from_ymd_opt(2024, 3, 1).unwrap());

        let mut t2 = make_task("Mid Task");
        t2.deadline = Some(NaiveDate::from_ymd_opt(2024, 6, 15).unwrap());

        let mut t3 = make_task("Late Task");
        t3.deadline = Some(NaiveDate::from_ymd_opt(2024, 12, 31).unwrap());

        repo.save(&t1).await.unwrap();
        repo.save(&t2).await.unwrap();
        repo.save(&t3).await.unwrap();

        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 6, 30).unwrap();

        let tasks = repo
            .find_by_date_range(user_id(), start, end)
            .await
            .unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[tokio::test]
    async fn test_update_via_save() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);
        let mut task = make_task("Original");

        repo.save(&task).await.unwrap();

        task.title = "Updated".to_string();
        task.status = TaskStatus::Done;
        task.urgency = UrgencyLevel::Critical;
        task.urgency_manual = true;
        repo.save(&task).await.unwrap();

        let found = repo.find_by_id(task.id).await.unwrap().unwrap();
        assert_eq!(found.title, "Updated");
        assert_eq!(found.status, TaskStatus::Done);
        assert_eq!(found.urgency, UrgencyLevel::Critical);
        assert!(found.urgency_manual);
    }

    #[tokio::test]
    async fn test_delete() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);
        let task = make_task("To Delete");

        repo.save(&task).await.unwrap();
        assert!(repo.find_by_id(task.id).await.unwrap().is_some());

        repo.delete(task.id).await.unwrap();
        assert!(repo.find_by_id(task.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_save_batch() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let tasks = vec![make_task("Batch 1"), make_task("Batch 2"), make_task("Batch 3")];

        repo.save_batch(&tasks).await.unwrap();

        let found = repo
            .find_by_user(user_id(), &TaskFilter::empty())
            .await
            .unwrap();
        assert_eq!(found.len(), 3);
    }

    #[tokio::test]
    async fn test_task_tags_junction() {
        let pool = setup().await;
        let task_repo = SqliteTaskRepository::new(pool.clone());
        let tag_repo = SqliteTagRepository::new(pool.clone());

        // Create tags first
        let tag1 = Tag {
            id: Uuid::new_v4(),
            user_id: user_id(),
            name: "urgent".to_string(),
            color: Some("#ff0000".to_string()),
        };
        let tag2 = Tag {
            id: Uuid::new_v4(),
            user_id: user_id(),
            name: "backend".to_string(),
            color: None,
        };
        tag_repo.save(&tag1).await.unwrap();
        tag_repo.save(&tag2).await.unwrap();

        // Create task with tags
        let mut task = make_task("Tagged Task");
        task.tags = vec![tag1.id, tag2.id];
        task_repo.save(&task).await.unwrap();

        // Verify tags are loaded
        let found = task_repo.find_by_id(task.id).await.unwrap().unwrap();
        assert_eq!(found.tags.len(), 2);
        assert!(found.tags.contains(&tag1.id));
        assert!(found.tags.contains(&tag2.id));

        // Update tags (remove one)
        task.tags = vec![tag1.id];
        task_repo.save(&task).await.unwrap();

        let found = task_repo.find_by_id(task.id).await.unwrap().unwrap();
        assert_eq!(found.tags.len(), 1);
        assert!(found.tags.contains(&tag1.id));
    }

    #[tokio::test]
    async fn test_find_by_user_with_tag_filter() {
        let pool = setup().await;
        let task_repo = SqliteTaskRepository::new(pool.clone());
        let tag_repo = SqliteTagRepository::new(pool.clone());

        let tag = Tag {
            id: Uuid::new_v4(),
            user_id: user_id(),
            name: "special".to_string(),
            color: None,
        };
        tag_repo.save(&tag).await.unwrap();

        let mut tagged_task = make_task("Tagged");
        tagged_task.tags = vec![tag.id];
        task_repo.save(&tagged_task).await.unwrap();

        let untagged_task = make_task("Untagged");
        task_repo.save(&untagged_task).await.unwrap();

        let filter = TaskFilter {
            tag_ids: Some(vec![tag.id]),
            ..TaskFilter::empty()
        };
        let tasks = task_repo
            .find_by_user(user_id(), &filter)
            .await
            .unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Tagged");
    }

    #[tokio::test]
    async fn test_find_by_user_with_deadline_filter() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let mut t1 = make_task("Early");
        t1.deadline = Some(NaiveDate::from_ymd_opt(2024, 3, 1).unwrap());

        let mut t2 = make_task("Late");
        t2.deadline = Some(NaiveDate::from_ymd_opt(2024, 12, 31).unwrap());

        repo.save(&t1).await.unwrap();
        repo.save(&t2).await.unwrap();

        let filter = TaskFilter {
            deadline_before: Some(NaiveDate::from_ymd_opt(2024, 6, 1).unwrap()),
            ..TaskFilter::empty()
        };
        let tasks = repo.find_by_user(user_id(), &filter).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Early");
    }

    #[tokio::test]
    async fn save_and_read_tracking_state() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let task = Task {
            id: Uuid::new_v4(),
            user_id: user_id(),
            title: "Tracked task".to_string(),
            description: None,
            source: Source::Jira,
            source_id: Some("SCB-999".to_string()),
            jira_status: None,
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Low,
            urgency_manual: false,
            impact: ImpactLevel::Low,
            tags: vec![],
            tracking_state: TrackingState::Inbox,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            notes: None,
        };

        repo.save(&task).await.unwrap();

        let loaded = repo.find_by_id(task.id).await.unwrap().unwrap();
        assert_eq!(loaded.tracking_state, TrackingState::Inbox);

        // Filter by tracking state
        let filter = TaskFilter {
            tracking_state: Some(vec![TrackingState::Followed]),
            ..TaskFilter::empty()
        };
        let results = repo.find_by_user(user_id(), &filter).await.unwrap();
        assert!(results.is_empty()); // task is Inbox, not Followed
    }

    #[tokio::test]
    async fn save_and_read_time_tracking_fields() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let mut task = make_task("Time Tracked");
        task.source = Source::Jira;
        task.source_id = Some("PROJ-42".to_string());
        task.jira_remaining_seconds = Some(7200);
        task.jira_original_estimate_seconds = Some(14400);
        task.jira_time_spent_seconds = Some(3600);
        task.remaining_hours_override = Some(5.0);
        task.estimated_hours_override = Some(10.0);

        repo.save(&task).await.unwrap();

        let loaded = repo.find_by_id(task.id).await.unwrap().unwrap();
        assert_eq!(loaded.jira_remaining_seconds, Some(7200));
        assert_eq!(loaded.jira_original_estimate_seconds, Some(14400));
        assert_eq!(loaded.jira_time_spent_seconds, Some(3600));
        assert_eq!(loaded.remaining_hours_override, Some(5.0));
        assert_eq!(loaded.estimated_hours_override, Some(10.0));
    }

    #[tokio::test]
    async fn save_and_read_notes() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let mut task = make_task("With Notes");
        task.notes = Some("# Plan\n- step 1\n- step 2".to_string());
        repo.save(&task).await.unwrap();

        let loaded = repo.find_by_id(task.id).await.unwrap().unwrap();
        assert_eq!(loaded.notes.as_deref(), Some("# Plan\n- step 1\n- step 2"));

        // Round-trip a None value
        let mut empty = make_task("No Notes");
        empty.notes = None;
        repo.save(&empty).await.unwrap();
        let loaded_empty = repo.find_by_id(empty.id).await.unwrap().unwrap();
        assert!(loaded_empty.notes.is_none());
    }

    #[tokio::test]
    async fn save_and_read_time_tracking_nulls() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let task = make_task("No Time Data");
        repo.save(&task).await.unwrap();

        let loaded = repo.find_by_id(task.id).await.unwrap().unwrap();
        assert!(loaded.jira_remaining_seconds.is_none());
        assert!(loaded.jira_original_estimate_seconds.is_none());
        assert!(loaded.jira_time_spent_seconds.is_none());
        assert!(loaded.remaining_hours_override.is_none());
        assert!(loaded.estimated_hours_override.is_none());
    }
}
