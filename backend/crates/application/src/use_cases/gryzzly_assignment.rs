use domain::types::{Task, TaskId};

use crate::errors::AppError;
use crate::repositories::{GryzzlyCatalogRepository, TaskRepository};

/// Assign (or clear, with `None`) the Gryzzly task for an aplan task. On assign, the
/// project id is snapshotted from the catalog so a future push never needs a live row.
pub async fn assign_gryzzly_task(
    task_repo: &dyn TaskRepository,
    catalog_repo: &dyn GryzzlyCatalogRepository,
    task_id: TaskId,
    gryzzly_task_id: Option<String>,
) -> Result<Task, AppError> {
    let mut task = task_repo
        .find_by_id(task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("task {task_id}")))?;

    match gryzzly_task_id {
        Some(gid) => {
            let entry = catalog_repo
                .find_by_gryzzly_task_id(task.user_id, &gid)
                .await?
                .ok_or_else(|| AppError::Validation(format!("unknown gryzzly task: {gid}")))?;
            task.gryzzly_task_id = Some(entry.gryzzly_task_id);
            task.gryzzly_project_id = Some(entry.gryzzly_project_id);
        }
        None => {
            task.gryzzly_task_id = None;
            task.gryzzly_project_id = None;
        }
    }
    task_repo.save(&task).await?;
    Ok(task)
}

#[cfg(test)]
mod tests {
    use super::*;

    use async_trait::async_trait;
    use chrono::Utc;
    use domain::types::{GryzzlyCatalogEntry, Source, Task, TaskId, TaskStatus, UserId};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    use crate::errors::RepositoryError;
    use crate::repositories::task_repository::TaskFilter;

    // ── Mock TaskRepository ───────────────────────────────────────────────────

    #[derive(Default)]
    struct MemTaskRepo {
        tasks: Mutex<HashMap<TaskId, Task>>,
    }

    impl MemTaskRepo {
        fn seed(&self, task: Task) {
            self.tasks.lock().unwrap().insert(task.id, task);
        }
    }

    #[async_trait]
    impl TaskRepository for MemTaskRepo {
        async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError> {
            Ok(self.tasks.lock().unwrap().get(&id).cloned())
        }

        async fn save(&self, task: &Task) -> Result<(), RepositoryError> {
            self.tasks.lock().unwrap().insert(task.id, task.clone());
            Ok(())
        }

        async fn find_by_user(
            &self,
            _user_id: UserId,
            _filter: &TaskFilter,
        ) -> Result<Vec<Task>, RepositoryError> {
            unimplemented!()
        }

        async fn find_by_source(
            &self,
            _user_id: UserId,
            _source: Source,
            _source_id: &str,
        ) -> Result<Option<Task>, RepositoryError> {
            unimplemented!()
        }

        async fn find_by_date_range(
            &self,
            _user_id: UserId,
            _start: chrono::NaiveDate,
            _end: chrono::NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            unimplemented!()
        }

        async fn find_planned_before(
            &self,
            _user_id: UserId,
            _before_date: chrono::NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            unimplemented!()
        }

        async fn save_batch(&self, _tasks: &[Task]) -> Result<(), RepositoryError> {
            unimplemented!()
        }

        async fn delete(&self, _id: TaskId) -> Result<(), RepositoryError> {
            unimplemented!()
        }

        async fn delete_stale_by_source(
            &self,
            _user_id: UserId,
            _source: Source,
            _keep_ids: &[String],
        ) -> Result<u64, RepositoryError> {
            unimplemented!()
        }
    }

    // ── Mock GryzzlyCatalogRepository ────────────────────────────────────────

    #[derive(Default)]
    struct MemCatalogRepo {
        rows: Mutex<HashMap<String, GryzzlyCatalogEntry>>,
    }

    impl MemCatalogRepo {
        fn seed(&self, entry: GryzzlyCatalogEntry) {
            self.rows
                .lock()
                .unwrap()
                .insert(entry.gryzzly_task_id.clone(), entry);
        }
    }

    #[async_trait]
    impl GryzzlyCatalogRepository for MemCatalogRepo {
        async fn upsert(&self, entry: &GryzzlyCatalogEntry) -> Result<(), RepositoryError> {
            self.rows
                .lock()
                .unwrap()
                .insert(entry.gryzzly_task_id.clone(), entry.clone());
            Ok(())
        }

