//! Repair of activity slots that lost their `task_id` — the "(no task)" hours.
//!
//! ## The damage
//!
//! A latent defect wrote tasks back with `INSERT OR REPLACE INTO tasks`. That
//! statement deletes before it inserts, which fired `activity_slots.task_id`'s
//! `ON DELETE SET NULL`: the task row came back byte-identical while every slot
//! pointing at it lost its attribution. The hours stayed in the database, worth what
//! they were worth, attached to nobody — `aplan journal` prints them as "(no task)"
//! and the timesheet reconstruction cannot bill them.
//!
//! ## Why the existing machinery could not fix it alone
//!
//! [`plan_task_projection`] rebuilds **one task's** projection over named half-days,
//! and its delete list is the slots whose `task_id == Some(task_id)`. An orphan's
//! `task_id` is `NULL`, so it matches nothing: a naive flush of the same task would
//! leave the orphan in place *and* write a fresh slot beside it, billing that
//! half-day twice. `aplan flush` also only ever names the half-days its own window
//! touches, so it never reaches a past date; and `aplan reattribute` refuses a move
//! whose source and destination are the same task, which is what "put this time back
//! where it already was" would be.
//!
//! So this repair adds exactly the two things that were missing — a way to *name* a
//! past range, and the deletion of the slots whose attribution is gone — and then
//! delegates the arithmetic to the same [`plan_task_projection`] /
//! [`apply_task_projection`] pair the flush and the reattribution use. One projection,
//! defined once: a second implementation of "what the entries say the slots are" is
//! how a repair starts disagreeing with the flush that runs after it.
//!
//! ## What it will not touch
//!
//! Only a slot that is both unattributed and owned by the projection
//! ([`domain::rules::slot_repair::is_repairable_orphan`], which reuses
//! [`domain::rules::reattribution::is_rebuildable`]). A `Manual` slot with no task is
//! **not** damage: it is a hand-run timer from before migration `014`, it never had a
//! task, and no worklog entry can reproduce it. And a half-day that holds no orphan is
//! never named at all, so a task's slots elsewhere in the range are neither read for
//! deletion nor rewritten.
//!
//! ## Why an orphan with nothing to replace it is still dropped
//!
//! A half-day can hold an orphan while the worklog holds no entry for it any more —
//! the entries were deleted, or moved to a day outside the range. The orphan is then
//! dropped and nothing is written in its place. The alternative — keeping it — leaves
//! a duration that names no task, that no report can attribute and no timesheet can
//! bill, sitting in a half-day this repair has just declared canonical. The slot's
//! `source` says the projection owns it, and a projection whose input is gone has no
//! output. The preview reports those hours separately (`orphan_hours` against
//! `slots_written`) so the operator sees exactly what is being discarded before
//! confirming.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Timelike, Utc};
use domain::rules::reattribution::{slot_hours, AffectedHalfDay};
use domain::rules::slot_repair::{is_repairable_orphan, orphaned_half_days};
use domain::rules::workload::half_day_of;
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::{
    ActivitySlotRepository, ConfigRepository, WorklogFilter, WorklogRepository,
    WORKLOG_FILTER_MAX_LIMIT,
};
use crate::time::{local_window, to_local};
use crate::use_cases::reattribution::{refuse_a_truncated_page, TaskTimeChange};
use crate::use_cases::worklog::{
    apply_task_projection, plan_task_projection, user_timezone, RebuildPlan,
};

/// What the caller asks for: an explicit local-date range, and whether to write.
///
/// The range is explicit and mandatory. A repair that defaulted to "everything" would
/// rewrite years of billing history on a typo, and one that defaulted to "today"
/// would never reach the damage, which is always in the past by the time anyone
/// notices it.
#[derive(Debug, Clone, Copy)]
pub struct SlotRepairRequest {
    /// First local day of the range (inclusive).
    pub from: NaiveDate,
    /// Last local day of the range (inclusive).
    pub to: NaiveDate,
    /// Write. Without it nothing is persisted and the outcome is a prediction.
    pub confirm: bool,
}

