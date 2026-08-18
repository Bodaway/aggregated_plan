use std::sync::Arc;

use async_graphql::{EmptySubscription, Schema};
use async_trait::async_trait;
use chrono::NaiveDate;
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
            .filter(|t| match (&filter.source_id, &t.source_id) {
                (Some(needle), Some(actual)) => actual == needle,
                (Some(_), None) => false,
                (None, _) => true,
            })
            .filter(|t| match &filter.title_contains {
                Some(needle) => t.title.to_lowercase().contains(&needle.to_lowercase()),
                None => true,
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

    async fn find_planned_before(
        &self,
        _user_id: UserId,
        _before_date: NaiveDate,
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

    async fn list_delegates(&self, user_id: UserId) -> Result<Vec<String>, RepositoryError> {
        let tasks = self.tasks.lock().unwrap();
        let mut names: Vec<String> = tasks
            .values()
            .filter(|t| t.user_id == user_id)
            .filter_map(|t| t.delegated_to.clone())
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
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

/// Activity slots kept in memory rather than discarded, so a resolver that reads
/// back what it wrote — the reattribution repair does, to report measured hours —
/// can be tested at all.
#[derive(Default)]
struct InMemoryActivitySlotRepository {
    slots: Mutex<Vec<ActivitySlot>>,
}

#[async_trait]
impl ActivitySlotRepository for InMemoryActivitySlotRepository {
    async fn find_by_id(
        &self,
        id: ActivitySlotId,
    ) -> Result<Option<ActivitySlot>, RepositoryError> {
        Ok(self
            .slots
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == id)
            .cloned())
    }
    async fn find_by_user_and_date(
        &self,
        user_id: UserId,
        date: NaiveDate,
    ) -> Result<Vec<ActivitySlot>, RepositoryError> {
        Ok(self
            .slots
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.user_id == user_id && s.date == date)
            .cloned()
            .collect())
    }
    async fn find_by_user_and_date_range(
        &self,
        user_id: UserId,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<ActivitySlot>, RepositoryError> {
        Ok(self
            .slots
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.user_id == user_id && s.date >= start && s.date <= end)
            .cloned()
            .collect())
    }
    async fn find_active(
        &self,
        user_id: UserId,
    ) -> Result<Option<ActivitySlot>, RepositoryError> {
        Ok(self
            .slots
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.user_id == user_id && s.end_time.is_none())
            .cloned())
    }
    async fn save(&self, slot: &ActivitySlot) -> Result<(), RepositoryError> {
        self.slots.lock().unwrap().push(slot.clone());
        Ok(())
    }
    async fn update(&self, slot: &ActivitySlot) -> Result<(), RepositoryError> {
        let mut slots = self.slots.lock().unwrap();
        if let Some(existing) = slots.iter_mut().find(|s| s.id == slot.id) {
            *existing = slot.clone();
        }
        Ok(())
    }
    async fn delete(&self, id: ActivitySlotId) -> Result<(), RepositoryError> {
        self.slots.lock().unwrap().retain(|s| s.id != id);
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

/// Was a pure discard-everything stub — `get`/`get_all` always answered empty
/// regardless of prior `set` calls. That made the config store unobservable from a
/// GraphQL test: no assertion could tell a key that was actually written from one
/// that never was. Now a real in-memory map, so `{ configuration }` is a faithful
/// instrument on what a resolver wrote via `ConfigRepository::set`.
struct StubConfigRepository {
    values: Mutex<HashMap<(UserId, String), String>>,
}
impl StubConfigRepository {
    fn new() -> Self {
        Self {
            values: Mutex::new(HashMap::new()),
        }
    }
}
#[async_trait]
impl ConfigRepository for StubConfigRepository {
    async fn get(
        &self,
        user_id: UserId,
        key: &str,
    ) -> Result<Option<String>, RepositoryError> {
        Ok(self.values.lock().unwrap().get(&(user_id, key.to_string())).cloned())
    }
    async fn set(
        &self,
        user_id: UserId,
        key: &str,
        value: &str,
    ) -> Result<(), RepositoryError> {
        self.values
            .lock()
            .unwrap()
            .insert((user_id, key.to_string()), value.to_string());
        Ok(())
    }
    async fn get_all(
        &self,
        user_id: UserId,
    ) -> Result<Vec<(String, String)>, RepositoryError> {
        Ok(self
            .values
            .lock()
            .unwrap()
            .iter()
            .filter(|((uid, _), _)| *uid == user_id)
            .map(|((_, k), v)| (k.clone(), v.clone()))
            .collect())
    }
}

struct InMemoryWorklogRepository {
    entries: Mutex<Vec<domain::types::WorklogEntry>>,
}

impl InMemoryWorklogRepository {
    fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl application::repositories::WorklogRepository for InMemoryWorklogRepository {
    async fn create(
        &self,
        entry: &domain::types::WorklogEntry,
    ) -> Result<(), RepositoryError> {
        self.entries.lock().unwrap().push(entry.clone());
        Ok(())
    }
    async fn update(
        &self,
        entry: &domain::types::WorklogEntry,
    ) -> Result<(), RepositoryError> {
        let mut v = self.entries.lock().unwrap();
        if let Some(slot) = v.iter_mut().find(|e| e.id == entry.id) {
            *slot = entry.clone();
        }
        Ok(())
    }
    async fn delete(
        &self,
        id: domain::types::WorklogEntryId,
        user_id: UserId,
    ) -> Result<bool, RepositoryError> {
        let mut v = self.entries.lock().unwrap();
        let before = v.len();
        v.retain(|e| !(e.id == id && e.user_id == user_id));
        Ok(v.len() < before)
    }
    async fn find_by_id(
        &self,
        id: domain::types::WorklogEntryId,
        user_id: UserId,
    ) -> Result<Option<domain::types::WorklogEntry>, RepositoryError> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.id == id && e.user_id == user_id)
            .cloned())
    }
    async fn list(
        &self,
        user_id: UserId,
        filter: &application::repositories::WorklogFilter,
    ) -> Result<Vec<domain::types::WorklogEntry>, RepositoryError> {
        let v = self.entries.lock().unwrap();
        let mut out: Vec<domain::types::WorklogEntry> = v
            .iter()
            .filter(|e| e.user_id == user_id)
            .filter(|e| match &filter.task_ids {
                Some(ids) => ids.contains(&e.task_id),
                None => true,
            })
            .filter(|e| match filter.from {
                Some(f) => e.logged_at >= f,
                None => true,
            })
            .filter(|e| match filter.to {
                Some(t) => e.logged_at < t,
                None => true,
            })
            .cloned()
            .collect();
        out.sort_by(|a, b| b.logged_at.cmp(&a.logged_at));
        let start = filter.offset as usize;
        let end = (start + filter.limit as usize).min(out.len());
        if start >= out.len() {
            Ok(vec![])
        } else {
            Ok(out[start..end].to_vec())
        }
    }
    async fn find_by_recurrence(
        &self,
        _user_id: UserId,
        _template_id: domain::types::recurrence::RecurrenceTemplateId,
        _limit: u32,
        _offset: u32,
    ) -> Result<Vec<domain::types::WorklogEntry>, RepositoryError> {
        Ok(vec![])
    }
    async fn find_by_id_prefix(
        &self,
        user_id: UserId,
        prefix: &str,
        limit: u32,
    ) -> Result<Vec<domain::types::WorklogEntry>, RepositoryError> {
        let entries = self.entries.lock().unwrap();
        let mut out: Vec<domain::types::WorklogEntry> = entries
            .iter()
            .filter(|e| e.user_id == user_id && e.id.to_string().starts_with(prefix))
            .cloned()
            .collect();
        out.truncate(limit.max(1) as usize);
        Ok(out)
    }
    async fn reassign_task(
        &self,
        user_id: UserId,
        ids: &[domain::types::WorklogEntryId],
        from_task: TaskId,
        to_task: TaskId,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, RepositoryError> {
        let mut entries = self.entries.lock().unwrap();
        let mut moved = 0u64;
        for entry in entries.iter_mut() {
            if entry.user_id == user_id && entry.task_id == from_task && ids.contains(&entry.id) {
                entry.task_id = to_task;
                entry.updated_at = now;
                moved += 1;
            }
        }
        Ok(moved)
    }
}

struct StubRecurrenceRepository;
#[async_trait]
impl application::repositories::RecurrenceRepository for StubRecurrenceRepository {
    async fn find_by_id(
        &self,
        _id: domain::types::recurrence::RecurrenceTemplateId,
    ) -> Result<Option<domain::types::recurrence::RecurrenceTemplate>, RepositoryError> {
        Ok(None)
    }
    async fn find_active_by_user(
        &self,
        _user_id: UserId,
    ) -> Result<Vec<domain::types::recurrence::RecurrenceTemplate>, RepositoryError> {
        Ok(vec![])
    }
    async fn save(
        &self,
        _template: &domain::types::recurrence::RecurrenceTemplate,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn deactivate(
        &self,
        _id: domain::types::recurrence::RecurrenceTemplateId,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
}

struct StubGraphTokenProvider;
#[async_trait]
impl application::services::GraphTokenProvider for StubGraphTokenProvider {
    async fn valid_access_token(
        &self,
        _user_id: UserId,
    ) -> Result<String, application::errors::AppError> {
        Err(application::errors::AppError::Configuration(
            "no token in tests".into(),
        ))
    }
}

/// Sessions kept in memory, mirroring the trait's semantics: `upsert` never rewrites
/// `started_at`, `list_open` excludes ended sessions, `end` is idempotent.
#[derive(Default)]
struct InMemorySessionRepository {
    rows: Mutex<Vec<Session>>,
}

#[async_trait]
impl application::repositories::SessionRepository for InMemorySessionRepository {
    async fn find_by_id(
        &self,
        id: &str,
        user_id: UserId,
    ) -> Result<Option<Session>, RepositoryError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == id && s.user_id == user_id)
            .cloned())
    }

    async fn upsert(&self, session: &Session) -> Result<(), RepositoryError> {
        let mut rows = self.rows.lock().unwrap();
        match rows.iter_mut().find(|s| s.id == session.id) {
            Some(existing) => {
                // Mirrors the SQL `ON CONFLICT ... DO UPDATE SET` list exactly:
                // `task_id`, `mode`, `label`, `last_seen_at`, `ended_at`.
                // `started_at` and `last_flush_at` are deliberately absent — a
                // rebind is the same session, and plan 2 anchors its flush
                // window on `started_at`.
                existing.task_id = session.task_id;
                existing.mode = session.mode;
                existing.label = session.label.clone();
                existing.last_seen_at = session.last_seen_at;
                existing.ended_at = session.ended_at;
            }
            None => rows.push(session.clone()),
        }
        Ok(())
    }

    async fn list_open(&self, user_id: UserId) -> Result<Vec<Session>, RepositoryError> {
        let mut open: Vec<Session> = self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.user_id == user_id && s.is_open())
            .cloned()
            .collect();
        open.sort_by(|a, b| b.last_seen_at.cmp(&a.last_seen_at));
        Ok(open)
    }

    async fn list_idle_open(
        &self,
        user_id: UserId,
        idle_before: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Session>, RepositoryError> {
        let mut idle: Vec<Session> = self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.user_id == user_id && s.is_open() && s.last_seen_at < idle_before)
            .cloned()
            .collect();
        idle.sort_by(|a, b| a.last_seen_at.cmp(&b.last_seen_at));
        Ok(idle)
    }

    async fn touch(
        &self,
        id: &str,
        user_id: UserId,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, RepositoryError> {
        let mut rows = self.rows.lock().unwrap();
        match rows
            .iter_mut()
            .find(|s| s.id == id && s.user_id == user_id && s.is_open())
        {
            Some(s) => {
                s.last_seen_at = at;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn set_last_flush(
        &self,
        id: &str,
        user_id: UserId,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, RepositoryError> {
        let mut rows = self.rows.lock().unwrap();
        match rows.iter_mut().find(|s| s.id == id && s.user_id == user_id) {
            Some(s) => {
                s.last_flush_at = Some(at);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn end(
        &self,
        id: &str,
        user_id: UserId,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, RepositoryError> {
        let mut rows = self.rows.lock().unwrap();
        match rows.iter_mut().find(|s| s.id == id && s.user_id == user_id) {
            Some(s) if s.is_open() => {
                s.ended_at = Some(at);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

/// Delegates every operation to a real in-memory session store except `touch`,
/// which always errors — pins `addWorklogEntry`'s contract that a touch
/// failure must not fail the worklog write that already succeeded (I2).
struct FailingTouchSessionRepository(InMemorySessionRepository);

impl FailingTouchSessionRepository {
    fn new() -> Self {
        Self(InMemorySessionRepository::default())
    }
}

#[async_trait]
impl application::repositories::SessionRepository for FailingTouchSessionRepository {
    async fn find_by_id(
        &self,
        id: &str,
        user_id: UserId,
    ) -> Result<Option<Session>, RepositoryError> {
        self.0.find_by_id(id, user_id).await
    }

    async fn upsert(&self, session: &Session) -> Result<(), RepositoryError> {
        self.0.upsert(session).await
    }

    async fn list_open(&self, user_id: UserId) -> Result<Vec<Session>, RepositoryError> {
        self.0.list_open(user_id).await
    }

    async fn list_idle_open(
        &self,
        user_id: UserId,
        idle_before: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Session>, RepositoryError> {
        self.0.list_idle_open(user_id, idle_before).await
    }

    async fn touch(
        &self,
        _id: &str,
        _user_id: UserId,
        _at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, RepositoryError> {
        Err(RepositoryError::Database(
            "touch always fails in this test double".to_string(),
        ))
    }

    async fn set_last_flush(
        &self,
        id: &str,
        user_id: UserId,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, RepositoryError> {
        self.0.set_last_flush(id, user_id, at).await
    }

    async fn end(
        &self,
        id: &str,
        user_id: UserId,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, RepositoryError> {
        self.0.end(id, user_id, at).await
    }
}

/// Delegates every operation to a real in-memory session store, but records the
/// order `set_last_flush` and `upsert` are called in. `setSessionMode(OFF)` must
/// flush the session's bound task *before* the mode write clears it — presence
/// of both calls is not enough to tell "flushed then cleared" apart from
/// "cleared then flushed too late to matter" (by which point the task id is
/// already gone and nothing is flushed at all), so the test asserts the
/// recorded order directly, the same way round 1's CLI-level ordering test did
/// against wire order.
struct OrderRecordingSessionRepository {
    inner: InMemorySessionRepository,
    order: Arc<Mutex<Vec<&'static str>>>,
}

impl OrderRecordingSessionRepository {
    fn new(order: Arc<Mutex<Vec<&'static str>>>) -> Self {
        Self {
            inner: InMemorySessionRepository::default(),
            order,
        }
    }
}

#[async_trait]
impl application::repositories::SessionRepository for OrderRecordingSessionRepository {
    async fn find_by_id(
        &self,
        id: &str,
        user_id: UserId,
    ) -> Result<Option<Session>, RepositoryError> {
        self.inner.find_by_id(id, user_id).await
    }

    async fn upsert(&self, session: &Session) -> Result<(), RepositoryError> {
        let tag = if session.mode == SessionMode::Off {
            "upsert_off"
        } else {
            "upsert_other"
        };
        self.order.lock().unwrap().push(tag);
        self.inner.upsert(session).await
    }

    async fn list_open(&self, user_id: UserId) -> Result<Vec<Session>, RepositoryError> {
        self.inner.list_open(user_id).await
    }

    async fn list_idle_open(
        &self,
        user_id: UserId,
        idle_before: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Session>, RepositoryError> {
        self.inner.list_idle_open(user_id, idle_before).await
    }

    async fn touch(
        &self,
        id: &str,
        user_id: UserId,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, RepositoryError> {
        self.inner.touch(id, user_id, at).await
    }

    async fn set_last_flush(
        &self,
        id: &str,
        user_id: UserId,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, RepositoryError> {
        self.order.lock().unwrap().push("set_last_flush");
        self.inner.set_last_flush(id, user_id, at).await
    }

    async fn end(
        &self,
        id: &str,
        user_id: UserId,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, RepositoryError> {
        self.inner.end(id, user_id, at).await
    }
}

// ---- Timesheet-draft in-memory repo (captures upserts) ----
struct InMemoryTimesheetDraftRepository {
    drafts: Mutex<HashMap<(UserId, chrono::NaiveDate), domain::types::TimesheetDraft>>,
}
impl InMemoryTimesheetDraftRepository {
    fn new() -> Self {
        Self {
            drafts: Mutex::new(HashMap::new()),
        }
    }
}
#[async_trait]
impl application::repositories::TimesheetDraftRepository for InMemoryTimesheetDraftRepository {
    async fn upsert(&self, draft: &domain::types::TimesheetDraft) -> Result<(), RepositoryError> {
        self.drafts
            .lock()
            .unwrap()
            .insert((draft.user_id, draft.date), draft.clone());
        Ok(())
    }
    async fn find_by_user_and_date(
        &self,
        user_id: UserId,
        date: chrono::NaiveDate,
    ) -> Result<Option<domain::types::TimesheetDraft>, RepositoryError> {
        Ok(self.drafts.lock().unwrap().get(&(user_id, date)).cloned())
    }
    async fn set_status(
        &self,
        user_id: UserId,
        date: chrono::NaiveDate,
        status: domain::types::TimesheetStatus,
    ) -> Result<(), RepositoryError> {
        if let Some(d) = self.drafts.lock().unwrap().get_mut(&(user_id, date)) {
            d.status = status;
        }
        Ok(())
    }
}

// ---- Signal-mapping in-memory repo ----
struct InMemorySignalMappingRepository {
    rows: Mutex<Vec<domain::types::SignalMapping>>,
}
impl InMemorySignalMappingRepository {
    fn new() -> Self {
        Self {
            rows: Mutex::new(vec![]),
        }
    }
}
#[async_trait]
impl application::repositories::SignalMappingRepository for InMemorySignalMappingRepository {
    async fn list_enabled(
        &self,
        user_id: UserId,
    ) -> Result<Vec<domain::types::SignalMapping>, RepositoryError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.user_id == user_id && m.is_enabled)
            .cloned()
            .collect())
    }
    async fn upsert(&self, m: &domain::types::SignalMapping) -> Result<(), RepositoryError> {
        let mut rows = self.rows.lock().unwrap();
        rows.retain(|r| !(r.user_id == m.user_id && r.kind == m.kind && r.pattern == m.pattern));
        rows.push(m.clone());
        Ok(())
    }
    async fn set_enabled(
        &self,
        _id: domain::types::SignalMappingId,
        _enabled: bool,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn delete(&self, _id: domain::types::SignalMappingId) -> Result<(), RepositoryError> {
        Ok(())
    }
}

// ---- Gryzzly catalog in-memory repo ----
struct InMemoryGryzzlyCatalogRepository {
    rows: Mutex<Vec<domain::types::GryzzlyCatalogEntry>>,
}
impl InMemoryGryzzlyCatalogRepository {
    fn new() -> Self {
        Self {
            rows: Mutex::new(vec![]),
        }
    }
}
#[async_trait]
impl application::repositories::GryzzlyCatalogRepository for InMemoryGryzzlyCatalogRepository {
    async fn upsert(&self, e: &domain::types::GryzzlyCatalogEntry) -> Result<(), RepositoryError> {
        self.rows.lock().unwrap().push(e.clone());
        Ok(())
    }
    async fn soft_prune_missing(&self, _u: UserId, _keep: &[String]) -> Result<u64, RepositoryError> {
        Ok(0)
    }
    async fn list_active(
        &self,
        user_id: UserId,
        _search: Option<&str>,
        _project: Option<&str>,
        _limit: i64,
    ) -> Result<Vec<domain::types::GryzzlyCatalogEntry>, RepositoryError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.user_id == user_id && e.is_active)
            .cloned()
            .collect())
    }
    async fn find_by_gryzzly_task_id(
        &self,
        user_id: UserId,
        gid: &str,
    ) -> Result<Option<domain::types::GryzzlyCatalogEntry>, RepositoryError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.user_id == user_id && e.gryzzly_task_id == gid)
            .cloned())
    }
}

// ---- Stub GitConnector (no commits) ----
struct StubGitConnector;
#[async_trait]
impl application::services::git_connector::GitConnector for StubGitConnector {
    async fn commits_between(
        &self,
        _repos: &[String],
        _from: chrono::DateTime<chrono::Utc>,
        _to: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<application::services::git_connector::GitCommit>, application::errors::AppError> {
        Ok(vec![])
    }
}

// ---- In-memory semantic-memory store (repository + retriever) ----

/// Backs both `MemoryRepository` and `MemoryRetriever` off one Vec, so a resolver
/// test can `remember` then `recall`.
///
/// `search` approximates FTS5: it pulls the quoted phrases out of the MATCH
/// expression and substring-matches them, ignoring AND/OR structure. The real
/// FTS semantics (adjacency, prefix expansion, de-pluralization) are covered by
/// `infrastructure::database::memory_repo` against real SQLite — this stub only
/// exists so the resolver plumbing is exercisable.
#[derive(Default)]
struct InMemoryMemoryStore {
    rows: Mutex<Vec<Memory>>,
}

impl InMemoryMemoryStore {
    /// Insert a row directly, for states the `remember` resolver cannot produce
    /// (already invalidated, already rejected). Everything else goes through the
    /// mutation.
    fn seed(&self, memory: Memory) {
        self.rows.lock().unwrap().push(memory);
    }
}

#[async_trait]
impl MemoryRepository for InMemoryMemoryStore {
    async fn create(&self, memory: &Memory) -> Result<(), RepositoryError> {
        self.rows.lock().unwrap().push(memory.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        id: MemoryId,
        user_id: UserId,
    ) -> Result<Option<Memory>, RepositoryError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.id == id && m.user_id == user_id)
            .cloned())
    }

    async fn list(
        &self,
        user_id: UserId,
        filter: &MemoryListFilter,
    ) -> Result<Vec<Memory>, RepositoryError> {
        let rows = self.rows.lock().unwrap();
        let mut found: Vec<Memory> = rows
            .iter()
            .filter(|m| m.user_id == user_id)
            .filter(|m| match &filter.status {
                None => true,
                Some(wanted) => wanted.contains(&m.status),
            })
            .filter(|m| filter.include_invalidated || m.invalidated_at.is_none())
            .cloned()
            .collect();
        found.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at));
        Ok(found
            .into_iter()
            .skip(filter.offset as usize)
            .take(filter.limit as usize)
            .collect())
    }

    async fn update(&self, memory: &Memory) -> Result<(), RepositoryError> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(row) = rows
            .iter_mut()
            .find(|m| m.id == memory.id && m.user_id == memory.user_id)
        {
            *row = memory.clone();
        }
        Ok(())
    }

    async fn apply_merge(
        &self,
        survivor: &Memory,
        discarded: MemoryId,
        user_id: UserId,
    ) -> Result<(), RepositoryError> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(row) = rows
            .iter_mut()
            .find(|m| m.id == survivor.id && m.user_id == survivor.user_id)
        {
            *row = survivor.clone();
        }
        rows.retain(|m| !(m.id == discarded && m.user_id == user_id));
        Ok(())
    }

    async fn apply_supersession(
        &self,
        invalidated: &Memory,
        successor: &Memory,
    ) -> Result<(), RepositoryError> {
        let mut rows = self.rows.lock().unwrap();
        for updated in [invalidated, successor] {
            if let Some(row) = rows
                .iter_mut()
                .find(|m| m.id == updated.id && m.user_id == updated.user_id)
            {
                *row = updated.clone();
            }
        }
        Ok(())
    }

    /// Mirrors the SQLite implementation: the user's memories whose id starts with
    /// `prefix`, newest first, no status filter. The absence of that filter is the
    /// point — a prefix unique among pending candidates but shared with an active
    /// memory must still come back as two matches.
    async fn find_by_id_prefix(
        &self,
        user_id: UserId,
        prefix: &str,
        limit: u32,
    ) -> Result<Vec<Memory>, RepositoryError> {
        let rows = self.rows.lock().unwrap();
        let mut found: Vec<Memory> = rows
            .iter()
            .filter(|m| m.user_id == user_id && m.id.to_string().starts_with(prefix))
            .cloned()
            .collect();
        found.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at));
        Ok(found.into_iter().take(limit as usize).collect())
    }

    async fn existing_source_refs(
        &self,
        user_id: UserId,
        prefix: &str,
    ) -> Result<Vec<String>, RepositoryError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.user_id == user_id)
            .filter_map(|m| m.source_ref.clone())
            .filter(|source_ref| source_ref.starts_with(prefix))
            .collect())
    }

    async fn supersession_chain(
        &self,
        user_id: UserId,
        from: MemoryId,
    ) -> Result<Vec<MemoryId>, RepositoryError> {
        let rows = self.rows.lock().unwrap();
        let mut chain: Vec<MemoryId> = Vec::new();
        let mut cursor = from;
        // Stop on an already-seen id so a loop in the stored data terminates.
        while let Some(next) = rows
            .iter()
            .find(|m| m.id == cursor && m.user_id == user_id)
            .and_then(|m| m.superseded_by)
        {
            if next == from || chain.contains(&next) {
                break;
            }
            chain.push(next);
            cursor = next;
        }
        Ok(chain)
    }
}

#[async_trait]
impl application::services::MemoryRetriever for InMemoryMemoryStore {
    async fn search(
        &self,
        user_id: UserId,
        query: &application::services::RecallQuery,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<domain::rules::recall::ScoredMemory>, RepositoryError> {
        let needles: Vec<String> = query
            .match_query
            .split('"')
            .skip(1)
            .step_by(2)
            .map(|phrase| phrase.to_lowercase())
            .collect();

        let rows = self.rows.lock().unwrap();
        let candidates: Vec<(Memory, f64)> = rows
            .iter()
            .filter(|m| m.user_id == user_id)
            .filter(|m| query.include_history || m.is_recallable())
            .filter(|m| {
                let haystack =
                    format!("{} {}", m.title, m.body.clone().unwrap_or_default()).to_lowercase();
                needles.iter().any(|n| haystack.contains(n))
            })
            .cloned()
            .map(|m| (m, -1.0))
            .collect();

        let mut ranked =
            domain::rules::recall::rank(candidates, &query.context, now, &query.weights);
        ranked.truncate(query.limit as usize);
        Ok(ranked)
    }
}

/// Memory-file source double. Empty by default; `files` can be seeded to exercise
/// the `importMemories` resolver without touching a real directory.
#[derive(Default)]
struct StubMemoryFileSource {
    files: Vec<application::services::MemoryFile>,
}

#[async_trait]
impl application::services::MemoryFileSource for StubMemoryFileSource {
    async fn list(
        &self,
        _directory: &str,
    ) -> Result<Vec<application::services::MemoryFile>, application::errors::AppError> {
        Ok(self.files.clone())
    }
}

// ─── Test schema builder ───

type TestSchema = Schema<CombinedQuery, CombinedMutation, EmptySubscription>;

/// Build the test schema from pre-seeded (or freshly-empty) repos for the four
/// timesheet-reconstruction dependencies. All other dependencies get fresh in-memory
/// (or stub) instances, matching `build_test_schema()`'s previous defaults.
fn build_test_schema_with(
    worklog_repo: Arc<dyn application::repositories::WorklogRepository>,
    task_repo: Arc<dyn TaskRepository>,
    gryzzly_catalog_repo: Arc<dyn application::repositories::GryzzlyCatalogRepository>,
    timesheet_draft_repo: Arc<dyn application::repositories::TimesheetDraftRepository>,
) -> TestSchema {
    build_test_schema_with_memory(
        worklog_repo,
        task_repo,
        gryzzly_catalog_repo,
        timesheet_draft_repo,
        Arc::new(InMemoryMemoryStore::default()),
        Arc::new(InMemorySessionRepository::default()),
    )
}

/// Same as `build_test_schema_with`, plus an explicit semantic-memory store, so a
/// memory test can keep a handle on it and seed rows the resolvers cannot produce
/// (already invalidated, already rejected), and an explicit session repo, so an
/// I2 test can swap in one that fails on `touch`.
fn build_test_schema_with_memory(
    worklog_repo: Arc<dyn application::repositories::WorklogRepository>,
    task_repo: Arc<dyn TaskRepository>,
    gryzzly_catalog_repo: Arc<dyn application::repositories::GryzzlyCatalogRepository>,
    timesheet_draft_repo: Arc<dyn application::repositories::TimesheetDraftRepository>,
    memory_store: Arc<InMemoryMemoryStore>,
    session_repo: Arc<dyn application::repositories::SessionRepository>,
) -> TestSchema {
    let default_user_id: UserId =
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("valid default UUID");

    let meeting_repo: Arc<dyn MeetingRepository> = Arc::new(StubMeetingRepository);
    let project_repo: Arc<dyn ProjectRepository> = Arc::new(InMemoryProjectRepository::new());
    let activity_repo: Arc<dyn ActivitySlotRepository> =
        Arc::new(InMemoryActivitySlotRepository::default());
    let alert_repo: Arc<dyn AlertRepository> = Arc::new(StubAlertRepository);
    let tag_repo: Arc<dyn TagRepository> = Arc::new(InMemoryTagRepository::new());
    let task_link_repo: Arc<dyn TaskLinkRepository> = Arc::new(StubTaskLinkRepository);
    let sync_repo: Arc<dyn SyncStatusRepository> = Arc::new(StubSyncStatusRepository);
    let config_repo: Arc<dyn ConfigRepository> = Arc::new(StubConfigRepository::new());
    let recurrence_repo: Arc<dyn application::repositories::RecurrenceRepository> =
        Arc::new(StubRecurrenceRepository);
    let graph_token_provider: Arc<dyn application::services::GraphTokenProvider> =
        Arc::new(StubGraphTokenProvider);
    let signal_mapping_repo: Arc<dyn application::repositories::SignalMappingRepository> =
        Arc::new(InMemorySignalMappingRepository::new());
    let git_connector: Arc<dyn application::services::git_connector::GitConnector> =
        Arc::new(StubGitConnector);
    // One store behind both traits, so `remember` is visible to `recall`.
    let memory_repo: Arc<dyn MemoryRepository> = memory_store.clone();
    let memory_retriever: Arc<dyn application::services::MemoryRetriever> = memory_store;
    let memory_file_source: Arc<dyn application::services::MemoryFileSource> =
        Arc::new(StubMemoryFileSource::default());

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
    .data(worklog_repo)
    .data(recurrence_repo)
    .data(gryzzly_catalog_repo)
    .data(timesheet_draft_repo)
    .data(signal_mapping_repo)
    .data(memory_repo)
    .data(memory_retriever)
    .data(memory_file_source)
    .data(git_connector)
    .data(graph_token_provider)
    .data(session_repo)
    .data(default_user_id)
    .finish()
}

fn build_test_schema() -> TestSchema {
    build_test_schema_with(
        Arc::new(InMemoryWorklogRepository::new()),
        Arc::new(InMemoryTaskRepository::new()),
        Arc::new(InMemoryGryzzlyCatalogRepository::new()),
        Arc::new(InMemoryTimesheetDraftRepository::new()),
    )
}

/// Default dependencies, with the semantic-memory store handed back for seeding.
fn build_memory_test_schema() -> (TestSchema, Arc<InMemoryMemoryStore>) {
    let memory_store = Arc::new(InMemoryMemoryStore::default());
    let schema = build_test_schema_with_memory(
        Arc::new(InMemoryWorklogRepository::new()),
        Arc::new(InMemoryTaskRepository::new()),
        Arc::new(InMemoryGryzzlyCatalogRepository::new()),
        Arc::new(InMemoryTimesheetDraftRepository::new()),
        memory_store.clone(),
        Arc::new(InMemorySessionRepository::default()),
    );
    (schema, memory_store)
}

/// Same dependencies as `build_test_schema()`, except the session repo always
/// errors on `touch` — used to pin the non-fatal-touch contract on
/// `addWorklogEntry` (I2): the worklog write must still succeed.
fn build_test_schema_with_failing_session_touch() -> TestSchema {
    build_test_schema_with_memory(
        Arc::new(InMemoryWorklogRepository::new()),
        Arc::new(InMemoryTaskRepository::new()),
        Arc::new(InMemoryGryzzlyCatalogRepository::new()),
        Arc::new(InMemoryTimesheetDraftRepository::new()),
        Arc::new(InMemoryMemoryStore::default()),
        Arc::new(FailingTouchSessionRepository::new()),
    )
}

/// Default dependencies, plus one already-created task — for session-binding tests
/// that need a real task id to point a session at.
async fn schema_with_one_task() -> (TestSchema, Uuid) {
    let schema = build_test_schema();
    let created = schema
        .execute(r#"mutation { createTask(input: { title: "Tracked task" }) { id } }"#)
        .await;
    assert!(created.errors.is_empty(), "{:?}", created.errors);
    let task_id = created.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    (schema, Uuid::parse_str(&task_id).unwrap())
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

/// Two manual slots on different tasks, 30 minutes into each other, must be
/// reported with both task titles and the exact minute count — not merely as
/// a non-empty list, which a stub returning any single overlap would also
/// satisfy.
#[tokio::test]
async fn activity_overlaps_reports_both_titles_and_minutes() {
    let schema = build_test_schema();

    let task_a = schema
        .execute(r#"mutation { createTask(input: { title: "Saft cadrage" }) { id } }"#)
        .await;
    assert!(task_a.errors.is_empty(), "Errors: {:?}", task_a.errors);
    let task_a_id = task_a.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let task_b = schema
        .execute(r#"mutation { createTask(input: { title: "Cartier" }) { id } }"#)
        .await;
    assert!(task_b.errors.is_empty(), "Errors: {:?}", task_b.errors);
    let task_b_id = task_b.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let slot_a = schema
        .execute(&format!(
            r#"mutation {{ createActivitySlot(input: {{ startTime: "2026-03-09T09:00:00Z", endTime: "2026-03-09T10:00:00Z", taskId: "{task_a_id}" }}) {{ id }} }}"#
        ))
        .await;
    assert!(slot_a.errors.is_empty(), "Errors: {:?}", slot_a.errors);
    let slot_a_id = slot_a.data.into_json().unwrap()["createActivitySlot"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let slot_b = schema
        .execute(&format!(
            r#"mutation {{ createActivitySlot(input: {{ startTime: "2026-03-09T09:30:00Z", endTime: "2026-03-09T11:00:00Z", taskId: "{task_b_id}" }}) {{ id }} }}"#
        ))
        .await;
    assert!(slot_b.errors.is_empty(), "Errors: {:?}", slot_b.errors);
    let slot_b_id = slot_b.data.into_json().unwrap()["createActivitySlot"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let result = schema
        .execute(
            r#"{ activityOverlaps(date: "2026-03-09") {
                minutes
                a { slotId sessionId task { title } }
                b { slotId sessionId task { title } }
            } }"#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let overlaps = data["activityOverlaps"].as_array().unwrap();
    assert_eq!(overlaps.len(), 1);
    assert_eq!(overlaps[0]["minutes"], 30);

    // Per-side pairing, keyed by the slotId we captured at creation — not by
    // GraphQL side ("a"/"b") or by title membership. `slotId` is asserted
    // here, not merely selected: a `From` impl that kept each side's own
    // `slot_id` but paired it with the *other* side's `task_id` (the swap
    // the paired-struct design exists to prevent) would still satisfy an
    // order-independent `titles.contains` check on both titles being present
    // somewhere, but fails this, because the slot identified by
    // `slot_a_id` would come back titled "Cartier" instead of its own task.
    for side in ["a", "b"] {
        let slot_id = overlaps[0][side]["slotId"].as_str().unwrap();
        let title = overlaps[0][side]["task"]["title"].as_str().unwrap();
        if slot_id == slot_a_id {
            assert_eq!(title, "Saft cadrage", "side {side} (slot A) has the wrong title");
        } else if slot_id == slot_b_id {
            assert_eq!(title, "Cartier", "side {side} (slot B) has the wrong title");
        } else {
            panic!("side {side} has slotId {slot_id}, which matches neither slot_a_id nor slot_b_id");
        }
    }

    // Both slots are manual (no session), which must survive to the GraphQL layer.
    assert!(overlaps[0]["a"]["sessionId"].is_null());
    assert!(overlaps[0]["b"]["sessionId"].is_null());
}

/// A clean day — no colliding slots — must report an empty list, not a
/// zero-minute entry: a warning on a day with no overlap would train the
/// user to ignore it (Task 9's concern, but the query must not manufacture
/// the case in the first place).
#[tokio::test]
async fn activity_overlaps_empty_on_a_clean_day() {
    let schema = build_test_schema();

    let result = schema
        .execute(r#"{ activityOverlaps(date: "2026-03-09") { minutes } }"#)
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["activityOverlaps"].as_array().unwrap().len(), 0);
}

/// Task 9's carried-in gap: Task 8 shipped with no assertion able to fail on
/// `sessionId` at this layer — both its tests above assert only `is_null()`
/// on two *manual* slots, so a hardcoded or swapped `None` in the
/// `From<ActivityOverlap> for ActivityOverlapGql` impl (`types/activity.rs`)
/// or in `ActivityOverlapSideGql::session_id` passes everything, because the
/// application layer (which is covered) hands the whole `ActivitySlot`
/// across intact — a swap there is near-impossible. This impl is the only
/// place a `manuel ↔ manuel` pair can be born from a real session's work.
///
/// Neither `createActivitySlot` nor `startActivity(taskId)` can mint a slot
/// carrying a session id, so this uses the real route that can:
/// `bindSession` → `addWorklogEntry(sessionId:)` → `flushWorklogTime(sessionId:)`.
/// `aplan.timezone` is pinned to UTC first so the materialized slot's LOCAL
/// half-day date (`derive_time_blocks` classifies by local wall-clock) can
/// never land on a different calendar day than the manual slot's UTC date
/// around midnight — with the timezone fixed to UTC the two computations
/// coincide by construction, not by luck of when the suite happens to run.
#[tokio::test]
async fn activity_overlaps_keeps_a_real_session_id_on_its_own_side_not_the_other() {
    let schema = build_test_schema();

    let tz = schema
        .execute(r#"mutation { updateConfiguration(key: "aplan.timezone", value: "UTC") }"#)
        .await;
    assert!(tz.errors.is_empty(), "Errors: {:?}", tz.errors);

    // Task A: manual, no session — the human, working by hand.
    let task_a = schema
        .execute(r#"mutation { createTask(input: { title: "Manuel task" }) { id } }"#)
        .await;
    assert!(task_a.errors.is_empty(), "Errors: {:?}", task_a.errors);
    let task_a_id = task_a.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Task B: session-tracked, materialized via the real route.
    let task_b = schema
        .execute(r#"mutation { createTask(input: { title: "Session task" }) { id } }"#)
        .await;
    assert!(task_b.errors.is_empty(), "Errors: {:?}", task_b.errors);
    let task_b_id = task_b.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let bound = schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_b_id}") {{ session {{ id }} }} }}"#
        ))
        .await;
    assert!(bound.errors.is_empty(), "Errors: {:?}", bound.errors);

    let added = schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_b_id}", body: "did the thing", sessionId: "s1") {{ loggedAt }} }}"#
        ))
        .await;
    assert!(added.errors.is_empty(), "Errors: {:?}", added.errors);
    let logged_at_str = added.data.into_json().unwrap()["addWorklogEntry"]["loggedAt"]
        .as_str()
        .unwrap()
        .to_string();
    let logged_at = chrono::DateTime::parse_from_rfc3339(&logged_at_str)
        .unwrap()
        .with_timezone(&chrono::Utc);

    let flushed = schema
        .execute(format!(
            r#"mutation {{ flushWorklogTime(taskId: "{task_b_id}", sessionId: "s1") {{ slotsWritten }} }}"#
        ))
        .await;
    assert!(flushed.errors.is_empty(), "Errors: {:?}", flushed.errors);
    let slots_written = flushed.data.into_json().unwrap()["flushWorklogTime"]["slotsWritten"]
        .as_i64()
        .unwrap();
    assert!(
        slots_written >= 1,
        "expected the session's own entry to materialize at least one slot"
    );

    // Task A's manual slot starts exactly at the entry's own instant (not
    // `logged_at - 30min`, which flaked once daily: `create_manual_activity_slot`
    // files a slot under `start_time.date_naive()`, so between 00:00 and 00:30
    // UTC a 30-minute-earlier start landed on the *previous* day, dropping this
    // slot out of the `activityOverlaps(date: logged_at's day)` query below and
    // failing the "exactly one pair" assertion. Starting exactly at `logged_at`
    // makes `start.date_naive() == logged_at.date_naive()` true by construction
    // — no floor/clamp arithmetic to get right — while `end` stays 30 minutes
    // out for a generous margin over the materialized (>= 1 minute) session
    // slot, whatever its exact bounds turn out to be; `end` crossing midnight
    // is harmless since filing keys on `start_time` alone.
    let start = logged_at.to_rfc3339();
    let end = (logged_at + chrono::Duration::minutes(30)).to_rfc3339();
    let slot_a = schema
        .execute(format!(
            r#"mutation {{ createActivitySlot(input: {{ startTime: "{start}", endTime: "{end}", taskId: "{task_a_id}" }}) {{ id }} }}"#
        ))
        .await;
    assert!(slot_a.errors.is_empty(), "Errors: {:?}", slot_a.errors);
    let slot_a_id = slot_a.data.into_json().unwrap()["createActivitySlot"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let date = logged_at.date_naive().to_string();
    let result = schema
        .execute(format!(
            r#"{{ activityOverlaps(date: "{date}") {{
                minutes
                a {{ slotId sessionId }}
                b {{ slotId sessionId }}
            }} }}"#
        ))
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let overlaps = data["activityOverlaps"].as_array().unwrap();
    assert_eq!(
        overlaps.len(),
        1,
        "expected exactly one overlapping pair, got {:?}",
        overlaps
    );

    // Per-side, keyed by the slotId captured at creation — not by GraphQL
    // side ("a"/"b") and not merely "a session id is present somewhere". A
    // hardcoded or swapped `None` in the `From` impl (or the side resolver)
    // would put "s1" on the wrong side, or drop it to null on both, and
    // still satisfy an unkeyed "s1 appears somewhere" check — it fails this
    // one either way, because task A's own slot is asserted null and task
    // B's own slot is asserted "s1", by id.
    let mut found_manual = false;
    let mut found_session = false;
    for side in ["a", "b"] {
        let slot_id = overlaps[0][side]["slotId"].as_str().unwrap();
        let session_id = overlaps[0][side]["sessionId"].as_str();
        if slot_id == slot_a_id {
            assert_eq!(
                session_id, None,
                "task A's own manual slot must carry a null sessionId, not s1"
            );
            found_manual = true;
        } else {
            assert_eq!(
                session_id,
                Some("s1"),
                "the session-materialized slot's own side must carry sessionId s1"
            );
            found_session = true;
        }
    }
    assert!(
        found_manual && found_session,
        "expected one manual side and one session side, got {:?}",
        overlaps[0]
    );
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

    let read = schema.execute(r#"{ configuration }"#).await;
    assert_eq!(
        read.data.into_json().unwrap()["configuration"]["jira.url"],
        "https://jira.test.com"
    );
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

#[tokio::test]
async fn tasks_query_filters_by_source_id() {
    let schema = build_test_schema();

    let _ = schema
        .execute(r#"mutation { createTask(input: { title: "Auth migration" }) { id } }"#)
        .await;

    // No-match: filter for a sourceId that none of the seeded tasks have.
    let no_match = schema
        .execute(r#"{ tasks(filter: { sourceId: "DOES-NOT-EXIST" }) { totalCount } }"#)
        .await;
    assert!(no_match.errors.is_empty(), "Errors: {:?}", no_match.errors);
    let no_match_data = no_match.data.into_json().unwrap();
    assert_eq!(no_match_data["tasks"]["totalCount"], 0);
}

#[tokio::test]
async fn searchable_tasks_excludes_dismissed() {
    let schema = build_test_schema();

    // Inbox (default) — included
    let _ = schema
        .execute(r#"mutation { createTask(input: { title: "Inbox task" }) { id } }"#)
        .await;

    // Followed — included
    let followed_res = schema
        .execute(r#"mutation { createTask(input: { title: "Followed task" }) { id } }"#)
        .await;
    let followed_id = followed_res.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = schema
        .execute(&format!(
            r#"mutation {{ setTrackingState(taskId: "{}", state: FOLLOWED) {{ id }} }}"#,
            followed_id
        ))
        .await;

    // Dismissed — excluded
    let dismissed_res = schema
        .execute(r#"mutation { createTask(input: { title: "Dismissed task" }) { id } }"#)
        .await;
    let dismissed_id = dismissed_res.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = schema
        .execute(&format!(
            r#"mutation {{ setTrackingState(taskId: "{}", state: DISMISSED) {{ id }} }}"#,
            dismissed_id
        ))
        .await;

    let result = schema
        .execute(r#"{ searchableTasks { id title } }"#)
        .await;
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

    let data = result.data.into_json().unwrap();
    let titles: Vec<String> = data["searchableTasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap().to_string())
        .collect();

    assert_eq!(titles.len(), 2);
    assert!(titles.contains(&"Inbox task".to_string()));
    assert!(titles.contains(&"Followed task".to_string()));
    assert!(!titles.contains(&"Dismissed task".to_string()));
}

#[tokio::test]
async fn tasks_query_filters_by_title_contains() {
    let schema = build_test_schema();

    let _ = schema
        .execute(r#"mutation { createTask(input: { title: "Auth migration" }) { id } }"#)
        .await;
    let _ = schema
        .execute(r#"mutation { createTask(input: { title: "Database backup" }) { id } }"#)
        .await;

    // Substring match (case-insensitive)
    let match_result = schema
        .execute(
            r#"{ tasks(filter: { titleContains: "AUTH" }) { totalCount edges { node { title } } } }"#,
        )
        .await;
    assert!(
        match_result.errors.is_empty(),
        "Errors: {:?}",
        match_result.errors
    );
    let data = match_result.data.into_json().unwrap();
    assert_eq!(data["tasks"]["totalCount"], 1);
    assert_eq!(data["tasks"]["edges"][0]["node"]["title"], "Auth migration");

    // No match
    let no_match = schema
        .execute(r#"{ tasks(filter: { titleContains: "xyzzy" }) { totalCount } }"#)
        .await;
    assert!(no_match.errors.is_empty(), "Errors: {:?}", no_match.errors);
    let no_match_data = no_match.data.into_json().unwrap();
    assert_eq!(no_match_data["tasks"]["totalCount"], 0);
}

#[tokio::test]
async fn searchable_tasks_resolves_tag_and_project_names() {
    let schema = build_test_schema();

    // Create project
    let project_res = schema
        .execute(
            r#"mutation { createProject(input: { name: "Platform Team" }) { id } }"#,
        )
        .await;
    assert!(project_res.errors.is_empty(), "create project: {:?}", project_res.errors);
    let project_id = project_res.data.into_json().unwrap()["createProject"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Create tag
    let tag_res = schema
        .execute(r#"mutation { createTag(name: "backend") { id } }"#)
        .await;
    assert!(tag_res.errors.is_empty(), "create tag: {:?}", tag_res.errors);
    let tag_id = tag_res.data.into_json().unwrap()["createTag"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Create task referencing both
    let task_res = schema
        .execute(&format!(
            r#"mutation {{ createTask(input: {{
                title: "Refactor auth middleware",
                projectId: "{}",
                tagIds: ["{}"]
            }}) {{ id }} }}"#,
            project_id, tag_id
        ))
        .await;
    assert!(task_res.errors.is_empty(), "create task: {:?}", task_res.errors);

    let result = schema
        .execute(r#"{ searchableTasks { title projectName tags } }"#)
        .await;
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

    let data = result.data.into_json().unwrap();
    let first = &data["searchableTasks"][0];
    assert_eq!(first["title"], "Refactor auth middleware");
    assert_eq!(first["projectName"], "Platform Team");
    assert_eq!(first["tags"][0], "backend");
}

#[tokio::test]
async fn update_task_sets_and_clears_delegated_to() {
    let schema = build_test_schema();

    let create = schema
        .execute(r#"mutation { createTask(input: { title: "Delegate me" }) { id } }"#)
        .await;
    assert!(create.errors.is_empty(), "Errors: {:?}", create.errors);
    let task_id = create.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Set
    let set = schema
        .execute(format!(
            r#"mutation {{ updateTask(id: "{}", input: {{ delegatedTo: "Marie" }}) {{ id delegatedTo }} }}"#,
            task_id
        ))
        .await;
    assert!(set.errors.is_empty(), "Errors: {:?}", set.errors);
    assert_eq!(
        set.data.into_json().unwrap()["updateTask"]["delegatedTo"],
        "Marie"
    );

    // Clear with explicit null
    let clear = schema
        .execute(format!(
            r#"mutation {{ updateTask(id: "{}", input: {{ delegatedTo: null }}) {{ id delegatedTo }} }}"#,
            task_id
        ))
        .await;
    assert!(clear.errors.is_empty(), "Errors: {:?}", clear.errors);
    assert!(clear.data.into_json().unwrap()["updateTask"]["delegatedTo"].is_null());
}

#[tokio::test]
async fn delegates_query_returns_learned_names() {
    let schema = build_test_schema();

    for (title, name) in [("T1", "Marie"), ("T2", "Ahmed"), ("T3", "Marie")] {
        let create = schema
            .execute(format!(
                r#"mutation {{ createTask(input: {{ title: "{}" }}) {{ id }} }}"#,
                title
            ))
            .await;
        let id = create.data.into_json().unwrap()["createTask"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let update = schema
            .execute(format!(
                r#"mutation {{ updateTask(id: "{}", input: {{ delegatedTo: "{}" }}) {{ id }} }}"#,
                id, name
            ))
            .await;
        assert!(update.errors.is_empty(), "Errors: {:?}", update.errors);
    }

    let result = schema.execute("{ delegates }").await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["delegates"], serde_json::json!(["Ahmed", "Marie"]));
}

// ─── Worklog Tests ───

#[tokio::test]
async fn flush_worklog_time_materializes_morning_slot() {
    let schema = build_test_schema();

    // Create a task to associate worklog entries with.
    let create_result = schema
        .execute(r#"mutation { createTask(input: { title: "Worklog Task" }) { id } }"#)
        .await;
    assert!(
        create_result.errors.is_empty(),
        "create task errors: {:?}",
        create_result.errors
    );
    let task_id = create_result.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Seed two worklog entries in the same local morning (Europe/Paris = UTC+2 in June).
    // 08:00 UTC = 10:00 Paris — morning half-day.
    for logged_at in ["2026-06-08T08:00:00Z", "2026-06-08T09:30:00Z"] {
        let seed = format!(
            r#"mutation {{ addWorklogEntry(taskId: "{}", body: "work", loggedAt: "{}") {{ id }} }}"#,
            task_id, logged_at
        );
        let r = schema.execute(&seed).await;
        assert!(r.errors.is_empty(), "seed worklog errors: {:?}", r.errors);
    }

    // Execute the flush.
    let query = format!(
        r#"mutation {{ flushWorklogTime(taskId: "{}") {{ slotsWritten activeSince }} }}"#,
        task_id
    );
    let result = schema.execute(&query).await;
    assert!(
        result.errors.is_empty(),
        "flushWorklogTime errors: {:?}",
        result.errors
    );
    let data = result.data.into_json().unwrap();
    let flush = &data["flushWorklogTime"];

    assert!(
        flush["slotsWritten"].as_i64().unwrap() >= 1,
        "expected at least one slot written, got: {}",
        flush["slotsWritten"]
    );
    assert!(
        flush["activeSince"].as_str().is_some(),
        "activeSince should be a datetime string"
    );
}

/// `sessions.last_flush_at`/`set_last_flush` shipped with no caller in plan 1; this
/// pins the caller: a flush given a `sessionId` advances that session's own row, not
/// the global `aplan.active_since` key.
#[tokio::test]
async fn flush_advances_the_sessions_own_window_not_the_global_key() {
    let (schema, task_id) = schema_with_one_task().await;
    schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_id}") {{ session {{ id }} }} }}"#
        ))
        .await;
    schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "x", sessionId: "s1") {{ id }} }}"#
        ))
        .await;

    let flushed = schema
        .execute(format!(
            r#"mutation {{ flushWorklogTime(taskId: "{task_id}", sessionId: "s1") {{ slotsWritten }} }}"#
        ))
        .await;
    assert!(flushed.errors.is_empty(), "{:?}", flushed.errors);

    let read = schema.execute(r#"{ claudeSession(id: "s1") { lastFlushAt } }"#).await;
    assert!(
        !read.data.into_json().unwrap()["claudeSession"]["lastFlushAt"].is_null(),
        "the session's own window must have advanced"
    );

    // Not the global key: a session flush must leave the human's own pointer alone,
    // or a later agent flush would jerk it to `now` and the human's next flush would
    // stop selecting the day they actually worked.
    let config = schema.execute(r#"{ configuration }"#).await;
    assert!(
        config.data.into_json().unwrap()["configuration"]["aplan.active_since"].is_null(),
        "a session flush must not touch the human's aplan.active_since key"
    );
}

/// The defect this task closes: one shared watermark meant flushing task B's session
/// advanced the mark for task A's session too, so A's entries were never
/// materialized. Two sessions bound to the same task must keep independent windows.
#[tokio::test]
async fn flushing_one_sessions_task_does_not_move_another_sessions_window() {
    let (schema, task_id) = schema_with_one_task().await;
    for id in ["s1", "s2"] {
        schema
            .execute(format!(
                r#"mutation {{ bindSession(sessionId: "{id}", taskId: "{task_id}") {{ session {{ id }} }} }}"#
            ))
            .await;
    }
    let flushed = schema
        .execute(format!(
            r#"mutation {{ flushWorklogTime(taskId: "{task_id}", sessionId: "s1") {{ slotsWritten }} }}"#
        ))
        .await;
    assert!(flushed.errors.is_empty(), "{:?}", flushed.errors);

    let this = schema.execute(r#"{ claudeSession(id: "s1") { lastFlushAt } }"#).await;
    assert!(
        !this.data.into_json().unwrap()["claudeSession"]["lastFlushAt"].is_null(),
        "s1's own window must have advanced"
    );

    let other = schema.execute(r#"{ claudeSession(id: "s2") { lastFlushAt } }"#).await;
    assert!(
        other.data.into_json().unwrap()["claudeSession"]["lastFlushAt"].is_null(),
        "s2's window is not s1's to advance — this is the shared-watermark bug"
    );
}

/// No `sessionId` at all — the human, working by hand — must still work, and must
/// still advance the human's own `aplan.active_since` pointer.
#[tokio::test]
async fn a_flush_without_a_session_still_uses_the_humans_pointer() {
    let (schema, task_id) = schema_with_one_task().await;

    let before = schema.execute(r#"{ configuration }"#).await;
    assert!(
        before.data.into_json().unwrap()["configuration"]["aplan.active_since"].is_null(),
        "aplan.active_since should be unset before the first human flush"
    );

    let flushed = schema
        .execute(format!(
            r#"mutation {{ flushWorklogTime(taskId: "{task_id}") {{ slotsWritten activeSince }} }}"#
        ))
        .await;
    assert!(flushed.errors.is_empty(), "{:?}", flushed.errors);
    let flushed_data = flushed.data.into_json().unwrap();
    let returned_active_since = flushed_data["flushWorklogTime"]["activeSince"]
        .as_str()
        .unwrap()
        .to_string();

    // The write, not just the response: `activeSince` on `FlushOutcome` proves
    // nothing about the config store on its own — assert the human's pointer was
    // actually advanced there, to the same value the mutation returned.
    let after = schema.execute(r#"{ configuration }"#).await;
    let stored = after.data.into_json().unwrap()["configuration"]["aplan.active_since"]
        .as_str()
        .map(str::to_string);
    assert_eq!(
        stored,
        Some(returned_active_since),
        "flushing without a session must advance the human's aplan.active_since key"
    );
}

/// A `sessionId` naming no row must be refused outright, never folded into "no
/// session" and silently answered from the human's pointer instead — plan 3's
/// hooks lean on a session-scoped call never touching `aplan.active_since`
/// (SPEC_TECHNIQUE.md §15.1). Before the fix, the unmatched `(None, Some(sid))`
/// case fell into the catch-all arm and advanced the human's key anyway.
#[tokio::test]
async fn flush_with_an_unknown_session_id_is_refused_not_a_silent_fallback() {
    let (schema, task_id) = schema_with_one_task().await;

    let flushed = schema
        .execute(format!(
            r#"mutation {{ flushWorklogTime(taskId: "{task_id}", sessionId: "ghost") {{ slotsWritten }} }}"#
        ))
        .await;
    assert!(
        !flushed.errors.is_empty(),
        "an unknown sessionId must be refused, not silently answered"
    );
    assert!(
        flushed.errors[0].message.contains("ghost"),
        "the error should name the unknown session id, got: {:?}",
        flushed.errors
    );

    let config = schema.execute(r#"{ configuration }"#).await;
    assert!(
        config.data.into_json().unwrap()["configuration"]["aplan.active_since"].is_null(),
        "the human's pointer must be untouched by a refused session-scoped flush"
    );
}

// ─── Reattribution Tests ───

/// Seed two tasks and a local morning of worklog entries on the first one, then
/// flush so the day carries the slots a real mis-attribution would have left.
async fn seed_a_misattributed_morning(schema: &TestSchema) -> (String, String) {
    let mut ids = Vec::new();
    for title in ["Wrong task", "Right task"] {
        let created = schema
            .execute(format!(
                r#"mutation {{ createTask(input: {{ title: "{title}" }}) {{ id }} }}"#
            ))
            .await;
        assert!(created.errors.is_empty(), "create: {:?}", created.errors);
        ids.push(
            created.data.into_json().unwrap()["createTask"]["id"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    let (wrong, right) = (ids[0].clone(), ids[1].clone());

    // 07:00 and 07:15 UTC = 09:00 and 09:15 Paris — one continuous local morning
    // stretch, hence one slot worth a quarter of an hour.
    for logged_at in ["2026-08-03T07:00:00Z", "2026-08-03T07:15:00Z"] {
        let seed = schema
            .execute(format!(
                r#"mutation {{ addWorklogEntry(taskId: "{wrong}", body: "work", loggedAt: "{logged_at}") {{ id }} }}"#
            ))
            .await;
        assert!(seed.errors.is_empty(), "seed: {:?}", seed.errors);
    }
    let flush = schema
        .execute(format!(
            r#"mutation {{ flushWorklogTime(taskId: "{wrong}") {{ slotsWritten }} }}"#
        ))
        .await;
    assert!(flush.errors.is_empty(), "flush: {:?}", flush.errors);

    (wrong, right)
}

const REATTRIBUTION_FIELDS: &str = "applied selectedEntries movedEntries affectedDates \
     slotsDiscarded slotsRebuilt source { taskId hoursBefore hoursAfter } \
     destination { taskId hoursBefore hoursAfter }";

/// The default call previews: this verb rewrites billing history, so writing must be
/// something the caller asked for in as many words.
#[tokio::test]
async fn reattributing_without_confirm_previews_and_writes_nothing() {
    let schema = build_test_schema();
    let (wrong, right) = seed_a_misattributed_morning(&schema).await;

    let result = schema
        .execute(format!(
            r#"mutation {{ reattributeWorklogEntries(input: {{ fromTask: "{wrong}", toTask: "{right}", since: "2026-08-03" }}) {{ {REATTRIBUTION_FIELDS} }} }}"#
        ))
        .await;
    assert!(result.errors.is_empty(), "preview: {:?}", result.errors);
    let out = result.data.into_json().unwrap()["reattributeWorklogEntries"].clone();

    assert_eq!(out["applied"], false);
    assert_eq!(out["movedEntries"], 0);
    assert_eq!(out["selectedEntries"].as_array().unwrap().len(), 2);
    assert_eq!(out["affectedDates"], serde_json::json!(["2026-08-03"]));
    assert_eq!(out["source"]["hoursBefore"], 0.25);
    assert_eq!(out["source"]["hoursAfter"], 0.0);
    assert_eq!(out["destination"]["hoursAfter"], 0.25);

    // And the entries are still where they were.
    let still = schema
        .execute(format!(
            r#"{{ worklogEntries(filter: {{ taskIds: ["{wrong}"], limit: 10 }}) {{ id }} }}"#
        ))
        .await;
    assert_eq!(
        still.data.into_json().unwrap()["worklogEntries"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn a_confirmed_reattribution_moves_the_day_and_its_hours() {
    let schema = build_test_schema();
    let (wrong, right) = seed_a_misattributed_morning(&schema).await;

    let result = schema
        .execute(format!(
            r#"mutation {{ reattributeWorklogEntries(input: {{ fromTask: "{wrong}", toTask: "{right}", since: "2026-08-03", confirm: true }}) {{ {REATTRIBUTION_FIELDS} }} }}"#
        ))
        .await;
    assert!(result.errors.is_empty(), "apply: {:?}", result.errors);
    let out = result.data.into_json().unwrap()["reattributeWorklogEntries"].clone();

    assert_eq!(out["applied"], true);
    assert_eq!(out["movedEntries"], 2);
    assert_eq!(out["slotsDiscarded"], 1);
    assert_eq!(out["slotsRebuilt"], 1, "one stretch of work, one slot");
    assert_eq!(out["source"]["hoursAfter"], 0.0);
    assert_eq!(out["destination"]["hoursAfter"], 0.25);

    let moved = schema
        .execute(format!(
            r#"{{ worklogEntries(filter: {{ taskIds: ["{right}"], limit: 10 }}) {{ id }} }}"#
        ))
        .await;
    assert_eq!(
        moved.data.into_json().unwrap()["worklogEntries"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn reattributing_a_task_onto_itself_is_refused() {
    let schema = build_test_schema();
    let (wrong, _right) = seed_a_misattributed_morning(&schema).await;

    let result = schema
        .execute(format!(
            r#"mutation {{ reattributeWorklogEntries(input: {{ fromTask: "{wrong}", toTask: "{wrong}", since: "2026-08-03", confirm: true }}) {{ applied }} }}"#
        ))
        .await;
    let message = result
        .errors
        .first()
        .map(|e| e.message.clone())
        .unwrap_or_default();
    assert!(
        message.contains("same task"),
        "expected a refusal naming the mistake, got {message:?}"
    );
}

#[tokio::test]
async fn a_day_with_no_entries_is_refused_rather_than_reported_as_done() {
    let schema = build_test_schema();
    let (wrong, right) = seed_a_misattributed_morning(&schema).await;

    let result = schema
        .execute(format!(
            r#"mutation {{ reattributeWorklogEntries(input: {{ fromTask: "{wrong}", toTask: "{right}", since: "2026-07-01", confirm: true }}) {{ applied }} }}"#
        ))
        .await;
    let message = result
        .errors
        .first()
        .map(|e| e.message.clone())
        .unwrap_or_default();
    assert!(
        message.contains("nothing to move"),
        "expected an empty-selection refusal, got {message:?}"
    );
}

/// An entry reference is resolved server-side, by prefix, so the three characters a
/// journal prints are enough — and an unknown one is a miss, not a silent no-op.
#[tokio::test]
async fn an_entry_reference_selects_exactly_that_entry() {
    let schema = build_test_schema();
    let (wrong, right) = seed_a_misattributed_morning(&schema).await;

    let listed = schema
        .execute(format!(
            r#"{{ worklogEntries(filter: {{ taskIds: ["{wrong}"], limit: 10 }}) {{ id loggedAt }} }}"#
        ))
        .await;
    let entries = listed.data.into_json().unwrap()["worklogEntries"].clone();
    let first = entries[0]["id"].as_str().unwrap().to_string();
    let prefix: String = first.chars().take(8).collect();

    let result = schema
        .execute(format!(
            r#"mutation {{ reattributeWorklogEntries(input: {{ fromTask: "{wrong}", toTask: "{right}", entryRefs: ["{prefix}"], confirm: true }}) {{ {REATTRIBUTION_FIELDS} }} }}"#
        ))
        .await;
    assert!(result.errors.is_empty(), "apply: {:?}", result.errors);
    let out = result.data.into_json().unwrap()["reattributeWorklogEntries"].clone();
    assert_eq!(out["selectedEntries"], serde_json::json!([first]));
    assert_eq!(out["movedEntries"], 1);
}

#[tokio::test]
async fn an_unknown_entry_reference_is_reported_as_not_found() {
    let schema = build_test_schema();
    let (wrong, right) = seed_a_misattributed_morning(&schema).await;

    let result = schema
        .execute(format!(
            r#"mutation {{ reattributeWorklogEntries(input: {{ fromTask: "{wrong}", toTask: "{right}", entryRefs: ["{}"], confirm: true }}) {{ applied }} }}"#,
            Uuid::new_v4()
        ))
        .await;
    let message = result
        .errors
        .first()
        .map(|e| e.message.clone())
        .unwrap_or_default();
    assert!(
        message.contains("Not found:"),
        "the CLI maps this prefix to exit 2, got {message:?}"
    );
}

// ─── Timesheet Reconstruction Tests (Plan 2) ───

#[tokio::test]
async fn run_reconstruction_on_empty_day_returns_zero() {
    let schema = build_test_schema();
    let res = schema
        .execute(
            r#"mutation { runTimesheetReconstruction(date: "2026-06-08") { totalHours dayConfidence status } }"#,
        )
        .await;
    assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
    let data = res.data.into_json().unwrap();
    assert_eq!(data["runTimesheetReconstruction"]["totalHours"], 0.0);
    assert_eq!(data["runTimesheetReconstruction"]["dayConfidence"], "LOW");
}

#[tokio::test]
async fn timesheet_draft_is_null_before_reconstruction() {
    let schema = build_test_schema();
    let res = schema
        .execute(r#"{ timesheetDraft(date: "2026-06-08") { totalHours } }"#)
        .await;
    assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
    let data = res.data.into_json().unwrap();
    assert!(data["timesheetDraft"].is_null());
}

#[tokio::test]
async fn learn_mapping_rejects_unknown_project() {
    let schema = build_test_schema();
    let res = schema
        .execute(
            r#"mutation { learnMapping(kind: REPO_PATH, pattern: "/repo", gryzzlyProjectId: "nope") { id } }"#,
        )
        .await;
    assert!(!res.errors.is_empty(), "expected validation error for unknown project");
}

/// Seeded happy-path: one worklog entry on a task assigned to Gryzzly project "p1",
/// timestamped so it lands in the Europe/Paris morning window (the stub config
/// returns None, so `resolve_tz` defaults to Europe/Paris — UTC+2 in June).
/// A single worklog is "low signal" (< 2 signals): the reconstruction engine keeps
/// p1's raw carry-forward hours and quarantines the rest of the target as
/// unattributed, rather than scaling p1 up to fill the whole day.
#[tokio::test]
async fn run_reconstruction_with_seeded_worklog_produces_project_line_and_fill() {
    let user_id: UserId =
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("valid default UUID");
    let date = NaiveDate::from_ymd_opt(2026, 6, 8).unwrap();
    let task_id: TaskId = Uuid::new_v4();
    let now = chrono::Utc::now();

    let task_repo = InMemoryTaskRepository::new();
    task_repo
        .save(&Task {
            id: task_id,
            user_id,
            title: "Timesheet task".to_string(),
            description: None,
            notes: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            delegated_to: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Medium,
            urgency_manual: false,
            impact: ImpactLevel::Medium,
            tags: vec![],
            tracking_state: TrackingState::Inbox,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            recurrence_id: None,
            occurrence_date: None,
            gryzzly_task_id: Some("g1".to_string()),
            gryzzly_project_id: Some("p1".to_string()),
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    // 09:00 UTC = 11:00 Europe/Paris (CEST, UTC+2 in June) — within the 08:00-12:00 morning window.
    let logged_at = chrono::DateTime::parse_from_rfc3339("2026-06-08T09:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let worklog_repo = InMemoryWorklogRepository::new();
    worklog_repo
        .create(&domain::types::WorklogEntry {
            id: Uuid::new_v4(),
            user_id,
            task_id,
            body: "Implemented feature".to_string(),
            logged_at,
            created_at: logged_at,
            updated_at: logged_at,
            session_id: None,
        })
        .await
        .unwrap();

    let catalog_repo = InMemoryGryzzlyCatalogRepository::new();
    catalog_repo
        .upsert(&domain::types::GryzzlyCatalogEntry {
            id: Uuid::new_v4(),
            user_id,
            gryzzly_task_id: "g1".to_string(),
            name: "Task 1".to_string(),
            gryzzly_project_id: "p1".to_string(),
            project_name: "Project One".to_string(),
            customer_name: None,
            is_active: true,
            project_status: None,
            last_synced_at: now,
        })
        .await
        .unwrap();

    let schema = build_test_schema_with(
        Arc::new(worklog_repo),
        Arc::new(task_repo),
        Arc::new(catalog_repo),
        Arc::new(InMemoryTimesheetDraftRepository::new()),
    );

    let query = format!(
        r#"mutation {{ runTimesheetReconstruction(date: "{date}") {{ status dayConfidence totalHours unattributedHours lines {{ gryzzlyProjectId hours }} }} }}"#
    );
    let res = schema.execute(&query).await;
    assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
    let data = res.data.into_json().unwrap();
    let day = &data["runTimesheetReconstruction"];

    assert_eq!(day["status"], "DRAFT");
    assert_eq!(day["dayConfidence"], "LOW");
    // The day totals its four quarters (8h with the default windows), not
    // `daily_target_hours` — see the quarter-arbitration design.
    assert!(
        (day["totalHours"].as_f64().unwrap() - 8.0).abs() < 1e-9,
        "expected total_hours=8.0, got {:?}",
        day["totalHours"]
    );
    assert!(
        day["unattributedHours"].as_f64().unwrap() > 0.0,
        "expected the unfilled remainder to be quarantined as unattributed"
    );
    let lines = day["lines"].as_array().unwrap();
    assert!(
        lines.iter().any(|l| l["gryzzlyProjectId"] == "p1"),
        "expected a p1 line, got {:?}",
        lines
    );

    // Now validate the draft and confirm the status transition sticks.
    let validate_query = format!(r#"mutation {{ validateTimesheet(date: "{date}") {{ status }} }}"#);
    let validate_res = schema.execute(&validate_query).await;
    assert!(validate_res.errors.is_empty(), "errors: {:?}", validate_res.errors);
    let validate_data = validate_res.data.into_json().unwrap();
    assert_eq!(validate_data["validateTimesheet"]["status"], "VALIDATED");
}

// ─── Semantic Memory Tests (remember / recall / pendingMemories) ───

/// The user the API injects into every resolver in tests. Seeded rows must carry
/// it, otherwise the resolvers filter them out as another user's memories.
fn memory_test_user() -> UserId {
    Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("valid default UUID")
}

/// A memory in a state `remember` cannot return: rejected, or already invalidated.
fn seeded_memory(
    title: &str,
    status: MemoryStatus,
    invalidated_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Memory {
    let mut memory = Memory::new(
        memory_test_user(),
        NewMemory {
            kind: MemoryKind::Decision,
            title: title.to_string(),
            body: None,
            occurred_at: None,
            source: MemorySource::Manual,
            source_ref: None,
            status,
            proposed_supersedes: None,
            project_id: None,
            task_id: None,
            stakeholders: vec![],
        },
        chrono::Utc::now(),
    )
    .expect("valid fixture memory");
    memory.invalidated_at = invalidated_at;
    memory
}

/// A memory with a CHOSEN id, so a test can build two ids that share a prefix
/// (ambiguity) or pin the short reference a verb is called with.
fn seeded_memory_with_id(id: &str, title: &str, status: MemoryStatus) -> Memory {
    let mut memory = seeded_memory(title, status, None);
    memory.id = Uuid::parse_str(id).expect("valid fixture UUID");
    memory
}

/// Read one seeded row back out of the store, to assert what a mutation wrote —
/// or, for the rejection paths, that it wrote nothing.
fn stored_memory(store: &InMemoryMemoryStore, id: &str) -> Memory {
    let wanted = Uuid::parse_str(id).expect("valid fixture UUID");
    store
        .rows
        .lock()
        .unwrap()
        .iter()
        .find(|m| m.id == wanted)
        .cloned()
        .expect("the seeded memory is still in the store")
}

/// Titles of the memories a `recall` document returned, best-first.
fn recall_titles(hits: &serde_json::Value) -> Vec<&str> {
    hits.as_array()
        .expect("recall returns a list")
        .iter()
        .map(|hit| hit["memory"]["title"].as_str().expect("a title"))
        .collect()
}

#[tokio::test]
async fn remember_mutation_returns_the_memory_it_recorded() {
    let schema = build_test_schema();
    let result = schema
        .execute(
            r#"
            mutation {
                remember(input: {
                    kind: DECISION
                    title: "Wave 0 limitee au perimetre Microsoft AI"
                    body: "Alternative ecartee: ouvrir la wave aux clients hors Microsoft"
                    occurredAt: "2026-06-12T14:00:00Z"
                    source: MANUAL
                    sourceRef: "session-42"
                    confirmed: true
                }) {
                    id
                    kind
                    title
                    body
                    occurredAt
                    recordedAt
                    invalidatedAt
                    supersededBy
                    source
                    sourceRef
                    status
                    projectId
                    taskId
                    stakeholders
                }
            }
        "#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let memory = &data["remember"];

    assert_eq!(memory["kind"], "DECISION");
    assert_eq!(memory["title"], "Wave 0 limitee au perimetre Microsoft AI");
    assert_eq!(
        memory["body"],
        "Alternative ecartee: ouvrir la wave aux clients hors Microsoft"
    );
    assert_eq!(memory["source"], "MANUAL");
    assert_eq!(memory["sourceRef"], "session-42");
    assert!(
        Uuid::parse_str(memory["id"].as_str().expect("an id")).is_ok(),
        "id must be a UUID, got {:?}",
        memory["id"]
    );

    // Compare instants, not the wire format of the datetime scalar.
    let expected_occurred = chrono::DateTime::parse_from_rfc3339("2026-06-12T14:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let occurred: chrono::DateTime<chrono::Utc> = memory["occurredAt"]
        .as_str()
        .expect("occurredAt")
        .parse()
        .expect("occurredAt is a datetime");
    assert_eq!(
        occurred, expected_occurred,
        "an explicitly backdated occurredAt must be kept"
    );
    let recorded: chrono::DateTime<chrono::Utc> = memory["recordedAt"]
        .as_str()
        .expect("recordedAt")
        .parse()
        .expect("recordedAt is a datetime");
    assert!(
        recorded > expected_occurred,
        "recordedAt is when aplan learned it, so it must be later than the backdated occurredAt"
    );

    // A fresh memory is never part of the truth history and links nothing.
    assert!(memory["invalidatedAt"].is_null());
    assert!(memory["supersededBy"].is_null());
    assert!(memory["projectId"].is_null());
    assert!(memory["taskId"].is_null());
    assert_eq!(memory["stakeholders"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn remember_records_stakeholders_and_recall_returns_them() {
    let schema = build_test_schema();
    let result = schema
        .execute(
            r#"
            mutation {
                remember(input: {
                    kind: COMMITMENT
                    title: "Certificat promis a Pierre pour la revue trimestrielle"
                    stakeholders: ["Pierre", "Sophie"]
                    confirmed: true
                }) {
                    kind
                    stakeholders
                }
            }
        "#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["remember"]["kind"], "COMMITMENT");
    let people: Vec<&str> = data["remember"]["stakeholders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    assert_eq!(people, vec!["Pierre", "Sophie"]);

    // The stakeholder side-table is a separate write path: check it survives a read.
    let recalled = schema
        .execute(r#"{ recall(q: "certificat") { memory { stakeholders } } }"#)
        .await;
    assert!(recalled.errors.is_empty(), "Errors: {:?}", recalled.errors);
    let recalled_data = recalled.data.into_json().unwrap();
    let hits = recalled_data["recall"].as_array().unwrap();
    assert_eq!(hits.len(), 1, "expected the memory back, got {hits:?}");
    let recalled_people: Vec<&str> = hits[0]["memory"]["stakeholders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    assert_eq!(recalled_people, vec!["Pierre", "Sophie"]);
}

#[tokio::test]
async fn remember_links_a_project_and_leaves_the_task_unset() {
    let schema = build_test_schema();
    let project_id = Uuid::new_v4();
    let query = format!(
        r#"mutation {{
            remember(input: {{
                kind: FACT
                title: "Le canal de distribution passe par le partenaire local"
                projectId: "{project_id}"
                confirmed: true
            }}) {{ projectId taskId }}
        }}"#
    );
    let result = schema.execute(&query).await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();

    assert_eq!(data["remember"]["projectId"], project_id.to_string());
    assert!(data["remember"]["taskId"].is_null());
}

#[tokio::test]
async fn remember_rejects_a_malformed_project_id() {
    let schema = build_test_schema();
    let result = schema
        .execute(
            r#"mutation {
                remember(input: { kind: FACT, title: "un fait", projectId: "not-a-uuid" }) { id }
            }"#,
        )
        .await;
    assert!(
        !result.errors.is_empty(),
        "an unparseable project ID must not be silently dropped"
    );
}

/// The pending/active gate the whole "Claude proposes, the user validates" design
/// rests on: no `confirmed` flag means the memory is a CANDIDATE, so it must be
/// queued and must not be readable through the ordinary recall path.
#[tokio::test]
async fn remember_defaults_to_the_validation_queue_and_stays_out_of_recall() {
    let schema = build_test_schema();
    let result = schema
        .execute(
            r#"mutation {
                remember(input: {
                    kind: DECISION
                    title: "Certificat Cartier a produire avant la livraison"
                }) { status }
            }"#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(
        data["remember"]["status"], "PENDING",
        "an unconfirmed memory must land in the queue, never straight into the facts"
    );

    let recalled = schema
        .execute(r#"{ recall(q: "certificat") { memory { title } } }"#)
        .await;
    assert!(recalled.errors.is_empty(), "Errors: {:?}", recalled.errors);
    let recalled_data = recalled.data.into_json().unwrap();
    assert!(
        recall_titles(&recalled_data["recall"]).is_empty(),
        "a not-yet-validated memory must not surface in the default recall path, got {:?}",
        recalled_data["recall"]
    );
}

#[tokio::test]
async fn remember_with_confirmed_skips_the_queue_and_is_recallable() {
    let schema = build_test_schema();
    let result = schema
        .execute(
            r#"mutation {
                remember(input: {
                    kind: DECISION
                    title: "Certificat Cartier a produire avant la livraison"
                    confirmed: true
                }) { status }
            }"#,
        )
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["remember"]["status"], "ACTIVE");

    let recalled = schema
        .execute(r#"{ recall(q: "certificat") { memory { title status } } }"#)
        .await;
    assert!(recalled.errors.is_empty(), "Errors: {:?}", recalled.errors);
    let recalled_data = recalled.data.into_json().unwrap();
    assert_eq!(
        recall_titles(&recalled_data["recall"]),
        vec!["Certificat Cartier a produire avant la livraison"]
    );
}

#[tokio::test]
async fn recall_returns_nothing_for_a_term_absent_from_the_corpus() {
    let schema = build_test_schema();
    let stored = schema
        .execute(
            r#"mutation {
                remember(input: {
                    kind: DECISION
                    title: "Certificat Cartier a produire avant la livraison"
                    confirmed: true
                }) { id }
            }"#,
        )
        .await;
    assert!(stored.errors.is_empty(), "Errors: {:?}", stored.errors);

    let recalled = schema
        .execute(r#"{ recall(q: "chiffrage") { memory { title } } }"#)
        .await;
    assert!(recalled.errors.is_empty(), "Errors: {:?}", recalled.errors);
    let recalled_data = recalled.data.into_json().unwrap();
    assert!(
        recall_titles(&recalled_data["recall"]).is_empty(),
        "an unrelated term must return nothing, got {:?}",
        recalled_data["recall"]
    );
}

/// `includeHistory` is the hard filter of the retrieval semantics seen from the
/// API: by default a superseded memory is invisible, so the caller cannot answer
/// today's question with yesterday's truth.
#[tokio::test]
async fn recall_hides_invalidated_memories_unless_history_is_requested() {
    let (schema, store) = build_memory_test_schema();
    store.seed(seeded_memory(
        "Certificat delivre par le canal historique",
        MemoryStatus::Active,
        Some(chrono::Utc::now()),
    ));
    store.seed(seeded_memory(
        "Certificat delivre par le nouveau canal",
        MemoryStatus::Active,
        None,
    ));

    let default_recall = schema
        .execute(r#"{ recall(q: "certificat") { memory { title invalidatedAt } } }"#)
        .await;
    assert!(
        default_recall.errors.is_empty(),
        "Errors: {:?}",
        default_recall.errors
    );
    let default_data = default_recall.data.into_json().unwrap();
    assert_eq!(
        recall_titles(&default_data["recall"]),
        vec!["Certificat delivre par le nouveau canal"],
        "the invalidated memory must be filtered out by default"
    );
    assert!(default_data["recall"][0]["memory"]["invalidatedAt"].is_null());

    let with_history = schema
        .execute(
            r#"{ recall(q: "certificat", includeHistory: true) { memory { title invalidatedAt } } }"#,
        )
        .await;
    assert!(
        with_history.errors.is_empty(),
        "Errors: {:?}",
        with_history.errors
    );
    let history_data = with_history.data.into_json().unwrap();
    let titles = recall_titles(&history_data["recall"]);
    assert_eq!(titles.len(), 2, "includeHistory must lift the filter");
    assert!(
        titles.contains(&"Certificat delivre par le canal historique"),
        "the invalidated memory must be reachable on demand, got {titles:?}"
    );
    assert!(
        history_data["recall"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["memory"]["invalidatedAt"].is_string()),
        "the invalidation timestamp must be exposed so the caller can date the change"
    );
}

/// Guards the limit default at the API boundary: an omitted `limit` must mean
/// "the usual page", never "no rows at all".
#[tokio::test]
async fn recall_without_an_explicit_limit_returns_rows() {
    let schema = build_test_schema();
    for title in &[
        "Certificat Cartier a produire avant la livraison",
        "Certificat exige par le service juridique",
        "Certificat archive dans l espace partage",
    ] {
        let query = format!(
            r#"mutation {{ remember(input: {{ kind: FACT, title: "{title}", confirmed: true }}) {{ id }} }}"#
        );
        let stored = schema.execute(&query).await;
        assert!(stored.errors.is_empty(), "Errors: {:?}", stored.errors);
    }

    let recalled = schema
        .execute(r#"{ recall(q: "certificat") { memory { title } score } }"#)
        .await;
    assert!(recalled.errors.is_empty(), "Errors: {:?}", recalled.errors);
    let recalled_data = recalled.data.into_json().unwrap();
    assert_eq!(
        recall_titles(&recalled_data["recall"]).len(),
        3,
        "every matching memory must come back when no limit is given, got {:?}",
        recalled_data["recall"]
    );
}

#[tokio::test]
async fn recall_honours_an_explicit_limit() {
    let schema = build_test_schema();
    for title in &[
        "Certificat Cartier a produire avant la livraison",
        "Certificat exige par le service juridique",
    ] {
        let query = format!(
            r#"mutation {{ remember(input: {{ kind: FACT, title: "{title}", confirmed: true }}) {{ id }} }}"#
        );
        let stored = schema.execute(&query).await;
        assert!(stored.errors.is_empty(), "Errors: {:?}", stored.errors);
    }

    let recalled = schema
        .execute(r#"{ recall(q: "certificat", limit: 1) { memory { title } } }"#)
        .await;
    assert!(recalled.errors.is_empty(), "Errors: {:?}", recalled.errors);
    let recalled_data = recalled.data.into_json().unwrap();
    assert_eq!(recall_titles(&recalled_data["recall"]).len(), 1);
}

#[tokio::test]
async fn pending_memories_returns_only_the_queue() {
    let (schema, store) = build_memory_test_schema();
    store.seed(seeded_memory(
        "candidat refuse par l utilisateur",
        MemoryStatus::Rejected,
        None,
    ));
    store.seed(seeded_memory(
        "fait deja valide",
        MemoryStatus::Active,
        None,
    ));
    let stored = schema
        .execute(
            r#"mutation {
                remember(input: { kind: DECISION, title: "candidat en attente" }) { id }
            }"#,
        )
        .await;
    assert!(stored.errors.is_empty(), "Errors: {:?}", stored.errors);

    // No explicit limit: the default page must not be empty either.
    let result = schema
        .execute("{ pendingMemories { id title status } }")
        .await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let rows = data["pendingMemories"].as_array().unwrap();

    let titles: Vec<&str> = rows.iter().map(|m| m["title"].as_str().unwrap()).collect();
    assert_eq!(
        titles,
        vec!["candidat en attente"],
        "only pending candidates belong in the validation queue"
    );
    assert_eq!(rows[0]["status"], "PENDING");
}

// ─── Short references on the mutating verbs ───
//
// `brief` prints `[m:7c1]` and `inbox` lists candidates: a short handle is the
// only id a user ever sees. A verb that reads with it but refuses it to WRITE
// forces a 36-character UUID to be retyped on the commands run several times a
// morning — so every id argument resolves the same way `memory(id:)` does.

#[tokio::test]
async fn accept_memory_takes_the_short_reference_the_inbox_displays() {
    let (schema, store) = build_memory_test_schema();
    let id = "7c1a0000-0000-0000-0000-000000000001";
    store.seed(seeded_memory_with_id(
        id,
        "Certificat Cartier a produire avant la livraison",
        MemoryStatus::Pending,
    ));

    let result = schema
        .execute(r#"mutation { acceptMemory(id: "m:7c1") { accepted { id status } } }"#)
        .await;

    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["acceptMemory"]["accepted"]["id"], id);
    assert_eq!(data["acceptMemory"]["accepted"]["status"], "ACTIVE");
    assert_eq!(stored_memory(&store, id).status, MemoryStatus::Active);
}

#[tokio::test]
async fn reject_memory_takes_a_short_reference() {
    let (schema, store) = build_memory_test_schema();
    let id = "7c1b0000-0000-0000-0000-000000000001";
    store.seed(seeded_memory_with_id(
        id,
        "Suggestion sans interet a ecarter",
        MemoryStatus::Pending,
    ));

    let result = schema
        .execute(r#"mutation { rejectMemory(id: "7c1b") { id status } }"#)
        .await;

    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["rejectMemory"]["id"], id);
    assert_eq!(data["rejectMemory"]["status"], "REJECTED");
    assert_eq!(stored_memory(&store, id).status, MemoryStatus::Rejected);
}

#[tokio::test]
async fn merge_memory_resolves_a_short_reference_on_both_arguments() {
    let (schema, store) = build_memory_test_schema();
    let candidate = "aa1a0000-0000-0000-0000-000000000001";
    let target = "bb1b0000-0000-0000-0000-000000000001";
    store.seed(seeded_memory_with_id(
        candidate,
        "Le certificat passe par le canal partenaire",
        MemoryStatus::Pending,
    ));
    store.seed(seeded_memory_with_id(
        target,
        "Certificat via partenaire",
        MemoryStatus::Active,
    ));

    let result = schema
        .execute(r#"mutation { mergeMemory(id: "aa1a", into: "m:bb1b") { survivor { id title } discardedId } }"#)
        .await;

    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(
        data["mergeMemory"]["survivor"]["id"], target,
        "the target keeps its identity; both short references must have resolved"
    );
    assert_eq!(
        data["mergeMemory"]["survivor"]["title"],
        "Le certificat passe par le canal partenaire"
    );
    assert_eq!(data["mergeMemory"]["discardedId"], candidate);
}

#[tokio::test]
async fn supersede_memory_resolves_a_short_reference_on_both_arguments() {
    let (schema, store) = build_memory_test_schema();
    let old = "cc1c0000-0000-0000-0000-000000000001";
    let successor = "dd1d0000-0000-0000-0000-000000000001";
    store.seed(seeded_memory_with_id(
        old,
        "Livraison prevue en juin",
        MemoryStatus::Active,
    ));
    store.seed(seeded_memory_with_id(
        successor,
        "Livraison reportee en septembre",
        MemoryStatus::Pending,
    ));

    let result = schema
        .execute(
            r#"mutation { supersedeMemory(old: "cc1c", by: "m:dd1d") {
                invalidated { id invalidatedAt supersededBy }
                successor { id status }
            } }"#,
        )
        .await;

    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["supersedeMemory"]["invalidated"]["id"], old);
    assert!(data["supersedeMemory"]["invalidated"]["invalidatedAt"].is_string());
    assert_eq!(
        data["supersedeMemory"]["invalidated"]["supersededBy"],
        successor
    );
    assert_eq!(data["supersedeMemory"]["successor"]["status"], "ACTIVE");
}

/// The half-write that would corrupt the bitemporal chain: both references are
/// resolved BEFORE anything is written, so a bad second argument leaves the first
/// row untouched.
#[tokio::test]
async fn supersede_memory_writes_nothing_when_the_successor_reference_is_unknown() {
    let (schema, store) = build_memory_test_schema();
    let old = "ee1e0000-0000-0000-0000-000000000001";
    store.seed(seeded_memory_with_id(
        old,
        "Livraison prevue en juin",
        MemoryStatus::Active,
    ));

    let result = schema
        .execute(r#"mutation { supersedeMemory(old: "ee1e", by: "fff9") { successor { id } } }"#)
        .await;

    let message = result
        .errors
        .first()
        .map(|e| e.message.clone())
        .unwrap_or_default();
    assert!(
        message.contains("Not found:"),
        "an unknown reference must be reported as not found (exit code 2), got {message:?}"
    );
    let untouched = stored_memory(&store, old);
    assert!(
        untouched.invalidated_at.is_none() && untouched.superseded_by.is_none(),
        "the old row must not be invalidated when the successor could not be resolved"
    );
}

#[tokio::test]
async fn merge_memory_writes_nothing_when_the_target_reference_is_unknown() {
    let (schema, store) = build_memory_test_schema();
    let candidate = "1a2a0000-0000-0000-0000-000000000001";
    store.seed(seeded_memory_with_id(
        candidate,
        "Reformulation d un fait connu",
        MemoryStatus::Pending,
    ));

    let result = schema
        .execute(r#"mutation { mergeMemory(id: "1a2a", into: "9f9f") { survivor { id } } }"#)
        .await;

    let message = result
        .errors
        .first()
        .map(|e| e.message.clone())
        .unwrap_or_default();
    assert!(
        message.contains("Not found:"),
        "an unknown merge target must be reported as not found, got {message:?}"
    );
    assert_eq!(
        stored_memory(&store, candidate).status,
        MemoryStatus::Pending,
        "the candidate must still be waiting in the queue"
    );
}

/// Ambiguity is decided against the WHOLE store, not just the queue: a prefix
/// that is unique among pending candidates today would otherwise start pointing
/// somewhere else the day an older memory shares it.
#[tokio::test]
async fn accept_memory_refuses_an_ambiguous_reference_and_names_every_candidate() {
    let (schema, store) = build_memory_test_schema();
    let candidate = "ab010000-0000-0000-0000-000000000001";
    let unrelated_active = "ab010000-0000-0000-0000-000000000002";
    store.seed(seeded_memory_with_id(
        candidate,
        "Candidat en attente de tri",
        MemoryStatus::Pending,
    ));
    store.seed(seeded_memory_with_id(
        unrelated_active,
        "Fait deja valide qui partage le prefixe",
        MemoryStatus::Active,
    ));

    let result = schema
        .execute(r#"mutation { acceptMemory(id: "ab01") { accepted { id } } }"#)
        .await;

    let message = result
        .errors
        .first()
        .map(|e| e.message.clone())
        .unwrap_or_default();
    assert!(
        message.contains("Ambiguous memory reference"),
        "an ambiguous reference must be refused (exit code 3), got {message:?}"
    );
    assert!(
        message.contains(candidate) && message.contains(unrelated_active),
        "the candidates must be named so the caller can pick one, got {message:?}"
    );
    assert_eq!(
        stored_memory(&store, candidate).status,
        MemoryStatus::Pending,
        "an ambiguous reference must not accept anything"
    );
}

#[tokio::test]
async fn reject_memory_reports_an_unknown_reference_as_not_found() {
    let (schema, _store) = build_memory_test_schema();
    let result = schema
        .execute(r#"mutation { rejectMemory(id: "9ab9") { id } }"#)
        .await;

    let message = result
        .errors
        .first()
        .map(|e| e.message.clone())
        .unwrap_or_default();
    assert!(
        message.contains("Not found:"),
        "an unknown id must be a not-found (exit code 2), never a generic failure, got {message:?}"
    );
}

/// Regression for the naming collision between the pre-existing Microsoft-OAuth
/// `session` query and the new Claude-session surface below: the argument-less
/// `{ session { authenticated account } }` the frontend's `use-session.ts` sends
/// must keep resolving under its original name, unmodified by this feature.
#[tokio::test]
async fn microsoft_session_query_resolves_without_an_id_argument() {
    let schema = build_test_schema();
    let result = schema
        .execute(r#"{ session { authenticated account } }"#)
        .await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["session"]["authenticated"], false);
    assert!(data["session"]["account"].is_null());
}

// ─── Session tracking (GraphQL surface) ───

#[tokio::test]
async fn bind_session_then_read_it_back() {
    let (schema, task_id) = schema_with_one_task().await;

    let bind = schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_id}", label: "/tmp/x") {{
                 session {{ id mode taskId label }} previousTaskId }} }}"#
        ))
        .await;
    assert!(bind.errors.is_empty(), "{:?}", bind.errors);
    let data = bind.data.into_json().unwrap();
    assert_eq!(data["bindSession"]["session"]["mode"], "TRACKING");
    assert_eq!(data["bindSession"]["session"]["taskId"], task_id.to_string());
    assert!(data["bindSession"]["previousTaskId"].is_null());

    let read = schema
        .execute(r#"{ claudeSession(id: "s1") { id mode label } }"#)
        .await;
    assert!(read.errors.is_empty(), "{:?}", read.errors);
    assert_eq!(read.data.into_json().unwrap()["claudeSession"]["label"], "/tmp/x");
}

#[tokio::test]
async fn set_session_mode_off_clears_the_task() {
    let (schema, task_id) = schema_with_one_task().await;
    schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_id}") {{ session {{ id }} }} }}"#
        ))
        .await;

    let off = schema
        .execute(r#"mutation { setSessionMode(sessionId: "s1", mode: OFF) { mode taskId } }"#)
        .await;

    assert!(off.errors.is_empty(), "{:?}", off.errors);
    let data = off.data.into_json().unwrap();
    assert_eq!(data["setSessionMode"]["mode"], "OFF");
    assert!(data["setSessionMode"]["taskId"].is_null());
}

/// `setSessionMode(OFF)` clears the session's task (locked above), and once
/// cleared no later lookup — not `flushWorklogTime`, not the reaper — can ever
/// find it again to flush it. A tracking session with a loggable entry must
/// therefore be flushed *before* that clearing write, or the entry's time is
/// gone for good. `.expect`-style presence is not enough here: the recorded
/// order is what tells "flushed then cleared" apart from "cleared then
/// flushed too late" (by which point the task id is already gone and nothing
/// flushes at all) — the same distinction round 1's CLI-level ordering test
/// drew against wire order, drawn here against repository call order instead.
#[tokio::test]
async fn set_session_mode_off_flushes_the_bound_task_before_clearing_it() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let schema = build_test_schema_with_memory(
        Arc::new(InMemoryWorklogRepository::new()),
        Arc::new(InMemoryTaskRepository::new()),
        Arc::new(InMemoryGryzzlyCatalogRepository::new()),
        Arc::new(InMemoryTimesheetDraftRepository::new()),
        Arc::new(InMemoryMemoryStore::default()),
        Arc::new(OrderRecordingSessionRepository::new(order.clone())),
    );

    let created = schema
        .execute(r#"mutation { createTask(input: { title: "Tracked task" }) { id } }"#)
        .await;
    assert!(created.errors.is_empty(), "{:?}", created.errors);
    let task_id = created.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let bind = schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_id}") {{ session {{ id }} }} }}"#
        ))
        .await;
    assert!(bind.errors.is_empty(), "{:?}", bind.errors);

    let logged = schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "did the thing", sessionId: "s1") {{ id }} }}"#
        ))
        .await;
    assert!(logged.errors.is_empty(), "{:?}", logged.errors);

    let off = schema
        .execute(r#"mutation { setSessionMode(sessionId: "s1", mode: OFF) { mode taskId lastFlushAt } }"#)
        .await;
    assert!(off.errors.is_empty(), "{:?}", off.errors);

    // Checked first and independently of the response below: the recorded
    // repository call order is the direct evidence for "before", regardless
    // of what the mutation's own return value happens to reflect.
    let recorded = order.lock().unwrap();
    let flush_at = recorded
        .iter()
        .position(|&e| e == "set_last_flush")
        .expect("set_last_flush must be called — off must flush before clearing");
    let clear_at = recorded
        .iter()
        .position(|&e| e == "upsert_off")
        .expect("the OFF upsert must be called — off must still clear the task");
    assert!(
        flush_at < clear_at,
        "set_last_flush (index {flush_at}) must precede the OFF upsert (index {clear_at})"
    );
    drop(recorded);

    let data = off.data.into_json().unwrap();
    assert_eq!(data["setSessionMode"]["mode"], "OFF");
    assert!(
        data["setSessionMode"]["taskId"].is_null(),
        "the clearing behaviour must survive this fix — off still clears the task"
    );
    assert!(
        !data["setSessionMode"]["lastFlushAt"].is_null(),
        "off must flush the session's bound task, advancing its watermark"
    );
}

#[tokio::test]
async fn open_sessions_excludes_an_ended_one() {
    let (schema, task_id) = schema_with_one_task().await;
    for id in ["s1", "s2"] {
        schema
            .execute(format!(
                r#"mutation {{ bindSession(sessionId: "{id}", taskId: "{task_id}") {{ session {{ id }} }} }}"#
            ))
            .await;
    }
    schema
        .execute(r#"mutation { endSession(sessionId: "s2") { id endedAt } }"#)
        .await;

    let open = schema.execute(r#"{ openClaudeSessions { id } }"#).await;

    assert!(open.errors.is_empty(), "{:?}", open.errors);
    let list = open.data.into_json().unwrap()["openClaudeSessions"].clone();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], "s1");
}

/// The end-to-end regression for the defect fixed by `bind_session` reviving a
/// session (commit 9c8d8c6): a bind after an end used to report success while
/// leaving the row closed, poisoning the id for its whole life. This drives the
/// sequence through the resolvers — `bindSession` -> `endSession` ->
/// `bindSession` again — so it would have caught the defect at the API layer,
/// not just at the repository layer.
#[tokio::test]
async fn a_bind_after_end_revives_the_session_through_the_resolvers() {
    let (schema, task_id) = schema_with_one_task().await;
    schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_id}") {{ session {{ id }} }} }}"#
        ))
        .await;

    let ended = schema
        .execute(r#"mutation { endSession(sessionId: "s1") { id endedAt } }"#)
        .await;
    assert!(ended.errors.is_empty(), "{:?}", ended.errors);
    assert!(
        !ended.data.into_json().unwrap()["endSession"]["endedAt"].is_null(),
        "endSession must close the session"
    );

    let new_task = schema
        .execute(r#"mutation { createTask(input: { title: "Second task" }) { id } }"#)
        .await;
    assert!(new_task.errors.is_empty(), "{:?}", new_task.errors);
    let new_task_id = new_task.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let rebind = schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{new_task_id}") {{
                 session {{ id endedAt taskId }} }} }}"#
        ))
        .await;
    assert!(rebind.errors.is_empty(), "{:?}", rebind.errors);
    let data = rebind.data.into_json().unwrap();
    assert!(
        data["bindSession"]["session"]["endedAt"].is_null(),
        "a bind after end must revive the session — endedAt must read back null"
    );
    assert_eq!(data["bindSession"]["session"]["taskId"], new_task_id);

    let open = schema.execute(r#"{ openClaudeSessions { id } }"#).await;
    assert!(open.errors.is_empty(), "{:?}", open.errors);
    let ids = open.data.into_json().unwrap()["openClaudeSessions"].clone();
    assert_eq!(
        ids.as_array().unwrap().len(),
        1,
        "openClaudeSessions must list the revived session again"
    );
    assert_eq!(ids[0]["id"], "s1");
}

#[tokio::test]
async fn an_entry_carries_the_session_that_wrote_it() {
    let (schema, task_id) = schema_with_one_task().await;
    schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_id}") {{ session {{ id }} }} }}"#
        ))
        .await;

    let added = schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "fait", sessionId: "s1") {{ id sessionId }} }}"#
        ))
        .await;

    assert!(added.errors.is_empty(), "{:?}", added.errors);
    assert_eq!(
        added.data.into_json().unwrap()["addWorklogEntry"]["sessionId"],
        "s1"
    );
}

#[tokio::test]
async fn an_entry_without_a_session_is_the_humans() {
    let (schema, task_id) = schema_with_one_task().await;

    let added = schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "fait") {{ sessionId }} }}"#
        ))
        .await;

    assert!(added.errors.is_empty(), "{:?}", added.errors);
    assert!(added.data.into_json().unwrap()["addWorklogEntry"]["sessionId"].is_null());
}

// ---------------------------------------------------------------------------
// I2 — `addWorklogEntry` with a `sessionId` refreshes that session's
// `lastSeenAt`, so a session bound in the morning and logging all day does
// not look idle to the reaper. Without a `sessionId`, no session is touched.
// ---------------------------------------------------------------------------

async fn last_seen_at_of(schema: &TestSchema, session_id: &str) -> serde_json::Value {
    let read = schema
        .execute(format!(r#"{{ claudeSession(id: "{session_id}") {{ lastSeenAt }} }}"#))
        .await;
    assert!(read.errors.is_empty(), "{:?}", read.errors);
    read.data.into_json().unwrap()["claudeSession"]["lastSeenAt"].clone()
}

#[tokio::test]
async fn add_worklog_entry_with_a_session_id_advances_its_last_seen_at() {
    let (schema, task_id) = schema_with_one_task().await;
    schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_id}") {{ session {{ id }} }} }}"#
        ))
        .await;
    let before = last_seen_at_of(&schema, "s1").await;

    // A real clock tick between the two reads, so `lastSeenAt` has somewhere to
    // move to — `bindSession` and the touch below both stamp `Utc::now()`.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let added = schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "fait", sessionId: "s1") {{ id }} }}"#
        ))
        .await;
    assert!(added.errors.is_empty(), "{:?}", added.errors);

    let after = last_seen_at_of(&schema, "s1").await;
    assert_ne!(
        before, after,
        "addWorklogEntry(sessionId: \"s1\") must bump the session's lastSeenAt"
    );
}

#[tokio::test]
async fn add_worklog_entry_without_a_session_id_leaves_last_seen_at_unchanged() {
    let (schema, task_id) = schema_with_one_task().await;
    schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_id}") {{ session {{ id }} }} }}"#
        ))
        .await;
    let before = last_seen_at_of(&schema, "s1").await;

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let added = schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "fait") {{ id }} }}"#
        ))
        .await;
    assert!(added.errors.is_empty(), "{:?}", added.errors);

    let after = last_seen_at_of(&schema, "s1").await;
    assert_eq!(
        before, after,
        "addWorklogEntry with no sessionId must not touch any session's lastSeenAt"
    );
}