        async fn soft_prune_missing(
            &self,
            _user_id: UserId,
            _keep_ids: &[String],
        ) -> Result<u64, RepositoryError> {
            unimplemented!()
        }

        async fn list_active(
            &self,
            _user_id: UserId,
            _search: Option<&str>,
            _project_filter: Option<&str>,
            _limit: i64,
        ) -> Result<Vec<GryzzlyCatalogEntry>, RepositoryError> {
            unimplemented!()
        }

        async fn find_by_gryzzly_task_id(
            &self,
            user_id: UserId,
            gryzzly_task_id: &str,
        ) -> Result<Option<GryzzlyCatalogEntry>, RepositoryError> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .values()
                .find(|e| e.user_id == user_id && e.gryzzly_task_id == gryzzly_task_id)
                .cloned())
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn make_task(user_id: UserId) -> Task {
        Task {
            id: Uuid::new_v4(),
            user_id,
            title: "Test task".to_string(),
            description: None,
            notes: None,
            status: TaskStatus::Todo,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            project_id: None,
            assignee: None,
            delegated_to: None,
            urgency: domain::types::common::UrgencyLevel::Medium,
            urgency_manual: false,
            impact: domain::types::common::ImpactLevel::Medium,
            tags: vec![],
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            estimated_hours_override: None,
            remaining_hours_override: None,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            tracking_state: domain::types::TrackingState::Inbox,
            recurrence_id: None,
            occurrence_date: None,
            gryzzly_task_id: None,
            gryzzly_project_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_catalog_entry(user_id: UserId, gid: &str, project_id: &str) -> GryzzlyCatalogEntry {
        GryzzlyCatalogEntry {
            id: Uuid::new_v4(),
            user_id,
            gryzzly_task_id: gid.to_string(),
            name: format!("Task {gid}"),
            gryzzly_project_id: project_id.to_string(),
            project_name: "Test Project".to_string(),
            customer_name: None,
            is_active: true,
            project_status: None,
            last_synced_at: Utc::now(),
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn assign_snapshots_project_id() {
        let user_id = make_user_id();
        let task_repo = MemTaskRepo::default();
        let catalog_repo = MemCatalogRepo::default();

        let task = make_task(user_id);
        let task_id = task.id;
        task_repo.seed(task);
        catalog_repo.seed(make_catalog_entry(user_id, "g1", "p1"));

        let result = assign_gryzzly_task(&task_repo, &catalog_repo, task_id, Some("g1".to_string()))
            .await
            .expect("assign should succeed");

        assert_eq!(result.gryzzly_task_id, Some("g1".to_string()));
        assert_eq!(result.gryzzly_project_id, Some("p1".to_string()));

        // Verify persisted
        let saved = task_repo.find_by_id(task_id).await.unwrap().unwrap();
        assert_eq!(saved.gryzzly_task_id, Some("g1".to_string()));
        assert_eq!(saved.gryzzly_project_id, Some("p1".to_string()));
    }

    #[tokio::test]
    async fn assign_unknown_task_is_rejected() {
        let user_id = make_user_id();
        let task_repo = MemTaskRepo::default();
        let catalog_repo = MemCatalogRepo::default();

        let task = make_task(user_id);
        let task_id = task.id;
        task_repo.seed(task);
        // catalog is empty — "g-unknown" does not exist

        let result = assign_gryzzly_task(
            &task_repo,
            &catalog_repo,
            task_id,
            Some("g-unknown".to_string()),
        )
        .await;

        assert!(result.is_err(), "assigning an unknown gryzzly task must return Err");
    }

    #[tokio::test]
    async fn clearing_assignment_nulls_both_fields() {
        let user_id = make_user_id();
        let task_repo = MemTaskRepo::default();
        let catalog_repo = MemCatalogRepo::default();

        let mut task = make_task(user_id);
        task.gryzzly_task_id = Some("g1".to_string());
        task.gryzzly_project_id = Some("p1".to_string());
        let task_id = task.id;
        task_repo.seed(task);

        let result = assign_gryzzly_task(&task_repo, &catalog_repo, task_id, None)
            .await
            .expect("clearing should succeed");

        assert_eq!(result.gryzzly_task_id, None);
        assert_eq!(result.gryzzly_project_id, None);

        // Verify persisted
        let saved = task_repo.find_by_id(task_id).await.unwrap().unwrap();
        assert_eq!(saved.gryzzly_task_id, None);
        assert_eq!(saved.gryzzly_project_id, None);
    }
}
