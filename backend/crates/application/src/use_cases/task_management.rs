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
    pub notes: Option<String>,
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
    pub notes: Option<Option<String>>,
    pub project_id: Option<Option<ProjectId>>,
    pub deadline: Option<Option<NaiveDate>>,
    pub planned_start: Option<Option<DateTime<Utc>>>,
    pub planned_end: Option<Option<DateTime<Utc>>>,
    pub estimated_hours: Option<Option<f32>>,
    pub status: Option<TaskStatus>,
    pub impact: Option<ImpactLevel>,
    pub urgency: Option<UrgencyLevel>,
    pub tags: Option<Vec<TagId>>,
    pub remaining_hours_override: Option<Option<f32>>,
    pub estimated_hours_override: Option<Option<f32>>,
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
        notes: input.notes,
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
        jira_remaining_seconds: None,
        jira_original_estimate_seconds: None,
        jira_time_spent_seconds: None,
        remaining_hours_override: None,
        estimated_hours_override: None,
        recurrence_id: None,
        occurrence_date: None,
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

impl UpdateTaskInput {
    /// Returns true if the input contains any template-level field that must be
    /// updated via `updateRecurringTask` rather than `updateTask`.
    fn has_template_only_fields(&self) -> bool {
        self.title.is_some()
            || self.description.is_some()
            || self.urgency.is_some()
            || self.impact.is_some()
            || self.estimated_hours.is_some()
            || self.project_id.is_some()
            || self.tags.is_some()
    }
}

/// Update an existing task. Returns the updated task.
///
/// For recurring instances, only per-instance fields (status, planned_start,
/// planned_end, deadline, notes, remaining_hours_override, estimated_hours_override)
/// are allowed. Template-level fields (title, description, urgency, impact,
/// estimated_hours, project_id, tags) must go through `updateRecurringTask`.
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

    if task.recurrence_id.is_some() && input.has_template_only_fields() {
        return Err(AppError::Validation(
            "Template-level fields (title, description, urgency, impact, estimated_hours, project_id, tags) must be updated via updateRecurringTask".to_string(),
        ));
    }

    if let Some(title) = input.title {
        task.title = title;
    }
    if let Some(description) = input.description {
        task.description = description;
    }
    if let Some(notes) = input.notes {
        task.notes = notes;
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
    if let Some(remaining) = input.remaining_hours_override {
        task.remaining_hours_override = remaining;
    }
    if let Some(estimated) = input.estimated_hours_override {
        task.estimated_hours_override = estimated;
    }

    task.updated_at = Utc::now();
    task_repo.save(&task).await?;
    Ok(task)
}

