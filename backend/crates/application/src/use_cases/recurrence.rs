use chrono::{Duration, NaiveDate, Utc};
use domain::types::common::{
    ImpactLevel, ProjectId, Source, TagId, TaskId, TaskStatus, TrackingState, UrgencyLevel, UserId,
};
use domain::types::recurrence::{RecurrenceRule, RecurrenceTemplate, RecurrenceTemplateId};
use domain::types::task::Task;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::{RecurrenceRepository, TaskRepository, WorklogRepository};

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

/// What a cancellation actually did to the series' instances.
///
/// Two counters rather than one, because the two outcomes are not
/// interchangeable: `deleted` rows are gone, `cancelled` rows are still there and
/// still carry their worklog entries. A caller that reports only a total cannot
/// tell the user which of their history survived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CancelRecurrenceOutcome {
    /// Instances removed outright: they carried no logged time.
    pub deleted: usize,
    /// Instances kept but marked `Cancelled`: they carried logged time, which
    /// reaches the client invoice and must never be destroyed by a cleanup.
    pub cancelled: usize,
}

/// Soft-delete a recurrence template and sweep its instances: every not-yet-due
/// `Todo` slot in the future is deleted (unchanged behaviour), and every `Todo`
/// instance in the past is either deleted — if it carries no logged time — or
/// marked `Cancelled` and kept, when it does. `Done` and already-`Cancelled`
/// instances are history and are never touched.
///
/// Returns how many instances were deleted outright versus kept-but-cancelled.
pub async fn cancel_recurrence(
    rec_repo: &dyn RecurrenceRepository,
    task_repo: &dyn TaskRepository,
    worklog_repo: &dyn WorklogRepository,
    id: RecurrenceTemplateId,
    caller_user_id: UserId,
    today: NaiveDate,
) -> Result<CancelRecurrenceOutcome, AppError> {
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

    // Every instance's presence is checked individually — never through
    // `WorklogRepository::find_by_recurrence`'s capped, newest-first page, which
    // would truncate away exactly the oldest entries this sweep cares about. See
    // `find_task_ids_with_entries`'s doc comment for why.
    let task_ids: Vec<TaskId> = instances.iter().map(|task| task.id).collect();
    let logged_task_ids = worklog_repo
        .find_task_ids_with_entries(caller_user_id, &task_ids)
        .await?;

    let mut outcome = CancelRecurrenceOutcome::default();

    for mut task in instances {
        let Some(occ) = task.occurrence_date else {
            continue;
        };

        // Already closed by a human decision — leave it exactly as it is.
        if matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled) {
            continue;
        }

        if occ >= today {
            // Unchanged behaviour on the future: a not-yet-due Todo slot is
            // simply removed when the series is cancelled.
            if task.status == TaskStatus::Todo {
                task_repo.delete(task.id).await?;
                outcome.deleted += 1;
            }
            continue;
        }

        if logged_task_ids.contains(&task.id) {
            task.status = TaskStatus::Cancelled;
            task.updated_at = Utc::now();
            task_repo.save(&task).await?;
            outcome.cancelled += 1;
        } else {
            task_repo.delete(task.id).await?;
            outcome.deleted += 1;
        }
    }

    Ok(outcome)
}

/// Materialize task instances for all active templates owned by `user_id`.
///
/// For each active template the function computes the generation window:
/// - `from = max(starts_on, last_generated_through + 1 day, today)` — never regenerate
///   already-created slots, and never backfill a watermark that fell behind: a missed
///   occurrence is a historical fact, not something to regenerate
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
        // Clamp on both ends: never before starts_on, and never before today.
        //
        // The `today` bound is not cosmetic. `last_generated_through` can be months
        // stale on a template nobody materialized (the engine had no scheduler until
        // this change), and without this clamp the first tick would backfill every
        // occurrence since that watermark — recreating in one night exactly the pile
        // of dead instances this work exists to remove. A missed occurrence is a
        // historical fact, not something to regenerate.
        let from = from.max(template.starts_on).max(today);

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