/// The touch is deliberately non-fatal (see `mutation.rs`'s comment on
/// `add_worklog_entry`): a touch failure must not fail the worklog write that
/// already succeeded. `build_test_schema_with_failing_session_touch` always
/// errors on `touch`, so a green result here is only possible if that swallow
/// is real, not just documented.
#[tokio::test]
async fn a_failing_touch_does_not_fail_the_worklog_write() {
    let schema = build_test_schema_with_failing_session_touch();
    let created = schema
        .execute(r#"mutation { createTask(input: { title: "Tracked task" }) { id } }"#)
        .await;
    assert!(created.errors.is_empty(), "{:?}", created.errors);
    let task_id = created.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let bind = schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_id}") {{ session {{ id }} }} }}"#
        ))
        .await;
    assert!(bind.errors.is_empty(), "{:?}", bind.errors);

    let added = schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "fait", sessionId: "s1") {{ id }} }}"#
        ))
        .await;

    assert!(
        added.errors.is_empty(),
        "a touch failure must not surface as a GraphQL error on the worklog write: {:?}",
        added.errors
    );
    assert!(added.data.into_json().unwrap()["addWorklogEntry"]["id"].is_string());
}

// ---------------------------------------------------------------------------
// `loggedAtLocal` and `rebuildWorklogProjection` — `aplan log --at`'s two halves.
// ---------------------------------------------------------------------------

