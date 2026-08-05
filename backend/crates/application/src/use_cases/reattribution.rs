//! Réattribution: move worklog entries — and the activity time derived from them —
//! from one task to another.
//!
//! ## Why the slots are re-derived rather than re-pointed
//!
//! `activity_slots` are a projection of worklog timestamps: one slot per stretch of
//! work inside a (local day, half-day), spanning that stretch's earliest to latest
//! entry ([`domain::rules::worklog_time::derive_time_blocks`]). Rewriting `task_id` on
//! the existing slots would be wrong for any selection that is not a whole half-day:
//! a slot carries the span of *several* entries, so a partial move would hand the
//! destination time that never moved and leave the source with none of the time that
//! stayed.
//!
//! So the correction moves the entries, drops the stale projection for the two tasks
//! **in the affected half-days only**, and rebuilds it from what the entries now say.
//! The result is exactly the slots a flush would have written had the entries been
//! logged on the right task in the first place.
//!
//! ## Why that cannot double-count
//!
//! - Deletion and rebuild are scoped to the *same* set: (user, task ∈ {source,
//!   destination}, half-day ∈ the half-days a moved entry falls in). A third task's
//!   slot on that half-day is never read and never written, and a morning is left
//!   alone when only the afternoon moved.
//! - The rebuild derives each task's slots from that task's own entries, so the
//!   half-day ends up carrying exactly the stretches those entries document — however
//!   many that is. Dropping the old projection first is what makes that exact: without
//!   it, a destination that already had a slot that morning would keep it *and* gain
//!   the rebuilt one, and the same morning would be billed twice.
//! - Only a slot that is both **closed** and **owned by the worklog projection** is
//!   replaced ([`domain::rules::reattribution::is_rebuildable`]). An open slot is a
//!   running timer: it holds no hours yet and stopping it is not this verb's
//!   business. A slot the projection does not own — a hand-made entry, a live
//!   timer's leftover, a row whose provenance the one-shot classification could not
//!   establish — was not derived from these entries in the first place, so rewriting
//!   it from them would destroy time no entry can reproduce.
//!
//! ## What it deliberately does not promise
//!
//! The pair's total hours in an affected half-day can move, in two ways, and both are
//! reported rather than hidden:
//!
//! - A *partial* move re-spans both sides: two entries ten minutes apart account for
//!   ten minutes on one task, and for two single-timestamp minima once split.
//! - A half-day carrying slots the entries do not account for — a leftover from a
//!   flush whose entries were since edited, or a flush that predates the gap rule and
//!   charged a whole afternoon as one stretch — is canonicalised to what the entries
//!   now say. Keeping the extra slot instead is not an option: it sits in the half-day
//!   the rebuild writes to, so it *is* the double count.
//!
//! Which is why nothing is written without `confirm`, and why the outcome carries the
//! before/after hours of both tasks.

use std::collections::HashMap;

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use domain::rules::reattribution::{
    plan_reattribution, slot_hours, AffectedHalfDay, EntryAttribution, ReattributionRefusal,
};
use domain::rules::workload::half_day_of;
use domain::rules::worklog_time::{derive_time_blocks, total_block_hours};
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::{
    ActivitySlotRepository, ConfigRepository, WorklogFilter, WorklogRepository,
    WORKLOG_FILTER_MAX_LIMIT,
};
use crate::use_cases::worklog::{apply_task_projection, plan_task_projection, user_timezone};

/// How many candidates an entry-reference lookup pulls before reporting a
/// collision. Mirrors the memory resolver: enough to name the ambiguity, never
/// enough to flood a terminal.
const ENTRY_MATCH_LIMIT: u32 = 10;

/// How many candidates an ambiguity message lists before it stops.
const AMBIGUITY_LISTED: usize = 5;

/// What the caller asks for. One of the two selections is used: explicit entry
/// references, or the source task's entries over a local date window.
#[derive(Debug, Clone)]
pub struct ReattributionRequest {
    pub from_task: TaskId,
    pub to_task: TaskId,
    /// Entry references: full UUIDs or id prefixes, as printed by `aplan journal`
    /// and `aplan consolidate pending`.
    pub entry_refs: Vec<String>,
    /// First local day of the window (inclusive).
    pub since: Option<NaiveDate>,
    /// Last local day of the window (inclusive). Defaults to `since`.
    pub until: Option<NaiveDate>,
    /// Write. Without it nothing is persisted and the outcome is a prediction.
    pub confirm: bool,
}

/// What one task's hours on the affected days were, and what they become.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaskTimeChange {
    pub task_id: TaskId,
    pub hours_before: f64,
    pub hours_after: f64,
}

/// The report a correction owes its operator: what moved, which days it touched,
/// and the hours on both sides before and after.
#[derive(Debug, Clone, PartialEq)]
pub struct ReattributionOutcome {
    /// `false` for a dry run, which wrote nothing at all.
    pub applied: bool,
    /// The entries the selection resolved to.
    pub selected_entries: Vec<WorklogEntryId>,
    /// How many rows actually moved. `0` on a dry run; below
    /// `selected_entries.len()` only if a row left the source task concurrently.
    pub moved_entries: u64,
    /// The local days whose slots were (or would be) rebuilt.
    pub affected_dates: Vec<NaiveDate>,
    /// Closed slots of the two tasks dropped from those days.
    pub slots_discarded: u32,
    /// Slots written back from the entries.
    pub slots_rebuilt: u32,
    pub source: TaskTimeChange,
    pub destination: TaskTimeChange,
}

impl ReattributionOutcome {
    /// Hours the two tasks account for on the affected days, before and after.
    ///
    /// Printed side by side on purpose: a partial move legitimately changes this
    /// total, and the operator is the one who decides whether that is what they
    /// meant.
    pub fn pair_hours(&self) -> (f64, f64) {
        (
            self.source.hours_before + self.destination.hours_before,
            self.source.hours_after + self.destination.hours_after,
        )
    }
}

