use chrono::{Duration, NaiveDate, Utc};
use domain::types::common::{
    ImpactLevel, ProjectId, Source, TagId, TaskId, TaskStatus, TrackingState, UrgencyLevel, UserId,
};
use domain::types::recurrence::{RecurrenceRule, RecurrenceTemplate, RecurrenceTemplateId};
use domain::types::task::Task;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::{RecurrenceRepository, TaskRepository};

// ─── Input DTOs ──────────────────────────────────────────────────────────────

/// Input data for creating a new recurring task template.
pub struct CreateRecurringTaskInput {
    pub user_id: UserId,
    pub title: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub project_id: Option<ProjectId>,
    pub urgency: UrgencyLevel,
    pub impact: ImpactLevel,
    pub estimated_hours: Option<f32>,
    pub tag_ids: Vec<TagId>,
    pub rule: RecurrenceRule,
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
    pub max_occurrences: Option<u32>,
}

/// Input data for updating an existing recurring task template.
///
/// Each field uses `Option` as a sentinel: `None` means "leave unchanged",
/// `Some(v)` means "update to v". For nullable fields the inner `Option`
/// is the actual value (`None` = clear, `Some(x)` = set to x).
pub struct UpdateRecurringTaskInput {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub notes: Option<Option<String>>,
    pub project_id: Option<Option<ProjectId>>,
    pub urgency: Option<UrgencyLevel>,
    pub impact: Option<ImpactLevel>,
    pub estimated_hours: Option<Option<f32>>,
    pub tag_ids: Option<Vec<TagId>>,
    pub rule: Option<RecurrenceRule>,
    pub starts_on: Option<NaiveDate>,
    pub ends_on: Option<Option<NaiveDate>>,
    pub max_occurrences: Option<Option<u32>>,
}

// ─── Use Cases ────────────────────────────────────────────────────────────────

/// Create a new recurring task template and persist it.
///
/// Does NOT materialize occurrences immediately — call `materialize_due_occurrences`
/// separately (or rely on the lazy trigger at query time).
pub async fn create_recurring_task(
    repo: &dyn RecurrenceRepository,
    input: CreateRecurringTaskInput,
) -> Result<RecurrenceTemplate, AppError> {
    let now = Utc::now();
    let template = RecurrenceTemplate {
        id: RecurrenceTemplateId::new(),
        user_id: input.user_id,
        title: input.title,
        description: input.description,
        notes: input.notes,
        project_id: input.project_id,
        urgency: input.urgency,
        urgency_manual: false,
        impact: input.impact,
        estimated_hours: input.estimated_hours,
        tags: input.tag_ids,
        rule: input.rule,
        starts_on: input.starts_on,
        ends_on: input.ends_on,
        max_occurrences: input.max_occurrences,
        last_generated_through: None,
        active: true,
        created_at: now,
        updated_at: now,
    };
    repo.save(&template).await?;
    Ok(template)
}

