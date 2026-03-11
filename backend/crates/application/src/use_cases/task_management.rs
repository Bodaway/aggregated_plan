use chrono::{DateTime, NaiveDate, Utc};
use domain::rules::urgency::calculate_urgency;
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::*;

/// Input data for creating a new personal task.
pub struct CreateTaskInput {
    pub title: String,
    pub description: Option<String>,
    pub project_id: Option<ProjectId>,
    pub deadline: Option<NaiveDate>,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub estimated_hours: Option<f32>,
    pub impact: Option<ImpactLevel>,
    pub urgency: Option<UrgencyLevel>,
    pub tags: Vec<TagId>,
}

/// Input data for updating an existing task.
pub struct UpdateTaskInput {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub project_id: Option<Option<ProjectId>>,
    pub deadline: Option<Option<NaiveDate>>,
    pub planned_start: Option<Option<DateTime<Utc>>>,
    pub planned_end: Option<Option<DateTime<Utc>>>,
    pub estimated_hours: Option<Option<f32>>,
    pub status: Option<TaskStatus>,
    pub impact: Option<ImpactLevel>,
    pub urgency: Option<UrgencyLevel>,
    pub tags: Option<Vec<TagId>>,
}

/// Create a new personal task with auto-calculated urgency if not provided.
pub async fn create_personal_task(
    task_repo: &dyn TaskRepository,
    user_id: UserId,
    input: CreateTaskInput,
    today: NaiveDate,
) -> Result<Task, AppError> {
    let now = Utc::now();

    let (urgency, urgency_manual) = match input.urgency {
        Some(u) => (u, true),
        None => (calculate_urgency(input.deadline, today), false),
    };

    let impact = input.impact.unwrap_or(ImpactLevel::Medium);

    let task = Task {
        id: Uuid::new_v4(),
        user_id,
        title: input.title,
        description: input.description,
        source: Source::Personal,
        source_id: None,
        jira_status: None,
        status: TaskStatus::Todo,
        project_id: input.project_id,
        assignee: None,
        deadline: input.deadline,
        planned_start: input.planned_start,
        planned_end: input.planned_end,
        estimated_hours: input.estimated_hours,
        urgency,
        urgency_manual,
        impact,
        tags: input.tags,
        tracking_state: TrackingState::Followed,
        created_at: now,
        updated_at: now,
    };

    task_repo.save(&task).await?;
    Ok(task)
}

/// Retrieve a single task by its identifier.
pub async fn get_task(
    task_repo: &dyn TaskRepository,
    task_id: TaskId,
) -> Result<Option<Task>, AppError> {
    let task = task_repo.find_by_id(task_id).await?;
    Ok(task)
}

/// Retrieve tasks for a user with optional filtering.
pub async fn get_tasks(
    task_repo: &dyn TaskRepository,
    user_id: UserId,
    filter: &TaskFilter,
) -> Result<Vec<Task>, AppError> {
    let tasks = task_repo.find_by_user(user_id, filter).await?;
    Ok(tasks)
}