/// The CLI sends a wall-clock reading and the server converts it, so that the one
/// `aplan.timezone` reading every projection uses is also the one that decides the
/// entry's instant. The test schema has no `aplan.timezone` row, so the default
/// (Europe/Paris) applies: 14:30 local in August is 12:30 UTC.
#[tokio::test]
async fn logged_at_local_is_converted_through_the_users_timezone() {
    let (schema, task_id) = schema_with_one_task().await;

    let added = schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "fait",
                 loggedAtLocal: "2026-08-06T14:30:00") {{ loggedAt }} }}"#
        ))
        .await;

    assert!(added.errors.is_empty(), "{:?}", added.errors);
    let logged_at = added.data.into_json().unwrap()["addWorklogEntry"]["loggedAt"]
        .as_str()
        .unwrap()
        .to_string();
    let parsed = chrono::DateTime::parse_from_rfc3339(&logged_at).expect("rfc3339");
    assert_eq!(
        parsed.with_timezone(&chrono::Utc).to_rfc3339(),
        "2026-08-06T12:30:00+00:00",
        "CEST is UTC+2 in August"
    );
}

/// Both arguments name the same decision. A caller that sent two disagreeing values
/// has a bug, and a silent winner would hide it inside billable hours.
#[tokio::test]
async fn logged_at_and_logged_at_local_together_are_refused() {
    let (schema, task_id) = schema_with_one_task().await;

    let added = schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "fait",
                 loggedAt: "2026-08-06T12:30:00+00:00",
                 loggedAtLocal: "2026-08-06T09:00:00") {{ id }} }}"#
        ))
        .await;

    assert!(!added.errors.is_empty(), "both arguments must be refused");
    assert!(
        added.errors[0].message.contains("pass one"),
        "{:?}",
        added.errors
    );
}