/// Close every occurrence the calendar has left behind.
///
/// An instance whose `occurrence_date` is strictly before `today` and whose status
/// is neither `Done` nor `Cancelled` becomes `Cancelled`. Everything else is left
/// exactly as it is: today's and future occurrences are still actionable, and a
/// `Done` or `Cancelled` instance already records a decision this sweep has no
/// business overwriting.
///
/// Walks **every** template of the user, deactivated ones included: cancelling a
/// series does not remove the instances it already generated, and those still need
/// closing.
///
/// Never sweeps an instance that carries evidence of real work: a stale, still-open
/// occurrence with at least one worklog entry is left exactly as it is, and does
/// not count toward the returned total. Cancelling it anyway would drop it out of
/// every task view, and if the developer put it back to `Todo` by hand the very
/// next sweep would cancel it again — every tick, forever. This is a single batched
/// presence check across every candidate, never `WorklogRepository::find_by_recurrence`'s
/// capped, newest-first page: see `find_task_ids_with_entries`'s doc comment for why.
///
/// Returns the number of instances swept.
pub async fn sweep_stale_occurrences(
    rec_repo: &dyn RecurrenceRepository,
    task_repo: &dyn TaskRepository,
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    today: NaiveDate,
) -> Result<usize, AppError> {
    let templates = rec_repo.find_by_user(user_id).await?;

    // Gather every stale, still-open instance across every template first, so the
    // worklog presence check below is one batched call instead of one per template.
    let mut candidates: Vec<Task> = Vec::new();
    for template in templates {
        for task in task_repo.find_by_recurrence(template.id).await? {
            let Some(occ) = task.occurrence_date else {
                continue;
            };
            if occ >= today {
                continue;
            }
            if matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled) {
                continue;
            }
            candidates.push(task);
        }
    }

    let task_ids: Vec<TaskId> = candidates.iter().map(|task| task.id).collect();
    let logged_task_ids = worklog_repo
        .find_task_ids_with_entries(user_id, &task_ids)
        .await?;

    let mut swept = 0usize;
    for mut task in candidates {
        if logged_task_ids.contains(&task.id) {
            continue;
        }

        task.status = TaskStatus::Cancelled;
        task.updated_at = Utc::now();
        task_repo.save(&task).await?;
        swept += 1;
    }

    Ok(swept)
}

/// What one maintenance tick did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecurrencePassOutcome {
    pub materialized: usize,
    pub swept: usize,
}

