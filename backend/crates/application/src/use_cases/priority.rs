use chrono::NaiveDate;
use domain::rules::priority::determine_quadrant;
use domain::rules::urgency::calculate_urgency;
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::*;

/// A task with its computed quadrant for the Eisenhower matrix.
pub struct TaskWithQuadrant {
    pub task: Task,
    pub quadrant: Quadrant,
}

/// Data for the full Eisenhower priority matrix, grouped by quadrant.
#[derive(serde::Serialize)]
pub struct PriorityMatrixData {
    pub urgent_important: Vec<Task>,
    pub important: Vec<Task>,
    pub urgent: Vec<Task>,
    pub neither: Vec<Task>,
}

/// Get all non-done tasks for a user grouped into Eisenhower matrix quadrants.
pub async fn get_priority_matrix(
    task_repo: &dyn TaskRepository,
    user_id: UserId,
    _today: NaiveDate,
) -> Result<PriorityMatrixData, AppError> {
    let filter = TaskFilter {
        tracking_state: Some(vec![TrackingState::Followed]),
        ..TaskFilter::empty()
    };
    let tasks = task_repo.find_by_user(user_id, &filter).await?;

    let mut urgent_important = Vec::new();
    let mut important = Vec::new();
    let mut urgent = Vec::new();
    let mut neither = Vec::new();

    for task in tasks {
        if task.status == TaskStatus::Done {
            continue;
        }
        let quadrant = determine_quadrant(task.urgency, task.impact);
        match quadrant {
            Quadrant::UrgentImportant => urgent_important.push(task),
            Quadrant::Important => important.push(task),
            Quadrant::Urgent => urgent.push(task),
            Quadrant::Neither => neither.push(task),
        }
    }

    Ok(PriorityMatrixData {
        urgent_important,
        important,
        urgent,
        neither,
    })
}