/// The end-to-end shape of the reported defect and its fix: an entry backdated to a
/// day the flush window does not cover produces no slot on its own, and naming that
/// day produces one.
#[tokio::test]
async fn rebuilding_a_named_day_materialises_a_backdated_entry() {
    let (schema, task_id) = schema_with_one_task().await;
    // Two local-morning entries twenty minutes apart on a day well in the past.
    for at in ["2026-08-06T10:00:00", "2026-08-06T10:20:00"] {
        let added = schema
            .execute(format!(
                r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "fait",
                     loggedAtLocal: "{at}") {{ id }} }}"#
            ))
            .await;
        assert!(added.errors.is_empty(), "{:?}", added.errors);
    }

    // A flush cannot see them: its window starts at the human's `active_since`,
    // which no test has moved back before August 6th.
    let flushed = schema
        .execute(format!(
            r#"mutation {{ flushWorklogTime(taskId: "{task_id}") {{ slotsWritten }} }}"#
        ))
        .await;
    assert!(flushed.errors.is_empty(), "{:?}", flushed.errors);

    let rebuilt = schema
        .execute(format!(
            r#"mutation {{ rebuildWorklogProjection(taskId: "{task_id}", date: "2026-08-06")
                 {{ date halfDays slotsDiscarded slotsWritten }} }}"#
        ))
        .await;
    assert!(rebuilt.errors.is_empty(), "{:?}", rebuilt.errors);
    let out = rebuilt.data.into_json().unwrap()["rebuildWorklogProjection"].clone();
    assert_eq!(out["date"], "2026-08-06");
    assert_eq!(out["halfDays"], serde_json::json!(["MORNING"]));
    assert_eq!(out["slotsWritten"], 1, "one uninterrupted morning stretch");

    // And the slot is really there, on that local day.
    let journal = schema
        .execute(r#"{ activityJournal(date: "2026-08-06") { halfDay durationMinutes } }"#)
        .await;
    assert!(journal.errors.is_empty(), "{:?}", journal.errors);
    let slots = journal.data.into_json().unwrap()["activityJournal"].clone();
    assert_eq!(slots.as_array().map(|a| a.len()), Some(1), "{slots:?}");
    assert_eq!(slots[0]["halfDay"], "MORNING");
    assert_eq!(slots[0]["durationMinutes"], 20, "the twenty minutes between the entries");
}

