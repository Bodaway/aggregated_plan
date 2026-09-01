use std::sync::Arc;

use async_graphql::{Context, MaybeUndefined, Object, Result, ID};
use chrono::NaiveDate;
use domain::types::common::{ImpactLevel, UrgencyLevel};
use domain::types::{BreakRule, TimesheetStatus, UserId};
use uuid::Uuid;

use application::repositories::*;
use application::services::{MemoryFileSource, MemoryRetriever};
use application::services::*;
use application::use_cases::{activity_tracking, alerts, configuration, deduplication, gryzzly_assignment, priority, sync, task_management, worklog as worklog_uc};
use application::use_cases::consolidation as consolidation_uc;
use application::use_cases::memory as memory_uc;
use application::use_cases::reattribution as reattribution_uc;
use application::use_cases::recurrence as recurrence_uc;
use application::use_cases::session_tracking;
use application::use_cases::slot_repair as slot_repair_uc;
use application::use_cases::timesheet::{self as timesheet_uc, load_reconstruction_config};
use infrastructure::connectors::excel::GraphExcelClient;
use infrastructure::connectors::gryzzly::{
    BrowserCookieTokenSource, HttpGryzzlyClient, StaticTokenSource,
};
use infrastructure::connectors::jira::HttpJiraClient;
use infrastructure::connectors::outlook::client::GraphOutlookClient;

use super::types::*;