/// One maintenance tick over the user's recurring series: materialize the horizon
/// ahead, then close what the calendar left behind.
///
/// The order matters and is not interchangeable. Materializing first means the
/// sweep runs against a set that already contains today's fresh slot — and since
/// the sweep only touches `occurrence_date < today`, that slot is out of its reach
/// by construction. Sweeping first would work too, but leaves the invariant resting
/// on timing rather than on the comparison; this way a tick can never close what it
/// has just opened.
pub async fn run_recurrence_pass(
    rec_repo: &dyn RecurrenceRepository,
    task_repo: &dyn TaskRepository,
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    today: NaiveDate,
    horizon_days: i64,
) -> Result<RecurrencePassOutcome, AppError> {
    let materialized =
        materialize_due_occurrences(rec_repo, task_repo, user_id, today, horizon_days).await?;
    let swept = sweep_stale_occurrences(rec_repo, task_repo, worklog_repo, user_id, today).await?;
    Ok(RecurrencePassOutcome {
        materialized,
        swept,
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::DateTime;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use domain::types::worklog::{WorklogEntry, WorklogEntryId};

    use crate::errors::RepositoryError;
    use crate::repositories::{TaskFilter, WorklogFilter, WORKLOG_FILTER_MAX_LIMIT};

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

        async fn find_by_user(
            &self,
            user_id: UserId,
        ) -> Result<Vec<RecurrenceTemplate>, RepositoryError> {
            let store = self.templates.lock().unwrap();
            Ok(store
                .values()
                .filter(|t| t.user_id == user_id)
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

    // ── In-memory WorklogRepository ────────────────────────────────────────────
    //
    // Minimal double: only `find_task_ids_with_entries` is real, since that is
    // the only method `cancel_recurrence` calls. Everything else either returns
    // an empty default or is `unimplemented!()` — a test that needs more than
    // presence-checking has no business exercising `cancel_recurrence`'s tests
    // through this double.

    struct InMemoryWorklogRepository {
        entries: Mutex<Vec<WorklogEntry>>,
    }

    impl InMemoryWorklogRepository {
        fn new() -> Self {
            Self {
                entries: Mutex::new(Vec::new()),
            }
        }

        /// Record that `task_id` carries at least one logged entry. The body and
        /// exact timestamp are irrelevant to every test that uses this — only
        /// presence matters — so `Utc::now()` is good enough.
        async fn push_entry_for_task(&self, user_id: UserId, task_id: TaskId) {
            self.push_entry_for_task_at(user_id, task_id, Utc::now()).await;
        }

        /// Same as `push_entry_for_task`, but with an explicit `logged_at`. Needed
        /// by the test proving presence survives a series with more than
        /// `WORKLOG_FILTER_MAX_LIMIT` entries: it must control which entry is
        /// oldest to show that ordering cannot hide it.
        async fn push_entry_for_task_at(
            &self,
            user_id: UserId,
            task_id: TaskId,
            logged_at: DateTime<Utc>,
        ) {
            let now = Utc::now();
            self.entries.lock().unwrap().push(WorklogEntry {
                id: Uuid::new_v4(),
                user_id,
                task_id,
                body: "worked".to_string(),
                logged_at,
                created_at: now,
                updated_at: now,
                session_id: None,
            });
        }
    }

    #[async_trait]
    impl WorklogRepository for InMemoryWorklogRepository {
        async fn create(&self, _entry: &WorklogEntry) -> Result<(), RepositoryError> {
            unimplemented!("not exercised by cancel_recurrence tests")
        }

        async fn update(&self, _entry: &WorklogEntry) -> Result<(), RepositoryError> {
            unimplemented!("not exercised by cancel_recurrence tests")
        }

        async fn delete(
            &self,
            _id: WorklogEntryId,
            _user_id: UserId,
        ) -> Result<bool, RepositoryError> {
            unimplemented!("not exercised by cancel_recurrence tests")
        }

        async fn find_by_id(
            &self,
            _id: WorklogEntryId,
            _user_id: UserId,
        ) -> Result<Option<WorklogEntry>, RepositoryError> {
            Ok(None)
        }

        async fn list(
            &self,
            _user_id: UserId,
            _filter: &WorklogFilter,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            Ok(Vec::new())
        }

        async fn find_by_recurrence(
            &self,
            _user_id: UserId,
            _template_id: RecurrenceTemplateId,
            _limit: u32,
            _offset: u32,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            Ok(Vec::new())
        }

        async fn find_task_ids_with_entries(
            &self,
            user_id: UserId,
            task_ids: &[TaskId],
        ) -> Result<std::collections::HashSet<TaskId>, RepositoryError> {
            let wanted: std::collections::HashSet<TaskId> =
                task_ids.iter().copied().collect();
            Ok(self
                .entries
                .lock()
                .unwrap()
                .iter()
                .filter(|entry| entry.user_id == user_id && wanted.contains(&entry.task_id))
                .map(|entry| entry.task_id)
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

    fn daily_template(user_id: UserId, starts_on: NaiveDate) -> RecurrenceTemplate {
        RecurrenceTemplate {
            id: RecurrenceTemplateId::new(),
            user_id,
            title: "Daily Standup".to_string(),
            description: None,
            notes: None,
            project_id: None,
            urgency: UrgencyLevel::Medium,
            urgency_manual: false,
            impact: ImpactLevel::Medium,
            estimated_hours: None,
            tags: Vec::new(),
            rule: RecurrenceRule::Daily { interval: 1 },
            starts_on,
            ends_on: None,
            max_occurrences: None,
            last_generated_through: None,
            active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    // Build a `Todo` instance for `template` at `date`, matching the shape
    // `materialize_due_occurrences` produces. Shared by every test that needs an
    // instance without going through materialization (which is now clamped to
    // never backfill the past — see the clamp below).
    fn instance_from_template(template: &RecurrenceTemplate, date: NaiveDate) -> Task {
        let now = Utc::now();
        Task {
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
            planned_start: Some(date.and_hms_opt(8, 0, 0).expect("valid time").and_utc()),
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
        }
    }

    async fn seed_past_instance(
        task_repo: &InMemoryTaskRepository,
        template: &RecurrenceTemplate,
        date: NaiveDate,
    ) {
        let task = instance_from_template(template, date);
        task_repo.save(&task).await.unwrap();
    }

    // Seed an instance with an explicit status, returning its id. Used by the
    // `cancel_recurrence` past-sweep tests to set up `Todo` instances that are
    // then, depending on the test, checked for deletion or cancellation.
    async fn save_instance(
        repo: &InMemoryTaskRepository,
        template: &RecurrenceTemplate,
        occurrence_date: NaiveDate,
        status: TaskStatus,
    ) -> TaskId {
        let mut task = instance_from_template(template, occurrence_date);
        task.status = status;
        repo.save(&task).await.unwrap();
        task.id
    }

    // ── Le clamp : un watermark périmé ne rejoue jamais le passé ──────────────
    #[tokio::test]
    async fn materialize_never_creates_occurrences_before_today() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();
        let today = today();

        // A daily template whose watermark stopped 123 days ago — the exact shape of
        // the two test templates found polluting the database on 2026-09-01.
        let mut template = daily_template(test_user_id(), today - Duration::days(200));
        template.last_generated_through = Some(today - Duration::days(123));
        rec_repo.save(&template).await.unwrap();

        materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today, 14)
            .await
            .unwrap();

        let instances = task_repo.find_by_recurrence(template.id).await.unwrap();
        assert!(
            !instances.is_empty(),
            "the horizon ahead of today must still be materialized"
        );
        for task in &instances {
            let occ = task.occurrence_date.expect("instance carries an occurrence date");
            assert!(
                occ >= today,
                "materialization must never backfill the past, got {occ} < {today}"
            );
        }
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

        // `materialize_due_occurrences` is now clamped to never backfill before
        // today (see the clamp in that function), so the past instance this test
        // needs to check survival of an update must be seeded directly rather than
        // produced by materialization.
        seed_past_instance(&task_repo, &template, starts).await;

        // Materialize the future window.
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

    // ── Test 8: cancel_recurrence deactivates + sweeps every Todo instance ────
    #[tokio::test]
    async fn test_8_cancel_recurrence() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();
        let worklog_repo = InMemoryWorklogRepository::new();

        // Template starting 3 days ago so we have both past and future instances.
        let starts = today() - Duration::days(3);
        let template = create_recurring_task(&rec_repo, daily_input(starts))
            .await
            .unwrap();

        // `materialize_due_occurrences` is now clamped to never backfill before
        // today, so the past instance this test needs (to check cancel_recurrence
        // now sweeps it too, absent logged time) must be seeded directly.
        seed_past_instance(&task_repo, &template, starts).await;

        materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today(), 7)
            .await
            .unwrap();

        let all_before = task_instances(&task_repo);
        let todo_before: usize = all_before
            .iter()
            .filter(|t| t.status == TaskStatus::Todo)
            .count();
        assert!(todo_before > 0);

        let outcome = cancel_recurrence(
            &rec_repo,
            &task_repo,
            &worklog_repo,
            template.id,
            test_user_id(),
            today(),
        )
        .await
        .unwrap();

        // No worklog was ever logged, so every Todo instance — past and future
        // alike — is deleted outright, not just the future ones.
        assert_eq!(outcome.deleted, todo_before);
        assert_eq!(outcome.cancelled, 0);

        // Template is deactivated.
        let tmpl = rec_repo.find_by_id(template.id).await.unwrap().unwrap();
        assert!(!tmpl.active, "template must be deactivated");

        // The seeded past instance carried no logged time, so it is gone too.
        let past_after: usize = task_instances(&task_repo)
            .iter()
            .filter(|t| t.occurrence_date.map(|d| d < today()).unwrap_or(false))
            .count();
        assert_eq!(
            past_after, 0,
            "a past Todo instance without logged time is swept, not preserved"
        );
    }

    // ── Purge du passé : sans temps loggé, on supprime ────────────────────────
    #[tokio::test]
    async fn cancel_recurrence_deletes_past_instances_without_worklog() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();
        let worklog_repo = InMemoryWorklogRepository::new();
        let today = today();

        let template = daily_template(test_user_id(), today - Duration::days(30));
        rec_repo.save(&template).await.unwrap();
        let stale = save_instance(&task_repo, &template, today - Duration::days(10), TaskStatus::Todo).await;

        let outcome = cancel_recurrence(
            &rec_repo, &task_repo, &worklog_repo, template.id, test_user_id(), today,
        )
        .await
        .unwrap();

        assert_eq!(outcome.deleted, 1);
        assert_eq!(outcome.cancelled, 0);
        assert!(task_repo.find_by_id(stale).await.unwrap().is_none());
    }

    // ── Purge du passé : avec temps loggé, on annule mais on garde ────────────
    #[tokio::test]
    async fn cancel_recurrence_preserves_past_instances_carrying_worklog() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();
        let worklog_repo = InMemoryWorklogRepository::new();
        let today = today();

        let template = daily_template(test_user_id(), today - Duration::days(30));
        rec_repo.save(&template).await.unwrap();
        let worked = save_instance(&task_repo, &template, today - Duration::days(10), TaskStatus::Todo).await;
        worklog_repo.push_entry_for_task(test_user_id(), worked).await;

        let outcome = cancel_recurrence(
            &rec_repo, &task_repo, &worklog_repo, template.id, test_user_id(), today,
        )
        .await
        .unwrap();

        assert_eq!(outcome.deleted, 0, "logged time is billing evidence, never deleted");
        assert_eq!(outcome.cancelled, 1);

        let kept = task_repo.find_by_id(worked).await.unwrap().expect("still there");
        assert_eq!(kept.status, TaskStatus::Cancelled);
    }

    // ── Le comportement sur le futur ne change pas ────────────────────────────
    #[tokio::test]
    async fn cancel_recurrence_still_deletes_future_todo_instances() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();
        let worklog_repo = InMemoryWorklogRepository::new();
        let today = today();

        let template = daily_template(test_user_id(), today);
        rec_repo.save(&template).await.unwrap();
        let future = save_instance(&task_repo, &template, today + Duration::days(3), TaskStatus::Todo).await;
        let done = save_instance(&task_repo, &template, today + Duration::days(4), TaskStatus::Done).await;

        let outcome = cancel_recurrence(
            &rec_repo, &task_repo, &worklog_repo, template.id, test_user_id(), today,
        )
        .await
        .unwrap();

        assert_eq!(outcome.deleted, 1);
        assert!(task_repo.find_by_id(future).await.unwrap().is_none());
        assert!(task_repo.find_by_id(done).await.unwrap().is_some(), "a Done instance is history");
    }

    // ── Ruling: presence cannot be decided from a single capped, newest-first
    // page. `WorklogRepository::find_by_recurrence` truncates at
    // `WORKLOG_FILTER_MAX_LIMIT`, ordered `logged_at DESC` — on a series with
    // more entries than that, the truncated tail is exactly the *oldest*
    // entries, i.e. the ones attached to the past instance this test seeds
    // first. `cancel_recurrence` must still find it. ─────────────────────────
    #[tokio::test]
    async fn cancel_recurrence_finds_logged_time_beyond_worklog_filter_max_limit() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();
        let worklog_repo = InMemoryWorklogRepository::new();
        let today = today();

        let template = daily_template(test_user_id(), today - Duration::days(30));
        rec_repo.save(&template).await.unwrap();
        let worked = save_instance(&task_repo, &template, today - Duration::days(10), TaskStatus::Todo).await;

        // The instance's own entry is logged a year ago: it is the single oldest
        // entry the series will have, so a page ordered `logged_at DESC` and
        // capped at `WORKLOG_FILTER_MAX_LIMIT` would push it out first.
        worklog_repo
            .push_entry_for_task_at(test_user_id(), worked, Utc::now() - Duration::days(365))
            .await;

        // Flood the same series with more than the cap in fresher entries, each
        // on its own instance, so the series holds more than
        // `WORKLOG_FILTER_MAX_LIMIT` entries in total.
        for i in 0..WORKLOG_FILTER_MAX_LIMIT {
            let filler = save_instance(
                &task_repo,
                &template,
                today - Duration::days(9),
                TaskStatus::Todo,
            )
            .await;
            worklog_repo
                .push_entry_for_task_at(
                    test_user_id(),
                    filler,
                    Utc::now() - Duration::days(1) + Duration::seconds(i as i64),
                )
                .await;
        }

        let outcome = cancel_recurrence(
            &rec_repo, &task_repo, &worklog_repo, template.id, test_user_id(), today,
        )
        .await
        .unwrap();

        // Every instance carries logged time (the seeded one plus the fillers),
        // so every one of them is cancelled, none is deleted — including the
        // seeded instance whose entry is the oldest in the whole series.
        assert_eq!(outcome.deleted, 0);
        assert_eq!(
            outcome.cancelled,
            1 + WORKLOG_FILTER_MAX_LIMIT as usize,
            "the oldest entry must still be found although the series logged \
             more than WORKLOG_FILTER_MAX_LIMIT entries"
        );
        let kept = task_repo.find_by_id(worked).await.unwrap().expect("still there");
        assert_eq!(kept.status, TaskStatus::Cancelled);
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
        let worklog_repo = InMemoryWorklogRepository::new();

        let template = create_recurring_task(&rec_repo, daily_input(today()))
            .await
            .unwrap();

        let result = cancel_recurrence(
            &rec_repo,
            &task_repo,
            &worklog_repo,
            template.id,
            other_user_id(),
            today(),
        )
        .await;

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

    // ── Le balayage ferme tout ce qui est périmé et non clos ──────────────────
    #[tokio::test]
    async fn sweep_cancels_every_stale_open_status() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();
        let worklog_repo = InMemoryWorklogRepository::new();
        let today = today();

        let template = daily_template(test_user_id(), today - Duration::days(30));
        rec_repo.save(&template).await.unwrap();

        let stale_todo = save_instance(&task_repo, &template, today - Duration::days(5), TaskStatus::Todo).await;
        let stale_wip = save_instance(&task_repo, &template, today - Duration::days(4), TaskStatus::InProgress).await;
        let stale_blocked = save_instance(&task_repo, &template, today - Duration::days(3), TaskStatus::Blocked).await;

        let swept = sweep_stale_occurrences(&rec_repo, &task_repo, &worklog_repo, test_user_id(), today)
            .await
            .unwrap();

        assert_eq!(swept, 3);
        for id in [stale_todo, stale_wip, stale_blocked] {
            let t = task_repo.find_by_id(id).await.unwrap().unwrap();
            assert_eq!(t.status, TaskStatus::Cancelled);
        }
    }

    // ── Ce que le balayage ne touche jamais ───────────────────────────────────
    #[tokio::test]
    async fn sweep_leaves_closed_current_and_future_alone() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();
        let worklog_repo = InMemoryWorklogRepository::new();
        let today = today();

        let template = daily_template(test_user_id(), today - Duration::days(30));
        rec_repo.save(&template).await.unwrap();

        let done = save_instance(&task_repo, &template, today - Duration::days(5), TaskStatus::Done).await;
        let already = save_instance(&task_repo, &template, today - Duration::days(4), TaskStatus::Cancelled).await;
        let current = save_instance(&task_repo, &template, today, TaskStatus::Todo).await;
        let future = save_instance(&task_repo, &template, today + Duration::days(2), TaskStatus::Todo).await;

        let swept = sweep_stale_occurrences(&rec_repo, &task_repo, &worklog_repo, test_user_id(), today)
            .await
            .unwrap();

        assert_eq!(swept, 0);
        assert_eq!(task_repo.find_by_id(done).await.unwrap().unwrap().status, TaskStatus::Done);
        assert_eq!(task_repo.find_by_id(already).await.unwrap().unwrap().status, TaskStatus::Cancelled);
        assert_eq!(task_repo.find_by_id(current).await.unwrap().unwrap().status, TaskStatus::Todo);
        assert_eq!(task_repo.find_by_id(future).await.unwrap().unwrap().status, TaskStatus::Todo);
    }

    // ── Un template désactivé laisse des instances à balayer ──────────────────
    #[tokio::test]
    async fn sweep_reaches_instances_of_deactivated_templates() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();
        let worklog_repo = InMemoryWorklogRepository::new();
        let today = today();

        let template = daily_template(test_user_id(), today - Duration::days(30));
        rec_repo.save(&template).await.unwrap();
        let stale = save_instance(&task_repo, &template, today - Duration::days(5), TaskStatus::Todo).await;
        rec_repo.deactivate(template.id).await.unwrap();

        let swept = sweep_stale_occurrences(&rec_repo, &task_repo, &worklog_repo, test_user_id(), today)
            .await
            .unwrap();

        assert_eq!(swept, 1, "deactivating a series must not strand its stale instances");
        assert_eq!(
            task_repo.find_by_id(stale).await.unwrap().unwrap().status,
            TaskStatus::Cancelled
        );
    }

    // ── Une occurrence qui porte du temps loggé survit au balayage ────────────
    #[tokio::test]
    async fn sweep_skips_stale_occurrence_carrying_worklog_entries() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();
        let worklog_repo = InMemoryWorklogRepository::new();
        let today = today();

        let template = daily_template(test_user_id(), today - Duration::days(30));
        rec_repo.save(&template).await.unwrap();

        let worked = save_instance(&task_repo, &template, today - Duration::days(5), TaskStatus::Todo).await;
        let untouched = save_instance(&task_repo, &template, today - Duration::days(4), TaskStatus::Todo).await;
        worklog_repo.push_entry_for_task(test_user_id(), worked).await;

        let swept = sweep_stale_occurrences(&rec_repo, &task_repo, &worklog_repo, test_user_id(), today)
            .await
            .unwrap();

        assert_eq!(swept, 1, "the instance carrying logged time must not count as swept");
        assert_eq!(
            task_repo.find_by_id(worked).await.unwrap().unwrap().status,
            TaskStatus::Todo,
            "an occurrence with worklog entries must survive the sweep untouched"
        );
        assert_eq!(
            task_repo.find_by_id(untouched).await.unwrap().unwrap().status,
            TaskStatus::Cancelled
        );
    }

    // ── L'ordre du tick : matérialiser puis balayer, jamais l'inverse ─────────
    #[tokio::test]
    async fn pass_materializes_then_sweeps_without_eating_todays_slot() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();
        let worklog_repo = InMemoryWorklogRepository::new();
        let today = today();

        let mut template = daily_template(test_user_id(), today - Duration::days(200));
        template.last_generated_through = Some(today - Duration::days(123));
        rec_repo.save(&template).await.unwrap();
        let stale =
            save_instance(&task_repo, &template, today - Duration::days(9), TaskStatus::Todo).await;

        let outcome = run_recurrence_pass(
            &rec_repo,
            &task_repo,
            &worklog_repo,
            test_user_id(),
            today,
            14,
        )
        .await
        .unwrap();

        assert!(outcome.materialized > 0, "the horizon ahead is materialized");
        assert_eq!(outcome.swept, 1, "only the pre-existing stale instance is swept");
        assert_eq!(
            task_repo.find_by_id(stale).await.unwrap().unwrap().status,
            TaskStatus::Cancelled
        );

        // Nothing the pass just created may have been swept by its own sweep.
        for task in task_repo.find_by_recurrence(template.id).await.unwrap() {
            let occ = task.occurrence_date.expect("instance carries an occurrence date");
            if occ >= today {
                assert_eq!(task.status, TaskStatus::Todo, "a fresh slot must stay open");
            }
        }
    }

    // ── Idempotence sur deux ticks du même jour ───────────────────────────────
    #[tokio::test]
    async fn a_second_pass_the_same_day_is_a_no_op() {
        let rec_repo = InMemoryRecurrenceRepository::new();
        let task_repo = InMemoryTaskRepository::new();
        let worklog_repo = InMemoryWorklogRepository::new();
        let today = today();

        let template = daily_template(test_user_id(), today);
        rec_repo.save(&template).await.unwrap();

        run_recurrence_pass(&rec_repo, &task_repo, &worklog_repo, test_user_id(), today, 14)
            .await
            .unwrap();
        let second =
            run_recurrence_pass(&rec_repo, &task_repo, &worklog_repo, test_user_id(), today, 14)
                .await
                .unwrap();

        assert_eq!(second.materialized, 0);
        assert_eq!(second.swept, 0);
    }

}