/// Move worklog entries from one task to another and rebuild the affected days'
/// activity slots.
///
/// Writes nothing unless `request.confirm` is set; the dry run reports the same
/// figures the apply would produce, computed from the same projection.
pub async fn reattribute_worklog_entries(
    worklog_repo: &dyn WorklogRepository,
    activity_repo: &dyn ActivitySlotRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    request: ReattributionRequest,
    now: DateTime<Utc>,
) -> Result<ReattributionOutcome, AppError> {
    // Before any lookup: moving time onto the task it is already on is a typo, and
    // reporting it as "nothing to move" would hide it.
    if request.from_task == request.to_task {
        return Err(refused(ReattributionRefusal::SameTask));
    }
    let by_reference = !request.entry_refs.is_empty();
    let by_window = request.since.is_some() || request.until.is_some();
    if by_reference && by_window {
        return Err(AppError::Validation(
            "choose either explicit entries or a date window, not both: \
             two selections would leave it unclear what was corrected"
                .into(),
        ));
    }
    if !by_reference && !by_window {
        return Err(AppError::Validation(
            "nothing selected: name the entries to move, or the day (or range) to move"
                .into(),
        ));
    }

    let tz = user_timezone(config_repo, user_id).await?;

    // ── Selection ────────────────────────────────────────────────────────────
    let selected: Vec<WorklogEntry> = if by_reference {
        let mut entries = Vec::with_capacity(request.entry_refs.len());
        for token in &request.entry_refs {
            entries.push(resolve_worklog_entry(worklog_repo, user_id, token).await?);
        }
        entries
    } else {
        let since = request
            .since
            .ok_or_else(|| AppError::Validation("a start date is required".into()))?;
        let until = request.until.unwrap_or(since);
        if until < since {
            return Err(AppError::Validation(format!(
                "the range ends before it starts ({since} → {until})"
            )));
        }
        let (from_utc, to_utc) = local_window(&tz, since, until);
        let page = worklog_repo
            .list(
                user_id,
                &WorklogFilter {
                    task_ids: Some(vec![request.from_task]),
                    from: Some(from_utc),
                    to: Some(to_utc),
                    limit: WORKLOG_FILTER_MAX_LIMIT,
                    offset: 0,
                },
            )
            .await?;
        refuse_a_truncated_page(page, "the selected window")?
    };

    let attributions: Vec<EntryAttribution> = selected
        .iter()
        .map(|entry| EntryAttribution {
            id: entry.id,
            task_id: entry.task_id,
            local_logged_at: to_local(&tz, entry.logged_at),
        })
        .collect();

    let plan = plan_reattribution(request.from_task, request.to_task, &attributions)
        .map_err(refused)?;

    let tasks = [request.from_task, request.to_task];
    let affected_dates = plan.affected_dates();

    // ── The half-days' full picture, for both tasks ──────────────────────────
    let day_entries = read_affected_half_days(
        worklog_repo,
        &tz,
        user_id,
        &tasks,
        &plan.affected_half_days,
    )
    .await?;

    // ── Hours before, from the projection currently persisted ────────────────
    //
    // One `plan_task_projection` call per task, sharing the same arithmetic the
    // apply below uses: its `delete` field does not depend on the worklog at all,
    // only on the activity slots that already exist, so calling it here — before
    // any entry has moved — reads the same slots the apply's own call will find
    // once it runs.
    let mut existing = Vec::new();
    for task in tasks {
        let rebuild = plan_task_projection(
            activity_repo,
            worklog_repo,
            user_id,
            task,
            &plan.affected_half_days,
            tz,
            now,
        )
        .await?;
        existing.extend(rebuild.delete);
    }
    let hours_before = |task: TaskId| {
        slot_hours(
            &existing
                .iter()
                .filter(|slot| slot.task_id == Some(task))
                .cloned()
                .collect::<Vec<_>>(),
        )
    };
    let source_before = hours_before(request.from_task);
    let destination_before = hours_before(request.to_task);

    if !request.confirm {
        // Predict, writing nothing: partition the half-days' entries as the move
        // would leave them, and project each side.
        let moved: Vec<WorklogEntryId> = plan.entry_ids.clone();
        let stays_on_source = |entry: &&LocalEntry| {
            entry.task_id == request.from_task && !moved.contains(&entry.id)
        };
        let lands_on_destination = |entry: &&LocalEntry| {
            entry.task_id == request.to_task || moved.contains(&entry.id)
        };
        let source_blocks = derive_time_blocks(&local_times(&day_entries, stays_on_source));
        let destination_blocks =
            derive_time_blocks(&local_times(&day_entries, lands_on_destination));

        return Ok(ReattributionOutcome {
            applied: false,
            selected_entries: plan.entry_ids,
            moved_entries: 0,
            affected_dates,
            slots_discarded: existing.len() as u32,
            slots_rebuilt: (source_blocks.len() + destination_blocks.len()) as u32,
            source: TaskTimeChange {
                task_id: request.from_task,
                hours_before: source_before,
                hours_after: total_block_hours(&source_blocks),
            },
            destination: TaskTimeChange {
                task_id: request.to_task,
                hours_before: destination_before,
                hours_after: total_block_hours(&destination_blocks),
            },
        });
    }

    // ── Apply ────────────────────────────────────────────────────────────────
    let moved_entries = worklog_repo
        .reassign_task(
            user_id,
            &plan.entry_ids,
            request.from_task,
            request.to_task,
            now,
        )
        .await?;

    // Drop the stale projection of these two tasks in these half-days, and rebuild
    // it from what the entries now say — re-read rather than assumed, so the hours
    // reported are measured, not predicted. One `plan_task_projection` call per
    // task, now that the move above has landed: its `delete` finds the same slots
    // `existing` already named (activity_slots did not change), and its `write` is
    // this task's entries as they now stand.
    let mut slots_rebuilt = 0u32;
    let mut written: Vec<ActivitySlot> = Vec::new();
    for task in tasks {
        let rebuild = plan_task_projection(
            activity_repo,
            worklog_repo,
            user_id,
            task,
            &plan.affected_half_days,
            tz,
            now,
        )
        .await?;
        apply_task_projection(activity_repo, &rebuild).await?;
        slots_rebuilt += rebuild.write.len() as u32;
        written.extend(rebuild.write);
    }
    let hours_after = |task: TaskId| {
        slot_hours(
            &written
                .iter()
                .filter(|slot| slot.task_id == Some(task))
                .cloned()
                .collect::<Vec<_>>(),
        )
    };

    Ok(ReattributionOutcome {
        applied: true,
        selected_entries: plan.entry_ids,
        moved_entries,
        affected_dates,
        slots_discarded: existing.len() as u32,
        slots_rebuilt,
        source: TaskTimeChange {
            task_id: request.from_task,
            hours_before: source_before,
            hours_after: hours_after(request.from_task),
        },
        destination: TaskTimeChange {
            task_id: request.to_task,
            hours_before: destination_before,
            hours_after: hours_after(request.to_task),
        },
    })
}