/// Root mutation type for the GraphQL schema.
#[derive(Default)]
pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// No-op mutation placeholder. Returns true.
    async fn noop(&self) -> bool {
        true
    }

    /// Create a new personal task.
    async fn create_task(
        &self,
        ctx: &Context<'_>,
        input: CreateTaskInput,
    ) -> Result<TaskGql> {
        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let today = chrono::Utc::now().date_naive();

        let app_input = convert_create_input(input)?;

        let task =
            task_management::create_personal_task(task_repo.as_ref(), *user_id, app_input, today)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Update an existing task.
    async fn update_task(
        &self,
        ctx: &Context<'_>,
        id: ID,
        input: UpdateTaskInput,
    ) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let task_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;
        let today = chrono::Utc::now().date_naive();

        let app_input = convert_update_input(input)?;

        let task = task_management::update_task(task_repo.as_ref(), task_id, app_input, today)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Delete a task by ID. Returns true on success.
    async fn delete_task(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let task_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        task_management::delete_task(task_repo.as_ref(), task_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    /// Mark a task as completed.
    async fn complete_task(&self, ctx: &Context<'_>, id: ID) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let task_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        let task = task_management::complete_task(task_repo.as_ref(), task_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Append a line of text to a task's user-owned `notes` field. Backs the
    /// activity-timer "quick note" feature: existing content is preserved and
    /// the new text is added on a new paragraph.
    async fn append_task_notes(
        &self,
        ctx: &Context<'_>,
        task_id: ID,
        text: String,
    ) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let id = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        let task = task_management::append_to_task_notes(task_repo.as_ref(), id, &text)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Add a timestamped worklog entry to a task. `sessionId` attributes the entry
    /// to the session that wrote it; omitted, it is the human's, working by hand.
    ///
    /// Two ways to place the entry in time, at most one at a time:
    /// - `loggedAt` — an absolute UTC instant, for a caller that already holds one
    ///   (the web UI builds it from a browser `Date`).
    /// - `loggedAtLocal` — a wall-clock reading in the user's own timezone, for a
    ///   caller that has only what a human typed (`aplan log --at 2026-08-06T14:30`).
    ///   Converted here, through the one `aplan.timezone` reading every other
    ///   projection uses, so the entry cannot land on a different local day than the
    ///   half-day arithmetic will later look for it in. A CLI doing this conversion
    ///   itself would be the second implementation of local-to-UTC that
    ///   `application::time` exists to prevent.
    ///
    /// Neither given, the entry is stamped `now`.
    async fn add_worklog_entry(
        &self,
        ctx: &Context<'_>,
        task_id: ID,
        body: String,
        logged_at: Option<chrono::DateTime<chrono::Utc>>,
        logged_at_local: Option<chrono::NaiveDateTime>,
        session_id: Option<String>,
    ) -> Result<WorklogEntryGql> {
        let repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let user_id = *ctx.data::<UserId>()?;
        let tid = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {e}")))?;
        let now = chrono::Utc::now();

        // Refused rather than given a precedence rule: the two arguments are the same
        // decision expressed twice, and a caller that sent both disagreeing values has
        // a bug that a silent winner would hide inside billable hours.
        let logged_at = match (logged_at, logged_at_local) {
            (Some(_), Some(_)) => {
                return Err(async_graphql::Error::new(
                    "loggedAt and loggedAtLocal both name the entry's instant — pass one",
                ))
            }
            (Some(utc), None) => Some(utc),
            (None, Some(local)) => {
                let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
                let tz = worklog_uc::user_timezone(config_repo.as_ref(), user_id)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                Some(application::time::local_to_utc(tz, local))
            }
            (None, None) => None,
        };

        let entry = worklog_uc::add_worklog_entry(
            repo.as_ref(),
            user_id,
            tid,
            body,
            logged_at,
            now,
            session_id.clone(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        // Refresh the session's `last_seen_at` in the same write, so a session bound
        // in the morning and logging all day does not look idle to the reaper. The
        // worklog entry is the valuable part of this call: a touch failure (or a
        // `sessionId` with no row, since nothing on this branch creates rows yet) is
        // logged and swallowed rather than failing the write that already succeeded.
        if let Some(sid) = session_id {
            let session_repo = ctx.data::<Arc<dyn SessionRepository>>()?;
            let _ = session_repo.touch(&sid, user_id, now).await.map_err(|e| {
                tracing::warn!("touch session {sid} after addWorklogEntry failed: {e}")
            });
        }

        Ok(WorklogEntryGql(entry))
    }

    // ─── Session tracking mutations ───

    /// Point a session at a task. Returns the task it was on before, if any, so the
    /// caller can flush it.
    async fn bind_session(
        &self,
        ctx: &Context<'_>,
        session_id: String,
        task_id: ID,
        label: Option<String>,
    ) -> Result<BindSessionResultGql> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn SessionRepository>>()?;
        let tid = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {e}")))?;

        let outcome = session_tracking::bind_session(
            repo.as_ref(),
            user_id,
            &session_id,
            tid,
            label,
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(BindSessionResultGql {
            previous_task_id: outcome.previous_task.map(|t| ID(t.to_string())),
            session: ClaudeSessionGql(outcome.session),
        })
    }

    /// Record what a session was told to do. `OFF` also clears its task.
    ///
    /// Switching to `OFF` clears `task_id` (`session_tracking::set_session_mode`),
    /// and once cleared no later lookup can find it: `flushWorklogTime` requires a
    /// non-null task id, and the reaper reads the same field, so nothing automatic
    /// would ever flush whatever this session logged while it was tracking. Flush
    /// that task first, against the session's own window, exactly as
    /// `flush_worklog_time` does for its `sessionId` path at `:356` — this is the
    /// same operation, just triggered by the mode switch instead of an explicit
    /// flush. The clearing itself is not touched: a stale task on an opted-out
    /// session is the original defect this whole feature exists to prevent.
    async fn set_session_mode(
        &self,
        ctx: &Context<'_>,
        session_id: String,
        mode: SessionModeGql,
        label: Option<String>,
    ) -> Result<ClaudeSessionGql> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn SessionRepository>>()?;
        let now = chrono::Utc::now();

        if mode == SessionModeGql::Off {
            let existing = repo
                .find_by_id(&session_id, user_id)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            if let Some(session) = existing {
                if let Some(task_id) = session.task_id {
                    let worklog_repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
                    let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
                    let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
                    let outcome = worklog_uc::materialize_worklog_time(
                        worklog_repo.as_ref(),
                        activity_repo.as_ref(),
                        config_repo.as_ref(),
                        user_id,
                        task_id,
                        session.flush_window_start(),
                        now,
                    )
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                    repo.set_last_flush(&session_id, user_id, outcome.active_since)
                        .await
                        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                }
            }
        }

        let session = session_tracking::set_session_mode(
            repo.as_ref(),
            user_id,
            &session_id,
            mode.into(),
            label,
            now,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(ClaudeSessionGql(session))
    }

    /// Close a session. Null when there was nothing open to close.
    async fn end_session(
        &self,
        ctx: &Context<'_>,
        session_id: String,
    ) -> Result<Option<ClaudeSessionGql>> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn SessionRepository>>()?;
        Ok(session_tracking::end_session(
            repo.as_ref(),
            user_id,
            &session_id,
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?
        .map(ClaudeSessionGql))
    }

    /// Update a worklog entry's body and/or logged_at. Only provided fields change.
    async fn update_worklog_entry(
        &self,
        ctx: &Context<'_>,
        id: ID,
        body: Option<String>,
        logged_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<WorklogEntryGql> {
        let repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let user_id = *ctx.data::<UserId>()?;
        let eid = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid entry ID: {e}")))?;
        let entry = worklog_uc::update_worklog_entry(
            repo.as_ref(),
            user_id,
            eid,
            body,
            logged_at,
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(WorklogEntryGql(entry))
    }

    /// Delete a worklog entry. Returns true if a row was removed.
    async fn delete_worklog_entry(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        let repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let user_id = *ctx.data::<UserId>()?;
        let eid = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid entry ID: {e}")))?;
        worklog_uc::delete_worklog_entry(repo.as_ref(), user_id, eid)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))
    }

    /// Materialize worklog entries into closed activity slots for the given task.
    ///
    /// A `sessionId` reads and advances that session's own flush window
    /// (`Session::flush_window_start()` / `SessionRepository::set_last_flush`); no
    /// `sessionId` at all reads and advances the human's `aplan.active_since`
    /// pointer instead. The two must never be crossed: sharing one key across every
    /// task is what made flushing task B advance the mark for task A too, losing
    /// A's time whenever two tasks interleaved. A `sessionId` naming no row is
    /// refused with an error — never a silent fallback onto the human's pointer,
    /// which would advance a key this call never should have touched.
    async fn flush_worklog_time(
        &self,
        ctx: &Context<'_>,
        task_id: ID,
        session_id: Option<String>,
    ) -> Result<FlushResultGql> {
        let user_id = *ctx.data::<UserId>()?;
        let worklog_repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        let sessions = ctx.data::<Arc<dyn SessionRepository>>()?;
        let tid = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {e}")))?;

        let now = chrono::Utc::now();

        // A session flushes against its own window; the human keeps the global key.
        // Sharing one key across tasks is what made flushing task B advance the mark
        // for task A, so the pair must never be crossed. A `sessionId` naming no row
        // is refused outright, not folded into "no session" below: a silent fallback
        // to the human's pointer here is exactly the shape that loses track of whose
        // time is whose. Past this point, `session == None` therefore always means
        // no `sessionId` was given at all.
        let session = match &session_id {
            Some(sid) => Some(
                sessions
                    .find_by_id(sid, user_id)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?
                    .ok_or_else(|| {
                        async_graphql::Error::new(format!("no session {sid} is known to aplan"))
                    })?,
            ),
            None => None,
        };

        let from = match &session {
            Some(s) => s.flush_window_start(),
            None => config_repo
                .get(user_id, "aplan.active_since")
                .await
                .ok()
                .flatten()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|| {
                    chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0)
                        .expect("epoch is a valid timestamp")
                }),
        };

        let outcome = worklog_uc::materialize_worklog_time(
            worklog_repo.as_ref(),
            activity_repo.as_ref(),
            config_repo.as_ref(),
            user_id,
            tid,
            from,
            now,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        // The unknown-id case already returned above, so `session` alone now
        // expresses the two real states: found (advance its own window) or
        // genuinely absent (advance the human's).
        match &session {
            Some(s) => {
                sessions
                    .set_last_flush(&s.id, user_id, outcome.active_since)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            }
            None => {
                configuration::set_config(
                    config_repo.as_ref(),
                    user_id,
                    "aplan.active_since",
                    &outcome.active_since.to_rfc3339(),
                )
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            }
        }

        Ok(FlushResultGql(outcome))
    }

    /// Rebuild one task's activity slots for one **named local day**, advancing no
    /// flush watermark.
    ///
    /// What `flushWorklogTime` cannot do: it discovers which half-days to rebuild from
    /// the entries inside its own window, and that window starts when the session did.
    /// An entry written with a backdated instant (`addWorklogEntry`'s `loggedAt` /
    /// `loggedAtLocal`, which `aplan log --at` uses to record yesterday's work) sits
    /// before that window, so the flush never learns its half-day exists and its hours
    /// never reach the timesheet — the entry is in the journal and the day still bills
    /// zero. Naming the day closes that.
    ///
    /// Only the half-days this task actually has entries in are rebuilt. Idempotent:
    /// it derives the slots from the entries and replaces only the ones the projection
    /// owns, so re-running converges instead of doubling the day.
    async fn rebuild_worklog_projection(
        &self,
        ctx: &Context<'_>,
        task_id: ID,
        date: NaiveDate,
    ) -> Result<DayRebuildResultGql> {
        let user_id = *ctx.data::<UserId>()?;
        let worklog_repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        let tid = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {e}")))?;

        let outcome = worklog_uc::rebuild_task_local_date(
            worklog_repo.as_ref(),
            activity_repo.as_ref(),
            config_repo.as_ref(),
            user_id,
            tid,
            date,
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(outcome.into())
    }

    /// Move worklog entries — and the activity time derived from them — from one
    /// task to another.
    ///
    /// The correction a wrong attribution needs: `addWorklogEntry` can only write to
    /// the task it is given, so before this existed a day recorded against the wrong
    /// task stayed wrong, and flowed into the timesheet and on to billing.
    ///
    /// Writes nothing unless `input.confirm` is true. The preview reports the same
    /// counts and the same before/after hours the write would produce, computed from
    /// the same projection — one code path, so the two cannot drift.
    async fn reattribute_worklog_entries(
        &self,
        ctx: &Context<'_>,
        input: ReattributeWorklogInput,
    ) -> Result<ReattributionResultGql> {
        let user_id = *ctx.data::<UserId>()?;
        let worklog_repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;

        let from_task = Uuid::parse_str(&input.from_task)
            .map_err(|e| async_graphql::Error::new(format!("Invalid source task ID: {e}")))?;
        let to_task = Uuid::parse_str(&input.to_task)
            .map_err(|e| async_graphql::Error::new(format!("Invalid destination task ID: {e}")))?;

        let outcome = reattribution_uc::reattribute_worklog_entries(
            worklog_repo.as_ref(),
            activity_repo.as_ref(),
            config_repo.as_ref(),
            user_id,
            reattribution_uc::ReattributionRequest {
                from_task,
                to_task,
                entry_refs: input.entry_refs.unwrap_or_default(),
                since: input.since,
                until: input.until,
                confirm: input.confirm.unwrap_or(false),
            },
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(outcome.into())
    }

    /// Give back their task to the activity slots of a local-date range that lost it.
    ///
    /// The damage this repairs: a write that used `INSERT OR REPLACE INTO tasks` fired
    /// `activity_slots.task_id`'s `ON DELETE SET NULL`, so slots the worklog projection
    /// owns came out unattributed — the "(no task)" hours of `aplan journal`. Neither
    /// `flushWorklogTime` (its window only ever names the present) nor
    /// `reattributeWorklogEntries` (it refuses a move onto the same task, and its
    /// delete list cannot match a NULL `task_id`) could reach them.
    ///
    /// Drops those slots and rewrites their half-days from the worklog entries, which
    /// still carry the attribution. Never touches a slot the projection does not own:
    /// an unattributed `manual` slot is a hand-run timer, not damage.
    ///
    /// Writes nothing unless `input.confirm` is true. The preview's figures are read
    /// off the very plans the write persists, so they cannot drift.
    async fn repair_orphaned_slots(
        &self,
        ctx: &Context<'_>,
        input: RepairOrphanedSlotsInput,
    ) -> Result<SlotRepairResultGql> {
        let user_id = *ctx.data::<UserId>()?;
        let worklog_repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;

        let outcome = slot_repair_uc::repair_orphaned_slots(
            worklog_repo.as_ref(),
            activity_repo.as_ref(),
            config_repo.as_ref(),
            user_id,
            slot_repair_uc::SlotRepairRequest {
                from: input.from,
                to: input.to,
                confirm: input.confirm.unwrap_or(false),
            },
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(outcome.into())
    }

    /// Set the tracking state of a task (inbox/followed/dismissed).
    async fn set_tracking_state(
        &self,
        ctx: &Context<'_>,
        task_id: ID,
        state: TrackingStateGql,
    ) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let id = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        let task = task_management::set_tracking_state(task_repo.as_ref(), id, state.into())
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Batch-set the tracking state for multiple tasks.
    async fn set_tracking_state_batch(
        &self,
        ctx: &Context<'_>,
        task_ids: Vec<ID>,
        state: TrackingStateGql,
    ) -> Result<Vec<TaskGql>> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let ids: Vec<Uuid> = task_ids
            .into_iter()
            .map(|id| {
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))
            })
            .collect::<Result<Vec<_>>>()?;

        let tasks =
            task_management::set_tracking_state_batch(task_repo.as_ref(), ids, state.into())
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(tasks.into_iter().map(TaskGql).collect())
    }

    /// Override the urgency level of a task (manual override).
    async fn update_priority(
        &self,
        ctx: &Context<'_>,
        task_id: ID,
        urgency: Option<UrgencyLevelGql>,
        impact: Option<ImpactLevelGql>,
    ) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let id = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        let mut task: Option<domain::types::Task> = None;

        if let Some(u) = urgency {
            task = Some(
                priority::override_urgency(task_repo.as_ref(), id, u.into())
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?,
            );
        }

        if let Some(i) = impact {
            task = Some(
                priority::override_impact(task_repo.as_ref(), id, i.into())
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?,
            );
        }

        match task {
            Some(t) => Ok(TaskGql(t)),
            None => Err(async_graphql::Error::new(
                "At least one of urgency or impact must be provided",
            )),
        }
    }

    /// Reset urgency to auto-calculated based on deadline.
    async fn reset_urgency(&self, ctx: &Context<'_>, task_id: ID) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let id = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;
        let today = chrono::Utc::now().date_naive();

        let task = priority::reset_urgency(task_repo.as_ref(), id, today)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Trigger a sync for a specific source (or all sources if not specified).
    /// Returns updated sync statuses.
    async fn force_sync(
        &self,
        ctx: &Context<'_>,
        source: Option<SourceGql>,
    ) -> Result<Vec<SyncStatusGql>> {
        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let meeting_repo = ctx.data::<Arc<dyn MeetingRepository>>()?;
        let project_repo = ctx.data::<Arc<dyn ProjectRepository>>()?;
        let sync_repo = ctx.data::<Arc<dyn SyncStatusRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        let gryzzly_catalog_repo = ctx.data::<Arc<dyn GryzzlyCatalogRepository>>()?;

        // Build clients dynamically from stored configuration.
        let jira_client: Option<Arc<dyn JiraClient>> = {
            let base_url = config_repo.get(*user_id, "jira.base_url").await.ok().flatten();
            let email = config_repo.get(*user_id, "jira.email").await.ok().flatten();
            let token = config_repo.get(*user_id, "jira.token").await.ok().flatten();
            match (base_url, email, token) {
                (Some(url), Some(em), Some(tok)) if !url.is_empty() && !em.is_empty() && !tok.is_empty() => {
                    Some(Arc::new(HttpJiraClient::new(url, em, tok)))
                }
                _ => None,
            }
        };
        // One Graph token serves both connectors.
        let graph_token_provider = ctx.data::<Arc<dyn GraphTokenProvider>>()?;
        let graph_token = graph_token_provider.valid_access_token(*user_id).await.ok();
        let outlook_client: Option<Arc<dyn OutlookClient>> = graph_token
            .clone()
            .map(|t| Arc::new(GraphOutlookClient::new(t)) as Arc<dyn OutlookClient>);
        let excel_client: Option<Arc<dyn ExcelClient>> = graph_token
            .map(|t| Arc::new(GraphExcelClient::new(t)) as Arc<dyn ExcelClient>);

        // Build the Gryzzly client from stored config. Gryzzly issues no API key:
        // auth is the `remember_token` session cookie from the browser login, so
        // the token source is either a hand-pasted value or the local cookie store.
        let gryzzly_base_url = config_repo
            .get(*user_id, "gryzzly.base_url")
            .await
            .ok()
            .flatten()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| "https://api.gryzzly.io".to_string());
        let manual_token = config_repo
            .get(*user_id, "gryzzly.token")
            .await
            .ok()
            .flatten()
            .filter(|t| !t.trim().is_empty());
        let cookie_profile = config_repo
            .get(*user_id, "gryzzly.cookie_profile")
            .await
            .ok()
            .flatten()
            .filter(|p| !p.trim().is_empty())
            .map(std::path::PathBuf::from);

        let gryzzly_tokens: Option<Arc<dyn GryzzlyTokenSource>> = match manual_token {
            Some(t) => Some(Arc::new(StaticTokenSource::new(&t))),
            None => {
                let source = BrowserCookieTokenSource::new(cookie_profile);
                // No cookie at all means "not configured". An *expired* cookie is
                // available, so its dated "log in again" message reaches the user
                // instead of being flattened into a bare Not configured.
                if source.available().await {
                    Some(Arc::new(source))
                } else {
                    None
                }
            }
        };
        let gryzzly_client: Option<Arc<dyn GryzzlyClient>> = gryzzly_tokens
            .map(|t| Arc::new(HttpGryzzlyClient::new(gryzzly_base_url, t)) as Arc<dyn GryzzlyClient>);

        let ctx = sync::SyncContext {
            task_repo: task_repo.as_ref(),
            meeting_repo: meeting_repo.as_ref(),
            project_repo: project_repo.as_ref(),
            sync_repo: sync_repo.as_ref(),
            config_repo: config_repo.as_ref(),
            jira_client: jira_client.as_deref(),
            outlook_client: outlook_client.as_deref(),
            excel_client: excel_client.as_deref(),
            gryzzly_client: gryzzly_client.as_deref(),
            gryzzly_catalog_repo: gryzzly_catalog_repo.as_ref(),
        };
        match source {
            Some(src) => {
                // Sync a single source.
                let domain_source: domain::types::Source = src.into();
                sync::sync_source(&ctx, domain_source, *user_id)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            }
            None => {
                // Sync all sources.
                sync::sync_all(&ctx, *user_id)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            }
        }

        // Return all sync statuses.
        let statuses = sync_repo
            .find_by_user(*user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(statuses.into_iter().map(SyncStatusGql).collect())
    }

    // ─── Deduplication mutations ───

    /// Manually link two tasks.
    async fn link_tasks(
        &self,
        ctx: &Context<'_>,
        task_id_primary: ID,
        task_id_secondary: ID,
    ) -> Result<bool> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let task_link_repo = ctx.data::<Arc<dyn TaskLinkRepository>>()?;
        let primary = Uuid::parse_str(&task_id_primary)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;
        let secondary = Uuid::parse_str(&task_id_secondary)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        deduplication::link_tasks(task_repo.as_ref(), task_link_repo.as_ref(), primary, secondary)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    /// Unlink two tasks by removing their link.
    async fn unlink_tasks(&self, ctx: &Context<'_>, link_id: ID) -> Result<bool> {
        let task_link_repo = ctx.data::<Arc<dyn TaskLinkRepository>>()?;
        let id = Uuid::parse_str(&link_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid link ID: {}", e)))?;

        deduplication::unlink_tasks(task_link_repo.as_ref(), id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    /// Confirm or reject a deduplication suggestion.
    async fn confirm_deduplication(
        &self,
        ctx: &Context<'_>,
        task_id_primary: ID,
        task_id_secondary: ID,
        accept: bool,
    ) -> Result<bool> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let task_link_repo = ctx.data::<Arc<dyn TaskLinkRepository>>()?;
        let primary = Uuid::parse_str(&task_id_primary)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;
        let secondary = Uuid::parse_str(&task_id_secondary)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        deduplication::confirm_suggestion(task_repo.as_ref(), task_link_repo.as_ref(), primary, secondary, accept)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    // ─── Alert mutations ───

    /// Resolve an alert by ID.
    async fn resolve_alert(&self, ctx: &Context<'_>, id: ID) -> Result<AlertGql> {
        let alert_repo = ctx.data::<Arc<dyn AlertRepository>>()?;
        let alert_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid alert ID: {}", e)))?;

        let alert = alerts::resolve_alert(alert_repo.as_ref(), alert_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(AlertGql(alert))
    }

    // ─── Activity tracking mutations ───

    /// Start tracking a new activity. Stops the previous active slot (if any).
    async fn start_activity(
        &self,
        ctx: &Context<'_>,
        task_id: Option<ID>,
    ) -> Result<ActivitySlotGql> {
        let user_id = ctx.data::<UserId>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let now = chrono::Utc::now();

        let tid = match task_id {
            Some(id) => Some(
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?,
            ),
            None => None,
        };

        let slot =
            activity_tracking::start_activity(activity_repo.as_ref(), *user_id, tid, now)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(ActivitySlotGql(slot))
    }

    /// Stop the currently active activity tracking.
    async fn stop_activity(&self, ctx: &Context<'_>) -> Result<Option<ActivitySlotGql>> {
        let user_id = ctx.data::<UserId>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let now = chrono::Utc::now();

        let slot = activity_tracking::stop_activity(activity_repo.as_ref(), *user_id, now)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(slot.map(ActivitySlotGql))
    }

    /// Update an existing activity slot.
    async fn update_activity_slot(
        &self,
        ctx: &Context<'_>,
        id: ID,
        input: UpdateActivitySlotInput,
    ) -> Result<ActivitySlotGql> {
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let slot_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid slot ID: {}", e)))?;

        // Convert MaybeUndefined task_id:
        // Undefined => None (don't change), Null => Some(None) (clear), Value => Some(Some(id))
        let task_id = match input.task_id {
            MaybeUndefined::Value(tid) => {
                let parsed = Uuid::parse_str(&tid)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;
                Some(Some(parsed))
            }
            MaybeUndefined::Null => Some(None),
            MaybeUndefined::Undefined => None,
        };

        let slot = activity_tracking::update_activity_slot(
            activity_repo.as_ref(),
            slot_id,
            task_id,
            input.start_time,
            input.end_time,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(ActivitySlotGql(slot))
    }

    /// Delete an activity slot by ID.
    async fn delete_activity_slot(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let slot_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid slot ID: {}", e)))?;

        activity_tracking::delete_activity_slot(activity_repo.as_ref(), slot_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    /// Create a manual activity slot with explicit start and end times.
    async fn create_activity_slot(
        &self,
        ctx: &Context<'_>,
        input: CreateActivitySlotInput,
    ) -> Result<ActivitySlotGql> {
        let user_id = ctx.data::<UserId>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;

        let task_id = match input.task_id {
            Some(id) => Some(
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?,
            ),
            None => None,
        };

        let slot = activity_tracking::create_manual_activity_slot(
            activity_repo.as_ref(),
            *user_id,
            input.start_time,
            input.end_time,
            task_id,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(ActivitySlotGql(slot))
    }

    // ─── Meeting-project association (Task 38) ───

    /// Update the project association of a meeting.
    async fn update_meeting_project(
        &self,
        ctx: &Context<'_>,
        meeting_id: ID,
        project_id: Option<ID>,
    ) -> Result<MeetingGql> {
        let meeting_repo = ctx.data::<Arc<dyn MeetingRepository>>()?;

        let mid = Uuid::parse_str(&meeting_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid meeting ID: {}", e)))?;

        let pid = match project_id {
            Some(id) => Some(
                Uuid::parse_str(&id).map_err(|e| {
                    async_graphql::Error::new(format!("Invalid project ID: {}", e))
                })?,
            ),
            None => None,
        };

        let mut meeting = meeting_repo
            .find_by_id(mid)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .ok_or_else(|| {
                async_graphql::Error::new(format!("Meeting {} not found", mid))
            })?;

        meeting.project_id = pid;
        meeting_repo
            .update(&meeting)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(MeetingGql(meeting))
    }

    // ─── Project management mutations ───

    /// Create a new project.
    async fn create_project(
        &self,
        ctx: &Context<'_>,
        input: CreateProjectInput,
    ) -> Result<ProjectGql> {
        let user_id = ctx.data::<UserId>()?;
        let project_repo = ctx.data::<Arc<dyn ProjectRepository>>()?;

        let project = domain::types::Project {
            id: Uuid::new_v4(),
            user_id: *user_id,
            name: input.name,
            source: domain::types::Source::Personal,
            source_id: None,
            status: domain::types::ProjectStatus::Active,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        project_repo
            .save(&project)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(ProjectGql(project))
    }

    // ─── Tag management mutations (Task 39) ───

    /// Create a new tag.
    async fn create_tag(
        &self,
        ctx: &Context<'_>,
        name: String,
        color: Option<String>,
    ) -> Result<TagGql> {
        let user_id = ctx.data::<UserId>()?;
        let tag_repo = ctx.data::<Arc<dyn TagRepository>>()?;

        let tag = configuration::create_tag(tag_repo.as_ref(), *user_id, name, color)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TagGql(tag))
    }

    /// Update an existing tag.
    async fn update_tag(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: Option<String>,
        color: Option<String>,
    ) -> Result<TagGql> {
        let tag_repo = ctx.data::<Arc<dyn TagRepository>>()?;
        let tag_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid tag ID: {}", e)))?;

        // Wrap color in Option<Option<String>> for the use case:
        // Some(color_value) means update, None means don't change.
        let color_update = color.map(|c| {
            if c.is_empty() {
                None
            } else {
                Some(c)
            }
        });

        let tag = configuration::update_tag(tag_repo.as_ref(), tag_id, name, color_update)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TagGql(tag))
    }

    /// Delete a tag by ID. Returns true on success.
    async fn delete_tag(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        let tag_repo = ctx.data::<Arc<dyn TagRepository>>()?;
        let tag_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid tag ID: {}", e)))?;

        configuration::delete_tag(tag_repo.as_ref(), tag_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    // ─── Configuration mutations ───

    /// Update a configuration key-value pair.
    async fn update_configuration(
        &self,
        ctx: &Context<'_>,
        key: String,
        value: String,
    ) -> Result<bool> {
        let user_id = ctx.data::<UserId>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;

        configuration::set_config(config_repo.as_ref(), *user_id, &key, &value)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(true)
    }

    /// Sign out by clearing all stored Microsoft OAuth tokens.
    /// Returns true on success.
    async fn sign_out(&self, ctx: &Context<'_>) -> Result<bool> {
        let user_id = ctx.data::<UserId>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        for key in [
            "microsoft.access_token",
            "microsoft.refresh_token",
            "microsoft.token_expires_at",
            "microsoft.account",
        ] {
            config_repo
                .set(*user_id, key, "")
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        }
        Ok(true)
    }

    // ─── Recurrence mutations ───

    /// Create a new recurring task template and materialize the first 14-day horizon.
    async fn create_recurring_task(
        &self,
        ctx: &Context<'_>,
        input: CreateRecurringTaskInput,
    ) -> Result<RecurrenceTemplateGql> {
        let user_id = ctx.data::<UserId>()?;
        let rec_repo = ctx.data::<Arc<dyn RecurrenceRepository>>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let today = chrono::Utc::now().date_naive();

        let app_input = convert_create_recurring_input(*user_id, input)?;

        let template = recurrence_uc::create_recurring_task(rec_repo.as_ref(), app_input)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        // Eagerly materialize the first horizon so instances are immediately available.
        let _ = recurrence_uc::materialize_due_occurrences(
            rec_repo.as_ref(),
            task_repo.as_ref(),
            *user_id,
            today,
            14,
        )
        .await
        .map_err(|e| tracing::warn!("materialize after create failed: {e}"));

        // Reload the template so the updated watermark is returned.
        let updated = rec_repo
            .find_by_id(template.id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .unwrap_or(template);

        Ok(RecurrenceTemplateGql(updated))
    }

    /// Update a recurring task template and re-materialize future instances.
    async fn update_recurring_task(
        &self,
        ctx: &Context<'_>,
        id: ID,
        input: UpdateRecurringTaskInput,
    ) -> Result<RecurrenceTemplateGql> {
        use domain::types::recurrence::RecurrenceTemplateId;

        let user_id = ctx.data::<UserId>()?;
        let rec_repo = ctx.data::<Arc<dyn RecurrenceRepository>>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let today = chrono::Utc::now().date_naive();

        let template_id = id
            .parse::<RecurrenceTemplateId>()
            .map_err(|e| async_graphql::Error::new(format!("Invalid template ID: {e}")))?;

        let app_input = convert_update_recurring_input(input)?;

        let template = recurrence_uc::update_recurring_task(
            rec_repo.as_ref(),
            task_repo.as_ref(),
            template_id,
            *user_id,
            app_input,
            today,
            14,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(RecurrenceTemplateGql(template))
    }

    /// Cancel a recurring task series. Deactivates the template and deletes
    /// all future Todo instances. Returns the count of deleted instances.
    async fn cancel_recurrence(&self, ctx: &Context<'_>, id: ID) -> Result<i32> {
        use domain::types::recurrence::RecurrenceTemplateId;

        let user_id = ctx.data::<UserId>()?;
        let rec_repo = ctx.data::<Arc<dyn RecurrenceRepository>>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let today = chrono::Utc::now().date_naive();

        let template_id = id
            .parse::<RecurrenceTemplateId>()
            .map_err(|e| async_graphql::Error::new(format!("Invalid template ID: {e}")))?;

        let deleted = recurrence_uc::cancel_recurrence(
            rec_repo.as_ref(),
            task_repo.as_ref(),
            template_id,
            *user_id,
            today,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(deleted as i32)
    }

    /// Skip (cancel) a single recurring task occurrence. Returns the updated task.
    async fn skip_occurrence(&self, ctx: &Context<'_>, task_id: ID) -> Result<TaskGql> {
        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;

        let tid = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {e}")))?;

        let task = recurrence_uc::skip_occurrence(task_repo.as_ref(), tid, *user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    // ─── Gryzzly assignment ───

    /// Assign a Gryzzly task to an aplan task. Snapshots the Gryzzly project id
    /// from the catalog at assign time. Pass `null` for `gryzzly_task_id` to clear.
    async fn assign_gryzzly_task(
        &self,
        ctx: &Context<'_>,
        task_id: ID,
        gryzzly_task_id: Option<ID>,
    ) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let catalog_repo = ctx.data::<Arc<dyn GryzzlyCatalogRepository>>()?;
        let tid = Uuid::parse_str(task_id.as_str())
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {e}")))?;
        let gid = gryzzly_task_id.map(|g| g.to_string());
        let task =
            gryzzly_assignment::assign_gryzzly_task(task_repo.as_ref(), catalog_repo.as_ref(), tid, gid)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(TaskGql(task))
    }

    // ─── Timesheet reconstruction (Plan 2) ───

    /// Reconstruct the day from ambient signals, persist the draft, return the full result.
    async fn run_timesheet_reconstruction(
        &self,
        ctx: &Context<'_>,
        date: NaiveDate,
    ) -> Result<ReconstructedDayGql> {
        let user_id = *ctx.data::<UserId>()?;
        let worklog_repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let meeting_repo = ctx.data::<Arc<dyn MeetingRepository>>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let catalog_repo = ctx.data::<Arc<dyn GryzzlyCatalogRepository>>()?;
        let mapping_repo = ctx.data::<Arc<dyn SignalMappingRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let git = ctx.data::<Arc<dyn GitConnector>>()?;
        let draft_repo = ctx.data::<Arc<dyn TimesheetDraftRepository>>()?;

        let cfg = load_reconstruction_config(config_repo.as_ref(), user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let day = timesheet_uc::reconstruct_timesheet(
            worklog_repo.as_ref(),
            meeting_repo.as_ref(),
            task_repo.as_ref(),
            catalog_repo.as_ref(),
            mapping_repo.as_ref(),
            config_repo.as_ref(),
            activity_repo.as_ref(),
            git.as_ref(),
            draft_repo.as_ref(),
            user_id,
            date,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        // If a validated/submitted/day-off draft already existed, reconstruct_timesheet did NOT
        // overwrite it — return the PERSISTED draft, not the recomputed (unpersisted) `day`,
        // so the client never sees fresh allocations mislabeled as validated or as a plain Draft
        // on a day off.
        let existing = draft_repo
            .find_by_user_and_date(user_id, date)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        match existing {
            Some(d) if matches!(d.status, TimesheetStatus::Validated | TimesheetStatus::Submitted | TimesheetStatus::DayOff) => {
                Ok(ReconstructedDayGql::from_draft(d, &cfg))
            }
            _ => Ok(ReconstructedDayGql::from_reconstructed(day, &cfg, TimesheetStatus::Draft)),
        }
    }

    /// Pin one lane's hours inside one quarter; the rest of that quarter rebalances.
    async fn set_quarter_share(
        &self,
        ctx: &Context<'_>,
        date: NaiveDate,
        quarter_index: i32,
        lane_key: String,
        hours: f64,
    ) -> Result<ReconstructedDayGql> {
        let (user_id, cfg, day) = {
            let user_id = *ctx.data::<UserId>()?;
            let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
            let cfg = load_reconstruction_config(config_repo.as_ref(), user_id)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            let day = timesheet_uc::set_quarter_share(
                ctx.data::<Arc<dyn WorklogRepository>>()?.as_ref(),
                ctx.data::<Arc<dyn MeetingRepository>>()?.as_ref(),
                ctx.data::<Arc<dyn TaskRepository>>()?.as_ref(),
                ctx.data::<Arc<dyn GryzzlyCatalogRepository>>()?.as_ref(),
                ctx.data::<Arc<dyn SignalMappingRepository>>()?.as_ref(),
                config_repo.as_ref(),
                ctx.data::<Arc<dyn ActivitySlotRepository>>()?.as_ref(),
                ctx.data::<Arc<dyn GitConnector>>()?.as_ref(),
                ctx.data::<Arc<dyn TimesheetDraftRepository>>()?.as_ref(),
                user_id,
                date,
                quarter_index.max(0) as u8,
                &lane_key,
                hours,
            )
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            (user_id, cfg, day)
        };
        let _ = user_id;
        Ok(ReconstructedDayGql::from_reconstructed(day, &cfg, TimesheetStatus::Draft))
    }

    /// Release one pinned share back to what the evidence says.
    async fn clear_quarter_share(
        &self,
        ctx: &Context<'_>,
        date: NaiveDate,
        quarter_index: i32,
        lane_key: String,
    ) -> Result<ReconstructedDayGql> {
        let user_id = *ctx.data::<UserId>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        let cfg = load_reconstruction_config(config_repo.as_ref(), user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let day = timesheet_uc::clear_quarter_share(
            ctx.data::<Arc<dyn WorklogRepository>>()?.as_ref(),
            ctx.data::<Arc<dyn MeetingRepository>>()?.as_ref(),
            ctx.data::<Arc<dyn TaskRepository>>()?.as_ref(),
            ctx.data::<Arc<dyn GryzzlyCatalogRepository>>()?.as_ref(),
            ctx.data::<Arc<dyn SignalMappingRepository>>()?.as_ref(),
            config_repo.as_ref(),
            ctx.data::<Arc<dyn ActivitySlotRepository>>()?.as_ref(),
            ctx.data::<Arc<dyn GitConnector>>()?.as_ref(),
            ctx.data::<Arc<dyn TimesheetDraftRepository>>()?.as_ref(),
            user_id,
            date,
            quarter_index.max(0) as u8,
            &lane_key,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(ReconstructedDayGql::from_reconstructed(day, &cfg, TimesheetStatus::Draft))
    }

    /// Drop every pin in one quarter.
    async fn reset_quarter(
        &self,
        ctx: &Context<'_>,
        date: NaiveDate,
        quarter_index: i32,
    ) -> Result<ReconstructedDayGql> {
        let user_id = *ctx.data::<UserId>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        let cfg = load_reconstruction_config(config_repo.as_ref(), user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let day = timesheet_uc::reset_quarter(
            ctx.data::<Arc<dyn WorklogRepository>>()?.as_ref(),
            ctx.data::<Arc<dyn MeetingRepository>>()?.as_ref(),
            ctx.data::<Arc<dyn TaskRepository>>()?.as_ref(),
            ctx.data::<Arc<dyn GryzzlyCatalogRepository>>()?.as_ref(),
            ctx.data::<Arc<dyn SignalMappingRepository>>()?.as_ref(),
            config_repo.as_ref(),
            ctx.data::<Arc<dyn ActivitySlotRepository>>()?.as_ref(),
            ctx.data::<Arc<dyn GitConnector>>()?.as_ref(),
            ctx.data::<Arc<dyn TimesheetDraftRepository>>()?.as_ref(),
            user_id,
            date,
            quarter_index.max(0) as u8,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(ReconstructedDayGql::from_reconstructed(day, &cfg, TimesheetStatus::Draft))
    }

    /// Mark a day's draft validated (ready to copy into Gryzzly).
    async fn validate_timesheet(&self, ctx: &Context<'_>, date: NaiveDate) -> Result<ReconstructedDayGql> {
        let user_id = *ctx.data::<UserId>()?;
        let draft_repo = ctx.data::<Arc<dyn TimesheetDraftRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        timesheet_uc::validate_timesheet(draft_repo.as_ref(), user_id, date)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let cfg = load_reconstruction_config(config_repo.as_ref(), user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let draft = draft_repo
            .find_by_user_and_date(user_id, date)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .ok_or_else(|| async_graphql::Error::new("no draft to validate"))?;
        Ok(ReconstructedDayGql::from_draft(draft, &cfg))
    }

    /// Mark a whole/half day off (suppresses reconstruction fill).
    async fn mark_day_off(
        &self,
        ctx: &Context<'_>,
        date: NaiveDate,
        scope: DayOffScopeGql,
    ) -> Result<ReconstructedDayGql> {
        let user_id = *ctx.data::<UserId>()?;
        let draft_repo = ctx.data::<Arc<dyn TimesheetDraftRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        timesheet_uc::mark_day_off(draft_repo.as_ref(), user_id, date, scope.into())
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let cfg = load_reconstruction_config(config_repo.as_ref(), user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let draft = draft_repo
            .find_by_user_and_date(user_id, date)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .ok_or_else(|| async_graphql::Error::new("no draft after mark_day_off"))?;
        Ok(ReconstructedDayGql::from_draft(draft, &cfg))
    }

    /// Learn a signal→Gryzzly-project mapping rule (validated against the live catalog).
    async fn learn_mapping(
        &self,
        ctx: &Context<'_>,
        kind: MappingKindGql,
        pattern: String,
        branch_pattern: Option<String>,
        gryzzly_project_id: ID,
    ) -> Result<SignalMappingGql> {
        let user_id = *ctx.data::<UserId>()?;
        let mapping_repo = ctx.data::<Arc<dyn SignalMappingRepository>>()?;
        let catalog_repo = ctx.data::<Arc<dyn GryzzlyCatalogRepository>>()?;
        let now = chrono::Utc::now();
        let mapping = timesheet_uc::learn_mapping(
            mapping_repo.as_ref(),
            catalog_repo.as_ref(),
            user_id,
            kind.into(),
            pattern,
            branch_pattern,
            gryzzly_project_id.to_string(),
            now,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(SignalMappingGql::from(mapping))
    }

    /// Record a semantic memory (`aplan remember`).
    ///
    /// Lands in the validation queue (`PENDING`) unless `confirmed` is true.
    /// Writes `memories` and its FTS row in one transaction, so the memory is
    /// searchable as soon as this returns.
    async fn remember(&self, ctx: &Context<'_>, input: RememberInputGql) -> Result<MemoryGql> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn MemoryRepository>>()?;

        let project_id = match input.project_id {
            None => None,
            Some(id) => Some(
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid project ID: {e}")))?,
            ),
        };
        let task_id = match input.task_id {
            None => None,
            Some(id) => Some(
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {e}")))?,
            ),
        };
        // A reference, not a raw id: the consolidation reads `[m:7c1]` out of a
        // brief and has nothing longer to pass. Resolved through the shared
        // resolver so an unknown or ambiguous one fails the write instead of
        // recording a claim nobody can act on.
        let proposed_supersedes = match &input.proposed_supersedes {
            None => None,
            Some(reference) => Some(resolve_memory_arg(repo.as_ref(), user_id, reference).await?),
        };

        let memory = memory_uc::remember(
            repo.as_ref(),
            user_id,
            memory_uc::RememberInput {
                kind: input.kind.into(),
                title: input.title,
                body: input.body,
                occurred_at: input.occurred_at,
                source: input
                    .source
                    .map(Into::into)
                    .unwrap_or(domain::types::MemorySource::ClaudeSession),
                source_ref: input.source_ref,
                confirmed: input.confirmed.unwrap_or(false),
                proposed_supersedes,
                project_id,
                task_id,
                stakeholders: input.stakeholders.unwrap_or_default(),
            },
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(MemoryGql::from(memory))
    }

    /// Accept a pending candidate (`aplan inbox accept`).
    ///
    /// Refuses a silent add: if the candidate looks like an existing active
    /// memory, nothing is written and the look-alikes come back in
    /// `nearDuplicates` so the caller can choose `mergeMemory` or
    /// `supersedeMemory`. `force` accepts anyway.
    ///
    /// `id` takes a full UUID **or** the short reference the brief and the inbox
    /// display (`m:7c1`, `7c1`) — see [`resolve_memory_arg`].
    async fn accept_memory(
        &self,
        ctx: &Context<'_>,
        id: ID,
        kind: Option<MemoryKindGql>,
        #[graphql(default = false)] force: bool,
    ) -> Result<AcceptMemoryResultGql> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn MemoryRepository>>()?;
        let retriever = ctx.data::<Arc<dyn MemoryRetriever>>()?;
        let candidate_id = resolve_memory_arg(repo.as_ref(), user_id, &id).await?;
        let outcome = memory_uc::accept_candidate(
            repo.as_ref(),
            retriever.as_ref(),
            user_id,
            candidate_id,
            kind.map(Into::into),
            force,
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(AcceptMemoryResultGql::from(outcome))
    }

    /// Reject a pending candidate (`aplan inbox reject`). The row is kept as a
    /// tombstone so the consolidation job cannot re-propose it.
    ///
    /// `id` takes a full UUID or a short reference.
    async fn reject_memory(&self, ctx: &Context<'_>, id: ID) -> Result<MemoryGql> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn MemoryRepository>>()?;
        let candidate_id = resolve_memory_arg(repo.as_ref(), user_id, &id).await?;
        let rejected = memory_uc::reject_candidate(repo.as_ref(), user_id, candidate_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(MemoryGql::from(rejected))
    }

    /// Merge a pending candidate into an active memory (`aplan inbox merge`):
    /// same fact, better wording. ONE row survives — this ERASES history. Use
    /// `supersedeMemory` when the fact itself changed.
    ///
    /// Both arguments take a full UUID or a short reference, and BOTH are resolved
    /// before the merge is applied.
    async fn merge_memory(
        &self,
        ctx: &Context<'_>,
        id: ID,
        into: ID,
    ) -> Result<MergeMemoryResultGql> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn MemoryRepository>>()?;
        let (candidate_id, target_id) =
            memory_uc::resolve_memory_id_pair(repo.as_ref(), user_id, &id, &into)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let outcome =
            memory_uc::merge_candidate(repo.as_ref(), user_id, candidate_id, target_id)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(MergeMemoryResultGql::from(outcome))
    }

    /// Supersede an active memory by another (`aplan inbox supersede`,
    /// `aplan memory supersede`): the fact CHANGED. BOTH rows survive, the old
    /// one carrying `invalidatedAt` and `supersededBy`.
    ///
    /// This is the ONLY path that writes `invalidatedAt`.
    ///
    /// Both arguments take a full UUID or a short reference, and BOTH are resolved
    /// before anything is written: half a supersession would either hide a fact
    /// with no successor, or leave two contradictory truths active.
    ///
    /// `old` may be omitted, in which case it falls back to `by`'s
    /// `proposedSupersedes` — the claim a consolidation run recorded. Omitting it on
    /// a memory that proposes nothing is a refused precondition, never a
    /// supersession of nothing.
    async fn supersede_memory(
        &self,
        ctx: &Context<'_>,
        old: Option<ID>,
        by: ID,
    ) -> Result<SupersedeMemoryResultGql> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn MemoryRepository>>()?;
        let (old_id, successor_id) = match &old {
            Some(old) => memory_uc::resolve_memory_id_pair(repo.as_ref(), user_id, old, &by)
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?,
            None => {
                // Still both ids before any write: reading the claim off the
                // candidate is a lookup, not a mutation.
                let successor_id = resolve_memory_arg(repo.as_ref(), user_id, &by).await?;
                let old_id =
                    memory_uc::proposed_supersession_target(repo.as_ref(), user_id, successor_id)
                        .await
                        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
                (old_id, successor_id)
            }
        };
        let outcome = memory_uc::supersede_memory(
            repo.as_ref(),
            user_id,
            old_id,
            successor_id,
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(SupersedeMemoryResultGql::from(outcome))
    }

    /// One-shot import of the harness memory files (`aplan memory import <dir>`).
    ///
    /// Idempotent: a file already imported is skipped, so re-running imports
    /// nothing. Read-only — aplan never writes into that directory.
    async fn import_memories(
        &self,
        ctx: &Context<'_>,
        directory: String,
    ) -> Result<MemoryImportResultGql> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn MemoryRepository>>()?;
        let source = ctx.data::<Arc<dyn MemoryFileSource>>()?;
        let outcome = memory_uc::import_memories(
            repo.as_ref(),
            source.as_ref(),
            user_id,
            &directory,
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(MemoryImportResultGql::from(outcome))
    }

    /// Stamp `consolidated_at` on the worklog entries a consolidation run has
    /// finished with (`aplan consolidate mark`).
    ///
    /// **Call this only after the memories those entries produced are persisted**
    /// (§6.2). A duplicate memory is recoverable — the rejection tombstones stop it
    /// coming back — whereas an entry marked and never turned into anything is lost
    /// for good.
    ///
    /// Idempotent: an id already consolidated, or belonging to another user, moves
    /// no row and is not an error, so a retry after a crash converges. `marked` is
    /// therefore allowed to be lower than `requested`.
    async fn mark_worklog_entries_consolidated(
        &self,
        ctx: &Context<'_>,
        ids: Vec<ID>,
    ) -> Result<MarkConsolidatedResultGql> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let mut parsed = Vec::with_capacity(ids.len());
        for id in &ids {
            parsed.push(Uuid::parse_str(id).map_err(|e| {
                async_graphql::Error::new(format!(
                    "Invalid worklog entry ID `{}`: {e}",
                    id.as_str()
                ))
            })?);
        }
        let outcome = consolidation_uc::mark_entries_consolidated(
            repo.as_ref(),
            user_id,
            &parsed,
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(MarkConsolidatedResultGql::from(outcome))
    }

    /// Record that a consolidation run happened (`aplan consolidate record-run`).
    ///
    /// Writes `memory.consolidation.last_run` into `configuration` — the very key
    /// `aplan brief` reads to render "Dernière consolidation : …". `sync_status`
    /// cannot carry it: its `source` column is under a closed `CHECK`.
    ///
    /// `at` defaults to now; it is settable so a run that finished at 17:30 can
    /// stamp its own start rather than the clock of whichever call came last.
    async fn record_consolidation_run(
        &self,
        ctx: &Context<'_>,
        at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<ConsolidationRunGql> {
        let user_id = *ctx.data::<UserId>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        let ran_at = consolidation_uc::record_consolidation_run(
            config_repo.as_ref(),
            user_id,
            at.unwrap_or_else(chrono::Utc::now),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(ConsolidationRunGql {
            key: consolidation_uc::CONSOLIDATION_LAST_RUN_KEY.to_string(),
            ran_at,
        })
    }

    // ─── Break-rule mutations (Task 8) ───

    /// Create a new break rule.
    async fn create_break_rule(
        &self,
        ctx: &Context<'_>,
        input: BreakRuleInput,
    ) -> Result<BreakRuleGql> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn BreakRuleRepository>>()?;
        let cadence = input.to_cadence().map_err(async_graphql::Error::new)?;
        let duration_seconds = input
            .validated_duration_seconds()
            .map_err(async_graphql::Error::new)?;
        let now = chrono::Utc::now();
        let rule = BreakRule {
            id: Uuid::new_v4(),
            user_id,
            kind: input.kind.into(),
            label: input.label,
            body: input.body,
            cadence,
            duration_seconds,
            priority: input.priority,
            enabled: input.enabled,
            urgency: input.urgency.into(),
            created_at: now,
            updated_at: now,
        };
        repo.create(&rule)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(BreakRuleGql::from(rule))
    }

    /// Update an existing break rule. `createdAt` is carried over from the row it
    /// replaces; `updatedAt` is stamped with the current instant.
    async fn update_break_rule(
        &self,
        ctx: &Context<'_>,
        id: ID,
        input: BreakRuleInput,
    ) -> Result<BreakRuleGql> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn BreakRuleRepository>>()?;
        let rule_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid break rule ID: {}", e)))?;
        let existing = repo
            .get(user_id, rule_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .ok_or_else(|| async_graphql::Error::new("Break rule not found"))?;
        let cadence = input.to_cadence().map_err(async_graphql::Error::new)?;
        let duration_seconds = input
            .validated_duration_seconds()
            .map_err(async_graphql::Error::new)?;
        let rule = BreakRule {
            id: rule_id,
            user_id,
            kind: input.kind.into(),
            label: input.label,
            body: input.body,
            cadence,
            duration_seconds,
            priority: input.priority,
            enabled: input.enabled,
            urgency: input.urgency.into(),
            created_at: existing.created_at,
            updated_at: chrono::Utc::now(),
        };
        repo.update(&rule)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(BreakRuleGql::from(rule))
    }

    /// Delete a break rule by ID. Returns true on success.
    async fn delete_break_rule(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn BreakRuleRepository>>()?;
        let rule_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid break rule ID: {}", e)))?;
        repo.delete(user_id, rule_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(true)
    }

    /// Cut the running break short — the overlay's single control, "J'y retourne".
    /// `abandoned`, not `taken`: the break was opened and did not reach its deadline.
    ///
    /// Idempotent, and the idempotence settles a race rather than being a nicety. The
    /// countdown can run out in the very second the button is pressed, and the tick's
    /// `taken` write has to win: it is the one that saw the deadline pass. Which is
    /// why the whole decision is delegated to `abandon_if_running` and made *in the
    /// write* — reading the row here and updating it after would be a check-then-act,
    /// and the tick's write, landing between the two, would be overwritten by this
    /// one. `false` therefore means "the break was not running any more", which covers
    /// a second press, a press on a break that has since ended, and a press the
    /// deadline beat by a hundredth of a second, all identically.
    async fn end_break(&self, ctx: &Context<'_>, event_id: ID) -> Result<bool> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn BreakEventRepository>>()?;
        let event_id = Uuid::parse_str(&event_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid break event ID: {}", e)))?;

        repo.abandon_if_running(user_id, event_id, chrono::Utc::now())
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))
    }
}

/// Resolve a memory-reference argument into a concrete id, BEFORE any write.
///
/// Delegates to the one resolver the read path uses, so a mutation accepts
/// exactly what `memory(id:)` accepts: a full UUID, or the short reference the
/// brief renders. Unknown reports "Not found", ambiguous reports every candidate
/// — the CLI turns those two messages into exit codes 2 and 3.
async fn resolve_memory_arg(repo: &dyn MemoryRepository, user_id: UserId, id: &ID) -> Result<Uuid> {
    memory_uc::resolve_memory_id(repo, user_id, id.as_str())
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))
}

// ─── Recurrence conversion helpers ───────────────────────────────────────────

fn convert_create_recurring_input(
    user_id: UserId,
    input: CreateRecurringTaskInput,
) -> async_graphql::Result<recurrence_uc::CreateRecurringTaskInput> {
    let urgency: UrgencyLevel = input.urgency.into();
    let impact: ImpactLevel = input.impact.into();

    let project_id = match input.project_id {
        Some(id) => Some(
            Uuid::parse_str(&id)
                .map_err(|e| async_graphql::Error::new(format!("Invalid project ID: {e}")))?,
        ),
        None => None,
    };

    let tag_ids: Vec<Uuid> = match input.tag_ids {
        Some(ids) => ids
            .into_iter()
            .map(|id| {
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid tag ID: {e}")))
            })
            .collect::<async_graphql::Result<Vec<_>>>()?,
        None => vec![],
    };

    if let Some(max) = input.max_occurrences {
        if max < 1 {
            return Err(async_graphql::Error::new("max_occurrences must be >= 1"));
        }
    }

    if let (Some(ends_on), starts_on) = (input.ends_on, input.starts_on) {
        if ends_on < starts_on {
            return Err(async_graphql::Error::new("ends_on must not be before starts_on"));
        }
    }

    let rule = input.rule.try_into_domain()?;

    Ok(recurrence_uc::CreateRecurringTaskInput {
        user_id,
        title: input.title,
        description: input.description,
        notes: input.notes,
        project_id,
        urgency,
        impact,
        estimated_hours: input.estimated_hours,
        tag_ids,
        rule,
        starts_on: input.starts_on,
        ends_on: input.ends_on,
        max_occurrences: input.max_occurrences.map(|n| n as u32),
    })
}

fn convert_update_recurring_input(
    input: UpdateRecurringTaskInput,
) -> async_graphql::Result<recurrence_uc::UpdateRecurringTaskInput> {
    let urgency: Option<UrgencyLevel> = input.urgency.map(|u| u.into());
    let impact: Option<ImpactLevel> = input.impact.map(|i| i.into());

    let project_id: Option<Option<Uuid>> = match input.project_id {
        Some(Some(id)) => Some(Some(
            Uuid::parse_str(&id)
                .map_err(|e| async_graphql::Error::new(format!("Invalid project ID: {e}")))?,
        )),
        Some(None) => Some(None),
        None => None,
    };

    let tag_ids: Option<Vec<Uuid>> = match input.tag_ids {
        Some(ids) => Some(
            ids.into_iter()
                .map(|id| {
                    Uuid::parse_str(&id)
                        .map_err(|e| async_graphql::Error::new(format!("Invalid tag ID: {e}")))
                })
                .collect::<async_graphql::Result<Vec<_>>>()?,
        ),
        None => None,
    };

    if let Some(Some(max)) = input.max_occurrences {
        if max < 1 {
            return Err(async_graphql::Error::new("max_occurrences must be >= 1"));
        }
    }

    let rule = match input.rule {
        Some(r) => Some(r.try_into_domain()?),
        None => None,
    };

    Ok(recurrence_uc::UpdateRecurringTaskInput {
        title: input.title,
        description: input.description,
        notes: input.notes,
        project_id,
        urgency,
        impact,
        estimated_hours: input.estimated_hours,
        tag_ids,
        rule,
        starts_on: input.starts_on,
        ends_on: input.ends_on,
        max_occurrences: input.max_occurrences.map(|opt| opt.map(|n| n as u32)),
    })
}

/// Convert GraphQL CreateTaskInput to application layer input.
fn convert_create_input(
    input: CreateTaskInput,
) -> Result<task_management::CreateTaskInput> {
    let project_id = match input.project_id {
        Some(id) => Some(
            Uuid::parse_str(&id)
                .map_err(|e| async_graphql::Error::new(format!("Invalid project ID: {}", e)))?,
        ),
        None => None,
    };

    let tag_ids: Vec<Uuid> = match input.tag_ids {
        Some(ids) => ids
            .into_iter()
            .map(|id| {
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid tag ID: {}", e)))
            })
            .collect::<Result<Vec<_>>>()?,
        None => vec![],
    };

    Ok(task_management::CreateTaskInput {
        title: input.title,
        description: input.description,
        notes: input.notes,
        project_id,
        deadline: input.deadline,
        planned_start: input.planned_start,
        planned_end: input.planned_end,
        estimated_hours: input.estimated_hours.map(|h| h as f32),
        impact: input.impact.map(|i| i.into()),
        urgency: input.urgency.map(|u| u.into()),
        tags: tag_ids,
    })
}

/// Convert GraphQL UpdateTaskInput to application layer input.
fn convert_update_input(
    input: UpdateTaskInput,
) -> Result<task_management::UpdateTaskInput> {
    let project_id = match input.project_id {
        Some(id) => Some(Some(
            Uuid::parse_str(&id)
                .map_err(|e| async_graphql::Error::new(format!("Invalid project ID: {}", e)))?,
        )),
        None => None,
    };

    let tag_ids = match input.tag_ids {
        Some(ids) => Some(
            ids.into_iter()
                .map(|id| {
                    Uuid::parse_str(&id)
                        .map_err(|e| async_graphql::Error::new(format!("Invalid tag ID: {}", e)))
                })
                .collect::<Result<Vec<_>>>()?,
        ),
        None => None,
    };

    Ok(task_management::UpdateTaskInput {
        title: input.title,
        description: input.description.map(Some),
        notes: input.notes.map(Some),
        project_id,
        deadline: match input.deadline {
            MaybeUndefined::Value(d) => Some(Some(d)),
            MaybeUndefined::Null      => Some(None),
            MaybeUndefined::Undefined => None,
        },
        planned_start: match input.planned_start {
            MaybeUndefined::Value(dt) => Some(Some(dt)),
            MaybeUndefined::Null      => Some(None),
            MaybeUndefined::Undefined => None,
        },
        planned_end: input.planned_end.map(Some),
        estimated_hours: input.estimated_hours.map(|h| Some(h as f32)),
        status: input.status.map(|s| s.into()),
        impact: input.impact.map(|i| i.into()),
        urgency: input.urgency.map(|u| u.into()),
        tags: tag_ids,
        remaining_hours_override: match input.remaining_hours_override {
            Some(Some(h)) => Some(Some(h as f32)),
            Some(None) => Some(None),
            None => None,
        },
        estimated_hours_override: match input.estimated_hours_override {
            Some(Some(h)) => Some(Some(h as f32)),
            Some(None) => Some(None),
            None => None,
        },
        delegated_to: match input.delegated_to {
            async_graphql::MaybeUndefined::Value(name) => Some(Some(name)),
            async_graphql::MaybeUndefined::Null => Some(None),
            async_graphql::MaybeUndefined::Undefined => None,
        },
    })
}