/// Update an existing task. Returns the updated task.
pub async fn update_task(
    task_repo: &dyn TaskRepository,
    task_id: TaskId,
    input: UpdateTaskInput,
    today: NaiveDate,
) -> Result<Task, AppError> {
    let mut task = task_repo
        .find_by_id(task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {}", task_id)))?;

    if let Some(title) = input.title {
        task.title = title;
    }
    if let Some(description) = input.description {
        task.description = description;
    }
    if let Some(project_id) = input.project_id {
        task.project_id = project_id;
    }
    if let Some(deadline) = input.deadline {
        task.deadline = deadline;
        // Recalculate urgency if not manually set
        if !task.urgency_manual {
            task.urgency = calculate_urgency(task.deadline, today);
        }
    }
    if let Some(planned_start) = input.planned_start {
        task.planned_start = planned_start;
    }
    if let Some(planned_end) = input.planned_end {
        task.planned_end = planned_end;
    }
    if let Some(estimated_hours) = input.estimated_hours {
        task.estimated_hours = estimated_hours;
    }
    if let Some(status) = input.status {
        task.status = status;
    }
    if let Some(impact) = input.impact {
        task.impact = impact;
    }
    if let Some(urgency) = input.urgency {
        task.urgency = urgency;
        task.urgency_manual = true;
    }
    if let Some(tags) = input.tags {
        task.tags = tags;
    }

    task.updated_at = Utc::now();
    task_repo.save(&task).await?;
    Ok(task)
}

/// Delete a task by its identifier.
pub async fn delete_task(
    task_repo: &dyn TaskRepository,
    task_id: TaskId,
) -> Result<(), AppError> {
    // Verify the task exists before deleting
    task_repo
        .find_by_id(task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {}", task_id)))?;

    task_repo.delete(task_id).await?;
    Ok(())
}

/// Mark a task as completed.
pub async fn complete_task(
    task_repo: &dyn TaskRepository,
    task_id: TaskId,
) -> Result<Task, AppError> {
    let mut task = task_repo
        .find_by_id(task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {}", task_id)))?;

    task.status = TaskStatus::Done;
    task.updated_at = Utc::now();
    task_repo.save(&task).await?;
    Ok(task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::errors::RepositoryError;

    /// In-memory task repository for testing.
    struct InMemoryTaskRepository {
        tasks: Mutex<HashMap<TaskId, Task>>,
    }

    impl InMemoryTaskRepository {
        fn new() -> Self {
            Self {
                tasks: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl TaskRepository for InMemoryTaskRepository {
        async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError> {
            let tasks = self.tasks.lock().unwrap();
            Ok(tasks.get(&id).cloned())
        }

        async fn find_by_user(
            &self,
            user_id: UserId,
            filter: &TaskFilter,
        ) -> Result<Vec<Task>, RepositoryError> {
            let tasks = self.tasks.lock().unwrap();
            let mut result: Vec<Task> = tasks
                .values()
                .filter(|t| t.user_id == user_id)
                .filter(|t| {
                    if let Some(ref statuses) = filter.status {
                        statuses.contains(&t.status)
                    } else {
                        true
                    }
                })
                .filter(|t| {
                    if let Some(ref sources) = filter.source {
                        sources.contains(&t.source)
                    } else {
                        true
                    }
                })
                .filter(|t| {
                    if let Some(ref pid) = filter.project_id {
                        t.project_id == Some(*pid)
                    } else {
                        true
                    }
                })
                .cloned()
                .collect();
            result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            Ok(result)
        }

        async fn find_by_source(
            &self,
            user_id: UserId,
            source: Source,
            source_id: &str,
        ) -> Result<Option<Task>, RepositoryError> {
            let tasks = self.tasks.lock().unwrap();
            Ok(tasks.values().find(|t| {
                t.user_id == user_id
                    && t.source == source
                    && t.source_id.as_deref() == Some(source_id)
            }).cloned())
        }

        async fn find_by_date_range(
            &self,
            user_id: UserId,
            start: NaiveDate,
            end: NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            let tasks = self.tasks.lock().unwrap();
            Ok(tasks
                .values()
                .filter(|t| {
                    t.user_id == user_id
                        && t.deadline
                            .map(|d| d >= start && d <= end)
                            .unwrap_or(false)
                })
                .cloned()
                .collect())
        }

        async fn save(&self, task: &Task) -> Result<(), RepositoryError> {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.insert(task.id, task.clone());
            Ok(())
        }

        async fn save_batch(&self, tasks: &[Task]) -> Result<(), RepositoryError> {
            let mut store = self.tasks.lock().unwrap();
            for task in tasks {
                store.insert(task.id, task.clone());
            }
            Ok(())
        }

        async fn delete(&self, id: TaskId) -> Result<(), RepositoryError> {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.remove(&id);
            Ok(())
        }

        async fn delete_stale_by_source(&self, _user_id: UserId, _source: Source, _keep_ids: &[String]) -> Result<u64, RepositoryError> {
            Ok(0)
        }
    }

    fn test_user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 7).unwrap()
    }

    #[tokio::test]
    async fn create_task_with_defaults() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "My Task".to_string(),
            description: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };

        let task = create_personal_task(&repo, test_user_id(), input, today())
            .await
            .unwrap();

        assert_eq!(task.title, "My Task");
        assert_eq!(task.source, Source::Personal);
        assert_eq!(task.status, TaskStatus::Todo);
        assert_eq!(task.impact, ImpactLevel::Medium);
        assert_eq!(task.urgency, UrgencyLevel::Low); // No deadline => Low
        assert!(!task.urgency_manual);
    }

    #[tokio::test]
    async fn create_task_with_manual_urgency() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "Urgent Task".to_string(),
            description: Some("desc".to_string()),
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: Some(3.0),
            impact: Some(ImpactLevel::Critical),
            urgency: Some(UrgencyLevel::High),
            tags: vec![],
        };

        let task = create_personal_task(&repo, test_user_id(), input, today())
            .await
            .unwrap();

        assert_eq!(task.urgency, UrgencyLevel::High);
        assert!(task.urgency_manual);
        assert_eq!(task.impact, ImpactLevel::Critical);
        assert_eq!(task.estimated_hours, Some(3.0));
    }

    #[tokio::test]
    async fn create_task_auto_urgency_from_deadline() {
        let repo = InMemoryTaskRepository::new();
        // Deadline is today => High urgency (0 business days)
        let input = CreateTaskInput {
            title: "Due Today".to_string(),
            description: None,
            project_id: None,
            deadline: Some(today()),
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };

        let task = create_personal_task(&repo, test_user_id(), input, today())
            .await
            .unwrap();

        assert_eq!(task.urgency, UrgencyLevel::High);
        assert!(!task.urgency_manual);
    }

    #[tokio::test]
    async fn get_task_found() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "Find Me".to_string(),
            description: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };

        let created = create_personal_task(&repo, test_user_id(), input, today())
            .await
            .unwrap();

        let found = get_task(&repo, created.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().title, "Find Me");
    }

    #[tokio::test]
    async fn get_task_not_found() {
        let repo = InMemoryTaskRepository::new();
        let found = get_task(&repo, Uuid::new_v4()).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn get_tasks_returns_user_tasks() {
        let repo = InMemoryTaskRepository::new();
        for title in &["A", "B", "C"] {
            let input = CreateTaskInput {
                title: title.to_string(),
                description: None,
                project_id: None,
                deadline: None,
                planned_start: None,
                planned_end: None,
                estimated_hours: None,
                impact: None,
                urgency: None,
                tags: vec![],
            };
            create_personal_task(&repo, test_user_id(), input, today())
                .await
                .unwrap();
        }

        let tasks = get_tasks(&repo, test_user_id(), &TaskFilter::empty())
            .await
            .unwrap();
        assert_eq!(tasks.len(), 3);
    }

    #[tokio::test]
    async fn update_task_changes_fields() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "Original".to_string(),
            description: Some("old desc".to_string()),
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };

        let created = create_personal_task(&repo, test_user_id(), input, today())
            .await
            .unwrap();

        let update = UpdateTaskInput {
            title: Some("Updated".to_string()),
            description: Some(Some("new desc".to_string())),
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: Some(Some(5.0)),
            status: Some(TaskStatus::InProgress),
            impact: Some(ImpactLevel::High),
            urgency: None,
            tags: None,
        };

        let updated = update_task(&repo, created.id, update, today())
            .await
            .unwrap();

        assert_eq!(updated.title, "Updated");
        assert_eq!(updated.description, Some("new desc".to_string()));
        assert_eq!(updated.status, TaskStatus::InProgress);
        assert_eq!(updated.impact, ImpactLevel::High);
        assert_eq!(updated.estimated_hours, Some(5.0));
        assert!(updated.updated_at > created.updated_at);
    }

    #[tokio::test]
    async fn update_task_not_found() {
        let repo = InMemoryTaskRepository::new();
        let update = UpdateTaskInput {
            title: Some("Nope".to_string()),
            description: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
        };

        let result = update_task(&repo, Uuid::new_v4(), update, today()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn update_task_with_manual_urgency() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "Task".to_string(),
            description: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };

        let created = create_personal_task(&repo, test_user_id(), input, today())
            .await
            .unwrap();
        assert!(!created.urgency_manual);

        let update = UpdateTaskInput {
            title: None,
            description: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: Some(UrgencyLevel::Critical),
            tags: None,
        };

        let updated = update_task(&repo, created.id, update, today())
            .await
            .unwrap();

        assert_eq!(updated.urgency, UrgencyLevel::Critical);
        assert!(updated.urgency_manual);
    }

    #[tokio::test]
    async fn delete_task_removes_it() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "Doomed".to_string(),
            description: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };

        let created = create_personal_task(&repo, test_user_id(), input, today())
            .await
            .unwrap();

        delete_task(&repo, created.id).await.unwrap();

        let found = get_task(&repo, created.id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn delete_task_not_found() {
        let repo = InMemoryTaskRepository::new();
        let result = delete_task(&repo, Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn complete_task_sets_done() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "Complete Me".to_string(),
            description: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };

        let created = create_personal_task(&repo, test_user_id(), input, today())
            .await
            .unwrap();
        assert_eq!(created.status, TaskStatus::Todo);

        let completed = complete_task(&repo, created.id).await.unwrap();
        assert_eq!(completed.status, TaskStatus::Done);
    }

    #[tokio::test]
    async fn complete_task_not_found() {
        let repo = InMemoryTaskRepository::new();
        let result = complete_task(&repo, Uuid::new_v4()).await;
        assert!(result.is_err());
    }
}