/// What one local day of the range holds, and what it becomes.
///
/// Per date rather than per half-day: the operator reads a date in `aplan journal`,
/// and a report keyed by something they cannot see would be a translation exercise.
#[derive(Debug, Clone, PartialEq)]
pub struct DateRepair {
    pub date: NaiveDate,
    /// Unattributed projection slots dropped from this date.
    pub orphans_dropped: u32,
    /// What those orphans were worth — the hours that had fallen off every report.
    /// Compare against the tasks' `hours_after` to see whether the time came back.
    pub orphan_hours: f64,
    /// Slots of the rebuilt tasks dropped from this date: their own projection, which
    /// the rewrite replaces. Not damage, and normally reproduced identically.
    pub slots_discarded: u32,
    /// Slots written back from the worklog on this date.
    pub slots_written: u32,
}

/// The report a repair owes its operator: what it would drop, what it would write,
/// and which task ends up with which hours.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotRepairOutcome {
    /// `false` for a preview, which wrote nothing at all.
    pub applied: bool,
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// One entry per local day that holds a repairable orphan, ascending. Empty means
    /// the range is clean — a success, not a refusal.
    pub dates: Vec<DateRepair>,
    /// The tasks whose half-days were rebuilt, with their hours before and after, in
    /// the order their first contributing entry appears.
    pub tasks: Vec<TaskTimeChange>,
    pub orphans_dropped: u32,
    pub orphan_hours: f64,
    pub slots_discarded: u32,
    pub slots_written: u32,
}

impl SlotRepairOutcome {
    /// Hours the rebuilt tasks account for in the touched half-days, before and after.
    ///
    /// The figure a repair is judged on: `after - before` should be about
    /// `orphan_hours`, since that is the time the orphans were holding. It is not an
    /// equality — an orphan and the rebuilt slots come from the same entries but the
    /// orphan may predate a change in the grouping rules — which is exactly why both
    /// numbers are printed side by side instead of one being asserted.
    pub fn task_hours(&self) -> (f64, f64) {
        (
            self.tasks.iter().map(|t| t.hours_before).sum::<f64>() + 0.0,
            self.tasks.iter().map(|t| t.hours_after).sum::<f64>() + 0.0,
        )
    }
}

/// Give the orphaned slots of a local-date range back to the tasks the worklog says
/// they belong to.
///
/// Writes nothing unless `request.confirm` is set. The preview's figures are not a
/// separate estimate: they are read off the very same [`RebuildPlan`]s the apply
/// persists, so `--confirm` applies exactly what was shown.
pub async fn repair_orphaned_slots(
    worklog_repo: &dyn WorklogRepository,
    activity_repo: &dyn ActivitySlotRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    request: SlotRepairRequest,
    now: DateTime<Utc>,
) -> Result<SlotRepairOutcome, AppError> {
    if request.to < request.from {
        return Err(AppError::Validation(format!(
            "the range ends before it starts ({} → {})",
            request.from, request.to
        )));
    }
    let tz = user_timezone(config_repo, user_id).await?;

    // ── The damage, and the half-days it sits in ─────────────────────────────
    let orphans: Vec<ActivitySlot> = activity_repo
        .find_by_user_and_date_range(user_id, request.from, request.to)
        .await?
        .into_iter()
        .filter(is_repairable_orphan)
        .collect();
    let half_days = orphaned_half_days(&orphans);
    if half_days.is_empty() {
        // A clean range is a no-op, never a refusal: this is a maintenance sweep, and
        // one that errored when there was nothing to sweep could not be re-run to
        // check its own work, nor scheduled.
        return Ok(SlotRepairOutcome {
            applied: request.confirm,
            from: request.from,
            to: request.to,
            dates: Vec::new(),
            tasks: Vec::new(),
            orphans_dropped: 0,
            orphan_hours: 0.0,
            slots_discarded: 0,
            slots_written: 0,
        });
    }

    // ── Who logged in those half-days ────────────────────────────────────────
    let per_task = tasks_of_the_affected_half_days(
        worklog_repo,
        user_id,
        tz,
        &half_days,
        request.from,
        request.to,
    )
    .await?;

    // ── What each of those tasks' half-days becomes ──────────────────────────
    //
    // Each task is asked about *its own* half-days only — the intersection of the
    // affected units with the ones it actually logged in. Naming a half-day a task
    // has no entry in would put its slots there on the delete list with nothing to
    // rewrite them from, which is how a repair deletes hours.
    let mut plans: Vec<RebuildPlan> = Vec::with_capacity(per_task.len());
    for (task_id, units) in &per_task {
        plans.push(
            plan_task_projection(activity_repo, worklog_repo, user_id, *task_id, units, tz, now)
                .await?,
        );
    }

    let outcome = summarise(&request, &orphans, &plans);

    if request.confirm {
        // Deletion before any write, for the reason `apply_task_projection` documents:
        // the reverse order leaves a window in which the half-day carries both the old
        // slot and its replacement, and a reader landing there sees doubled hours. The
        // orphans go first because they are not on any plan's delete list — no plan
        // can claim a slot whose `task_id` is NULL — so nothing else would ever drop
        // them.
        for orphan in &orphans {
            activity_repo.delete(orphan.id).await?;
        }
        for plan in &plans {
            apply_task_projection(activity_repo, plan).await?;
        }
    }

    Ok(outcome)
}