/// Update a recurrence template.
///
/// After saving the updated template this function deletes all future task instances where:
/// - `recurrence_id == id`
/// - `status == Todo`
/// - `occurrence_date >= today`
///
/// Past instances (occurrence_date < today) and any instance whose status is not Todo are
/// preserved unchanged — this includes instances the user has already started or completed.
///
/// Worklog-linked preservation (skip deletion if the instance has worklog entries) is not yet
/// implemented; instances with worklog entries may be deleted if their status is still Todo.
///
/// After deletion the horizon is re-materialized so the updated template's rules and metadata
/// are reflected in new instances.
pub async fn update_recurring_task(
    rec_repo: &dyn RecurrenceRepository,
    task_repo: &dyn TaskRepository,
    id: RecurrenceTemplateId,
    caller_user_id: UserId,
    input: UpdateRecurringTaskInput,
    today: NaiveDate,
    horizon_days: i64,
) -> Result<RecurrenceTemplate, AppError> {
    let mut template = rec_repo
        .find_by_id(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("RecurrenceTemplate {}", id)))?;

    // IDOR guard: return NotFound (not Forbidden) to avoid leaking existence of
    // templates owned by other users.
    if template.user_id != caller_user_id {
        return Err(AppError::NotFound(format!("RecurrenceTemplate {}", id)));
    }

    // Apply updates
    if let Some(title) = input.title {
        template.title = title;
    }
    if let Some(description) = input.description {
        template.description = description;
    }
    if let Some(notes) = input.notes {
        template.notes = notes;
    }
    if let Some(project_id) = input.project_id {
        template.project_id = project_id;
    }
    if let Some(urgency) = input.urgency {
        template.urgency = urgency;
    }
    if let Some(impact) = input.impact {
        template.impact = impact;
    }
    if let Some(estimated_hours) = input.estimated_hours {
        template.estimated_hours = estimated_hours;
    }
    if let Some(tag_ids) = input.tag_ids {
        template.tags = tag_ids;
    }
    if let Some(rule) = input.rule {
        template.rule = rule;
    }
    if let Some(starts_on) = input.starts_on {
        template.starts_on = starts_on;
    }
    if let Some(ends_on) = input.ends_on {
        template.ends_on = ends_on;
    }
    if let Some(max_occurrences) = input.max_occurrences {
        template.max_occurrences = max_occurrences;
    }
    // Reset the watermark so re-materialization covers the full horizon.
    template.last_generated_through = None;
    template.updated_at = Utc::now();

    rec_repo.save(&template).await?;

    // Delete future Todo instances so they will be recreated from updated template data.
    let future_instances = task_repo.find_by_recurrence(id).await?;
    for task in future_instances {
        if task.status == TaskStatus::Todo {
            if let Some(occ) = task.occurrence_date {
                if occ >= today {
                    task_repo.delete(task.id).await?;
                }
            }
        }
    }

    // Re-materialize with the new template.
    materialize_due_occurrences(rec_repo, task_repo, template.user_id, today, horizon_days)
        .await?;

    // Reload to get the updated watermark written by materialize_due_occurrences.
    let updated = rec_repo
        .find_by_id(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("RecurrenceTemplate {}", id)))?;

    Ok(updated)
}

/// Soft-delete a recurrence template and delete all future Todo instances.
///
/// Returns the count of task instances deleted.
pub async fn cancel_recurrence(
    rec_repo: &dyn RecurrenceRepository,
    task_repo: &dyn TaskRepository,
    id: RecurrenceTemplateId,
    caller_user_id: UserId,
    today: NaiveDate,
) -> Result<usize, AppError> {
    // Verify the template exists and belongs to the caller.
    // Return NotFound (not Forbidden) to avoid leaking existence of templates
    // owned by other users.
    let template = rec_repo
        .find_by_id(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("RecurrenceTemplate {}", id)))?;

    if template.user_id != caller_user_id {
        return Err(AppError::NotFound(format!("RecurrenceTemplate {}", id)));
    }

    rec_repo.deactivate(id).await?;

    let instances = task_repo.find_by_recurrence(id).await?;
    let mut deleted = 0usize;
    for task in instances {
        if task.status == TaskStatus::Todo {
            if let Some(occ) = task.occurrence_date {
                if occ >= today {
                    task_repo.delete(task.id).await?;
                    deleted += 1;
                }
            }
        }
    }
    Ok(deleted)
}