/// Override the urgency level of a task (manual override).
pub async fn override_urgency(
    task_repo: &dyn TaskRepository,
    task_id: TaskId,
    urgency: UrgencyLevel,
) -> Result<Task, AppError> {
    let mut task = task_repo
        .find_by_id(task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {}", task_id)))?;

    task.urgency = urgency;
    task.urgency_manual = true;
    task.updated_at = chrono::Utc::now();
    task_repo.save(&task).await?;
    Ok(task)
}

/// Override the impact level of a task.
pub async fn override_impact(
    task_repo: &dyn TaskRepository,
    task_id: TaskId,
    impact: ImpactLevel,
) -> Result<Task, AppError> {
    let mut task = task_repo
        .find_by_id(task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {}", task_id)))?;

    task.impact = impact;
    task.updated_at = chrono::Utc::now();
    task_repo.save(&task).await?;
    Ok(task)
}

/// Reset urgency to auto-calculated based on deadline.
pub async fn reset_urgency(
    task_repo: &dyn TaskRepository,
    task_id: TaskId,
    today: NaiveDate,
) -> Result<Task, AppError> {
    let mut task = task_repo
        .find_by_id(task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {}", task_id)))?;

    task.urgency = calculate_urgency(task.deadline, today);
    task.urgency_manual = false;
    task.updated_at = chrono::Utc::now();
    task_repo.save(&task).await?;
    Ok(task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

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

        fn insert(&self, task: Task) {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.insert(task.id, task);
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
            Ok(tasks
                .values()
                .filter(|t| t.user_id == user_id)
                .filter(|t| {
                    if let Some(ref states) = filter.tracking_state {
                        states.contains(&t.tracking_state)
                    } else {
                        true
                    }
                })
                .cloned()
                .collect())
        }

        async fn find_by_source(
            &self,
            _user_id: UserId,
            _source: Source,
            _source_id: &str,
        ) -> Result<Option<Task>, RepositoryError> {
            Ok(None)
        }

        async fn find_by_date_range(
            &self,
            _user_id: UserId,
            _start: NaiveDate,
            _end: NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(vec![])
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

    fn make_task(title: &str, urgency: UrgencyLevel, impact: ImpactLevel) -> Task {
        Task {
            id: Uuid::new_v4(),
            user_id: test_user_id(),
            title: title.to_string(),
            description: None,
            notes: None,
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
            urgency,
            urgency_manual: false,
            impact,
            tags: vec![],
            tracking_state: TrackingState::Followed,
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
    async fn priority_matrix_groups_by_quadrant() {
        let repo = InMemoryTaskRepository::new();

        // UrgentImportant: High urgency, High impact
        repo.insert(make_task("UI", UrgencyLevel::High, ImpactLevel::High));
        // Important: Low urgency, High impact
        repo.insert(make_task("I", UrgencyLevel::Low, ImpactLevel::High));
        // Urgent: High urgency, Low impact
        repo.insert(make_task("U", UrgencyLevel::High, ImpactLevel::Low));
        // Neither: Low urgency, Low impact
        repo.insert(make_task("N", UrgencyLevel::Low, ImpactLevel::Low));

        let matrix = get_priority_matrix(&repo, test_user_id(), today())
            .await
            .unwrap();

        assert_eq!(matrix.urgent_important.len(), 1);
        assert_eq!(matrix.urgent_important[0].title, "UI");
        assert_eq!(matrix.important.len(), 1);
        assert_eq!(matrix.important[0].title, "I");
        assert_eq!(matrix.urgent.len(), 1);
        assert_eq!(matrix.urgent[0].title, "U");
        assert_eq!(matrix.neither.len(), 1);
        assert_eq!(matrix.neither[0].title, "N");
    }

    #[tokio::test]
    async fn priority_matrix_excludes_non_followed_tasks() {
        let repo = InMemoryTaskRepository::new();

        // Inbox task should be excluded
        let mut inbox_task = make_task("Inbox", UrgencyLevel::High, ImpactLevel::High);
        inbox_task.tracking_state = TrackingState::Inbox;
        repo.insert(inbox_task);

        // Dismissed task should be excluded
        let mut dismissed_task = make_task("Dismissed", UrgencyLevel::High, ImpactLevel::High);
        dismissed_task.tracking_state = TrackingState::Dismissed;
        repo.insert(dismissed_task);

        // Followed task should be included
        repo.insert(make_task("Followed", UrgencyLevel::High, ImpactLevel::High));

        let matrix = get_priority_matrix(&repo, test_user_id(), today())
            .await
            .unwrap();

        assert_eq!(matrix.urgent_important.len(), 1);
        assert_eq!(matrix.urgent_important[0].title, "Followed");
    }

    #[tokio::test]
    async fn priority_matrix_excludes_done_tasks() {
        let repo = InMemoryTaskRepository::new();

        let mut done_task = make_task("Done", UrgencyLevel::High, ImpactLevel::High);
        done_task.status = TaskStatus::Done;
        repo.insert(done_task);

        repo.insert(make_task("Active", UrgencyLevel::High, ImpactLevel::High));

        let matrix = get_priority_matrix(&repo, test_user_id(), today())
            .await
            .unwrap();

        assert_eq!(matrix.urgent_important.len(), 1);
        assert_eq!(matrix.urgent_important[0].title, "Active");
    }

    #[tokio::test]
    async fn override_urgency_sets_manual() {
        let repo = InMemoryTaskRepository::new();
        let task = make_task("Task", UrgencyLevel::Low, ImpactLevel::Medium);
        let task_id = task.id;
        repo.insert(task);

        let updated = override_urgency(&repo, task_id, UrgencyLevel::Critical)
            .await
            .unwrap();

        assert_eq!(updated.urgency, UrgencyLevel::Critical);
        assert!(updated.urgency_manual);
    }

    #[tokio::test]
    async fn override_urgency_not_found() {
        let repo = InMemoryTaskRepository::new();
        let result = override_urgency(&repo, Uuid::new_v4(), UrgencyLevel::High).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn override_impact_updates() {
        let repo = InMemoryTaskRepository::new();
        let task = make_task("Task", UrgencyLevel::Low, ImpactLevel::Low);
        let task_id = task.id;
        repo.insert(task);

        let updated = override_impact(&repo, task_id, ImpactLevel::Critical)
            .await
            .unwrap();

        assert_eq!(updated.impact, ImpactLevel::Critical);
    }

    #[tokio::test]
    async fn override_impact_not_found() {
        let repo = InMemoryTaskRepository::new();
        let result = override_impact(&repo, Uuid::new_v4(), ImpactLevel::High).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn reset_urgency_clears_manual() {
        let repo = InMemoryTaskRepository::new();
        let mut task = make_task("Task", UrgencyLevel::Critical, ImpactLevel::Medium);
        task.urgency_manual = true;
        // No deadline => auto urgency should be Low
        task.deadline = None;
        let task_id = task.id;
        repo.insert(task);

        let updated = reset_urgency(&repo, task_id, today()).await.unwrap();

        assert_eq!(updated.urgency, UrgencyLevel::Low);
        assert!(!updated.urgency_manual);
    }

    #[tokio::test]
    async fn reset_urgency_with_deadline() {
        let repo = InMemoryTaskRepository::new();
        let mut task = make_task("Task", UrgencyLevel::Low, ImpactLevel::Medium);
        task.urgency_manual = true;
        // Deadline is today (Saturday March 7, 2026) => business days = 0 => High
        task.deadline = Some(today());
        let task_id = task.id;
        repo.insert(task);

        let updated = reset_urgency(&repo, task_id, today()).await.unwrap();

        assert_eq!(updated.urgency, UrgencyLevel::High);
        assert!(!updated.urgency_manual);
    }

    #[tokio::test]
    async fn reset_urgency_not_found() {
        let repo = InMemoryTaskRepository::new();
        let result = reset_urgency(&repo, Uuid::new_v4(), today()).await;
        assert!(result.is_err());
    }
}