/// Resolve one entry reference — a full UUID or an id prefix — into exactly one
/// entry.
///
/// The same three outcomes as the memory resolver, and the same reason for
/// refusing the third: a prefix shared by two entries could name two different
/// hours of work, and guessing would move the wrong one.
pub async fn resolve_worklog_entry(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    token: &str,
) -> Result<WorklogEntry, AppError> {
    let prefix = domain::rules::brief::parse_id_reference(token)
        .ok_or_else(|| AppError::NotFound(format!("worklog entry `{token}`")))?;

    if let Ok(id) = Uuid::parse_str(&prefix) {
        return worklog_repo
            .find_by_id(id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("worklog entry `{token}`")));
    }

    let mut matches = worklog_repo
        .find_by_id_prefix(user_id, &prefix, ENTRY_MATCH_LIMIT)
        .await?;
    match matches.len() {
        0 => Err(AppError::NotFound(format!("worklog entry `{token}`"))),
        1 => Ok(matches.remove(0)),
        _ => Err(AppError::Ambiguous(describe_ambiguous_entry(
            token, &matches,
        ))),
    }
}

/// One wording for "this reference matches several entries", ids in full so the
/// operator can copy one back.
fn describe_ambiguous_entry(token: &str, candidates: &[WorklogEntry]) -> String {
    let listed = candidates
        .iter()
        .take(AMBIGUITY_LISTED)
        .map(|entry| {
            format!(
                "  - {} {} {}",
                entry.id,
                entry.logged_at.to_rfc3339(),
                first_line(&entry.body)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Ambiguous worklog entry reference `{token}`: {} matches; please add more characters\n{listed}",
        candidates.len()
    )
}

fn first_line(body: &str) -> &str {
    body.lines().next().unwrap_or("").trim()
}

/// A refusal is a precondition the store will not leave, never a missing row: the
/// CLI turns this into exit 4.
fn refused(refusal: ReattributionRefusal) -> AppError {
    AppError::Validation(refusal.to_string())
}

/// An entry with its local wall-clock already resolved, so the timezone is read
/// once per call.
#[derive(Debug, Clone)]
struct LocalEntry {
    id: WorklogEntryId,
    task_id: TaskId,
    local_logged_at: NaiveDateTime,
}

/// Every entry of `tasks` that falls in one of `units`, in local terms.
///
/// Read as one window over the whole span and filtered back to the affected
/// half-days: a sparse selection (three entries, three weeks apart) must not rebuild
/// the days in between, and an afternoon correction must not pull in the morning.
async fn read_affected_half_days(
    worklog_repo: &dyn WorklogRepository,
    tz: &Tz,
    user_id: UserId,
    tasks: &[TaskId],
    units: &[AffectedHalfDay],
) -> Result<Vec<LocalEntry>, AppError> {
    let dates: Vec<NaiveDate> = units.iter().map(|unit| unit.date).collect();
    let (Some(first), Some(last)) = (dates.first(), dates.last()) else {
        return Ok(vec![]);
    };
    let (from_utc, to_utc) = local_window(tz, *first, *last);
    let page = worklog_repo
        .list(
            user_id,
            &WorklogFilter {
                task_ids: Some(tasks.to_vec()),
                from: Some(from_utc),
                to: Some(to_utc),
                limit: WORKLOG_FILTER_MAX_LIMIT,
                offset: 0,
            },
        )
        .await?;
    let page = refuse_a_truncated_page(page, "the affected half-days")?;

    Ok(page
        .into_iter()
        .map(|entry| LocalEntry {
            id: entry.id,
            task_id: entry.task_id,
            local_logged_at: to_local(tz, entry.logged_at),
        })
        .filter(|entry| covers(units, entry.local_logged_at.date(), local_half_day(entry)))
        .collect())
}

/// The half-day an entry's local timestamp falls in, by the projection's own rule.
fn local_half_day(entry: &LocalEntry) -> HalfDay {
    half_day_of(entry.local_logged_at.time().hour())
}

fn covers(units: &[AffectedHalfDay], date: NaiveDate, half_day: HalfDay) -> bool {
    units
        .iter()
        .any(|unit| unit.date == date && unit.half_day == half_day)
}

/// Local wall-clock timestamps of the entries a predicate keeps.
fn local_times(
    entries: &[LocalEntry],
    keep: impl Fn(&&LocalEntry) -> bool,
) -> Vec<NaiveDateTime> {
    entries
        .iter()
        .filter(keep)
        .map(|entry| entry.local_logged_at)
        .collect()
}

/// A page that came back full is a page that may have been cut, and a correction
/// that silently moved the first 1 000 entries of a month would be worse than one
/// that refused.
///
/// `pub(crate)`: [`crate::use_cases::slot_classification`] reads the same
/// `WorklogRepository::list` page cap and needs the identical guard, not a second
/// copy of it.
pub(crate) fn refuse_a_truncated_page(
    entries: Vec<WorklogEntry>,
    what: &str,
) -> Result<Vec<WorklogEntry>, AppError> {
    if entries.len() as u32 >= WORKLOG_FILTER_MAX_LIMIT {
        return Err(AppError::Validation(format!(
            "{what} holds at least {WORKLOG_FILTER_MAX_LIMIT} worklog entries, \
             which is the page cap: narrow the range and correct it in several passes"
        )));
    }
    Ok(entries)
}

/// UTC half-open window `[start of `since`, start of the day after `until`)`,
/// matching the repository's `logged_at >= from AND logged_at < to`.
///
/// `pub(crate)`: [`crate::use_cases::worklog::plan_task_projection`] needs the same
/// local-day-to-UTC conversion to scope its own worklog read, not a second copy of
/// it — two implementations of "which UTC instants a local day spans" could
/// disagree, and a disagreement there puts one entry on two different local days.
pub(crate) fn local_window(
    tz: &Tz,
    since: NaiveDate,
    until: NaiveDate,
) -> (DateTime<Utc>, DateTime<Utc>) {
    (
        local_day_start(tz, since),
        local_day_start(tz, until.succ_opt().unwrap_or(until)),
    )
}

/// The instant a local day begins, in UTC.
///
/// A local midnight that does not exist — a zone whose DST jump lands on 00:00 —
/// is walked forward to the first instant that does, rather than dropped: a window
/// that silently started at the wrong instant would select the wrong day.
fn local_day_start(tz: &Tz, date: NaiveDate) -> DateTime<Utc> {
    let midnight = date.and_time(NaiveTime::MIN);
    if let Some(local) = tz.from_local_datetime(&midnight).earliest() {
        return local.with_timezone(&Utc);
    }
    tz.from_local_datetime(&(midnight + Duration::hours(1)))
        .earliest()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&midnight))
}