/// Materialize task instances for all active templates owned by `user_id`.
///
/// For each active template the function computes the generation window:
/// - `from = max(starts_on, last_generated_through + 1 day)` — never regenerate already-created slots
/// - `to   = today + horizon_days`
///
/// Occurrences are then truncated according to `ends_on` (if set) and `max_occurrences`
/// (counting existing instances already in the task table).
///
/// The function calls `task_repo.find_by_recurrence_slot` before each save to guarantee
/// idempotency — an occurrence that already exists is silently skipped regardless of the
/// `save` implementation's behaviour.
///
/// After a successful pass the template's `last_generated_through` watermark is updated to `to`.
///
/// Returns the total number of new task instances created across all templates.
pub async fn materialize_due_occurrences(
    rec_repo: &dyn RecurrenceRepository,
    task_repo: &dyn TaskRepository,
    user_id: UserId,
    today: NaiveDate,
    horizon_days: i64,
) -> Result<usize, AppError> {
    let templates = rec_repo.find_active_by_user(user_id).await?;
    let to = today + Duration::days(horizon_days);
    let mut total_created = 0usize;

    for mut template in templates {
        // Determine the start of the generation window.
        let from = match template.last_generated_through {
            Some(last) => last + Duration::days(1),
            None => template.starts_on,
        };
        // Clamp: never go before starts_on.
        let from = from.max(template.starts_on);

        if from > to {
            // Already fully generated through the horizon.
            continue;
        }

        // Truncate `to` at ends_on if configured.
        let to_clamped = match template.ends_on {
            Some(end) if end < to => end,
            _ => to,
        };

        // Compute candidate occurrence dates.
        let candidates = template.rule.occurrences_in(template.starts_on, from, to_clamped);

        // Apply max_occurrences: count how many instances already exist for this template.
        let (effective_candidates, _) = if let Some(max) = template.max_occurrences {
            let existing = task_repo.find_by_recurrence(template.id).await?;
            let already = existing.len() as u32;
            let remaining = max.saturating_sub(already);
            let truncated: Vec<NaiveDate> = candidates.into_iter().take(remaining as usize).collect();
            (truncated, already)
        } else {
            (candidates, 0)
        };

        let now = Utc::now();
        let mut created_this_template = 0usize;

        for date in effective_candidates {
            // Idempotency check — skip if an instance already exists for this slot.
            if task_repo
                .find_by_recurrence_slot(template.id, date)
                .await?
                .is_some()
            {
                continue;
            }

            let task = Task {
                id: Uuid::new_v4(),
                user_id: template.user_id,
                title: template.title.clone(),
                description: template.description.clone(),
                notes: template.notes.clone(),
                source: Source::Personal,
                source_id: None,
                jira_status: None,
                status: TaskStatus::Todo,
                project_id: template.project_id,
                assignee: None,
                delegated_to: None,
                deadline: None,
                planned_start: Some(
                    date.and_hms_opt(8, 0, 0)
                        .expect("valid time")
                        .and_utc(),
                ),
                planned_end: None,
                estimated_hours: template.estimated_hours,
                urgency: template.urgency,
                urgency_manual: template.urgency_manual,
                impact: template.impact,
                tags: template.tags.clone(),
                tracking_state: TrackingState::Followed,
                jira_remaining_seconds: None,
                jira_original_estimate_seconds: None,
                jira_time_spent_seconds: None,
                remaining_hours_override: None,
                estimated_hours_override: None,
                recurrence_id: Some(template.id),
                occurrence_date: Some(date),
                gryzzly_task_id: None,
                gryzzly_project_id: None,
                created_at: now,
                updated_at: now,
            };

            task_repo.save(&task).await?;
            created_this_template += 1;
        }

        // Advance the watermark to `to` (not to_clamped) so the next call knows where
        // we generated through even if ends_on was the limiting factor.
        template.last_generated_through = Some(to);
        template.updated_at = Utc::now();
        rec_repo.save(&template).await?;

        total_created += created_this_template;
    }

    Ok(total_created)
}

