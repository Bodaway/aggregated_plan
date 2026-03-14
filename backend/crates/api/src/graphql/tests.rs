use std::sync::Arc;

use async_graphql::{EmptySubscription, Schema};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use domain::types::*;
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::*;

use super::mutation::MutationRoot;
use super::query::QueryRoot;
use super::schema::{CombinedMutation, CombinedQuery};

// ─── In-memory repository implementations for testing ───

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

struct InMemoryProjectRepository {
    projects: Mutex<HashMap<ProjectId, Project>>,
}

impl InMemoryProjectRepository {
    fn new() -> Self {
        Self {
            projects: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl ProjectRepository for InMemoryProjectRepository {
    async fn find_by_id(&self, id: ProjectId) -> Result<Option<Project>, RepositoryError> {
        let projects = self.projects.lock().unwrap();
        Ok(projects.get(&id).cloned())
    }

    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<Project>, RepositoryError> {
        let projects = self.projects.lock().unwrap();
        Ok(projects
            .values()
            .filter(|p| p.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn find_by_source(
        &self,
        _user_id: UserId,
        _source: Source,
        _source_id: &str,
    ) -> Result<Option<Project>, RepositoryError> {
        Ok(None)
    }

    async fn save(&self, project: &Project) -> Result<(), RepositoryError> {
        let mut projects = self.projects.lock().unwrap();
        projects.insert(project.id, project.clone());
        Ok(())
    }

    async fn delete(&self, id: ProjectId) -> Result<(), RepositoryError> {
        let mut projects = self.projects.lock().unwrap();
        projects.remove(&id);
        Ok(())
    }
}

struct InMemoryTagRepository {
    tags: Mutex<HashMap<TagId, Tag>>,
}

impl InMemoryTagRepository {
    fn new() -> Self {
        Self {
            tags: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl TagRepository for InMemoryTagRepository {
    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<Tag>, RepositoryError> {
        let tags = self.tags.lock().unwrap();
        Ok(tags
            .values()
            .filter(|t| t.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn find_by_id(&self, id: TagId) -> Result<Option<Tag>, RepositoryError> {
        let tags = self.tags.lock().unwrap();
        Ok(tags.get(&id).cloned())
    }

    async fn save(&self, tag: &Tag) -> Result<(), RepositoryError> {
        let mut tags = self.tags.lock().unwrap();
        tags.insert(tag.id, tag.clone());
        Ok(())
    }

    async fn update(&self, tag: &Tag) -> Result<(), RepositoryError> {
        let mut tags = self.tags.lock().unwrap();
        tags.insert(tag.id, tag.clone());
        Ok(())
    }

    async fn delete(&self, id: TagId) -> Result<(), RepositoryError> {
        let mut tags = self.tags.lock().unwrap();
        tags.remove(&id);
        Ok(())
    }
}

// ─── Stub repositories for types we don't test but need for schema ───

struct StubMeetingRepository;
#[async_trait]
impl MeetingRepository for StubMeetingRepository {
    async fn find_by_id(
        &self,
        _id: MeetingId,
    ) -> Result<Option<Meeting>, RepositoryError> {
        Ok(None)
    }
    async fn update(&self, _meeting: &Meeting) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn find_by_user_and_date(
        &self,
        _user_id: UserId,
        _date: NaiveDate,
    ) -> Result<Vec<Meeting>, RepositoryError> {
        Ok(vec![])
    }
    async fn find_by_user_and_range(
        &self,
        _user_id: UserId,
        _start: NaiveDate,
        _end: NaiveDate,
    ) -> Result<Vec<Meeting>, RepositoryError> {
        Ok(vec![])
    }
    async fn find_by_project(
        &self,
        _user_id: UserId,
        _project_id: ProjectId,
    ) -> Result<Vec<Meeting>, RepositoryError> {
        Ok(vec![])
    }
    async fn upsert_batch(&self, _meetings: &[Meeting]) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn delete_stale(
        &self,
        _user_id: UserId,
        _current_ids: &[String],
    ) -> Result<u64, RepositoryError> {
        Ok(0)
    }
}

struct StubActivitySlotRepository;
#[async_trait]
impl ActivitySlotRepository for StubActivitySlotRepository {
    async fn find_by_id(
        &self,
        _id: ActivitySlotId,
    ) -> Result<Option<ActivitySlot>, RepositoryError> {
        Ok(None)
    }
    async fn find_by_user_and_date(
        &self,
        _user_id: UserId,
        _date: NaiveDate,
    ) -> Result<Vec<ActivitySlot>, RepositoryError> {
        Ok(vec![])
    }
    async fn find_active(
        &self,
        _user_id: UserId,
    ) -> Result<Option<ActivitySlot>, RepositoryError> {
        Ok(None)
    }
    async fn save(&self, _slot: &ActivitySlot) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn update(&self, _slot: &ActivitySlot) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn delete(&self, _id: ActivitySlotId) -> Result<(), RepositoryError> {
        Ok(())
    }
}

struct StubAlertRepository;
#[async_trait]
impl AlertRepository for StubAlertRepository {
    async fn find_by_id(
        &self,
        _id: AlertId,
    ) -> Result<Option<Alert>, RepositoryError> {
        Ok(None)
    }
    async fn find_by_user(
        &self,
        _user_id: UserId,
        _resolved: Option<bool>,
    ) -> Result<Vec<Alert>, RepositoryError> {
        Ok(vec![])
    }
    async fn find_unresolved(
        &self,
        _user_id: UserId,
    ) -> Result<Vec<Alert>, RepositoryError> {
        Ok(vec![])
    }
    async fn save(&self, _alert: &Alert) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn save_batch(&self, _alerts: &[Alert]) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn update(&self, _alert: &Alert) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn delete_resolved(&self, _user_id: UserId) -> Result<u64, RepositoryError> {
        Ok(0)
    }
}

struct StubTaskLinkRepository;
#[async_trait]
impl TaskLinkRepository for StubTaskLinkRepository {
    async fn find_by_user(&self, _user_id: UserId) -> Result<Vec<TaskLink>, RepositoryError> {
        Ok(vec![])
    }
    async fn find_rejected_pairs(
        &self,
        _user_id: UserId,
    ) -> Result<Vec<(TaskId, TaskId)>, RepositoryError> {
        Ok(vec![])
    }
    async fn save(&self, _link: &TaskLink) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn delete(&self, _id: TaskLinkId) -> Result<(), RepositoryError> {
        Ok(())
    }
}

struct StubSyncStatusRepository;
#[async_trait]
impl SyncStatusRepository for StubSyncStatusRepository {
    async fn find_by_user(
        &self,
        _user_id: UserId,
    ) -> Result<Vec<SyncStatus>, RepositoryError> {
        Ok(vec![])
    }
    async fn upsert(&self, _status: &SyncStatus) -> Result<(), RepositoryError> {
        Ok(())
    }
}

struct StubConfigRepository;
#[async_trait]
impl ConfigRepository for StubConfigRepository {
    async fn get(
        &self,
        _user_id: UserId,
        _key: &str,
    ) -> Result<Option<String>, RepositoryError> {
        Ok(None)
    }
    async fn set(
        &self,
        _user_id: UserId,
        _key: &str,
        _value: &str,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn get_all(
        &self,
        _user_id: UserId,
    ) -> Result<Vec<(String, String)>, RepositoryError> {
        Ok(vec![])
    }
}

// ─── Test schema builder ───

type TestSchema = Schema<CombinedQuery, CombinedMutation, EmptySubscription>;

fn build_test_schema() -> TestSchema {
    let default_user_id: UserId =
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("valid default UUID");

    let task_repo: Arc<dyn TaskRepository> = Arc::new(InMemoryTaskRepository::new());
    let meeting_repo: Arc<dyn MeetingRepository> = Arc::new(StubMeetingRepository);
    let project_repo: Arc<dyn ProjectRepository> = Arc::new(InMemoryProjectRepository::new());
    let activity_repo: Arc<dyn ActivitySlotRepository> = Arc::new(StubActivitySlotRepository);
    let alert_repo: Arc<dyn AlertRepository> = Arc::new(StubAlertRepository);
    let tag_repo: Arc<dyn TagRepository> = Arc::new(InMemoryTagRepository::new());
    let task_link_repo: Arc<dyn TaskLinkRepository> = Arc::new(StubTaskLinkRepository);
    let sync_repo: Arc<dyn SyncStatusRepository> = Arc::new(StubSyncStatusRepository);
    let config_repo: Arc<dyn ConfigRepository> = Arc::new(StubConfigRepository);

    Schema::build(
        CombinedQuery(QueryRoot),
        CombinedMutation(MutationRoot),
        EmptySubscription,
    )
    .data(task_repo)
    .data(meeting_repo)
    .data(project_repo)
    .data(activity_repo)
    .data(alert_repo)
    .data(tag_repo)
    .data(task_link_repo)
    .data(sync_repo)
    .data(config_repo)
    .data(default_user_id)
    .finish()
}

// ─── Tests ───

#[tokio::test]
async fn health_query_returns_true() {
    let schema = build_test_schema();
    let result = schema.execute("{ health }").await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["health"], true);
}

#[tokio::test]
async fn create_task_mutation() {
    let schema = build_test_schema();
    let query = r#"
        mutation {
            createTask(input: {
                title: "Test Task"
                description: "A test description"
            }) {
                id
                title
                description
                source
                status
                urgency
                urgencyManual
                impact
                quadrant
            }
        }
    "#;

    let result = schema.execute(query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let task = &data["createTask"];

    assert_eq!(task["title"], "Test Task");
    assert_eq!(task["description"], "A test description");
    assert_eq!(task["source"], "PERSONAL");
    assert_eq!(task["status"], "TODO");
    assert_eq!(task["impact"], "MEDIUM");
    assert_eq!(task["urgencyManual"], false);
    assert!(task["id"].as_str().is_some());
}

#[tokio::test]
async fn create_and_fetch_task() {
    let schema = build_test_schema();

    // Create a task
    let create_result = schema
        .execute(
            r#"
            mutation {
                createTask(input: { title: "Fetch Me" }) {
                    id
                }
            }
        "#,
        )
        .await;
    assert!(
        create_result.errors.is_empty(),
        "Errors: {:?}",
        create_result.errors
    );
    let create_data = create_result.data.into_json().unwrap();
    let task_id = create_data["createTask"]["id"].as_str().unwrap();

    // Fetch the task by ID
    let query = format!(r#"{{ task(id: "{}") {{ id title }} }}"#, task_id);
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();

    assert_eq!(data["task"]["id"], task_id);
    assert_eq!(data["task"]["title"], "Fetch Me");
}

#[tokio::test]
async fn task_not_found_returns_null() {
    let schema = build_test_schema();
    let fake_id = Uuid::new_v4().to_string();
    let query = format!(r#"{{ task(id: "{}") {{ id title }} }}"#, fake_id);
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert!(data["task"].is_null());
}

#[tokio::test]
async fn tasks_query_with_pagination() {
    let schema = build_test_schema();

    // Create 3 tasks
    for title in &["Task A", "Task B", "Task C"] {
        let query = format!(
            r#"mutation {{ createTask(input: {{ title: "{}" }}) {{ id }} }}"#,
            title
        );
        let r = schema.execute(&query).await;
        assert!(r.errors.is_empty(), "Errors: {:?}", r.errors);
    }

    // Fetch first 2
    let result = schema
        .execute(r#"{ tasks(first: 2) { edges { node { title } cursor } pageInfo { hasNextPage hasPreviousPage endCursor } totalCount } }"#)
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let tasks = &data["tasks"];

    assert_eq!(tasks["totalCount"], 3);
    assert_eq!(tasks["edges"].as_array().unwrap().len(), 2);
    assert_eq!(tasks["pageInfo"]["hasNextPage"], true);
    assert_eq!(tasks["pageInfo"]["hasPreviousPage"], false);

    // Fetch remaining using cursor
    let end_cursor = tasks["pageInfo"]["endCursor"].as_str().unwrap();
    let query = format!(
        r#"{{ tasks(first: 10, after: "{}") {{ edges {{ node {{ title }} }} pageInfo {{ hasNextPage }} totalCount }} }}"#,
        end_cursor
    );
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let tasks = &data["tasks"];

    assert_eq!(tasks["edges"].as_array().unwrap().len(), 1);
    assert_eq!(tasks["pageInfo"]["hasNextPage"], false);
}

#[tokio::test]
async fn update_task_mutation() {
    let schema = build_test_schema();

    // Create task
    let create_result = schema
        .execute(
            r#"mutation { createTask(input: { title: "Original" }) { id } }"#,
        )
        .await;
    let create_data = create_result.data.into_json().unwrap();
    let task_id = create_data["createTask"]["id"].as_str().unwrap();

    // Update task
    let query = format!(
        r#"mutation {{ updateTask(id: "{}", input: {{ title: "Updated", status: IN_PROGRESS, impact: HIGH }}) {{ id title status impact }} }}"#,
        task_id
    );
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();

    assert_eq!(data["updateTask"]["title"], "Updated");
    assert_eq!(data["updateTask"]["status"], "IN_PROGRESS");
    assert_eq!(data["updateTask"]["impact"], "HIGH");
}

#[tokio::test]
async fn delete_task_mutation() {
    let schema = build_test_schema();

    // Create task
    let create_result = schema
        .execute(
            r#"mutation { createTask(input: { title: "Delete Me" }) { id } }"#,
        )
        .await;
    let create_data = create_result.data.into_json().unwrap();
    let task_id = create_data["createTask"]["id"].as_str().unwrap();

    // Delete task
    let query = format!(r#"mutation {{ deleteTask(id: "{}") }}"#, task_id);
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["deleteTask"], true);

    // Verify it's gone
    let query = format!(r#"{{ task(id: "{}") {{ id }} }}"#, task_id);
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert!(data["task"].is_null());
}

#[tokio::test]
async fn delete_task_not_found_returns_error() {
    let schema = build_test_schema();
    let fake_id = Uuid::new_v4().to_string();
    let query = format!(r#"mutation {{ deleteTask(id: "{}") }}"#, fake_id);
    let result = schema.execute(&query).await;
    assert!(!result.errors.is_empty(), "Expected an error");
}

#[tokio::test]
async fn complete_task_mutation() {
    let schema = build_test_schema();

    // Create task
    let create_result = schema
        .execute(
            r#"mutation { createTask(input: { title: "Complete Me" }) { id status } }"#,
        )
        .await;
    let create_data = create_result.data.into_json().unwrap();
    let task_id = create_data["createTask"]["id"].as_str().unwrap();
    assert_eq!(create_data["createTask"]["status"], "TODO");

    // Complete task
    let query = format!(
        r#"mutation {{ completeTask(id: "{}") {{ id status }} }}"#,
        task_id
    );
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["completeTask"]["status"], "DONE");
}

#[tokio::test]
async fn priority_matrix_query() {
    let schema = build_test_schema();

    // Create tasks with different urgency/impact
    let queries = [
        r#"mutation { createTask(input: { title: "UI Task", urgency: CRITICAL, impact: CRITICAL }) { id } }"#,
        r#"mutation { createTask(input: { title: "Important Task", urgency: LOW, impact: HIGH }) { id } }"#,
        r#"mutation { createTask(input: { title: "Urgent Task", urgency: HIGH, impact: LOW }) { id } }"#,
        r#"mutation { createTask(input: { title: "Neither Task", urgency: LOW, impact: LOW }) { id } }"#,
    ];

    for q in &queries {
        let r = schema.execute(*q).await;
        assert!(r.errors.is_empty(), "Errors: {:?}", r.errors);
    }

    // Query the priority matrix
    let result = schema
        .execute(
            r#"{
                priorityMatrix {
                    urgentImportant { title }
                    important { title }
                    urgent { title }
                    neither { title }
                }
            }"#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let matrix = &data["priorityMatrix"];

    assert_eq!(matrix["urgentImportant"].as_array().unwrap().len(), 1);
    assert_eq!(matrix["urgentImportant"][0]["title"], "UI Task");
    assert_eq!(matrix["important"].as_array().unwrap().len(), 1);
    assert_eq!(matrix["important"][0]["title"], "Important Task");
    assert_eq!(matrix["urgent"].as_array().unwrap().len(), 1);
    assert_eq!(matrix["urgent"][0]["title"], "Urgent Task");
    assert_eq!(matrix["neither"].as_array().unwrap().len(), 1);
    assert_eq!(matrix["neither"][0]["title"], "Neither Task");
}

#[tokio::test]
async fn update_priority_urgency() {
    let schema = build_test_schema();

    // Create task
    let create_result = schema
        .execute(
            r#"mutation { createTask(input: { title: "Priority Task" }) { id urgency urgencyManual } }"#,
        )
        .await;
    let create_data = create_result.data.into_json().unwrap();
    let task_id = create_data["createTask"]["id"].as_str().unwrap();
    assert_eq!(create_data["createTask"]["urgencyManual"], false);

    // Update priority
    let query = format!(
        r#"mutation {{ updatePriority(taskId: "{}", urgency: CRITICAL) {{ id urgency urgencyManual }} }}"#,
        task_id
    );
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();

    assert_eq!(data["updatePriority"]["urgency"], "CRITICAL");
    assert_eq!(data["updatePriority"]["urgencyManual"], true);
}

#[tokio::test]
async fn update_priority_impact() {
    let schema = build_test_schema();

    // Create task
    let create_result = schema
        .execute(
            r#"mutation { createTask(input: { title: "Impact Task" }) { id impact } }"#,
        )
        .await;
    let create_data = create_result.data.into_json().unwrap();
    let task_id = create_data["createTask"]["id"].as_str().unwrap();
    assert_eq!(create_data["createTask"]["impact"], "MEDIUM");

    // Update impact
    let query = format!(
        r#"mutation {{ updatePriority(taskId: "{}", impact: CRITICAL) {{ id impact }} }}"#,
        task_id
    );
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();

    assert_eq!(data["updatePriority"]["impact"], "CRITICAL");
}

#[tokio::test]
async fn update_priority_requires_at_least_one_field() {
    let schema = build_test_schema();

    let create_result = schema
        .execute(
            r#"mutation { createTask(input: { title: "Task" }) { id } }"#,
        )
        .await;
    let create_data = create_result.data.into_json().unwrap();
    let task_id = create_data["createTask"]["id"].as_str().unwrap();

    let query = format!(
        r#"mutation {{ updatePriority(taskId: "{}") {{ id }} }}"#,
        task_id
    );
    let result = schema.execute(&query).await;
    assert!(!result.errors.is_empty(), "Expected an error");
}

#[tokio::test]
async fn reset_urgency_mutation() {
    let schema = build_test_schema();

    // Create task with manual urgency
    let create_result = schema
        .execute(
            r#"mutation { createTask(input: { title: "Reset Task", urgency: CRITICAL }) { id urgency urgencyManual } }"#,
        )
        .await;
    let create_data = create_result.data.into_json().unwrap();
    let task_id = create_data["createTask"]["id"].as_str().unwrap();
    assert_eq!(create_data["createTask"]["urgency"], "CRITICAL");
    assert_eq!(create_data["createTask"]["urgencyManual"], true);

    // Reset urgency
    let query = format!(
        r#"mutation {{ resetUrgency(taskId: "{}") {{ id urgency urgencyManual }} }}"#,
        task_id
    );
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();

    // No deadline => Low urgency
    assert_eq!(data["resetUrgency"]["urgency"], "LOW");
    assert_eq!(data["resetUrgency"]["urgencyManual"], false);
}

#[tokio::test]
async fn projects_query_returns_empty() {
    let schema = build_test_schema();
    let result = schema
        .execute("{ projects { id name source status } }")
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["projects"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn tags_query_returns_empty() {
    let schema = build_test_schema();
    let result = schema.execute("{ tags { id name color } }").await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["tags"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_task_with_urgency_and_impact() {
    let schema = build_test_schema();
    let result = schema
        .execute(
            r#"
            mutation {
                createTask(input: {
                    title: "Full Task"
                    urgency: HIGH
                    impact: CRITICAL
                    estimatedHours: 8.5
                }) {
                    title
                    urgency
                    urgencyManual
                    impact
                    estimatedHours
                    quadrant
                }
            }
        "#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let task = &data["createTask"];

    assert_eq!(task["urgency"], "HIGH");
    assert_eq!(task["urgencyManual"], true);
    assert_eq!(task["impact"], "CRITICAL");
    assert_eq!(task["estimatedHours"], 8.5);
    assert_eq!(task["quadrant"], "URGENT_IMPORTANT");
}

#[tokio::test]
async fn tasks_query_with_status_filter() {
    let schema = build_test_schema();

    // Create tasks
    let create_result = schema
        .execute(
            r#"mutation { createTask(input: { title: "Todo Task" }) { id } }"#,
        )
        .await;
    assert!(create_result.errors.is_empty());

    let create_result = schema
        .execute(
            r#"mutation { createTask(input: { title: "Done Task" }) { id } }"#,
        )
        .await;
    let create_data = create_result.data.into_json().unwrap();
    let done_id = create_data["createTask"]["id"].as_str().unwrap();

    // Complete the second task
    let query = format!(
        r#"mutation {{ completeTask(id: "{}") {{ id status }} }}"#,
        done_id
    );
    let r = schema.execute(&query).await;
    assert!(r.errors.is_empty());

    // Filter by status
    let result = schema
        .execute(
            r#"{ tasks(filter: { status: [TODO] }) { edges { node { title status } } totalCount } }"#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let tasks = &data["tasks"];

    assert_eq!(tasks["totalCount"], 1);
    assert_eq!(tasks["edges"][0]["node"]["title"], "Todo Task");
    assert_eq!(tasks["edges"][0]["node"]["status"], "TODO");
}

#[tokio::test]
async fn noop_mutation_still_works() {
    let schema = build_test_schema();
    let result = schema.execute("mutation { noop }").await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["noop"], true);
}

#[tokio::test]
async fn daily_dashboard_returns_structure() {
    let schema = build_test_schema();

    // Create a task first
    let create_result = schema
        .execute(
            r#"mutation { createTask(input: { title: "Dashboard Task" }) { id } }"#,
        )
        .await;
    assert!(
        create_result.errors.is_empty(),
        "Errors: {:?}",
        create_result.errors
    );

    // Query the daily dashboard
    let result = schema
        .execute(
            r#"{
                dailyDashboard(date: "2026-03-09") {
                    date
                    tasks { title }
                    meetings { title }
                    alerts { message }
                    syncStatuses { source status }
                    weeklyWorkload {
                        weekStart
                        capacity
                        totalPlanned
                        totalMeetings
                        capacityHours
                        overload
                        excessHours
                        halfDays {
                            date
                            halfDay
                            consumption
                            isFree
                        }
                    }
                }
            }"#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let dashboard = &data["dailyDashboard"];

    assert_eq!(dashboard["date"], "2026-03-09");

    // Tasks should include the one we created (it's in TODO status)
    let tasks = dashboard["tasks"].as_array().unwrap();
    assert!(
        tasks.iter().any(|t| t["title"] == "Dashboard Task"),
        "Expected 'Dashboard Task' in results"
    );

    // Meetings, alerts, sync statuses are empty from stubs
    assert_eq!(dashboard["meetings"].as_array().unwrap().len(), 0);
    assert_eq!(dashboard["alerts"].as_array().unwrap().len(), 0);
    assert_eq!(dashboard["syncStatuses"].as_array().unwrap().len(), 0);

    // Weekly workload should have 10 slots
    let workload = &dashboard["weeklyWorkload"];
    assert_eq!(workload["weekStart"], "2026-03-09");
    assert_eq!(workload["capacity"], 10);
    assert_eq!(workload["capacityHours"], 40.0);
    assert_eq!(workload["overload"], false);
    assert_eq!(workload["excessHours"], 0.0);

    let slots = workload["halfDays"].as_array().unwrap();
    assert_eq!(slots.len(), 10);

    // First slot should be Monday Morning
    assert_eq!(slots[0]["date"], "2026-03-09");
    assert_eq!(slots[0]["halfDay"], "MORNING");
    assert_eq!(slots[0]["isFree"], true);

    // Last slot should be Friday Afternoon
    assert_eq!(slots[9]["date"], "2026-03-13");
    assert_eq!(slots[9]["halfDay"], "AFTERNOON");
}

// ─── Tag CRUD Tests ───

#[tokio::test]
async fn create_tag_mutation() {
    let schema = build_test_schema();

    let result = schema
        .execute(
            r##"mutation { createTag(name: "frontend", color: "#ff0000") { id name color } }"##,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let tag = &data["createTag"];

    assert_eq!(tag["name"], "frontend");
    assert_eq!(tag["color"], "#ff0000");
    assert!(tag["id"].as_str().is_some());
}

#[tokio::test]
async fn create_tag_without_color() {
    let schema = build_test_schema();

    let result = schema
        .execute(r#"mutation { createTag(name: "backend") { id name color } }"#)
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let tag = &data["createTag"];

    assert_eq!(tag["name"], "backend");
    assert!(tag["color"].is_null());
}

#[tokio::test]
async fn create_and_list_tags() {
    let schema = build_test_schema();

    // Create two tags
    schema
        .execute(r#"mutation { createTag(name: "tag1") { id } }"#)
        .await;
    schema
        .execute(r#"mutation { createTag(name: "tag2") { id } }"#)
        .await;

    // List tags
    let result = schema
        .execute("{ tags { id name } }")
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let tags = data["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
}

#[tokio::test]
async fn update_tag_mutation() {
    let schema = build_test_schema();

    // Create tag
    let create_result = schema
        .execute(r#"mutation { createTag(name: "old") { id } }"#)
        .await;
    let create_data = create_result.data.into_json().unwrap();
    let tag_id = create_data["createTag"]["id"].as_str().unwrap();

    // Update tag
    let query = format!(
        r##"mutation {{ updateTag(id: "{}", name: "new", color: "#00ff00") {{ id name color }} }}"##,
        tag_id
    );
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();

    assert_eq!(data["updateTag"]["name"], "new");
    assert_eq!(data["updateTag"]["color"], "#00ff00");
}

#[tokio::test]
async fn delete_tag_mutation() {
    let schema = build_test_schema();

    // Create tag
    let create_result = schema
        .execute(r#"mutation { createTag(name: "delete-me") { id } }"#)
        .await;
    let create_data = create_result.data.into_json().unwrap();
    let tag_id = create_data["createTag"]["id"].as_str().unwrap();

    // Delete tag
    let query = format!(r#"mutation {{ deleteTag(id: "{}") }}"#, tag_id);
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["deleteTag"], true);

    // Verify tags list is empty
    let result = schema.execute("{ tags { id } }").await;
    let data = result.data.into_json().unwrap();
    assert_eq!(data["tags"].as_array().unwrap().len(), 0);
}

// ─── Activity Tracking Tests ───

#[tokio::test]
async fn start_activity_mutation() {
    let schema = build_test_schema();

    let result = schema
        .execute(r#"mutation { startActivity { id halfDay startTime endTime taskId } }"#)
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let slot = &data["startActivity"];

    assert!(slot["id"].as_str().is_some());
    assert!(slot["endTime"].is_null(), "New activity should have no end time");
}

#[tokio::test]
async fn start_activity_with_task_id() {
    let schema = build_test_schema();

    // Create a task first
    let create_result = schema
        .execute(r#"mutation { createTask(input: { title: "Work Item" }) { id } }"#)
        .await;
    let create_data = create_result.data.into_json().unwrap();
    let task_id = create_data["createTask"]["id"].as_str().unwrap();

    // Start activity linked to the task
    let query = format!(
        r#"mutation {{ startActivity(taskId: "{}") {{ id taskId }} }}"#,
        task_id
    );
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();

    assert_eq!(data["startActivity"]["taskId"], task_id);
}

#[tokio::test]
async fn stop_activity_mutation_when_no_active() {
    let schema = build_test_schema();

    let result = schema
        .execute(r#"mutation { stopActivity { id endTime } }"#)
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();

    assert!(data["stopActivity"].is_null(), "No active activity should return null");
}

#[tokio::test]
async fn current_activity_query_returns_null_when_none() {
    let schema = build_test_schema();

    let result = schema
        .execute(r#"{ currentActivity { id } }"#)
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert!(data["currentActivity"].is_null());
}

#[tokio::test]
async fn activity_journal_query_returns_empty() {
    let schema = build_test_schema();

    let result = schema
        .execute(r#"{ activityJournal(date: "2026-03-09") { id halfDay startTime } }"#)
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["activityJournal"].as_array().unwrap().len(), 0);
}

// ─── Alerts Tests ───

#[tokio::test]
async fn alerts_query_returns_empty() {
    let schema = build_test_schema();

    let result = schema
        .execute(
            r#"{ alerts { edges { node { id message alertType severity resolved } cursor } pageInfo { hasNextPage } totalCount } }"#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let alerts = &data["alerts"];

    assert_eq!(alerts["totalCount"], 0);
    assert_eq!(alerts["edges"].as_array().unwrap().len(), 0);
    assert_eq!(alerts["pageInfo"]["hasNextPage"], false);
}

#[tokio::test]
async fn alerts_query_with_resolved_filter() {
    let schema = build_test_schema();

    let result = schema
        .execute(
            r#"{ alerts(resolved: false) { edges { node { id } } totalCount } }"#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["alerts"]["totalCount"], 0);
}

// ─── Configuration Tests ───

#[tokio::test]
async fn configuration_query_returns_empty_object() {
    let schema = build_test_schema();

    let result = schema
        .execute(r#"{ configuration }"#)
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let config = data["configuration"].as_object().unwrap();
    assert!(config.is_empty());
}

#[tokio::test]
async fn update_configuration_mutation() {
    let schema = build_test_schema();

    let result = schema
        .execute(
            r#"mutation { updateConfiguration(key: "jira.url", value: "https://jira.test.com") }"#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["updateConfiguration"], true);
}

// ─── Deduplication Tests ───

#[tokio::test]
async fn deduplication_suggestions_query_empty() {
    let schema = build_test_schema();

    let result = schema
        .execute(
            r#"{ deduplicationSuggestions { id confidenceScore taskA { title } taskB { title } } }"#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let suggestions = data["deduplicationSuggestions"].as_array().unwrap();
    assert!(suggestions.is_empty());
}

#[tokio::test]
async fn link_tasks_mutation() {
    let schema = build_test_schema();

    // Create two tasks
    let create1 = schema
        .execute(r#"mutation { createTask(input: { title: "Task A" }) { id } }"#)
        .await;
    let data1 = create1.data.into_json().unwrap();
    let id1 = data1["createTask"]["id"].as_str().unwrap();

    let create2 = schema
        .execute(r#"mutation { createTask(input: { title: "Task B" }) { id } }"#)
        .await;
    let data2 = create2.data.into_json().unwrap();
    let id2 = data2["createTask"]["id"].as_str().unwrap();

    // Link them
    let query = format!(
        r#"mutation {{ linkTasks(taskIdPrimary: "{}", taskIdSecondary: "{}") }}"#,
        id1, id2
    );
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["linkTasks"], true);
}

#[tokio::test]
async fn confirm_deduplication_mutation() {
    let schema = build_test_schema();

    let id1 = Uuid::new_v4().to_string();
    let id2 = Uuid::new_v4().to_string();

    let query = format!(
        r#"mutation {{ confirmDeduplication(taskIdPrimary: "{}", taskIdSecondary: "{}", accept: true) }}"#,
        id1, id2
    );
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["confirmDeduplication"], true);
}

#[tokio::test]
async fn unlink_tasks_mutation() {
    let schema = build_test_schema();

    let link_id = Uuid::new_v4().to_string();
    let query = format!(r#"mutation {{ unlinkTasks(linkId: "{}") }}"#, link_id);
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["unlinkTasks"], true);
}

// ─── Sync Status Tests ───

#[tokio::test]
async fn sync_statuses_query_returns_empty() {
    let schema = build_test_schema();

    let result = schema
        .execute(r#"{ syncStatuses { source status lastSyncAt errorMessage } }"#)
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let statuses = data["syncStatuses"].as_array().unwrap();
    assert!(statuses.is_empty());
}

#[tokio::test]
async fn weekly_workload_returns_structure() {
    let schema = build_test_schema();

    let result = schema
        .execute(
            r#"{
                weeklyWorkload(weekStart: "2026-03-09") {
                    weekStart
                    capacity
                    totalPlanned
                    totalMeetings
                    capacityHours
                    overload
                    excessHours
                    halfDays {
                        date
                        halfDay
                        consumption
                        isFree
                        meetings { title }
                        tasks { title }
                    }
                }
            }"#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let workload = &data["weeklyWorkload"];

    assert_eq!(workload["weekStart"], "2026-03-09");
    assert_eq!(workload["capacity"], 10);
    assert_eq!(workload["capacityHours"], 40.0);
    assert_eq!(workload["totalPlanned"], 0.0);
    assert_eq!(workload["totalMeetings"], 0.0);
    assert_eq!(workload["overload"], false);
    assert_eq!(workload["excessHours"], 0.0);

    let slots = workload["halfDays"].as_array().unwrap();
    assert_eq!(slots.len(), 10);

    // All slots should be free with no meetings or tasks
    for slot in slots {
        assert_eq!(slot["isFree"], true);
        assert_eq!(slot["consumption"], 0.0);
        assert_eq!(slot["meetings"].as_array().unwrap().len(), 0);
        assert_eq!(slot["tasks"].as_array().unwrap().len(), 0);
    }
}