fn to_local(tz: &Tz, at: DateTime<Utc>) -> NaiveDateTime {
    tz.from_utc_datetime(&at.naive_utc()).naive_local()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use domain::types::recurrence::RecurrenceTemplateId;
    use std::sync::Mutex;

    use crate::errors::RepositoryError;

    // ─── Doubles ─────────────────────────────────────────────────────────────

    #[derive(Default)]
    struct FakeWorklogRepo {
        entries: Mutex<Vec<WorklogEntry>>,
        reassign_calls: Mutex<Vec<(Vec<WorklogEntryId>, TaskId, TaskId)>>,
    }

    impl FakeWorklogRepo {
        fn push(&self, entry: WorklogEntry) {
            self.entries.lock().expect("lock").push(entry);
        }
        fn task_of(&self, id: WorklogEntryId) -> Option<TaskId> {
            self.entries
                .lock()
                .expect("lock")
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.task_id)
        }
    }

    #[async_trait]
    impl WorklogRepository for FakeWorklogRepo {
        async fn create(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
            self.push(entry.clone());
            Ok(())
        }
        async fn update(&self, _entry: &WorklogEntry) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn delete(
            &self,
            _id: WorklogEntryId,
            _user_id: UserId,
        ) -> Result<bool, RepositoryError> {
            Ok(false)
        }
        async fn find_by_id(
            &self,
            id: WorklogEntryId,
            user_id: UserId,
        ) -> Result<Option<WorklogEntry>, RepositoryError> {
            Ok(self
                .entries
                .lock()
                .expect("lock")
                .iter()
                .find(|e| e.id == id && e.user_id == user_id)
                .cloned())
        }
        async fn find_by_recurrence(
            &self,
            _user_id: UserId,
            _template_id: RecurrenceTemplateId,
            _limit: u32,
            _offset: u32,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            Ok(vec![])
        }
        async fn list(
            &self,
            user_id: UserId,
            filter: &WorklogFilter,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            let entries = self.entries.lock().expect("lock");
            let mut out: Vec<WorklogEntry> = entries
                .iter()
                .filter(|e| e.user_id == user_id)
                .filter(|e| match &filter.task_ids {
                    Some(ids) => ids.contains(&e.task_id),
                    None => true,
                })
                .filter(|e| filter.from.map(|f| e.logged_at >= f).unwrap_or(true))
                .filter(|e| filter.to.map(|t| e.logged_at < t).unwrap_or(true))
                .cloned()
                .collect();
            out.sort_by(|a, b| b.logged_at.cmp(&a.logged_at));
            out.truncate(filter.effective_limit() as usize);
            Ok(out)
        }
        async fn find_by_id_prefix(
            &self,
            user_id: UserId,
            prefix: &str,
            limit: u32,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            let entries = self.entries.lock().expect("lock");
            let mut out: Vec<WorklogEntry> = entries
                .iter()
                .filter(|e| e.user_id == user_id && e.id.to_string().starts_with(prefix))
                .cloned()
                .collect();
            out.truncate(limit as usize);
            Ok(out)
        }
        async fn reassign_task(
            &self,
            user_id: UserId,
            ids: &[WorklogEntryId],
            from_task: TaskId,
            to_task: TaskId,
            now: DateTime<Utc>,
        ) -> Result<u64, RepositoryError> {
            self.reassign_calls
                .lock()
                .expect("lock")
                .push((ids.to_vec(), from_task, to_task));
            let mut entries = self.entries.lock().expect("lock");
            let mut moved = 0u64;
            for entry in entries.iter_mut() {
                if entry.user_id == user_id
                    && entry.task_id == from_task
                    && ids.contains(&entry.id)
                {
                    entry.task_id = to_task;
                    entry.updated_at = now;
                    moved += 1;
                }
            }
            Ok(moved)
        }
    }

    #[derive(Default)]
    struct FakeActivityRepo {
        slots: Mutex<Vec<ActivitySlot>>,
        deleted: Mutex<Vec<ActivitySlotId>>,
    }

    impl FakeActivityRepo {
        fn push(&self, slot: ActivitySlot) {
            self.slots.lock().expect("lock").push(slot);
        }
        fn of_task(&self, task: TaskId) -> Vec<ActivitySlot> {
            self.slots
                .lock()
                .expect("lock")
                .iter()
                .filter(|s| s.task_id == Some(task))
                .cloned()
                .collect()
        }
        fn on(&self, task: TaskId, date: NaiveDate, half_day: HalfDay) -> Vec<ActivitySlot> {
            self.of_task(task)
                .into_iter()
                .filter(|s| s.date == date && s.half_day == half_day)
                .collect()
        }
        fn ids(&self) -> Vec<ActivitySlotId> {
            self.slots.lock().expect("lock").iter().map(|s| s.id).collect()
        }
    }

    #[async_trait]
    impl ActivitySlotRepository for FakeActivityRepo {
        async fn find_by_id(
            &self,
            id: ActivitySlotId,
        ) -> Result<Option<ActivitySlot>, RepositoryError> {
            Ok(self
                .slots
                .lock()
                .expect("lock")
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
                .expect("lock")
                .iter()
                .filter(|s| s.user_id == user_id && s.date == date)
                .cloned()
                .collect())
        }
        async fn find_active(
            &self,
            _user_id: UserId,
        ) -> Result<Option<ActivitySlot>, RepositoryError> {
            Ok(None)
        }
        async fn find_by_user_and_date_range(
            &self,
            user_id: UserId,
            start_date: NaiveDate,
            end_date: NaiveDate,
        ) -> Result<Vec<ActivitySlot>, RepositoryError> {
            Ok(self
                .slots
                .lock()
                .expect("lock")
                .iter()
                .filter(|s| s.user_id == user_id && s.date >= start_date && s.date <= end_date)
                .cloned()
                .collect())
        }
        async fn save(&self, slot: &ActivitySlot) -> Result<(), RepositoryError> {
            self.push(slot.clone());
            Ok(())
        }
        async fn update(&self, _slot: &ActivitySlot) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn delete(&self, id: ActivitySlotId) -> Result<(), RepositoryError> {
            self.deleted.lock().expect("lock").push(id);
            self.slots.lock().expect("lock").retain(|s| s.id != id);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeConfigRepo {
        map: Mutex<HashMap<String, String>>,
    }

    #[async_trait]
    impl ConfigRepository for FakeConfigRepo {
        async fn get(
            &self,
            _user_id: UserId,
            key: &str,
        ) -> Result<Option<String>, RepositoryError> {
            Ok(self.map.lock().expect("lock").get(key).cloned())
        }
        async fn get_all(
            &self,
            _user_id: UserId,
        ) -> Result<Vec<(String, String)>, RepositoryError> {
            Ok(self
                .map
                .lock()
                .expect("lock")
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }
        async fn set(
            &self,
            _user_id: UserId,
            key: &str,
            value: &str,
        ) -> Result<(), RepositoryError> {
            self.map
                .lock()
                .expect("lock")
                .insert(key.to_string(), value.to_string());
            Ok(())
        }
    }

    // ─── Fixture ─────────────────────────────────────────────────────────────

    /// Paris in August is UTC+2, so a UTC hour reads two hours later locally. The
    /// fixtures are written in UTC and the assertions in local half-days, which is
    /// exactly the conversion the projection has to get right.
    fn utc(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, day, hour, minute, 0)
            .single()
            .expect("valid instant")
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, d).expect("valid date")
    }

    fn now() -> DateTime<Utc> {
        utc(4, 6, 0)
    }

    struct World {
        worklog: FakeWorklogRepo,
        activity: FakeActivityRepo,
        config: FakeConfigRepo,
        user: UserId,
        source: TaskId,
        destination: TaskId,
        elsewhere: TaskId,
    }

    impl World {
        fn new() -> Self {
            Self {
                worklog: FakeWorklogRepo::default(),
                activity: FakeActivityRepo::default(),
                config: FakeConfigRepo::default(),
                user: Uuid::new_v4(),
                source: Uuid::new_v4(),
                destination: Uuid::new_v4(),
                elsewhere: Uuid::new_v4(),
            }
        }

        fn log(&self, task: TaskId, at: DateTime<Utc>) -> WorklogEntryId {
            let entry = WorklogEntry::new(self.user, task, "work".into(), at, at)
                .expect("valid entry");
            let id = entry.id;
            self.worklog.push(entry);
            id
        }

        fn log_for(&self, user: UserId, task: TaskId, at: DateTime<Utc>) -> WorklogEntryId {
            let entry =
                WorklogEntry::new(user, task, "work".into(), at, at).expect("valid entry");
            let id = entry.id;
            self.worklog.push(entry);
            id
        }

        /// An entry under an id the caller chose, so a test that needs two ids to
        /// relate in a particular way — sharing a prefix, say — gets that relation by
        /// construction instead of waiting for randomness to supply it.
        fn log_with_id(
            &self,
            id: WorklogEntryId,
            task: TaskId,
            at: DateTime<Utc>,
        ) -> WorklogEntryId {
            let mut entry = WorklogEntry::new(self.user, task, "work".into(), at, at)
                .expect("valid entry");
            entry.id = id;
            self.worklog.push(entry);
            id
        }

        /// A slot as a flush would have left it — closed, so `Worklog`-sourced. The
        /// one caller that passes `end: None` wants a running timer, which a flush
        /// never produces (`from_worklog` has no way to leave `end_time` open), so
        /// that case is `Manual` instead — the same distinction the constructors
        /// themselves encode.
        fn slot(
            &self,
            task: TaskId,
            start: DateTime<Utc>,
            end: Option<DateTime<Utc>>,
            half_day: HalfDay,
        ) -> ActivitySlotId {
            let source = if end.is_some() {
                SlotSource::Worklog
            } else {
                SlotSource::Manual
            };
            let slot = ActivitySlot {
                id: Uuid::new_v4(),
                user_id: self.user,
                task_id: Some(task),
                start_time: start,
                end_time: end,
                half_day,
                date: to_local(&paris(), start).date(),
                created_at: start,
                session_id: None,
                source,
            };
            let id = slot.id;
            self.activity.push(slot);
            id
        }

        async fn run(
            &self,
            request: ReattributionRequest,
        ) -> Result<ReattributionOutcome, AppError> {
            reattribute_worklog_entries(
                &self.worklog,
                &self.activity,
                &self.config,
                self.user,
                request,
                now(),
            )
            .await
        }

        fn on_day(&self, since: NaiveDate, confirm: bool) -> ReattributionRequest {
            ReattributionRequest {
                from_task: self.source,
                to_task: self.destination,
                entry_refs: vec![],
                since: Some(since),
                until: None,
                confirm,
            }
        }

        fn by_ref(&self, refs: &[String], confirm: bool) -> ReattributionRequest {
            ReattributionRequest {
                from_task: self.source,
                to_task: self.destination,
                entry_refs: refs.to_vec(),
                since: None,
                until: None,
                confirm,
            }
        }
    }

    fn paris() -> Tz {
        "Europe/Paris".parse().expect("known zone")
    }

    fn entry_id(literal: &str) -> WorklogEntryId {
        Uuid::parse_str(literal).expect("valid uuid literal")
    }

    fn hours(slots: &[ActivitySlot]) -> f64 {
        slot_hours(slots)
    }

    // ─── Moving a day ────────────────────────────────────────────────────────

    /// The defect this verb exists for: a day recorded against the wrong task.
    #[tokio::test]
    async fn a_days_entries_and_the_time_derived_from_them_move_together() {
        let w = World::new();
        let morning_a = w.log(w.source, utc(3, 7, 0)); // local 09:00
        let morning_b = w.log(w.source, utc(3, 7, 15)); // local 09:15 — same stretch
        let afternoon = w.log(w.source, utc(3, 12, 0)); // local 14:00
        w.slot(w.source, utc(3, 7, 0), Some(utc(3, 7, 15)), HalfDay::Morning);
        w.slot(
            w.source,
            utc(3, 12, 0),
            Some(utc(3, 12, 1)),
            HalfDay::Afternoon,
        );

        let outcome = w.run(w.on_day(day(3), true)).await.expect("applies");

        assert!(outcome.applied);
        assert_eq!(outcome.moved_entries, 3);
        assert_eq!(outcome.affected_dates, vec![day(3)]);
        for entry in [morning_a, morning_b, afternoon] {
            assert_eq!(w.worklog.task_of(entry), Some(w.destination));
        }
        assert!(
            w.activity.of_task(w.source).is_empty(),
            "the source keeps no slot for a day it no longer has entries on"
        );
        assert_eq!(w.activity.on(w.destination, day(3), HalfDay::Morning).len(), 1);
        assert_eq!(
            w.activity.on(w.destination, day(3), HalfDay::Afternoon).len(),
            1
        );
    }

    /// Double counting, first shape: the destination already worked that morning. The
    /// entries the two tasks end up sharing are one continuous stretch, so the morning
    /// must end up with ONE slot — the alternative is the same morning billed twice.
    #[tokio::test]
    async fn the_destination_does_not_bill_a_shared_morning_twice() {
        let w = World::new();
        w.log(w.destination, utc(3, 6, 30)); // local 08:30, already the destination's
        w.slot(
            w.destination,
            utc(3, 6, 30),
            Some(utc(3, 6, 31)),
            HalfDay::Morning,
        );
        for i in 1..=4 {
            w.log(w.source, utc(3, 6, 30) + Duration::minutes(15 * i)); // 08:45 → 09:30
        }
        w.slot(w.source, utc(3, 6, 45), Some(utc(3, 7, 30)), HalfDay::Morning);

        let outcome = w.run(w.on_day(day(3), true)).await.expect("applies");

        let morning = w.activity.on(w.destination, day(3), HalfDay::Morning);
        assert_eq!(morning.len(), 1, "one stretch, one slot");
        assert_eq!(morning[0].start_time, utc(3, 6, 30));
        assert_eq!(morning[0].end_time, Some(utc(3, 7, 30)));
        assert_eq!(hours(&morning), 1.0);
        assert_eq!(
            outcome.destination.hours_after, 1.0,
            "not 1.0 + the old slot's minute"
        );
    }

    /// Double counting, second shape: another task worked the same half-day. Its
    /// slot is out of scope and must come out untouched, id included.
    #[tokio::test]
    async fn a_third_tasks_slot_on_the_same_half_day_is_never_touched() {
        let w = World::new();
        w.log(w.source, utc(3, 7, 0));
        w.slot(w.source, utc(3, 7, 0), Some(utc(3, 7, 30)), HalfDay::Morning);
        w.log(w.elsewhere, utc(3, 8, 0));
        let untouchable = w.slot(
            w.elsewhere,
            utc(3, 8, 0),
            Some(utc(3, 9, 0)),
            HalfDay::Morning,
        );

        w.run(w.on_day(day(3), true)).await.expect("applies");

        let theirs = w.activity.of_task(w.elsewhere);
        assert_eq!(theirs.len(), 1);
        assert_eq!(theirs[0].id, untouchable, "the slot was not rebuilt");
        assert_eq!(hours(&theirs), 1.0);
        assert!(
            !w.activity
                .deleted
                .lock()
                .expect("lock")
                .contains(&untouchable),
            "a third task's slot must not even be considered for deletion"
        );
    }

    /// A partial move re-spans BOTH sides: the source keeps a slot covering only
    /// what stayed, which is why re-deriving beats re-pointing.
    #[tokio::test]
    async fn a_partial_move_leaves_the_source_a_slot_for_what_stayed() {
        let w = World::new();
        w.log(w.source, utc(3, 7, 0)); // local 09:00 — stays
        w.log(w.source, utc(3, 7, 10)); // local 09:10 — stays
        let moved = w.log(w.source, utc(3, 7, 20)); // local 09:20 — moves
        w.slot(w.source, utc(3, 7, 0), Some(utc(3, 7, 20)), HalfDay::Morning);

        w.run(w.by_ref(&[moved.to_string()], true))
            .await
            .expect("applies");

        let kept = w.activity.on(w.source, day(3), HalfDay::Morning);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].start_time, utc(3, 7, 0));
        assert_eq!(
            kept[0].end_time,
            Some(utc(3, 7, 10)),
            "the source spans the entries that did not move, and no longer the one that did"
        );
        let landed = w.activity.on(w.destination, day(3), HalfDay::Morning);
        assert_eq!(landed.len(), 1);
        assert_eq!(landed[0].start_time, utc(3, 7, 20));
        assert_eq!(landed[0].end_time, Some(utc(3, 7, 21)));
    }

    /// A whole half-day changing hands must not change what the day is worth.
    #[tokio::test]
    async fn moving_a_whole_half_day_conserves_the_pairs_hours() {
        let w = World::new();
        // Local 09:00 → 11:30 at the densest cadence the gap rule reads as one
        // stretch, so the projection is worth the 2h30 the seeded slot claims.
        for i in 0..=10 {
            w.log(w.source, utc(3, 7, 0) + Duration::minutes(15 * i));
        }
        w.slot(w.source, utc(3, 7, 0), Some(utc(3, 9, 30)), HalfDay::Morning);

        let outcome = w.run(w.on_day(day(3), true)).await.expect("applies");

        let (before, after) = outcome.pair_hours();
        assert_eq!(before, 2.5);
        assert_eq!(after, 2.5);
        assert_eq!(outcome.source.hours_after, 0.0);
        assert_eq!(outcome.destination.hours_after, 2.5);
    }

    /// Days the selection never touched keep their projection, ids and all.
    #[tokio::test]
    async fn a_day_outside_the_selection_keeps_its_slots() {
        let w = World::new();
        w.log(w.source, utc(3, 7, 0));
        w.slot(w.source, utc(3, 7, 0), Some(utc(3, 7, 30)), HalfDay::Morning);
        w.log(w.source, utc(5, 7, 0));
        let other_day = w.slot(w.source, utc(5, 7, 0), Some(utc(5, 8, 0)), HalfDay::Morning);

        w.run(w.on_day(day(3), true)).await.expect("applies");

        let surviving = w.activity.of_task(w.source);
        assert_eq!(surviving.len(), 1);
        assert_eq!(surviving[0].id, other_day);
    }

    /// Half-day scoping: an afternoon correction must leave the morning's slot exactly
    /// as it was, id included. The morning belongs to entries that did not move.
    #[tokio::test]
    async fn only_the_half_day_that_moved_is_rebuilt() {
        let w = World::new();
        w.log(w.source, utc(3, 7, 0)); // local 09:00 — morning, stays
        let morning = w.slot(w.source, utc(3, 7, 0), Some(utc(3, 8, 0)), HalfDay::Morning);
        let afternoon_entry = w.log(w.source, utc(3, 12, 0)); // local 14:00
        w.slot(
            w.source,
            utc(3, 12, 0),
            Some(utc(3, 14, 0)),
            HalfDay::Afternoon,
        );

        let outcome = w
            .run(w.by_ref(&[afternoon_entry.to_string()], true))
            .await
            .expect("applies");

        let kept = w.activity.on(w.source, day(3), HalfDay::Morning);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, morning, "the morning slot was not rebuilt");
        assert_eq!(
            outcome.slots_discarded, 1,
            "only the afternoon's slot was in scope"
        );
        assert_eq!(
            outcome.source.hours_before, 2.0,
            "the reported before-hours cover the afternoon only, not the morning"
        );
        assert!(w.activity.on(w.source, day(3), HalfDay::Afternoon).is_empty());
    }

    /// The state the real database is in: several partial flushes left two closed slots
    /// in one afternoon, overlapping what the entries say. The rebuild canonicalises
    /// them to what the entries now project to — here one continuous stretch — and the
    /// outcome reports the hours so the change is visible before it is applied.
    #[tokio::test]
    async fn several_partial_slots_in_one_half_day_are_replaced_by_the_entries_projection() {
        let w = World::new();
        // Local 14:00 → 17:00, uninterrupted under the gap rule.
        for i in 0..=12 {
            w.log(w.source, utc(3, 12, 0) + Duration::minutes(15 * i));
        }
        w.slot(
            w.source,
            utc(3, 12, 0),
            Some(utc(3, 13, 0)),
            HalfDay::Afternoon,
        );
        w.slot(
            w.source,
            utc(3, 13, 30),
            Some(utc(3, 15, 0)),
            HalfDay::Afternoon,
        );

        let preview = w.run(w.on_day(day(3), false)).await.expect("previews");
        assert_eq!(preview.slots_discarded, 2);
        assert_eq!(preview.slots_rebuilt, 1);
        assert_eq!(preview.source.hours_before, 2.5, "1h + 1h30 as persisted");
        assert_eq!(
            preview.destination.hours_after, 3.0,
            "one span from 14:00 to 17:00 local"
        );

        let applied = w.run(w.on_day(day(3), true)).await.expect("applies");
        assert_eq!(
            w.activity.on(w.destination, day(3), HalfDay::Afternoon).len(),
            1
        );
        assert!(w.activity.of_task(w.source).is_empty());
        assert_eq!(applied.destination.hours_after, 3.0);
    }

    // ─── The dry run ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_dry_run_writes_absolutely_nothing() {
        let w = World::new();
        let entry = w.log(w.source, utc(3, 7, 0));
        let existing = w.slot(w.source, utc(3, 7, 0), Some(utc(3, 9, 0)), HalfDay::Morning);

        let outcome = w.run(w.on_day(day(3), false)).await.expect("previews");

        assert!(!outcome.applied);
        assert_eq!(outcome.moved_entries, 0);
        assert_eq!(outcome.selected_entries, vec![entry]);
        assert_eq!(w.worklog.task_of(entry), Some(w.source));
        assert_eq!(w.activity.ids(), vec![existing]);
        assert!(w.worklog.reassign_calls.lock().expect("lock").is_empty());
        assert!(w.activity.deleted.lock().expect("lock").is_empty());
    }

    /// The dry run is only worth reading if it predicts what the apply does. Two
    /// identical worlds, one previewed and one applied, must report the same
    /// figures — otherwise `--confirm` is a leap of faith.
    #[tokio::test]
    async fn a_dry_run_reports_the_same_figures_as_the_apply() {
        async fn world_with(confirm: bool) -> ReattributionOutcome {
            let w = World::new();
            w.log(w.destination, utc(3, 6, 30));
            w.slot(
                w.destination,
                utc(3, 6, 30),
                Some(utc(3, 6, 31)),
                HalfDay::Morning,
            );
            w.log(w.source, utc(3, 7, 0));
            w.log(w.source, utc(3, 9, 30));
            w.log(w.source, utc(3, 12, 0));
            w.slot(w.source, utc(3, 7, 0), Some(utc(3, 9, 30)), HalfDay::Morning);
            w.slot(
                w.source,
                utc(3, 12, 0),
                Some(utc(3, 12, 1)),
                HalfDay::Afternoon,
            );
            w.run(w.on_day(day(3), confirm)).await.expect("runs")
        }

        let preview = world_with(false).await;
        let applied = world_with(true).await;

        assert_eq!(preview.affected_dates, applied.affected_dates);
        assert_eq!(preview.slots_discarded, applied.slots_discarded);
        assert_eq!(preview.slots_rebuilt, applied.slots_rebuilt);
        assert_eq!(preview.source.hours_before, applied.source.hours_before);
        assert_eq!(preview.source.hours_after, applied.source.hours_after);
        assert_eq!(
            preview.destination.hours_before,
            applied.destination.hours_before
        );
        assert_eq!(
            preview.destination.hours_after,
            applied.destination.hours_after
        );
        assert_eq!(preview.selected_entries.len(), 3);
    }

    // ─── Slots that are not a projection ─────────────────────────────────────

    /// An open slot is a running timer. Rebuilding a day must not stop it.
    #[tokio::test]
    async fn an_open_slot_on_an_affected_day_keeps_running() {
        let w = World::new();
        w.log(w.source, utc(3, 7, 0));
        let running = w.slot(w.source, utc(3, 6, 0), None, HalfDay::Morning);

        let outcome = w.run(w.on_day(day(3), true)).await.expect("applies");

        assert!(
            w.activity
                .slots
                .lock()
                .expect("lock")
                .iter()
                .any(|s| s.id == running),
            "the running slot survived"
        );
        assert_eq!(
            outcome.slots_discarded, 0,
            "an open slot is not part of the projection being replaced"
        );
    }

    // ─── Refusals ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn the_same_source_and_destination_is_refused_before_anything_is_read() {
        let w = World::new();
        let request = ReattributionRequest {
            from_task: w.source,
            to_task: w.source,
            entry_refs: vec![],
            since: Some(day(3)),
            until: None,
            confirm: true,
        };
        let err = w.run(request).await.expect_err("must refuse");
        assert!(
            matches!(&err, AppError::Validation(m) if m.contains("same task")),
            "got {err}"
        );
    }

    #[tokio::test]
    async fn an_entry_that_belongs_to_another_task_is_refused() {
        let w = World::new();
        let stranger = w.log(w.elsewhere, utc(3, 7, 0));
        let err = w
            .run(w.by_ref(&[stranger.to_string()], true))
            .await
            .expect_err("must refuse");
        assert!(
            matches!(&err, AppError::Validation(m) if m.contains("not to the source task")),
            "got {err}"
        );
        assert_eq!(w.worklog.task_of(stranger), Some(w.elsewhere));
    }

    #[tokio::test]
    async fn a_day_with_nothing_on_it_is_refused() {
        let w = World::new();
        w.log(w.source, utc(5, 7, 0));
        let err = w.run(w.on_day(day(3), true)).await.expect_err("must refuse");
        assert!(
            matches!(&err, AppError::Validation(m) if m.contains("nothing to move")),
            "got {err}"
        );
    }

    #[tokio::test]
    async fn an_unknown_entry_reference_is_a_miss_not_a_refusal() {
        let w = World::new();
        let err = w
            .run(w.by_ref(&[Uuid::new_v4().to_string()], true))
            .await
            .expect_err("must fail");
        assert!(matches!(err, AppError::NotFound(_)), "got {err}");
    }

    #[tokio::test]
    async fn a_token_that_is_not_an_id_at_all_is_a_miss() {
        let w = World::new();
        let err = w
            .run(w.by_ref(&["zzz not an id".into()], true))
            .await
            .expect_err("must fail");
        assert!(matches!(err, AppError::NotFound(_)), "got {err}");
    }

    /// A prefix shared by two entries could name two different hours of work.
    #[tokio::test]
    async fn an_ambiguous_prefix_is_reported_never_guessed() {
        let w = World::new();
        // The collision is constructed, not hoped for: two ids differing only in
        // their last character, so this prefix names both of them every run.
        const SHARED: &str = "a1b2c3d4";
        let first = w.log_with_id(
            entry_id("a1b2c3d4-0000-4000-8000-000000000001"),
            w.source,
            utc(3, 7, 0),
        );
        let second = w.log_with_id(
            entry_id("a1b2c3d4-0000-4000-8000-000000000002"),
            w.source,
            utc(3, 8, 0),
        );

        let err = w
            .run(w.by_ref(&[SHARED.into()], true))
            .await
            .expect_err("must refuse to guess");
        assert!(matches!(err, AppError::Ambiguous(_)), "got {err}");
        // Reported, and reported usefully: both candidates named in full.
        let message = err.to_string();
        assert!(message.contains(&first.to_string()), "got {message}");
        assert!(message.contains(&second.to_string()), "got {message}");
        // And never silently resolved to one of them.
        assert_eq!(w.worklog.task_of(first), Some(w.source));
        assert_eq!(w.worklog.task_of(second), Some(w.source));
        assert!(w.worklog.reassign_calls.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn a_full_uuid_reference_resolves() {
        let w = World::new();
        let entry = w.log(w.source, utc(3, 7, 0));
        let outcome = w
            .run(w.by_ref(&[entry.to_string()], true))
            .await
            .expect("applies");
        assert_eq!(outcome.selected_entries, vec![entry]);
    }

    /// The prefix form the journal prints, three characters, resolves too.
    #[tokio::test]
    async fn a_short_prefix_resolves_when_it_is_unique() {
        let w = World::new();
        let entry = w.log(w.source, utc(3, 7, 0));
        let prefix: String = entry.to_string().chars().take(3).collect();
        let outcome = w.run(w.by_ref(&[prefix], true)).await.expect("applies");
        assert_eq!(outcome.selected_entries, vec![entry]);
    }

    #[tokio::test]
    async fn entry_references_and_a_date_window_together_are_refused() {
        let w = World::new();
        let entry = w.log(w.source, utc(3, 7, 0));
        let request = ReattributionRequest {
            from_task: w.source,
            to_task: w.destination,
            entry_refs: vec![entry.to_string()],
            since: Some(day(3)),
            until: None,
            confirm: false,
        };
        let err = w.run(request).await.expect_err("must refuse");
        assert!(
            matches!(&err, AppError::Validation(m) if m.contains("not both")),
            "got {err}"
        );
    }

    #[tokio::test]
    async fn no_selection_at_all_is_refused() {
        let w = World::new();
        let err = w
            .run(w.by_ref(&[], false))
            .await
            .expect_err("must refuse");
        assert!(
            matches!(&err, AppError::Validation(m) if m.contains("nothing selected")),
            "got {err}"
        );
    }

    #[tokio::test]
    async fn a_range_that_ends_before_it_starts_is_refused() {
        let w = World::new();
        let request = ReattributionRequest {
            from_task: w.source,
            to_task: w.destination,
            entry_refs: vec![],
            since: Some(day(5)),
            until: Some(day(3)),
            confirm: false,
        };
        let err = w.run(request).await.expect_err("must refuse");
        assert!(
            matches!(&err, AppError::Validation(m) if m.contains("ends before")),
            "got {err}"
        );
    }

    /// The `LIMIT 0` family of bug, the other way round: a page that came back full
    /// may have been cut, and moving the first 1 000 entries of a month silently is
    /// not a correction, it is a second defect.
    #[tokio::test]
    async fn a_selection_at_the_page_cap_is_refused_rather_than_truncated() {
        let w = World::new();
        for i in 0..WORKLOG_FILTER_MAX_LIMIT {
            w.log(w.source, utc(3, 7, 0) + Duration::seconds(i as i64));
        }
        let err = w.run(w.on_day(day(3), false)).await.expect_err("must refuse");
        assert!(
            matches!(&err, AppError::Validation(m) if m.contains("page cap")),
            "got {err}"
        );
    }

    // ─── Scoping ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_date_window_selects_only_the_source_tasks_entries() {
        let w = World::new();
        let mine = w.log(w.source, utc(3, 7, 0));
        w.log(w.destination, utc(3, 8, 0));
        w.log(w.elsewhere, utc(3, 9, 0));

        let outcome = w.run(w.on_day(day(3), false)).await.expect("previews");

        assert_eq!(outcome.selected_entries, vec![mine]);
    }

    #[tokio::test]
    async fn another_users_entry_is_out_of_reach() {
        let w = World::new();
        let theirs = w.log_for(Uuid::new_v4(), w.source, utc(3, 7, 0));
        let err = w
            .run(w.by_ref(&[theirs.to_string()], true))
            .await
            .expect_err("must fail");
        assert!(matches!(err, AppError::NotFound(_)), "got {err}");
    }

    #[tokio::test]
    async fn a_range_covers_every_day_that_carries_an_entry() {
        let w = World::new();
        w.log(w.source, utc(3, 7, 0));
        w.log(w.source, utc(5, 7, 0));
        let request = ReattributionRequest {
            from_task: w.source,
            to_task: w.destination,
            entry_refs: vec![],
            since: Some(day(3)),
            until: Some(day(5)),
            confirm: true,
        };
        let outcome = w.run(request).await.expect("applies");
        assert_eq!(outcome.affected_dates, vec![day(3), day(5)]);
        assert_eq!(outcome.moved_entries, 2);
        assert_eq!(outcome.slots_rebuilt, 2);
    }

    /// The reassignment must be handed the source task, so a row that left it
    /// concurrently cannot be dragged along.
    #[tokio::test]
    async fn the_reassignment_names_the_source_it_is_allowed_to_take_from() {
        let w = World::new();
        let entry = w.log(w.source, utc(3, 7, 0));
        w.run(w.on_day(day(3), true)).await.expect("applies");
        let calls = w.worklog.reassign_calls.lock().expect("lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], (vec![entry], w.source, w.destination));
    }
}