/// Append a block of text to the task's `notes` field, creating it if absent.
///
/// Existing content is preserved and the new text is added after a blank line so
/// that successive entries form a readable journal. This is the backing operation
/// for the activity timer "quick note" feature.
pub async fn append_to_task_notes(
    task_repo: &dyn TaskRepository,
    task_id: TaskId,
    text: &str,
) -> Result<Task, AppError> {
    let mut task = task_repo
        .find_by_id(task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {}", task_id)))?;

    task.notes = Some(match task.notes.take() {
        Some(existing) if !existing.is_empty() => format!("{existing}\n\n{text}"),
        _ => text.to_string(),
    });
    task.updated_at = Utc::now();
    task_repo.save(&task).await?;
    Ok(task)
}

/// Delete a task by its identifier.
///
/// Returns `AppError::Validation` if the task is a recurring instance.
/// Use `cancel_recurrence` or `skip_occurrence` (via the recurrence use case) instead.
pub async fn delete_task(
    task_repo: &dyn TaskRepository,
    task_id: TaskId,
) -> Result<(), AppError> {
    // Verify the task exists before deleting
    let task = task_repo
        .find_by_id(task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {}", task_id)))?;

    if task.recurrence_id.is_some() {
        return Err(AppError::Validation(
            "Use cancelRecurrence or skipOccurrence for recurring instances".to_string(),
        ));
    }

    task_repo.delete(task_id).await?;
    Ok(())
}

/// Update the tracking state of a task (inbox → followed/dismissed).
pub async fn set_tracking_state(
    repo: &dyn TaskRepository,
    task_id: TaskId,
    state: TrackingState,
) -> Result<Task, AppError> {
    let mut task = repo
        .find_by_id(task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {} not found", task_id)))?;

    task.tracking_state = state;
    task.updated_at = chrono::Utc::now();
    repo.save(&task).await?;
    Ok(task)
}

/// Batch-update the tracking state for multiple tasks.
pub async fn set_tracking_state_batch(
    repo: &dyn TaskRepository,
    task_ids: Vec<TaskId>,
    state: TrackingState,
) -> Result<Vec<Task>, AppError> {
    let mut results = Vec::with_capacity(task_ids.len());
    for id in task_ids {
        results.push(set_tracking_state(repo, id, state).await?);
    }
    Ok(results)
}

/// Carry forward tasks whose planned_start is before `current_monday` to that Monday.
///
/// Only non-Done tasks are rescheduled. Recurring instances (those with a non-null
/// `recurrence_id`) are exempt — their scheduling is managed by the recurrence engine.
/// Returns the number of tasks updated.
pub async fn carry_forward_tasks(
    task_repo: &dyn TaskRepository,
    user_id: UserId,
    current_monday: NaiveDate,
) -> Result<usize, AppError> {
    let tasks = task_repo
        .find_planned_before(user_id, current_monday)
        .await?;

    // Recurring instances are exempt from carry-forward: they represent a specific
    // occurrence slot and must not be silently rescheduled by this mechanism.
    let tasks: Vec<_> = tasks.into_iter().filter(|t| t.recurrence_id.is_none()).collect();

    let new_start = current_monday
        .and_hms_opt(8, 0, 0)
        .expect("valid time")
        .and_utc();

    let count = tasks.len();
    for mut task in tasks {
        task.planned_start = Some(new_start);
        task.updated_at = Utc::now();
        task_repo.save(&task).await?;
    }
    Ok(count)
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
                .filter(|t| {
                    if let Some(ref states) = filter.tracking_state {
                        states.contains(&t.tracking_state)
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

        async fn find_planned_before(
            &self,
            user_id: UserId,
            before_date: NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            let tasks = self.tasks.lock().unwrap();
            Ok(tasks
                .values()
                .filter(|t| {
                    t.user_id == user_id
                        && t.status != TaskStatus::Done
                        && t.status != TaskStatus::Cancelled
                        && t.planned_start
                            .map(|dt| dt.date_naive() < before_date)
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
            notes: None,
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
            notes: None,
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
            notes: None,
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
            notes: None,
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
                notes: None,
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
            notes: None,
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
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: Some(Some(5.0)),
            status: Some(TaskStatus::InProgress),
            impact: Some(ImpactLevel::High),
            urgency: None,
            tags: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
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
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
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
            notes: None,
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
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: Some(UrgencyLevel::Critical),
            tags: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
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
            notes: None,
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
            notes: None,
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

    #[tokio::test]
    async fn update_task_with_time_overrides() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "Jira Task".to_string(),
            description: None,
            notes: None,
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

        assert!(created.remaining_hours_override.is_none());
        assert!(created.estimated_hours_override.is_none());

        let update = UpdateTaskInput {
            title: None,
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: Some(Some(4.5)),
            estimated_hours_override: Some(Some(8.0)),
        };

        let updated = update_task(&repo, created.id, update, today())
            .await
            .unwrap();

        assert_eq!(updated.remaining_hours_override, Some(4.5));
        assert_eq!(updated.estimated_hours_override, Some(8.0));
    }

    #[tokio::test]
    async fn update_task_clear_time_overrides() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "Task".to_string(),
            description: None,
            notes: None,
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

        // Set overrides
        let update1 = UpdateTaskInput {
            title: None,
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: Some(Some(4.5)),
            estimated_hours_override: Some(Some(8.0)),
        };
        let t = update_task(&repo, created.id, update1, today()).await.unwrap();
        assert_eq!(t.remaining_hours_override, Some(4.5));

        // Clear overrides with Some(None)
        let update2 = UpdateTaskInput {
            title: None,
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: Some(None),
            estimated_hours_override: Some(None),
        };
        let cleared = update_task(&repo, created.id, update2, today()).await.unwrap();
        assert!(cleared.remaining_hours_override.is_none());
        assert!(cleared.estimated_hours_override.is_none());
    }

    #[tokio::test]
    async fn set_tracking_state_updates_task() {
        use domain::types::TrackingState;
        let repo = InMemoryTaskRepository::new();
        let user_id = Uuid::new_v4();
        let today = chrono::Utc::now().date_naive();

        let input = CreateTaskInput {
            title: "Test task".to_string(),
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };

        let task = create_personal_task(&repo, user_id, input, today).await.unwrap();
        assert_eq!(task.tracking_state, TrackingState::Followed);

        let updated = set_tracking_state(&repo, task.id, TrackingState::Dismissed).await.unwrap();
        assert_eq!(updated.tracking_state, TrackingState::Dismissed);
    }

    #[tokio::test]
    async fn append_to_task_notes_creates_when_empty() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "Task".to_string(),
            description: None,
            notes: None,
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
        assert!(created.notes.is_none());

        let updated = append_to_task_notes(&repo, created.id, "[09:00] first note")
            .await
            .unwrap();
        assert_eq!(updated.notes.as_deref(), Some("[09:00] first note"));
    }

    #[tokio::test]
    async fn append_to_task_notes_appends_to_existing() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "Task".to_string(),
            description: None,
            notes: Some("# Plan\n- step 1".to_string()),
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

        let updated = append_to_task_notes(&repo, created.id, "[09:30] step 1 done")
            .await
            .unwrap();
        assert_eq!(
            updated.notes.as_deref(),
            Some("# Plan\n- step 1\n\n[09:30] step 1 done")
        );

        // Append a second time
        let updated2 = append_to_task_notes(&repo, updated.id, "[10:00] starting step 2")
            .await
            .unwrap();
        assert_eq!(
            updated2.notes.as_deref(),
            Some("# Plan\n- step 1\n\n[09:30] step 1 done\n\n[10:00] starting step 2")
        );
    }

    #[tokio::test]
    async fn append_to_task_notes_not_found() {
        let repo = InMemoryTaskRepository::new();
        let result = append_to_task_notes(&repo, Uuid::new_v4(), "anything").await;
        assert!(result.is_err());
    }

    /// Helper: create an in-memory task with recurrence_id set.
    fn make_recurring_task(
        repo: &InMemoryTaskRepository,
        user_id: UserId,
        planned_start: Option<DateTime<Utc>>,
    ) -> Task {
        use domain::types::recurrence::RecurrenceTemplateId;
        let now = Utc::now();
        let task = Task {
            id: Uuid::new_v4(),
            user_id,
            title: "Recurring".to_string(),
            description: None,
            notes: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            deadline: None,
            planned_start,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Low,
            urgency_manual: false,
            impact: ImpactLevel::Low,
            tags: vec![],
            tracking_state: TrackingState::Followed,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            recurrence_id: Some(RecurrenceTemplateId::new()),
            occurrence_date: planned_start.map(|dt| dt.date_naive()),
            created_at: now,
            updated_at: now,
        };
        let mut store = repo.tasks.lock().unwrap();
        store.insert(task.id, task.clone());
        task
    }

    // Test 11: carry_forward_tasks does NOT move recurring instances.
    #[tokio::test]
    async fn carry_forward_skips_recurring_instances() {
        let repo = InMemoryTaskRepository::new();
        let last_monday = NaiveDate::from_ymd_opt(2026, 4, 6).unwrap();
        let this_monday = NaiveDate::from_ymd_opt(2026, 4, 13).unwrap();

        // Regular stale task — SHOULD be moved.
        let stale_input = CreateTaskInput {
            title: "Stale".to_string(),
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: Some(last_monday.and_hms_opt(8, 0, 0).unwrap().and_utc()),
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };
        let stale = create_personal_task(&repo, test_user_id(), stale_input, today())
            .await
            .unwrap();

        // Recurring stale task — must NOT be moved.
        let recurring =
            make_recurring_task(&repo, test_user_id(), Some(last_monday.and_hms_opt(8, 0, 0).unwrap().and_utc()));

        let count = carry_forward_tasks(&repo, test_user_id(), this_monday)
            .await
            .unwrap();

        // Only the regular stale task is counted.
        assert_eq!(count, 1);

        // Regular stale task was moved.
        let updated_stale = repo.find_by_id(stale.id).await.unwrap().unwrap();
        assert_eq!(updated_stale.planned_start.unwrap().date_naive(), this_monday);

        // Recurring task was NOT moved.
        let unchanged_recurring = repo.find_by_id(recurring.id).await.unwrap().unwrap();
        assert_eq!(
            unchanged_recurring.planned_start.unwrap().date_naive(),
            last_monday,
            "recurring instance must not be carried forward"
        );
    }

    // Test 12: update_task rejects recurring instances when template-only fields are provided.
    #[tokio::test]
    async fn update_task_rejects_recurring_instance() {
        let repo = InMemoryTaskRepository::new();
        let recurring = make_recurring_task(&repo, test_user_id(), None);

        let update = UpdateTaskInput {
            title: Some("Attempted edit".to_string()),
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
        };

        let result = update_task(&repo, recurring.id, update, today()).await;
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "expected Validation error for recurring instance with title, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn update_task_recurring_instance_allows_status() {
        let repo = InMemoryTaskRepository::new();
        let recurring = make_recurring_task(&repo, test_user_id(), None);

        let update = UpdateTaskInput {
            title: None,
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: Some(TaskStatus::Done),
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
        };

        let result = update_task(&repo, recurring.id, update, today()).await;
        assert!(result.is_ok(), "status update on recurring instance should succeed");
        assert_eq!(result.unwrap().status, TaskStatus::Done);
    }

    #[tokio::test]
    async fn update_task_recurring_instance_allows_planned_start_and_overrides() {
        let repo = InMemoryTaskRepository::new();
        let recurring = make_recurring_task(&repo, test_user_id(), None);
        let new_start = chrono::DateTime::parse_from_rfc3339("2026-05-01T08:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);

        let update = UpdateTaskInput {
            title: None,
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: Some(Some(new_start)),
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: Some(Some(2.5)),
            estimated_hours_override: None,
        };

        let result = update_task(&repo, recurring.id, update, today()).await;
        assert!(result.is_ok(), "planned_start + override update on recurring instance should succeed");
        let updated = result.unwrap();
        assert_eq!(updated.planned_start, Some(new_start));
        assert_eq!(updated.remaining_hours_override, Some(2.5));
    }

    #[tokio::test]
    async fn update_task_recurring_instance_rejects_title() {
        let repo = InMemoryTaskRepository::new();
        let recurring = make_recurring_task(&repo, test_user_id(), None);

        let update = UpdateTaskInput {
            title: Some("Changed title".to_string()),
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
        };

        let result = update_task(&repo, recurring.id, update, today()).await;
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "title change on recurring instance should return Validation error"
        );
    }

    #[tokio::test]
    async fn update_task_recurring_instance_rejects_urgency() {
        let repo = InMemoryTaskRepository::new();
        let recurring = make_recurring_task(&repo, test_user_id(), None);

        let update = UpdateTaskInput {
            title: None,
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: Some(UrgencyLevel::Critical),
            tags: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
        };

        let result = update_task(&repo, recurring.id, update, today()).await;
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "urgency change on recurring instance should return Validation error"
        );
    }

    // Test 13: delete_task rejects recurring instances.
    #[tokio::test]
    async fn delete_task_rejects_recurring_instance() {
        let repo = InMemoryTaskRepository::new();
        let recurring = make_recurring_task(&repo, test_user_id(), None);

        let result = delete_task(&repo, recurring.id).await;
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "expected Validation error for recurring instance, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn carry_forward_moves_past_week_tasks_to_monday() {
        let repo = InMemoryTaskRepository::new();
        let last_monday = NaiveDate::from_ymd_opt(2026, 4, 6).unwrap();
        let this_monday = NaiveDate::from_ymd_opt(2026, 4, 13).unwrap();

        // Task planned last Monday — should be carried forward
        let stale = CreateTaskInput {
            title: "Stale Task".to_string(),
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: Some(last_monday.and_hms_opt(8, 0, 0).unwrap().and_utc()),
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };
        let stale_task = create_personal_task(&repo, test_user_id(), stale, today())
            .await
            .unwrap();

        // Done task from last week — must NOT be moved
        let done = CreateTaskInput {
            title: "Done Task".to_string(),
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: Some(last_monday.and_hms_opt(8, 0, 0).unwrap().and_utc()),
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };
        let done_task = create_personal_task(&repo, test_user_id(), done, today())
            .await
            .unwrap();
        // Mark as done
        complete_task(&repo, done_task.id).await.unwrap();

        // Task already on current Monday — must NOT be affected
        let current = CreateTaskInput {
            title: "Current Task".to_string(),
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: Some(this_monday.and_hms_opt(8, 0, 0).unwrap().and_utc()),
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };
        let current_task = create_personal_task(&repo, test_user_id(), current, today())
            .await
            .unwrap();

        let count = carry_forward_tasks(&repo, test_user_id(), this_monday)
            .await
            .unwrap();

        assert_eq!(count, 1, "only the stale active task should be moved");

        let updated = repo.find_by_id(stale_task.id).await.unwrap().unwrap();
        assert_eq!(
            updated.planned_start.unwrap().date_naive(),
            this_monday,
            "stale task should be rescheduled to current Monday"
        );

        let done_after = repo.find_by_id(done_task.id).await.unwrap().unwrap();
        assert_eq!(
            done_after.planned_start.unwrap().date_naive(),
            last_monday,
            "done task should not be moved"
        );

        let current_after = repo.find_by_id(current_task.id).await.unwrap().unwrap();
        assert_eq!(
            current_after.planned_start.unwrap().date_naive(),
            this_monday,
            "current week task unchanged"
        );
    }

    // Deferred test 11 from Wave 3A: carry_forward must not carry Cancelled tasks.
    #[tokio::test]
    async fn carry_forward_does_not_carry_cancelled_tasks() {
        let repo = InMemoryTaskRepository::new();
        let last_monday = NaiveDate::from_ymd_opt(2026, 4, 6).unwrap();
        let this_monday = NaiveDate::from_ymd_opt(2026, 4, 13).unwrap();

        // Create a task in the past with status = Cancelled, no recurrence_id.
        let now = Utc::now();
        let cancelled_task = Task {
            id: Uuid::new_v4(),
            user_id: test_user_id(),
            title: "Cancelled task".to_string(),
            description: None,
            notes: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::Cancelled,
            project_id: None,
            assignee: None,
            deadline: None,
            planned_start: Some(last_monday.and_hms_opt(8, 0, 0).unwrap().and_utc()),
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Low,
            urgency_manual: false,
            impact: ImpactLevel::Low,
            tags: vec![],
            tracking_state: TrackingState::Followed,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            recurrence_id: None,
            occurrence_date: None,
            created_at: now,
            updated_at: now,
        };
        {
            let mut store = repo.tasks.lock().unwrap();
            store.insert(cancelled_task.id, cancelled_task.clone());
        }

        let count = carry_forward_tasks(&repo, test_user_id(), this_monday)
            .await
            .unwrap();

        // Cancelled task must not be carried forward.
        assert_eq!(count, 0, "cancelled task should not be carried forward");

        let after = repo.find_by_id(cancelled_task.id).await.unwrap().unwrap();
        assert_eq!(
            after.planned_start.unwrap().date_naive(),
            last_monday,
            "cancelled task planned_start must remain unchanged"
        );
    }
}
