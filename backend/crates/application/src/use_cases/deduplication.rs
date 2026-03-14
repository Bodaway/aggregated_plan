use std::collections::HashSet;

use chrono::Utc;
use domain::rules::dedup::{calculate_similarity, find_jira_key_in_text, DEDUP_CONFIDENCE_THRESHOLD};
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::*;

/// A suggested duplicate pair with its similarity details.
pub struct DeduplicationSuggestion {
    pub id: Uuid,
    pub task_a: Task,
    pub task_b: Task,
    pub confidence_score: f64,
    pub title_similarity: f64,
    pub assignee_match: bool,
    pub project_match: bool,
}

/// Detect potential duplicate tasks for a user.
pub async fn find_suggestions(
    task_repo: &dyn TaskRepository,
    task_link_repo: &dyn TaskLinkRepository,
    user_id: UserId,
) -> Result<Vec<DeduplicationSuggestion>, AppError> {
    // 1. Get all active tasks for the user
    let filter = TaskFilter {
        status: Some(vec![TaskStatus::Todo, TaskStatus::InProgress]),
        ..TaskFilter::empty()
    };
    let tasks = task_repo.find_by_user(user_id, &filter).await?;

    // 2. Get rejected pairs to exclude
    let rejected_pairs = task_link_repo.find_rejected_pairs(user_id).await?;
    let rejected_set: HashSet<(TaskId, TaskId)> = rejected_pairs
        .iter()
        .flat_map(|(a, b)| vec![(*a, *b), (*b, *a)])
        .collect();

    // 3. Get existing links to exclude
    let existing_links = task_link_repo.find_by_user(user_id).await?;
    let linked_set: HashSet<(TaskId, TaskId)> = existing_links
        .iter()
        .filter(|l| l.link_type != TaskLinkType::Rejected)
        .flat_map(|l| {
            vec![
                (l.task_id_primary, l.task_id_secondary),
                (l.task_id_secondary, l.task_id_primary),
            ]
        })
        .collect();

    let mut suggestions = Vec::new();

    // 4. For each pair (i, j) where i < j
    for i in 0..tasks.len() {
        for j in (i + 1)..tasks.len() {
            let task_a = &tasks[i];
            let task_b = &tasks[j];

            // Check if already linked or rejected
            if linked_set.contains(&(task_a.id, task_b.id)) {
                continue;
            }
            if rejected_set.contains(&(task_a.id, task_b.id)) {
                continue;
            }

            // R08: Jira key match
            let mut jira_match = false;
            if let Some(ref source_id) = task_a.source_id {
                if task_a.source == Source::Jira {
                    if find_jira_key_in_text(source_id, &task_b.title)
                        || task_b
                            .description
                            .as_deref()
                            .map(|d| find_jira_key_in_text(source_id, d))
                            .unwrap_or(false)
                    {
                        jira_match = true;
                    }
                }
            }
            if !jira_match {
                if let Some(ref source_id) = task_b.source_id {
                    if task_b.source == Source::Jira {
                        if find_jira_key_in_text(source_id, &task_a.title)
                            || task_a
                                .description
                                .as_deref()
                                .map(|d| find_jira_key_in_text(source_id, d))
                                .unwrap_or(false)
                        {
                            jira_match = true;
                        }
                    }
                }
            }

            if jira_match {
                suggestions.push(DeduplicationSuggestion {
                    id: Uuid::new_v4(),
                    task_a: task_a.clone(),
                    task_b: task_b.clone(),
                    confidence_score: 1.0,
                    title_similarity: 1.0,
                    assignee_match: true,
                    project_match: true,
                });
                continue;
            }

            // R09: Similarity scoring
            let project_a = task_a.project_id.map(|p| p.to_string());
            let project_b = task_b.project_id.map(|p| p.to_string());

            let score = calculate_similarity(
                &task_a.title,
                &task_b.title,
                task_a.assignee.as_deref(),
                task_b.assignee.as_deref(),
                project_a.as_deref(),
                project_b.as_deref(),
            );

            if score.overall >= DEDUP_CONFIDENCE_THRESHOLD {
                suggestions.push(DeduplicationSuggestion {
                    id: Uuid::new_v4(),
                    task_a: task_a.clone(),
                    task_b: task_b.clone(),
                    confidence_score: score.overall,
                    title_similarity: score.title_score,
                    assignee_match: score.assignee_match,
                    project_match: score.project_match,
                });
            }
        }
    }

    // 5. Sort by confidence descending
    suggestions.sort_by(|a, b| {
        b.confidence_score
            .partial_cmp(&a.confidence_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(suggestions)
}

/// Confirm or reject a deduplication suggestion.
pub async fn confirm_suggestion(
    task_link_repo: &dyn TaskLinkRepository,
    task_id_primary: TaskId,
    task_id_secondary: TaskId,
    accept: bool,
) -> Result<(), AppError> {
    let link_type = if accept {
        TaskLinkType::AutoMerged
    } else {
        TaskLinkType::Rejected
    };

    let link = TaskLink {
        id: Uuid::new_v4(),
        task_id_primary,
        task_id_secondary,
        link_type,
        confidence_score: None,
        created_at: Utc::now(),
    };

    task_link_repo.save(&link).await?;
    Ok(())
}

/// Manually link two tasks.
pub async fn link_tasks(
    task_link_repo: &dyn TaskLinkRepository,
    task_id_primary: TaskId,
    task_id_secondary: TaskId,
) -> Result<(), AppError> {
    let link = TaskLink {
        id: Uuid::new_v4(),
        task_id_primary,
        task_id_secondary,
        link_type: TaskLinkType::ManualMerged,
        confidence_score: None,
        created_at: Utc::now(),
    };

    task_link_repo.save(&link).await?;
    Ok(())
}

/// Unlink two tasks by deleting their link.
pub async fn unlink_tasks(
    task_link_repo: &dyn TaskLinkRepository,
    link_id: TaskLinkId,
) -> Result<(), AppError> {
    task_link_repo.delete(link_id).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{NaiveDate, Utc};
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::errors::RepositoryError;
    use crate::repositories::TaskFilter;

    // ---- In-memory repos ----

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
                    if let Some(ref statuses) = filter.status {
                        statuses.contains(&t.status)
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

        async fn search(&self, _user_id: UserId, _query: &str, _limit: usize) -> Result<Vec<TaskSearchResult>, RepositoryError> {
            Ok(vec![])
        }
    }

    struct InMemoryTaskLinkRepository {
        links: Mutex<HashMap<TaskLinkId, TaskLink>>,
    }

    impl InMemoryTaskLinkRepository {
        fn new() -> Self {
            Self {
                links: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl TaskLinkRepository for InMemoryTaskLinkRepository {
        async fn find_by_user(
            &self,
            _user_id: UserId,
        ) -> Result<Vec<TaskLink>, RepositoryError> {
            let links = self.links.lock().unwrap();
            Ok(links.values().cloned().collect())
        }

        async fn find_rejected_pairs(
            &self,
            _user_id: UserId,
        ) -> Result<Vec<(TaskId, TaskId)>, RepositoryError> {
            let links = self.links.lock().unwrap();
            Ok(links
                .values()
                .filter(|l| l.link_type == TaskLinkType::Rejected)
                .map(|l| (l.task_id_primary, l.task_id_secondary))
                .collect())
        }

        async fn save(&self, link: &TaskLink) -> Result<(), RepositoryError> {
            let mut links = self.links.lock().unwrap();
            links.insert(link.id, link.clone());
            Ok(())
        }

        async fn delete(&self, id: TaskLinkId) -> Result<(), RepositoryError> {
            let mut links = self.links.lock().unwrap();
            links.remove(&id);
            Ok(())
        }
    }

    fn test_user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn make_task(title: &str) -> Task {
        Task {
            id: Uuid::new_v4(),
            user_id: test_user_id(),
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
            urgency: UrgencyLevel::Low,
            urgency_manual: false,
            impact: ImpactLevel::Medium,
            tags: vec![],
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

    fn make_jira_task(title: &str, jira_key: &str) -> Task {
        let mut task = make_task(title);
        task.source = Source::Jira;
        task.source_id = Some(jira_key.to_string());
        task
    }

    // ---- Tests ----

    #[tokio::test]
    async fn find_suggestions_empty_when_no_tasks() {
        let task_repo = InMemoryTaskRepository::new();
        let link_repo = InMemoryTaskLinkRepository::new();

        let suggestions = find_suggestions(&task_repo, &link_repo, test_user_id())
            .await
            .unwrap();

        assert!(suggestions.is_empty());
    }

    #[tokio::test]
    async fn find_suggestions_no_duplicates_for_different_tasks() {
        let task_repo = InMemoryTaskRepository::new();
        let link_repo = InMemoryTaskLinkRepository::new();

        task_repo.insert(make_task("Build frontend login page"));
        task_repo.insert(make_task("Configure database replication"));

        let suggestions = find_suggestions(&task_repo, &link_repo, test_user_id())
            .await
            .unwrap();

        assert!(
            suggestions.is_empty(),
            "Different tasks should not generate suggestions"
        );
    }

    #[tokio::test]
    async fn find_suggestions_detects_similar_titles_with_assignee() {
        let task_repo = InMemoryTaskRepository::new();
        let link_repo = InMemoryTaskLinkRepository::new();

        // Two tasks with identical titles AND same assignee yields
        // overall = 1.0 * 0.6 + 0.2 (assignee) = 0.8 which passes the 0.7 threshold.
        let mut t1 = make_task("Fix login page bug");
        t1.assignee = Some("alice".to_string());
        let mut t2 = make_task("Fix login page bug");
        t2.assignee = Some("alice".to_string());
        task_repo.insert(t1);
        task_repo.insert(t2);

        let suggestions = find_suggestions(&task_repo, &link_repo, test_user_id())
            .await
            .unwrap();

        assert_eq!(suggestions.len(), 1);
        assert!(suggestions[0].confidence_score >= DEDUP_CONFIDENCE_THRESHOLD);
    }

    #[tokio::test]
    async fn find_suggestions_detects_jira_key_match() {
        let task_repo = InMemoryTaskRepository::new();
        let link_repo = InMemoryTaskLinkRepository::new();

        // Jira task with key PROJ-123
        task_repo.insert(make_jira_task("Implement feature X", "PROJ-123"));
        // Personal task mentioning the key in its title
        let mut personal_task = make_task("Work on PROJ-123 feature X");
        personal_task.source = Source::Personal;
        task_repo.insert(personal_task);

        let suggestions = find_suggestions(&task_repo, &link_repo, test_user_id())
            .await
            .unwrap();

        assert!(!suggestions.is_empty());
        assert!(
            (suggestions[0].confidence_score - 1.0).abs() < f64::EPSILON,
            "Jira key match should have confidence 1.0"
        );
    }

    #[tokio::test]
    async fn find_suggestions_excludes_rejected_pairs() {
        let task_repo = InMemoryTaskRepository::new();
        let link_repo = InMemoryTaskLinkRepository::new();

        // Use Jira key matching to guarantee they would match
        let task_a = make_jira_task("Fix login page bug", "PROJ-100");
        let mut task_b = make_task("Fix PROJ-100 login page bug");
        task_b.source = Source::Personal;
        let a_id = task_a.id;
        let b_id = task_b.id;
        task_repo.insert(task_a);
        task_repo.insert(task_b);

        // Reject the pair
        confirm_suggestion(&link_repo, a_id, b_id, false)
            .await
            .unwrap();

        let suggestions = find_suggestions(&task_repo, &link_repo, test_user_id())
            .await
            .unwrap();

        assert!(
            suggestions.is_empty(),
            "Rejected pairs should be excluded from suggestions"
        );
    }

    #[tokio::test]
    async fn find_suggestions_excludes_already_linked_pairs() {
        let task_repo = InMemoryTaskRepository::new();
        let link_repo = InMemoryTaskLinkRepository::new();

        // Use Jira key matching to guarantee they would match
        let task_a = make_jira_task("Fix login page bug", "PROJ-200");
        let mut task_b = make_task("Fix PROJ-200 login page bug");
        task_b.source = Source::Personal;
        let a_id = task_a.id;
        let b_id = task_b.id;
        task_repo.insert(task_a);
        task_repo.insert(task_b);

        // Link the pair
        link_tasks(&link_repo, a_id, b_id).await.unwrap();

        let suggestions = find_suggestions(&task_repo, &link_repo, test_user_id())
            .await
            .unwrap();

        assert!(
            suggestions.is_empty(),
            "Already linked pairs should be excluded from suggestions"
        );
    }

    #[tokio::test]
    async fn confirm_suggestion_accept_creates_auto_merged_link() {
        let link_repo = InMemoryTaskLinkRepository::new();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        confirm_suggestion(&link_repo, a, b, true).await.unwrap();

        let links = link_repo.find_by_user(test_user_id()).await.unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].link_type, TaskLinkType::AutoMerged);
        assert_eq!(links[0].task_id_primary, a);
        assert_eq!(links[0].task_id_secondary, b);
    }

    #[tokio::test]
    async fn confirm_suggestion_reject_creates_rejected_link() {
        let link_repo = InMemoryTaskLinkRepository::new();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        confirm_suggestion(&link_repo, a, b, false).await.unwrap();

        let links = link_repo.find_by_user(test_user_id()).await.unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].link_type, TaskLinkType::Rejected);
    }

    #[tokio::test]
    async fn link_tasks_creates_manual_merged_link() {
        let link_repo = InMemoryTaskLinkRepository::new();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        link_tasks(&link_repo, a, b).await.unwrap();

        let links = link_repo.find_by_user(test_user_id()).await.unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].link_type, TaskLinkType::ManualMerged);
        assert_eq!(links[0].task_id_primary, a);
        assert_eq!(links[0].task_id_secondary, b);
    }

    #[tokio::test]
    async fn unlink_tasks_removes_link() {
        let link_repo = InMemoryTaskLinkRepository::new();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        link_tasks(&link_repo, a, b).await.unwrap();

        let links = link_repo.find_by_user(test_user_id()).await.unwrap();
        assert_eq!(links.len(), 1);

        let link_id = links[0].id;
        unlink_tasks(&link_repo, link_id).await.unwrap();

        let links_after = link_repo.find_by_user(test_user_id()).await.unwrap();
        assert!(links_after.is_empty());
    }

    #[tokio::test]
    async fn find_suggestions_sorted_by_confidence_descending() {
        let task_repo = InMemoryTaskRepository::new();
        let link_repo = InMemoryTaskLinkRepository::new();

        // Pair 1: Jira key match (always 1.0)
        task_repo.insert(make_jira_task("Build API endpoint", "API-42"));
        let mut personal = make_task("Review API-42 endpoint code");
        personal.source = Source::Personal;
        task_repo.insert(personal);

        // Pair 2: same title + same assignee = 0.6 + 0.2 = 0.8
        let mut t1 = make_task("Database migration script");
        t1.assignee = Some("bob".to_string());
        let mut t2 = make_task("Database migration script");
        t2.assignee = Some("bob".to_string());
        task_repo.insert(t1);
        task_repo.insert(t2);

        let suggestions = find_suggestions(&task_repo, &link_repo, test_user_id())
            .await
            .unwrap();

        assert!(
            suggestions.len() >= 2,
            "Expected at least 2 suggestions, got {}",
            suggestions.len()
        );
        // Should be sorted descending by confidence
        for i in 1..suggestions.len() {
            assert!(
                suggestions[i - 1].confidence_score >= suggestions[i].confidence_score,
                "Suggestions should be sorted by confidence descending"
            );
        }
        // First suggestion should be the Jira match (1.0)
        assert!(
            (suggestions[0].confidence_score - 1.0).abs() < f64::EPSILON,
            "First suggestion should be the Jira key match with confidence 1.0"
        );
    }
}