/// Mark a single recurring task instance as skipped (Cancelled).
///
/// The task must have a non-null `recurrence_id`; otherwise returns `AppError::Validation`.
/// Returns `AppError::NotFound` (not Forbidden) when `caller_user_id` does not own the task,
/// to avoid leaking the existence of tasks owned by other users.
pub async fn skip_occurrence(
    task_repo: &dyn TaskRepository,
    task_id: TaskId,
    caller_user_id: UserId,
) -> Result<Task, AppError> {
    let mut task = task_repo
        .find_by_id(task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {}", task_id)))?;

    // IDOR guard.
    if task.user_id != caller_user_id {
        return Err(AppError::NotFound(format!("Task {}", task_id)));
    }

    if task.recurrence_id.is_none() {
        return Err(AppError::Validation(
            "skip_occurrence requires a recurring task instance (recurrence_id must be set)"
                .to_string(),
        ));
    }

    task.status = TaskStatus::Cancelled;
    task.updated_at = Utc::now();
    task_repo.save(&task).await?;
    Ok(task)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::errors::RepositoryError;
    use crate::repositories::TaskFilter;

    // ── In-memory RecurrenceRepository ────────────────────────────────────────

    struct InMemoryRecurrenceRepository {
        templates: Mutex<HashMap<RecurrenceTemplateId, RecurrenceTemplate>>,
    }

    impl InMemoryRecurrenceRepository {
        fn new() -> Self {
            Self {
                templates: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl RecurrenceRepository for InMemoryRecurrenceRepository {
        async fn find_by_id(
            &self,
            id: RecurrenceTemplateId,
        ) -> Result<Option<RecurrenceTemplate>, RepositoryError> {
            let store = self.templates.lock().unwrap();
            Ok(store.get(&id).cloned())
        }

        async fn find_active_by_user(
            &self,
            user_id: UserId,
        ) -> Result<Vec<RecurrenceTemplate>, RepositoryError> {
            let store = self.templates.lock().unwrap();
            Ok(store
                .values()
                .filter(|t| t.user_id == user_id && t.active)
                .cloned()
                .collect())
        }

        async fn save(&self, template: &RecurrenceTemplate) -> Result<(), RepositoryError> {
            let mut store = self.templates.lock().unwrap();
            store.insert(template.id, template.clone());
            Ok(())
        }

        async fn deactivate(&self, id: RecurrenceTemplateId) -> Result<(), RepositoryError> {
            let mut store = self.templates.lock().unwrap();
            if let Some(t) = store.get_mut(&id) {
                t.active = false;
            }
            Ok(())
        }
    }

    // ── In-memory TaskRepository with recurrence slot index ───────────────────

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
            Ok(self.tasks.lock().unwrap().get(&id).cloned())
        }

        async fn find_by_user(
            &self,
            user_id: UserId,
            _filter: &TaskFilter,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(self
                .tasks
                .lock()
                .unwrap()
                .values()
                .filter(|t| t.user_id == user_id)
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

        async fn find_overdue(
            &self,
            user_id: UserId,
            today: NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(self
                .tasks
                .lock()
                .unwrap()
                .values()
                .filter(|t| {
                    t.user_id == user_id
                        && t.status != TaskStatus::Done
                        && t.status != TaskStatus::Cancelled
                        && (t.planned_start
                            .map(|dt| dt.date_naive() < today)
                            .unwrap_or(false)
                            || t.deadline.map(|d| d < today).unwrap_or(false))
                })
                .cloned()
                .collect())
        }

        async fn save(&self, task: &Task) -> Result<(), RepositoryError> {
            self.tasks.lock().unwrap().insert(task.id, task.clone());
            Ok(())
        }

        async fn save_batch(&self, tasks: &[Task]) -> Result<(), RepositoryError> {
            let mut store = self.tasks.lock().unwrap();
            for t in tasks {
                store.insert(t.id, t.clone());
            }
            Ok(())
        }

        async fn delete(&self, id: TaskId) -> Result<(), RepositoryError> {
            self.tasks.lock().unwrap().remove(&id);
            Ok(())
        }

        async fn delete_stale_by_source(
            &self,
            _user_id: UserId,
            _source: Source,
            _keep_ids: &[String],
        ) -> Result<u64, RepositoryError> {
            Ok(0)
        }

        // Override to actually look up by (recurrence_id, occurrence_date).
        async fn find_by_recurrence_slot(
            &self,
            template_id: RecurrenceTemplateId,
            occurrence_date: NaiveDate,
        ) -> Result<Option<Task>, RepositoryError> {
            Ok(self
                .tasks
                .lock()
                .unwrap()
                .values()
                .find(|t| {
                    t.recurrence_id == Some(template_id)
                        && t.occurrence_date == Some(occurrence_date)
                })
                .cloned())
        }

        async fn find_by_recurrence(
            &self,
            template_id: RecurrenceTemplateId,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(self
                .tasks
                .lock()
                .unwrap()
                .values()
                .filter(|t| t.recurrence_id == Some(template_id))
                .cloned()
                .collect())
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn test_user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn other_user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap()
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 4, 27).unwrap()
    }

    fn daily_input(starts_on: NaiveDate) -> CreateRecurringTaskInput {
        CreateRecurringTaskInput {
            user_id: test_user_id(),
            title: "Daily Standup".to_string(),
            description: None,
            notes: None,
            project_id: None,
            urgency: UrgencyLevel::Medium,
            impact: ImpactLevel::Medium,
            estimated_hours: None,
            tag_ids: vec![],
            rule: RecurrenceRule::Daily { interval: 1 },
            starts_on,
            ends_on: None,
            max_occurrences: None,
        }
    }

    fn task_instances(task_repo: &InMemoryTaskRepository) -> Vec<Task> {
        let store = task_repo.tasks.lock().unwrap();
        let mut v: Vec<Task> = store.values().cloned().collect();
        v.sort_by_key(|t| t.occurrence_date);
        v
    }

    // ── Test 1: materialize daily template, horizon 14 ────────────────────────
    // Daily interval=1 starting today, horizon=14 → occurrences_in returns today + 14 days = 15.
    #[tokio::test]
    async fn test_1_materialize_daily_creates_15_instances() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();
        let starts = today();

        let template = create_recurring_task(&rec_repo, daily_input(starts))
            .await
            .unwrap();

        let count = materialize_due_occurrences(
            &rec_repo,
            &task_repo,
            test_user_id(),
            today(),
            14,
        )
        .await
        .unwrap();

        assert_eq!(count, 15, "today + 14 days inclusive = 15 occurrences");

        let tasks = task_instances(&task_repo);
        assert_eq!(tasks.len(), 15);
        assert_eq!(tasks[0].occurrence_date, Some(starts));
        assert_eq!(tasks[0].recurrence_id, Some(template.id));
        assert_eq!(tasks[0].status, TaskStatus::Todo);
        assert_eq!(tasks[0].source, Source::Personal);
    }

    // ── Test 2: materialize twice is idempotent ───────────────────────────────
    #[tokio::test]
    async fn test_2_materialize_idempotent() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();

        create_recurring_task(&rec_repo, daily_input(today()))
            .await
            .unwrap();

        let first = materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today(), 14)
            .await
            .unwrap();
        let second = materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today(), 14)
            .await
            .unwrap();

        assert_eq!(first, 15);
        assert_eq!(second, 0, "second call should create zero new instances");
        assert_eq!(task_instances(&task_repo).len(), 15);
    }

    // ── Test 3: ends_on caps generation ──────────────────────────────────────
    #[tokio::test]
    async fn test_3_materialize_respects_ends_on() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();

        let ends = today() + Duration::days(4); // today + 4 more days = 5 instances
        let input = CreateRecurringTaskInput {
            ends_on: Some(ends),
            ..daily_input(today())
        };
        create_recurring_task(&rec_repo, input).await.unwrap();

        let count =
            materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today(), 14)
                .await
                .unwrap();

        assert_eq!(count, 5, "only occurrences up to ends_on are created");
    }

    // ── Test 4: max_occurrences caps generation ───────────────────────────────
    #[tokio::test]
    async fn test_4_materialize_respects_max_occurrences() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();

        let input = CreateRecurringTaskInput {
            max_occurrences: Some(3),
            ..daily_input(today())
        };
        create_recurring_task(&rec_repo, input).await.unwrap();

        // First call: 3 instances.
        let first =
            materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today(), 14)
                .await
                .unwrap();
        assert_eq!(first, 3);

        // Second call: no more allowed.
        let second =
            materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today(), 14)
                .await
                .unwrap();
        assert_eq!(second, 0);

        assert_eq!(task_instances(&task_repo).len(), 3);
    }

    // ── Test 5: update_recurring_task re-materializes with new title ──────────
    #[tokio::test]
    async fn test_5_update_recurring_task_rematerializes_new_title() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();

        let template = create_recurring_task(&rec_repo, daily_input(today()))
            .await
            .unwrap();

        // Materialize first batch.
        materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today(), 14)
            .await
            .unwrap();
        assert_eq!(task_instances(&task_repo).len(), 15);

        // Update template title.
        let update_input = UpdateRecurringTaskInput {
            title: Some("Updated Daily".to_string()),
            description: None,
            notes: None,
            project_id: None,
            urgency: None,
            impact: None,
            estimated_hours: None,
            tag_ids: None,
            rule: None,
            starts_on: None,
            ends_on: None,
            max_occurrences: None,
        };
        update_recurring_task(
            &rec_repo,
            &task_repo,
            template.id,
            test_user_id(),
            update_input,
            today(),
            14,
        )
        .await
        .unwrap();

        let tasks = task_instances(&task_repo);
        assert_eq!(tasks.len(), 15);
        // All re-materialized tasks carry the new title.
        assert!(
            tasks.iter().all(|t| t.title == "Updated Daily"),
            "all instances should have the updated title"
        );
    }

    // ── Test 6: update_recurring_task preserves past instances ───────────────
    #[tokio::test]
    async fn test_6_update_preserves_past_instances() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();

        // Start 5 days in the past so we have past occurrences.
        let starts = today() - Duration::days(5);
        let template = create_recurring_task(&rec_repo, daily_input(starts))
            .await
            .unwrap();

        // Materialize past + future.
        materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today(), 14)
            .await
            .unwrap();

        let before_count = task_instances(&task_repo).len();
        assert!(before_count > 0);

        // Count past instances (occurrence_date < today).
        let past_before: Vec<_> = task_instances(&task_repo)
            .into_iter()
            .filter(|t| t.occurrence_date.unwrap() < today())
            .collect();
        let past_count = past_before.len();
        assert!(past_count > 0, "there should be past instances");

        // Update the template.
        let update_input = UpdateRecurringTaskInput {
            title: Some("New Name".to_string()),
            description: None,
            notes: None,
            project_id: None,
            urgency: None,
            impact: None,
            estimated_hours: None,
            tag_ids: None,
            rule: None,
            starts_on: None,
            ends_on: None,
            max_occurrences: None,
        };
        update_recurring_task(
            &rec_repo,
            &task_repo,
            template.id,
            test_user_id(),
            update_input,
            today(),
            14,
        )
        .await
        .unwrap();

        // Past instances (occurrence_date < today) still exist with their original title.
        let past_after: Vec<_> = task_instances(&task_repo)
            .into_iter()
            .filter(|t| t.occurrence_date.unwrap() < today())
            .collect();
        assert_eq!(
            past_after.len(),
            past_count,
            "past instances should be preserved"
        );
        // Past instances still have the OLD title (they were not deleted/recreated).
        assert!(
            past_after
                .iter()
                .all(|t| t.title == "Daily Standup"),
            "past instances should retain the old title"
        );
    }

    // ── Test 7: update preserves in-progress instances ────────────────────────
    #[tokio::test]
    async fn test_7_update_preserves_in_progress_instances() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();

        let template = create_recurring_task(&rec_repo, daily_input(today()))
            .await
            .unwrap();

        materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today(), 14)
            .await
            .unwrap();

        // Mark the first future instance as InProgress.
        let tasks_before = task_instances(&task_repo);
        let in_progress_id = tasks_before[0].id;
        let mut in_progress_task = tasks_before[0].clone();
        in_progress_task.status = TaskStatus::InProgress;
        task_repo.save(&in_progress_task).await.unwrap();

        // Update the template.
        let update_input = UpdateRecurringTaskInput {
            title: Some("Rescheduled".to_string()),
            description: None,
            notes: None,
            project_id: None,
            urgency: None,
            impact: None,
            estimated_hours: None,
            tag_ids: None,
            rule: None,
            starts_on: None,
            ends_on: None,
            max_occurrences: None,
        };
        update_recurring_task(
            &rec_repo,
            &task_repo,
            template.id,
            test_user_id(),
            update_input,
            today(),
            14,
        )
        .await
        .unwrap();

        // The InProgress instance is preserved.
        let preserved = task_repo.find_by_id(in_progress_id).await.unwrap();
        assert!(
            preserved.is_some(),
            "in-progress instance must not be deleted"
        );
        assert_eq!(preserved.unwrap().status, TaskStatus::InProgress);
    }

    // ── Test 8: cancel_recurrence deactivates + deletes future Todo ───────────
    #[tokio::test]
    async fn test_8_cancel_recurrence() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();

        // Template starting 3 days ago so we have both past and future instances.
        let starts = today() - Duration::days(3);
        let template = create_recurring_task(&rec_repo, daily_input(starts))
            .await
            .unwrap();

        materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today(), 7)
            .await
            .unwrap();

        let all_before = task_instances(&task_repo);
        let future_todo_before: usize = all_before
            .iter()
            .filter(|t| {
                t.status == TaskStatus::Todo
                    && t.occurrence_date.map(|d| d >= today()).unwrap_or(false)
            })
            .count();
        assert!(future_todo_before > 0);

        let deleted = cancel_recurrence(&rec_repo, &task_repo, template.id, test_user_id(), today())
            .await
            .unwrap();

        assert_eq!(deleted, future_todo_before);

        // Template is deactivated.
        let tmpl = rec_repo.find_by_id(template.id).await.unwrap().unwrap();
        assert!(!tmpl.active, "template must be deactivated");

        // Past instances still exist.
        let past_after: usize = task_instances(&task_repo)
            .iter()
            .filter(|t| t.occurrence_date.map(|d| d < today()).unwrap_or(false))
            .count();
        assert!(past_after > 0, "past instances should be preserved");
    }

    // ── Test 9: skip_occurrence sets status to Cancelled ─────────────────────
    #[tokio::test]
    async fn test_9_skip_occurrence_cancels_task() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();

        let template = create_recurring_task(&rec_repo, daily_input(today()))
            .await
            .unwrap();

        materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today(), 7)
            .await
            .unwrap();

        let tasks = task_instances(&task_repo);
        assert!(!tasks.is_empty());
        let task_id = tasks[0].id;

        let skipped = skip_occurrence(&task_repo, task_id, test_user_id()).await.unwrap();

        assert_eq!(skipped.status, TaskStatus::Cancelled);
        assert_eq!(skipped.recurrence_id, Some(template.id));

        // Verify it's persisted.
        let persisted = task_repo.find_by_id(task_id).await.unwrap().unwrap();
        assert_eq!(persisted.status, TaskStatus::Cancelled);
    }

    // ── Test 10: skip_occurrence rejects non-recurring tasks ─────────────────
    #[tokio::test]
    async fn test_10_skip_occurrence_rejects_non_recurring() {
        let task_repo = InMemoryTaskRepository::new();

        // Create a plain (non-recurring) task directly.
        let now = Utc::now();
        let plain_task = Task {
            id: Uuid::new_v4(),
            user_id: test_user_id(),
            title: "Plain task".to_string(),
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
            gryzzly_task_id: None,
            gryzzly_project_id: None,
            created_at: now,
            updated_at: now,
        };
        task_repo.save(&plain_task).await.unwrap();

        let result = skip_occurrence(&task_repo, plain_task.id, test_user_id()).await;
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "expected Validation error, got {result:?}"
        );
    }

    // ── Test 11: update_recurring_task rejects wrong owner ───────────────────
    #[tokio::test]
    async fn test_11_update_recurring_task_rejects_wrong_owner() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();

        let template = create_recurring_task(&rec_repo, daily_input(today()))
            .await
            .unwrap();

        let update_input = UpdateRecurringTaskInput {
            title: Some("Hijack".to_string()),
            description: None,
            notes: None,
            project_id: None,
            urgency: None,
            impact: None,
            estimated_hours: None,
            tag_ids: None,
            rule: None,
            starts_on: None,
            ends_on: None,
            max_occurrences: None,
        };

        let result = update_recurring_task(
            &rec_repo,
            &task_repo,
            template.id,
            other_user_id(),
            update_input,
            today(),
            14,
        )
        .await;

        assert!(
            matches!(result, Err(AppError::NotFound(_))),
            "expected NotFound for wrong owner, got {result:?}"
        );
    }

    // ── Test 12: cancel_recurrence rejects wrong owner ────────────────────────
    #[tokio::test]
    async fn test_12_cancel_recurrence_rejects_wrong_owner() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();

        let template = create_recurring_task(&rec_repo, daily_input(today()))
            .await
            .unwrap();

        let result =
            cancel_recurrence(&rec_repo, &task_repo, template.id, other_user_id(), today()).await;

        assert!(
            matches!(result, Err(AppError::NotFound(_))),
            "expected NotFound for wrong owner, got {result:?}"
        );

        // Template must still be active — the cancel was blocked.
        let tmpl = rec_repo.find_by_id(template.id).await.unwrap().unwrap();
        assert!(tmpl.active, "template must remain active after blocked cancel");
    }

    // ── Test 13: skip_occurrence rejects wrong owner ──────────────────────────
    #[tokio::test]
    async fn test_13_skip_occurrence_rejects_wrong_owner() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();

        create_recurring_task(&rec_repo, daily_input(today()))
            .await
            .unwrap();

        materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today(), 7)
            .await
            .unwrap();

        let tasks = task_instances(&task_repo);
        assert!(!tasks.is_empty());
        let task_id = tasks[0].id;

        let result = skip_occurrence(&task_repo, task_id, other_user_id()).await;

        assert!(
            matches!(result, Err(AppError::NotFound(_))),
            "expected NotFound for wrong owner, got {result:?}"
        );
    }
}