/// Idempotent: a second rebuild replaces what the first wrote instead of adding to
/// it. A rebuild that doubled the day would inflate an invoice.
#[tokio::test]
async fn rebuilding_the_same_day_twice_does_not_double_it() {
    let (schema, task_id) = schema_with_one_task().await;
    schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "fait",
                 loggedAtLocal: "2026-08-06T10:00:00") {{ id }} }}"#
        ))
        .await;

    let first = schema
        .execute(format!(
            r#"mutation {{ rebuildWorklogProjection(taskId: "{task_id}", date: "2026-08-06")
                 {{ slotsDiscarded slotsWritten }} }}"#
        ))
        .await;
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert_eq!(first.data.into_json().unwrap()["rebuildWorklogProjection"]["slotsDiscarded"], 0);

    let second = schema
        .execute(format!(
            r#"mutation {{ rebuildWorklogProjection(taskId: "{task_id}", date: "2026-08-06")
                 {{ slotsDiscarded slotsWritten }} }}"#
        ))
        .await;
    assert!(second.errors.is_empty(), "{:?}", second.errors);
    let out = second.data.into_json().unwrap()["rebuildWorklogProjection"].clone();
    assert_eq!(out["slotsDiscarded"], 1, "the first pass's slot is replaced, not kept");
    assert_eq!(out["slotsWritten"], 1);

    let journal = schema
        .execute(r#"{ activityJournal(date: "2026-08-06") { id } }"#)
        .await;
    let slots = journal.data.into_json().unwrap()["activityJournal"].clone();
    assert_eq!(slots.as_array().map(|a| a.len()), Some(1), "still one slot: {slots:?}");
}

