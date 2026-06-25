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
        delegated_to: Row::try_get(row, "delegated_to").ok().flatten(),
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
        recurrence_id: {
            let rid: Option<String> = Row::try_get(row, "recurrence_id")
                .map_err(|e| RepositoryError::Database(e.to_string()))?;
            rid.map(|s| {
                s.parse::<RecurrenceTemplateId>()
                    .map_err(|e| RepositoryError::Database(format!("invalid recurrence_id '{}': {}", s, e)))
            }).transpose()?
        },
        occurrence_date: {
            let od: Option<String> = Row::try_get(row, "occurrence_date")
                .map_err(|e| RepositoryError::Database(e.to_string()))?;
            parse_optional_date(od)?
        },
        gryzzly_task_id: row.try_get("gryzzly_task_id").ok().flatten(),
        gryzzly_project_id: row.try_get("gryzzly_project_id").ok().flatten(),
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
        // Exclude tasks that are the loser (task_id_secondary) of any merge link.
        // Rejected links must NOT hide anything.
        let mut sql = String::from(
            "SELECT t.* FROM tasks t \
             WHERE t.user_id = ? \
             AND t.id NOT IN ( \
               SELECT tl.task_id_secondary FROM task_links tl \
               WHERE tl.link_type IN ('auto_merged','manual_merged') \
             )",
        );
        let mut bind_values: Vec<String> = vec![user_id.to_string()];

        if let Some(ref statuses) = filter.status {
            if !statuses.is_empty() {
                let placeholders: Vec<&str> = statuses.iter().map(|_| "?").collect();
                sql.push_str(&format!(" AND t.status IN ({})", placeholders.join(",")));
                for s in statuses {
                    bind_values.push(task_status_to_str(*s).to_string());
                }
            }
        }

        if let Some(ref sources) = filter.source {
            if !sources.is_empty() {
                let placeholders: Vec<&str> = sources.iter().map(|_| "?").collect();
                sql.push_str(&format!(" AND t.source IN ({})", placeholders.join(",")));
                for s in sources {
                    bind_values.push(source_to_str(*s).to_string());
                }
            }
        }

        if let Some(ref pid) = filter.project_id {
            sql.push_str(" AND t.project_id = ?");
            bind_values.push(pid.to_string());
        }

        if let Some(ref assignee) = filter.assignee {
            sql.push_str(" AND t.assignee = ?");
            bind_values.push(assignee.clone());
        }

        if let Some(ref before) = filter.deadline_before {
            sql.push_str(" AND t.deadline IS NOT NULL AND t.deadline <= ?");
            bind_values.push(before.format("%Y-%m-%d").to_string());
        }

        if let Some(ref after) = filter.deadline_after {
            sql.push_str(" AND t.deadline IS NOT NULL AND t.deadline >= ?");
            bind_values.push(after.format("%Y-%m-%d").to_string());
        }

        if let Some(ref tag_ids) = filter.tag_ids {
            if !tag_ids.is_empty() {
                let placeholders: Vec<&str> = tag_ids.iter().map(|_| "?").collect();
                sql.push_str(&format!(
                    " AND t.id IN (SELECT task_id FROM task_tags WHERE tag_id IN ({}))",
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
                sql.push_str(&format!(" AND t.tracking_state IN ({})", placeholders.join(",")));
                for s in states {
                    bind_values.push(s.to_string());
                }
            }
        }

        if let Some(ref sid) = filter.source_id {
            sql.push_str(" AND t.source_id = ?");
            bind_values.push(sid.clone());
        }

        if let Some(ref needle) = filter.title_contains {
            sql.push_str(" AND LOWER(t.title) LIKE ?");
            bind_values.push(format!("%{}%", needle.to_lowercase()));
        }

        sql.push_str(" ORDER BY t.created_at DESC");

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
        // Exclude merged losers (task_id_secondary of AutoMerged/ManualMerged links).
        let rows = sqlx::query(
            "SELECT t.* FROM tasks t \
             WHERE t.user_id = ? \
             AND t.id NOT IN ( \
               SELECT tl.task_id_secondary FROM task_links tl \
               WHERE tl.link_type IN ('auto_merged','manual_merged') \
             ) \
             AND ( \
               (t.deadline IS NOT NULL AND t.deadline >= ? AND t.deadline <= ?) \
               OR (t.planned_start IS NOT NULL AND date(t.planned_start) >= ? AND date(t.planned_start) <= ?) \
             ) ORDER BY COALESCE(date(t.planned_start), t.deadline)",
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

    async fn find_planned_before(
        &self,
        user_id: UserId,
        before_date: NaiveDate,
    ) -> Result<Vec<Task>, RepositoryError> {
        let before_str = before_date.format("%Y-%m-%d").to_string();
        let rows = sqlx::query(
            "SELECT * FROM tasks \
             WHERE user_id = ? \
               AND planned_start IS NOT NULL \
               AND date(planned_start) < ? \
               AND status NOT IN ('done', 'cancelled') \
             ORDER BY planned_start",
        )
        .bind(user_id.to_string())
        .bind(&before_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        let mut tasks: Vec<Task> = rows.iter().map(map_task_row).collect::<Result<_, _>>()?;
        load_tags_for_tasks(&self.pool, &mut tasks).await?;
        Ok(tasks)
    }

    async fn save(&self, task: &Task) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT OR REPLACE INTO tasks (id, user_id, title, description, notes, source, source_id, jira_status, status, project_id, assignee, delegated_to, deadline, planned_start, planned_end, estimated_hours, urgency, urgency_manual, impact, tracking_state, jira_remaining_seconds, jira_original_estimate_seconds, jira_time_spent_seconds, remaining_hours_override, estimated_hours_override, recurrence_id, occurrence_date, gryzzly_task_id, gryzzly_project_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(&task.delegated_to)
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
        .bind(task.recurrence_id.map(|id| id.to_string()))
        .bind(task.occurrence_date.map(|d| d.format("%Y-%m-%d").to_string()))
        .bind(&task.gryzzly_task_id)
        .bind(&task.gryzzly_project_id)
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

    async fn find_by_recurrence_slot(
        &self,
        template_id: domain::types::recurrence::RecurrenceTemplateId,
        occurrence_date: NaiveDate,
    ) -> Result<Option<Task>, RepositoryError> {
        let date_str = occurrence_date.format("%Y-%m-%d").to_string();
        let rows = sqlx::query(
            "SELECT * FROM tasks WHERE recurrence_id = ? AND occurrence_date = ? LIMIT 1",
        )
        .bind(template_id.to_string())
        .bind(&date_str)
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

    async fn find_by_recurrence(
        &self,
        template_id: domain::types::recurrence::RecurrenceTemplateId,
    ) -> Result<Vec<Task>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM tasks WHERE recurrence_id = ? ORDER BY occurrence_date",
        )
        .bind(template_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        let mut tasks: Vec<Task> = rows.iter().map(map_task_row).collect::<Result<_, _>>()?;
        load_tags_for_tasks(&self.pool, &mut tasks).await?;

        Ok(tasks)
    }

    async fn list_delegates(&self, user_id: UserId) -> Result<Vec<String>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT DISTINCT delegated_to FROM tasks \
             WHERE user_id = ? AND delegated_to IS NOT NULL \
             ORDER BY delegated_to",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|row| Row::get(row, "delegated_to"))
            .collect())
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
            delegated_to: None,
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
            recurrence_id: None,
            occurrence_date: None,
            gryzzly_task_id: None,
            gryzzly_project_id: None,
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
    async fn test_find_by_user_with_source_id_filter() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let mut t1 = make_task("Auth migration");
        t1.source = Source::Jira;
        t1.source_id = Some("AP-123".to_string());

        let mut t2 = make_task("Database backup");
        t2.source = Source::Jira;
        t2.source_id = Some("AP-456".to_string());

        repo.save(&t1).await.unwrap();
        repo.save(&t2).await.unwrap();

        let filter = TaskFilter {
            source_id: Some("AP-123".to_string()),
            ..TaskFilter::empty()
        };
        let tasks = repo.find_by_user(user_id(), &filter).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Auth migration");
    }

    #[tokio::test]
    async fn test_find_by_user_with_title_contains_filter() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        repo.save(&make_task("Auth migration")).await.unwrap();
        repo.save(&make_task("Authorize users")).await.unwrap();
        repo.save(&make_task("Database backup")).await.unwrap();

        let filter = TaskFilter {
            title_contains: Some("auth".to_string()),
            ..TaskFilter::empty()
        };
        let tasks = repo.find_by_user(user_id(), &filter).await.unwrap();
        assert_eq!(tasks.len(), 2);
        let titles: Vec<&str> = tasks.iter().map(|t| t.title.as_str()).collect();
        assert!(titles.contains(&"Auth migration"));
        assert!(titles.contains(&"Authorize users"));
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
            delegated_to: None,
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
            recurrence_id: None,
            occurrence_date: None,
            gryzzly_task_id: None,
            gryzzly_project_id: None,
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
    async fn save_and_read_delegated_to() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let mut task = make_task("Delegated");
        task.delegated_to = Some("Marie".to_string());
        repo.save(&task).await.unwrap();

        let loaded = repo.find_by_id(task.id).await.unwrap().unwrap();
        assert_eq!(loaded.delegated_to.as_deref(), Some("Marie"));

        // Clearing: save with None overwrites the previous value
        task.delegated_to = None;
        repo.save(&task).await.unwrap();
        let cleared = repo.find_by_id(task.id).await.unwrap().unwrap();
        assert!(cleared.delegated_to.is_none());
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

    #[tokio::test]
    async fn find_planned_before_returns_only_active_past_tasks() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let last_monday = NaiveDate::from_ymd_opt(2026, 4, 6).unwrap();
        let this_monday = NaiveDate::from_ymd_opt(2026, 4, 13).unwrap();

        // Active task planned last Monday — should be returned
        let mut stale = make_task("Stale");
        stale.planned_start = Some(last_monday.and_hms_opt(8, 0, 0).unwrap().and_utc());
        stale.status = TaskStatus::Todo;
        repo.save(&stale).await.unwrap();

        // Done task planned last Monday — must NOT be returned
        let mut done = make_task("Done");
        done.planned_start = Some(last_monday.and_hms_opt(8, 0, 0).unwrap().and_utc());
        done.status = TaskStatus::Done;
        repo.save(&done).await.unwrap();

        // Task planned this Monday — must NOT be returned
        let mut current = make_task("Current");
        current.planned_start = Some(this_monday.and_hms_opt(8, 0, 0).unwrap().and_utc());
        current.status = TaskStatus::Todo;
        repo.save(&current).await.unwrap();

        // Task with no planned_start — must NOT be returned
        let no_date = make_task("No Date");
        repo.save(&no_date).await.unwrap();

        let results = repo.find_planned_before(user_id(), this_monday).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Stale");
    }

    // Test 8: find_by_recurrence_slot and find_by_recurrence return correct tasks
    #[tokio::test]
    async fn find_by_recurrence_slot_and_list() {
        use domain::types::recurrence::RecurrenceTemplateId;

        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool.clone());

        let template_id = RecurrenceTemplateId::new();
        let occurrence = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();

        // Insert a stub recurrence template row so the FK constraint is satisfied.
        sqlx::query(
            "INSERT INTO task_recurrences \
             (id, user_id, title, urgency, urgency_manual, impact, rule_json, starts_on, active, created_at, updated_at) \
             VALUES (?, ?, 'stub', 2, 0, 2, '{\"kind\":\"daily\",\"interval\":1}', '2026-01-01', 1, ?, ?)",
        )
        .bind(template_id.to_string())
        .bind(user_id().to_string())
        .bind("2026-01-01T00:00:00+00:00")
        .bind("2026-01-01T00:00:00+00:00")
        .execute(&pool)
        .await
        .unwrap();

        let mut task = make_task("Recurring instance");
        task.recurrence_id = Some(template_id);
        task.occurrence_date = Some(occurrence);
        repo.save(&task).await.unwrap();

        // find_by_recurrence_slot should return the task
        let found = repo
            .find_by_recurrence_slot(template_id, occurrence)
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, task.id);

        // find_by_recurrence should return it in a list
        let list = repo.find_by_recurrence(template_id).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, task.id);

        // find_by_recurrence_slot with a different date returns None
        let other_date = NaiveDate::from_ymd_opt(2026, 5, 2).unwrap();
        let not_found = repo
            .find_by_recurrence_slot(template_id, other_date)
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    // Test 9: Unique partial index on (recurrence_id, occurrence_date).
    //
    // SQLite's INSERT OR REPLACE resolves the unique-index conflict by deleting the old row
    // and inserting the new one (not an error). So saving a second task with the same slot
    // silently overwrites the first. The use-case layer prevents this from happening in
    // practice by calling `find_by_recurrence_slot` before every materialization save.
    // This test documents the actual DB behavior.
    #[tokio::test]
    async fn recurrence_slot_unique_index_insert_or_replace_overwrites() {
        use domain::types::recurrence::RecurrenceTemplateId;

        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool.clone());

        let template_id = RecurrenceTemplateId::new();
        let occurrence = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();

        sqlx::query(
            "INSERT INTO task_recurrences \
             (id, user_id, title, urgency, urgency_manual, impact, rule_json, starts_on, active, created_at, updated_at) \
             VALUES (?, ?, 'stub', 2, 0, 2, '{\"kind\":\"daily\",\"interval\":1}', '2026-01-01', 1, ?, ?)",
        )
        .bind(template_id.to_string())
        .bind(user_id().to_string())
        .bind("2026-01-01T00:00:00+00:00")
        .bind("2026-01-01T00:00:00+00:00")
        .execute(&pool)
        .await
        .unwrap();

        let mut task1 = make_task("First");
        task1.recurrence_id = Some(template_id);
        task1.occurrence_date = Some(occurrence);
        repo.save(&task1).await.unwrap();

        // INSERT OR REPLACE: when a second distinct task (different PK) has the same
        // (recurrence_id, occurrence_date), SQLite deletes the conflicting row (task1)
        // and inserts task2. End result: slot still holds exactly one row.
        let mut task2 = make_task("Second");
        task2.recurrence_id = Some(template_id);
        task2.occurrence_date = Some(occurrence);
        repo.save(&task2).await.unwrap(); // succeeds — old row is replaced

        // Exactly one row remains for the slot, and it is task2
        let list = repo.find_by_recurrence(template_id).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Second");

        let found = repo
            .find_by_recurrence_slot(template_id, occurrence)
            .await
            .unwrap()
            .expect("slot must exist");
        assert_eq!(found.title, "Second");
    }

    #[tokio::test]
    async fn list_delegates_returns_distinct_sorted_names() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let mut t1 = make_task("A");
        t1.delegated_to = Some("Marie".to_string());
        let mut t2 = make_task("B");
        t2.delegated_to = Some("Ahmed".to_string());
        let mut t3 = make_task("C");
        t3.delegated_to = Some("Marie".to_string()); // duplicate
        let t4 = make_task("D"); // not delegated
        for t in [&t1, &t2, &t3, &t4] {
            repo.save(t).await.unwrap();
        }

        let names = repo.list_delegates(user_id()).await.unwrap();
        assert_eq!(names, vec!["Ahmed".to_string(), "Marie".to_string()]);
    }

    // ─── Merge-loser exclusion tests ───

    /// After A(primary)/B(secondary) AutoMerged link is created,
    /// find_by_user excludes B (the loser) but still returns A (the survivor).
    #[tokio::test]
    async fn find_by_user_excludes_merged_loser() {
        use crate::database::task_link_repo::SqliteTaskLinkRepository;
        use application::repositories::TaskLinkRepository;

        let pool = setup().await;
        let task_repo = SqliteTaskRepository::new(pool.clone());
        let link_repo = SqliteTaskLinkRepository::new(pool.clone());

        let survivor = make_task("Survivor Task");
        let loser = make_task("Loser Task");
        let survivor_id = survivor.id;
        let loser_id = loser.id;
        task_repo.save(&survivor).await.unwrap();
        task_repo.save(&loser).await.unwrap();

        // Create AutoMerged link: survivor = primary, loser = secondary
        let link = TaskLink {
            id: Uuid::new_v4(),
            task_id_primary: survivor_id,
            task_id_secondary: loser_id,
            link_type: TaskLinkType::AutoMerged,
            confidence_score: Some(1.0),
            created_at: Utc::now(),
        };
        link_repo.save(&link).await.unwrap();

        let tasks = task_repo
            .find_by_user(user_id(), &TaskFilter::empty())
            .await
            .unwrap();

        let ids: Vec<_> = tasks.iter().map(|t| t.id).collect();
        assert!(ids.contains(&survivor_id), "survivor must be visible");
        assert!(!ids.contains(&loser_id), "loser must be hidden");
    }

    /// After A(primary)/B(secondary) ManualMerged link is created,
    /// find_by_user excludes B.
    #[tokio::test]
    async fn find_by_user_excludes_manual_merged_loser() {
        use crate::database::task_link_repo::SqliteTaskLinkRepository;
        use application::repositories::TaskLinkRepository;

        let pool = setup().await;
        let task_repo = SqliteTaskRepository::new(pool.clone());
        let link_repo = SqliteTaskLinkRepository::new(pool.clone());

        let survivor = make_task("Manual Survivor");
        let loser = make_task("Manual Loser");
        let survivor_id = survivor.id;
        let loser_id = loser.id;
        task_repo.save(&survivor).await.unwrap();
        task_repo.save(&loser).await.unwrap();

        let link = TaskLink {
            id: Uuid::new_v4(),
            task_id_primary: survivor_id,
            task_id_secondary: loser_id,
            link_type: TaskLinkType::ManualMerged,
            confidence_score: None,
            created_at: Utc::now(),
        };
        link_repo.save(&link).await.unwrap();

        let tasks = task_repo
            .find_by_user(user_id(), &TaskFilter::empty())
            .await
            .unwrap();

        let ids: Vec<_> = tasks.iter().map(|t| t.id).collect();
        assert!(ids.contains(&survivor_id));
        assert!(!ids.contains(&loser_id));
    }

    /// Rejected links must NOT hide the secondary task.
    #[tokio::test]
    async fn find_by_user_does_not_hide_rejected_secondary() {
        use crate::database::task_link_repo::SqliteTaskLinkRepository;
        use application::repositories::TaskLinkRepository;

        let pool = setup().await;
        let task_repo = SqliteTaskRepository::new(pool.clone());
        let link_repo = SqliteTaskLinkRepository::new(pool.clone());

        let t1 = make_task("Task A");
        let t2 = make_task("Task B");
        let t1_id = t1.id;
        let t2_id = t2.id;
        task_repo.save(&t1).await.unwrap();
        task_repo.save(&t2).await.unwrap();

        let link = TaskLink {
            id: Uuid::new_v4(),
            task_id_primary: t1_id,
            task_id_secondary: t2_id,
            link_type: TaskLinkType::Rejected,
            confidence_score: None,
            created_at: Utc::now(),
        };
        link_repo.save(&link).await.unwrap();

        let tasks = task_repo
            .find_by_user(user_id(), &TaskFilter::empty())
            .await
            .unwrap();

        assert_eq!(tasks.len(), 2, "Rejected link must not hide either task");
    }

    /// find_by_date_range also excludes merged losers.
    #[tokio::test]
    async fn find_by_date_range_excludes_merged_loser() {
        use crate::database::task_link_repo::SqliteTaskLinkRepository;
        use application::repositories::TaskLinkRepository;

        let pool = setup().await;
        let task_repo = SqliteTaskRepository::new(pool.clone());
        let link_repo = SqliteTaskLinkRepository::new(pool.clone());

        let deadline = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();

        let mut survivor = make_task("Survivor Dated");
        survivor.deadline = Some(deadline);
        let mut loser = make_task("Loser Dated");
        loser.deadline = Some(deadline);
        let survivor_id = survivor.id;
        let loser_id = loser.id;

        task_repo.save(&survivor).await.unwrap();
        task_repo.save(&loser).await.unwrap();

        let link = TaskLink {
            id: Uuid::new_v4(),
            task_id_primary: survivor_id,
            task_id_secondary: loser_id,
            link_type: TaskLinkType::AutoMerged,
            confidence_score: Some(1.0),
            created_at: Utc::now(),
        };
        link_repo.save(&link).await.unwrap();

        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let tasks = task_repo
            .find_by_date_range(user_id(), start, end)
            .await
            .unwrap();

        let ids: Vec<_> = tasks.iter().map(|t| t.id).collect();
        assert!(ids.contains(&survivor_id), "survivor must appear");
        assert!(!ids.contains(&loser_id), "loser must be hidden");
    }

    #[tokio::test]
    async fn task_persists_gryzzly_assignment() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);
        let mut t = make_task("Gryzzly Task");
        t.gryzzly_task_id = Some("g-123".into());
        t.gryzzly_project_id = Some("p-9".into());
        repo.save(&t).await.unwrap();

        let loaded = repo.find_by_id(t.id).await.unwrap().unwrap();
        assert_eq!(loaded.gryzzly_task_id.as_deref(), Some("g-123"));
        assert_eq!(loaded.gryzzly_project_id.as_deref(), Some("p-9"));
    }

    // Test 10: find_planned_before excludes BOTH Done AND Cancelled tasks (BLOCKER regression)
    #[tokio::test]
    async fn find_planned_before_excludes_done_and_cancelled() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let past = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        let cutoff = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();

        let mut todo_task = make_task("Todo past");
        todo_task.planned_start = Some(past.and_hms_opt(8, 0, 0).unwrap().and_utc());
        todo_task.status = TaskStatus::Todo;
        repo.save(&todo_task).await.unwrap();

        let mut done_task = make_task("Done past");
        done_task.planned_start = Some(past.and_hms_opt(8, 0, 0).unwrap().and_utc());
        done_task.status = TaskStatus::Done;
        repo.save(&done_task).await.unwrap();

        let mut cancelled_task = make_task("Cancelled past");
        cancelled_task.planned_start = Some(past.and_hms_opt(8, 0, 0).unwrap().and_utc());
        cancelled_task.status = TaskStatus::Cancelled;
        repo.save(&cancelled_task).await.unwrap();

        let results = repo.find_planned_before(user_id(), cutoff).await.unwrap();

        // Only the Todo task must be returned
        assert_eq!(results.len(), 1, "Expected 1 result, got: {:?}", results.iter().map(|t| &t.title).collect::<Vec<_>>());
        assert_eq!(results[0].title, "Todo past");
    }
}