/// The tasks that logged in the affected half-days, each with the units it logged in.
///
/// One read for the whole range rather than one per half-day, then filtered back to
/// the affected units: a range that spans a quiet week must not send the rebuild to
/// that week's days, and an afternoon's damage must not pull in the morning.
///
/// Ordered by each task's first contributing entry, so a report listing two tasks
/// lists them in the order the day happened rather than in the order a hash map
/// happened to iterate.
async fn tasks_of_the_affected_half_days(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    tz: chrono_tz::Tz,
    half_days: &[AffectedHalfDay],
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<(TaskId, Vec<AffectedHalfDay>)>, AppError> {
    let (from_utc, to_utc) = local_window(tz, from, to);
    let page = worklog_repo
        .list(
            user_id,
            &WorklogFilter {
                // Every task: which ones are involved is precisely what is unknown —
                // the orphan lost the only pointer that used to say so.
                task_ids: None,
                from: Some(from_utc),
                to: Some(to_utc),
                limit: WORKLOG_FILTER_MAX_LIMIT,
                offset: 0,
            },
        )
        .await?;
    let mut entries = refuse_a_truncated_page(page, "the repaired range")?;
    entries.sort_by_key(|entry| entry.logged_at);

    let mut order: Vec<TaskId> = Vec::new();
    let mut units: HashMap<TaskId, Vec<AffectedHalfDay>> = HashMap::new();
    for entry in &entries {
        let local = to_local(entry.logged_at, tz);
        let unit = AffectedHalfDay {
            date: local.date(),
            half_day: half_day_of(local.time().hour()),
        };
        if !half_days.contains(&unit) {
            continue;
        }
        let mine = units.entry(entry.task_id).or_insert_with(|| {
            order.push(entry.task_id);
            Vec::new()
        });
        if !mine.contains(&unit) {
            mine.push(unit);
        }
    }

    Ok(order
        .into_iter()
        .filter_map(|task_id| units.remove(&task_id).map(|units| (task_id, units)))
        .collect())
}

/// Read the report off the plans, before anything is written.
///
/// Deliberately not a second computation: `delete` and `write` are the very lists the
/// apply persists, so the preview and the write cannot report different figures.
fn summarise(
    request: &SlotRepairRequest,
    orphans: &[ActivitySlot],
    plans: &[RebuildPlan],
) -> SlotRepairOutcome {
    let mut dates: Vec<NaiveDate> = Vec::new();
    for slot in orphans {
        if !dates.contains(&slot.date) {
            dates.push(slot.date);
        }
    }
    dates.sort();

    let on_date = |slots: &[ActivitySlot], date: NaiveDate| -> Vec<ActivitySlot> {
        slots.iter().filter(|s| s.date == date).cloned().collect()
    };

    let per_date: Vec<DateRepair> = dates
        .iter()
        .map(|date| {
            let dropped = on_date(orphans, *date);
            let discarded: u32 = plans
                .iter()
                .map(|plan| on_date(&plan.delete, *date).len() as u32)
                .sum();
            let written: u32 = plans
                .iter()
                .map(|plan| on_date(&plan.write, *date).len() as u32)
                .sum();
            DateRepair {
                date: *date,
                orphans_dropped: dropped.len() as u32,
                orphan_hours: slot_hours(&dropped),
                slots_discarded: discarded,
                slots_written: written,
            }
        })
        .collect();

    let tasks: Vec<TaskTimeChange> = plans
        .iter()
        .map(|plan| TaskTimeChange {
            task_id: plan.task_id,
            hours_before: slot_hours(&plan.delete),
            hours_after: slot_hours(&plan.write),
        })
        .collect();

    SlotRepairOutcome {
        applied: request.confirm,
        from: request.from,
        to: request.to,
        orphans_dropped: per_date.iter().map(|d| d.orphans_dropped).sum(),
        orphan_hours: per_date.iter().map(|d| d.orphan_hours).sum::<f64>() + 0.0,
        slots_discarded: per_date.iter().map(|d| d.slots_discarded).sum(),
        slots_written: per_date.iter().map(|d| d.slots_written).sum(),
        dates: per_date,
        tasks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    use crate::use_cases::worklog::tests::{FakeActivityRepo, FakeConfigRepo, FakeRepo};

    // Paris in August is UTC+2, and no timezone is configured, so the default
    // `Europe/Paris` applies: a UTC hour reads two hours later locally. Fixtures are
    // written in UTC, assertions in local half-days — the conversion the projection
    // has to get right.

    fn user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("valid uuid")
    }

    fn task_a() -> TaskId {
        Uuid::parse_str("00000000-0000-0000-0000-0000000000aa").expect("valid uuid")
    }

    fn task_b() -> TaskId {
        Uuid::parse_str("00000000-0000-0000-0000-0000000000bb").expect("valid uuid")
    }

    fn utc(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, day, hour, minute, 0)
            .single()
            .expect("valid instant")
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, d).expect("valid date")
    }

    fn now() -> DateTime<Utc> {
        utc(11, 8, 0)
    }

    struct World {
        worklog: FakeRepo,
        activity: FakeActivityRepo,
        config: FakeConfigRepo,
    }

    impl World {
        fn new() -> Self {
            Self {
                worklog: FakeRepo::default(),
                activity: FakeActivityRepo::default(),
                config: FakeConfigRepo::default(),
            }
        }

        async fn log(&self, task: TaskId, at: DateTime<Utc>) {
            let entry = WorklogEntry::new(user_id(), task, "work".into(), at, at)
                .expect("valid entry");
            self.worklog.create(&entry).await.expect("stored");
        }

        /// A slot the flush wrote and `ON DELETE SET NULL` then stripped: closed,
        /// `Worklog`-sourced, `task_id` gone. Exactly the 16 rows on the real day.
        async fn orphan(
            &self,
            start: DateTime<Utc>,
            end: DateTime<Utc>,
            half_day: HalfDay,
            date: NaiveDate,
        ) -> ActivitySlotId {
            self.push(ActivitySlot {
                id: Uuid::new_v4(),
                user_id: user_id(),
                task_id: None,
                start_time: start,
                end_time: Some(end),
                half_day,
                date,
                created_at: start,
                session_id: None,
                source: SlotSource::Worklog,
            })
            .await
        }

        /// A hand-run timer from before migration `014`: closed, no task, `Manual`.
        /// Never damage, and never this repair's to touch.
        async fn hand_made(
            &self,
            start: DateTime<Utc>,
            end: DateTime<Utc>,
            half_day: HalfDay,
            date: NaiveDate,
        ) -> ActivitySlotId {
            self.push(ActivitySlot::manual(
                user_id(),
                None,
                start,
                Some(end),
                half_day,
                date,
                start,
            ))
            .await
        }

        /// A slot the flush wrote and that still has its task.
        async fn intact(
            &self,
            task: TaskId,
            start: DateTime<Utc>,
            end: DateTime<Utc>,
            half_day: HalfDay,
            date: NaiveDate,
        ) -> ActivitySlotId {
            self.push(ActivitySlot::from_worklog(
                user_id(),
                task,
                None,
                start,
                end,
                half_day,
                date,
                start,
            ))
            .await
        }

        async fn push(&self, slot: ActivitySlot) -> ActivitySlotId {
            let id = slot.id;
            self.activity.save(&slot).await.expect("stored");
            id
        }

        async fn run(&self, from: u32, to: u32, confirm: bool) -> SlotRepairOutcome {
            self.try_run(from, to, confirm).await.expect("repairs")
        }

        async fn try_run(
            &self,
            from: u32,
            to: u32,
            confirm: bool,
        ) -> Result<SlotRepairOutcome, AppError> {
            repair_orphaned_slots(
                &self.worklog,
                &self.activity,
                &self.config,
                user_id(),
                SlotRepairRequest {
                    from: day(from),
                    to: day(to),
                    confirm,
                },
                now(),
            )
            .await
        }

        async fn slots_on(&self, d: u32) -> Vec<ActivitySlot> {
            self.activity
                .find_by_user_and_date(user_id(), day(d))
                .await
                .expect("read")
        }

        async fn survives(&self, id: ActivitySlotId) -> Option<ActivitySlot> {
            self.activity.find_by_id(id).await.expect("read")
        }
    }

    /// A world matching the real 2026-08-04 afternoon: entries intact, the slot they
    /// project to stripped of its task.
    async fn a_stripped_afternoon() -> (World, ActivitySlotId) {
        let w = World::new();
        w.log(task_a(), utc(4, 12, 0)).await; // local 14:00
        w.log(task_a(), utc(4, 12, 30)).await; // local 14:30 — same stretch
        let orphan = w
            .orphan(utc(4, 12, 0), utc(4, 12, 30), HalfDay::Afternoon, day(4))
            .await;
        (w, orphan)
    }

    // ─── The repair ──────────────────────────────────────────────────────────

    /// The defect this verb exists for: time that shows as "(no task)" goes back to
    /// the task whose entries produced it.
    #[tokio::test]
    async fn an_orphaned_worklog_slot_gets_its_task_back() {
        let (w, orphan) = a_stripped_afternoon().await;

        let outcome = w.run(4, 4, true).await;

        assert!(outcome.applied);
        assert!(w.survives(orphan).await.is_none(), "the orphan was dropped");
        let afternoon = w.slots_on(4).await;
        assert_eq!(afternoon.len(), 1, "one stretch of work, one slot");
        assert_eq!(afternoon[0].task_id, Some(task_a()));
        assert_eq!(afternoon[0].start_time, utc(4, 12, 0));
        assert_eq!(afternoon[0].end_time, Some(utc(4, 12, 30)));
        assert_eq!(afternoon[0].source, SlotSource::Worklog);
        assert_eq!(outcome.orphans_dropped, 1);
        assert_eq!(outcome.slots_written, 1);
        assert_eq!(outcome.orphan_hours, 0.5);
        assert_eq!(
            outcome.tasks,
            vec![TaskTimeChange {
                task_id: task_a(),
                hours_before: 0.0,
                hours_after: 0.5,
            }],
            "the half-hour the orphan was holding lands on the task"
        );
    }

    /// The 95 rows that must survive untouched: a closed slot with no task that was
    /// never a projection. Deleting one destroys time no entry can reproduce.
    #[tokio::test]
    async fn a_manual_orphan_in_range_is_left_exactly_as_it_was() {
        let (w, _) = a_stripped_afternoon().await;
        let hand_made = w
            .hand_made(utc(4, 13, 0), utc(4, 14, 0), HalfDay::Afternoon, day(4))
            .await;

        w.run(4, 4, true).await;

        let kept = w.survives(hand_made).await.expect("the hand-made slot survives");
        assert_eq!(kept.id, hand_made, "not rebuilt, not replaced — the same row");
        assert_eq!(kept.task_id, None, "its NULL task was never damage");
        assert_eq!(kept.start_time, utc(4, 13, 0));
        assert_eq!(kept.end_time, Some(utc(4, 14, 0)));
        assert_eq!(kept.source, SlotSource::Manual);
    }

    /// The range is the blast radius. A day outside it keeps its damage — which is
    /// what makes running the repair one day at a time meaningful.
    #[tokio::test]
    async fn an_orphan_outside_the_range_is_untouched() {
        let (w, _) = a_stripped_afternoon().await;
        w.log(task_a(), utc(6, 12, 0)).await;
        let elsewhere = w
            .orphan(utc(6, 12, 0), utc(6, 13, 0), HalfDay::Afternoon, day(6))
            .await;

        let outcome = w.run(4, 4, true).await;

        let still_orphaned = w.survives(elsewhere).await.expect("untouched");
        assert_eq!(still_orphaned.task_id, None);
        assert_eq!(w.slots_on(6).await.len(), 1, "nothing was written there either");
        assert_eq!(outcome.dates.len(), 1);
        assert_eq!(outcome.dates[0].date, day(4));
    }

    /// A half-day inside the range that holds no orphan is not rebuilt: its slots are
    /// not damage, and canonicalising them is work this repair has no mandate for.
    #[tokio::test]
    async fn a_half_day_with_no_orphan_keeps_its_slots_untouched() {
        let (w, _) = a_stripped_afternoon().await;
        w.log(task_a(), utc(4, 7, 0)).await; // local 09:00 — the morning
        let morning = w
            .intact(task_a(), utc(4, 7, 0), utc(4, 8, 0), HalfDay::Morning, day(4))
            .await;

        w.run(4, 4, true).await;

        let kept = w.survives(morning).await.expect("the morning slot survives");
        assert_eq!(kept.id, morning, "the same row, not a rebuilt one");
        assert_eq!(
            kept.end_time,
            Some(utc(4, 8, 0)),
            "an hour the entries alone would not have projected"
        );
    }

    /// Two tasks in one damaged half-day: both come back, and the half-day ends up
    /// worth what the entries say — not that plus the orphan it replaced.
    #[tokio::test]
    async fn two_tasks_sharing_a_half_day_are_both_rebuilt_without_doubling_the_hours() {
        let w = World::new();
        w.log(task_a(), utc(4, 12, 0)).await; // local 14:00
        w.log(task_a(), utc(4, 12, 30)).await; // local 14:30
        w.log(task_b(), utc(4, 13, 30)).await; // local 15:30
        w.log(task_b(), utc(4, 14, 0)).await; // local 16:00
        // One orphan spanning the whole afternoon, as a flush that predates the gap
        // rule would have left it.
        w.orphan(utc(4, 12, 0), utc(4, 14, 0), HalfDay::Afternoon, day(4))
            .await;

        let outcome = w.run(4, 4, true).await;

        let afternoon = w.slots_on(4).await;
        assert_eq!(afternoon.len(), 2, "one slot per task, not per entry");
        let mut owners: Vec<Option<TaskId>> = afternoon.iter().map(|s| s.task_id).collect();
        owners.sort();
        let mut expected = vec![Some(task_a()), Some(task_b())];
        expected.sort();
        assert_eq!(owners, expected);
        assert_eq!(
            slot_hours(&afternoon),
            1.0,
            "two half-hour stretches — not the orphan's two hours on top"
        );
        assert_eq!(outcome.orphan_hours, 2.0, "what the orphan was holding");
        assert_eq!(outcome.task_hours(), (0.0, 1.0));
        assert_eq!(outcome.tasks.len(), 2);
        assert_eq!(
            outcome.tasks[0].task_id,
            task_a(),
            "reported in the order the afternoon happened"
        );
    }

    /// An orphan the worklog can no longer explain: its entries are gone. The slot is
    /// dropped and nothing is invented in its place — an unattributable duration in a
    /// half-day the repair has just declared canonical is worse than none.
    #[tokio::test]
    async fn an_orphan_whose_worklog_is_gone_is_dropped_without_inventing_a_slot() {
        let w = World::new();
        let orphan = w
            .orphan(utc(10, 6, 0), utc(10, 7, 0), HalfDay::Morning, day(10))
            .await;

        let outcome = w.run(10, 10, true).await;

        assert!(w.survives(orphan).await.is_none(), "dropped");
        assert!(
            w.slots_on(10).await.is_empty(),
            "nothing was written from entries that do not exist"
        );
        assert_eq!(outcome.dates.len(), 1);
        assert_eq!(outcome.dates[0].orphans_dropped, 1);
        assert_eq!(outcome.dates[0].orphan_hours, 1.0);
        assert_eq!(
            outcome.dates[0].slots_written, 0,
            "the discarded hour is visible in the report, never silent"
        );
        assert!(outcome.tasks.is_empty());
    }

    // ─── The preview ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_preview_writes_absolutely_nothing() {
        let (w, orphan) = a_stripped_afternoon().await;

        let outcome = w.run(4, 4, false).await;

        assert!(!outcome.applied);
        let untouched = w.survives(orphan).await.expect("still there");
        assert_eq!(untouched.task_id, None, "still orphaned");
        assert_eq!(w.slots_on(4).await.len(), 1, "no slot was written");
        assert_eq!(outcome.orphans_dropped, 1, "and it still says what it would do");
        assert_eq!(outcome.slots_written, 1);
    }

    /// The preview is only worth reading if it predicts the write. Two identical
    /// worlds, one previewed and one applied, must report the same figures —
    /// otherwise `--confirm` is a leap of faith.
    #[tokio::test]
    async fn a_preview_reports_the_same_figures_as_the_apply() {
        async fn world_with(confirm: bool) -> SlotRepairOutcome {
            let w = World::new();
            w.log(task_a(), utc(4, 12, 0)).await;
            w.log(task_a(), utc(4, 12, 30)).await;
            w.log(task_b(), utc(4, 14, 0)).await;
            w.orphan(utc(4, 12, 0), utc(4, 12, 30), HalfDay::Afternoon, day(4))
                .await;
            w.intact(
                task_b(),
                utc(4, 14, 0),
                utc(4, 14, 1),
                HalfDay::Afternoon,
                day(4),
            )
            .await;
            w.run(4, 4, confirm).await
        }

        let preview = world_with(false).await;
        let applied = world_with(true).await;

        assert_eq!(preview.dates, applied.dates);
        assert_eq!(preview.tasks, applied.tasks);
        assert_eq!(preview.orphans_dropped, applied.orphans_dropped);
        assert_eq!(preview.orphan_hours, applied.orphan_hours);
        assert_eq!(preview.slots_discarded, applied.slots_discarded);
        assert_eq!(preview.slots_written, applied.slots_written);
        assert_ne!(preview.applied, applied.applied);
    }

    // ─── Running it again ────────────────────────────────────────────────────

    /// A repair that cannot be re-run cannot be checked. The second pass finds a
    /// clean range, reports nothing, and leaves the first pass's work alone.
    #[tokio::test]
    async fn repairing_twice_reports_a_clean_range_and_changes_nothing() {
        let (w, _) = a_stripped_afternoon().await;

        w.run(4, 4, true).await;
        let rebuilt = w.slots_on(4).await;
        let second = w.run(4, 4, true).await;

        assert!(second.dates.is_empty(), "nothing left to repair");
        assert_eq!(second.orphans_dropped, 0);
        assert_eq!(second.slots_written, 0);
        let after = w.slots_on(4).await;
        assert_eq!(after.len(), rebuilt.len());
        assert_eq!(
            after[0].id, rebuilt[0].id,
            "the second pass did not rewrite the first pass's slot"
        );
    }

    /// The whole point of `--from`/`--to`: a range with no damage in it is a
    /// successful no-op, not an error. A sweep that refused an empty range could not
    /// be scheduled, and could not be used to verify a repair.
    #[tokio::test]
    async fn a_clean_range_is_a_success_not_a_refusal() {
        let w = World::new();
        w.log(task_a(), utc(4, 12, 0)).await;

        let outcome = w.run(4, 6, true).await;

        assert!(outcome.dates.is_empty());
        assert!(outcome.tasks.is_empty());
        assert_eq!(outcome.orphan_hours, 0.0);
        assert!(outcome.orphan_hours.is_sign_positive(), "0.0, never -0.0");
    }

    // ─── The report ──────────────────────────────────────────────────────────

    /// Per date, ascending, with the counts the operator confirms on.
    #[tokio::test]
    async fn every_damaged_date_is_reported_with_its_own_counts() {
        let w = World::new();
        // 2026-08-10 morning: two orphans, one task.
        w.log(task_a(), utc(10, 6, 0)).await; // local 08:00
        w.log(task_a(), utc(10, 6, 30)).await; // local 08:30
        // Two quarters of an hour: durations whose sum is exact in binary, so the
        // assertion below is about the arithmetic and not about float rounding.
        w.orphan(utc(10, 6, 0), utc(10, 6, 15), HalfDay::Morning, day(10))
            .await;
        w.orphan(utc(10, 6, 20), utc(10, 6, 35), HalfDay::Morning, day(10))
            .await;
        // 2026-08-04 afternoon: one orphan, one task.
        w.log(task_b(), utc(4, 12, 0)).await;
        w.log(task_b(), utc(4, 12, 30)).await;
        w.orphan(utc(4, 12, 0), utc(4, 12, 30), HalfDay::Afternoon, day(4))
            .await;

        let outcome = w.run(4, 10, false).await;

        assert_eq!(
            outcome.dates,
            vec![
                DateRepair {
                    date: day(4),
                    orphans_dropped: 1,
                    orphan_hours: 0.5,
                    slots_discarded: 0,
                    slots_written: 1,
                },
                DateRepair {
                    date: day(10),
                    orphans_dropped: 2,
                    orphan_hours: 0.5,
                    slots_discarded: 0,
                    slots_written: 1,
                },
            ]
        );
        assert_eq!(outcome.orphans_dropped, 3);
        assert_eq!(outcome.slots_written, 2);
        assert_eq!(outcome.from, day(4));
        assert_eq!(outcome.to, day(10));
    }

    /// A rebuilt task's own stale slot is dropped by its plan, and reported as such —
    /// distinct from an orphan, because one is damage and the other is routine.
    #[tokio::test]
    async fn a_rebuilt_tasks_own_slot_is_counted_as_discarded_not_as_an_orphan() {
        let (w, _) = a_stripped_afternoon().await;
        w.intact(
            task_a(),
            utc(4, 12, 0),
            utc(4, 12, 30),
            HalfDay::Afternoon,
            day(4),
        )
        .await;

        let outcome = w.run(4, 4, true).await;

        assert_eq!(outcome.orphans_dropped, 1);
        assert_eq!(outcome.slots_discarded, 1);
        assert_eq!(outcome.slots_written, 1);
        assert_eq!(
            w.slots_on(4).await.len(),
            1,
            "the half-day is not billed twice"
        );
    }

    // ─── Refusals ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_range_that_ends_before_it_starts_is_refused() {
        let w = World::new();
        let err = w.try_run(10, 4, true).await.expect_err("must refuse");
        assert!(
            matches!(&err, AppError::Validation(m) if m.contains("ends before")),
            "got {err}"
        );
    }
}