/// A day the task never logged in is a no-op, not an error — the CLI has already
/// written the operator's entry by the time this runs.
#[tokio::test]
async fn rebuilding_a_day_without_entries_is_a_success() {
    let (schema, task_id) = schema_with_one_task().await;
    let rebuilt = schema
        .execute(format!(
            r#"mutation {{ rebuildWorklogProjection(taskId: "{task_id}", date: "2026-08-06")
                 {{ halfDays slotsWritten }} }}"#
        ))
        .await;
    assert!(rebuilt.errors.is_empty(), "{:?}", rebuilt.errors);
    let out = rebuilt.data.into_json().unwrap()["rebuildWorklogProjection"].clone();
    assert_eq!(out["halfDays"], serde_json::json!([]));
    assert_eq!(out["slotsWritten"], 0);
}

// ─── Brief: preferences ───

/// A memory of a chosen kind, active immediately — for tests that must tell
/// which section of the brief a memory landed in, not just that the query ran.
fn seeded_memory_of_kind(kind: MemoryKind, title: &str) -> Memory {
    Memory::new(
        memory_test_user(),
        NewMemory {
            kind,
            title: title.to_string(),
            body: None,
            occurred_at: None,
            source: MemorySource::Manual,
            source_ref: None,
            status: MemoryStatus::Active,
            proposed_supersedes: None,
            project_id: None,
            task_id: None,
            stakeholders: vec![],
        },
        chrono::Utc::now(),
    )
    .expect("valid fixture memory")
}

/// Two active memories of different kinds are seeded so the assertion can tell
/// `preferences` from `commitments` apart: an empty-fixture test would pass even
/// if the resolver read the wrong section (see review finding on this test).
#[tokio::test]
async fn brief_exposes_preferences() {
    let (schema, store) = build_memory_test_schema();
    store.seed(seeded_memory_of_kind(
        MemoryKind::Preference,
        "Toujours confirmer avant de merger",
    ));
    store.seed(seeded_memory_of_kind(
        MemoryKind::Commitment,
        "Livrer le rapport Cartier",
    ));

    let response = schema
        .execute(
            r#"{ brief(variant: SESSION) { preferences { title reference } preferenceTotal } }"#,
        )
        .await;

    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let preferences = data["brief"]["preferences"].as_array().unwrap();

    assert_eq!(
        preferences.len(),
        1,
        "only the preference-kind memory belongs here, not the commitment: {preferences:?}"
    );
    assert_eq!(preferences[0]["title"], "Toujours confirmer avant de merger");
    assert!(
        !preferences[0]["reference"].as_str().unwrap().is_empty(),
        "the short reference `recall`/the CLI accept must not be blank"
    );
    assert_eq!(data["brief"]["preferenceTotal"], 1);
}
