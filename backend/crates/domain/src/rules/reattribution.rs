//! Réattribution: moving worklog entries — and the time derived from them — from
//! one task to another.
//!
//! The decisions this module owns are the ones that must not depend on I/O: what
//! the selection is allowed to be, and which local days the correction touches.
//!
//! Why the half-days matter enough to be computed here: `activity_slots` are a
//! *projection* of worklog timestamps ([`crate::rules::worklog_time`]), one slot per
//! stretch of work, and a stretch never straddles a (local day, half-day) — which
//! makes the half-day the smallest unit a repair can scope itself to. Moving an entry
//! invalidates the projection of exactly two tasks in exactly the half-days that entry
//! falls in — no more, so a third task sharing the half-day is never in scope and the
//! morning is untouched when only the afternoon moved, and no less, so no stale slot
//! survives in a half-day the rebuild also writes to, where it would be counted twice.
//! How *many* slots a half-day holds is deliberately not part of the argument: the
//! repair drops every closed slot of those tasks in those half-days before rebuilding.

use chrono::{NaiveDate, Timelike};

use crate::rules::workload::half_day_of;
use crate::types::activity::ActivitySlot;
use crate::types::common::{HalfDay, TaskId};
use crate::types::worklog::WorklogEntryId;

/// One selected entry as the reattribution reads it.
///
/// `local_logged_at` is wall-clock in the user's timezone, not UTC: slots are
/// grouped by *local* day and half-day, so the day a correction touches is a local
/// day. Converting is the application layer's job — this module never guesses a
/// timezone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryAttribution {
    pub id: WorklogEntryId,
    /// The task the entry belongs to **right now**.
    pub task_id: TaskId,
    pub local_logged_at: chrono::NaiveDateTime,
}

/// Why a reattribution was refused before anything was written.
///
/// Three distinct refusals rather than one message, because the caller turns them
/// into a process exit code and an operator has to tell "I mistyped the source" from
/// "there is nothing on that day".
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReattributionRefusal {
    #[error("source and destination are the same task: nothing would move")]
    SameTask,
    #[error("no worklog entry matches the selection: nothing to move")]
    EmptySelection,
    #[error(
        "worklog entry {entry} belongs to task {actual}, not to the source task {expected}: \
         refusing to move time off a task that was not named"
    )]
    ForeignEntry {
        entry: WorklogEntryId,
        expected: TaskId,
        actual: TaskId,
    },
}

/// One (local day, half-day) the move invalidates the projection of — the unit a
/// slot is written in, and therefore the unit a repair is scoped to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AffectedHalfDay {
    pub date: NaiveDate,
    pub half_day: HalfDay,
}

/// A validated reattribution: what moves, and which half-days must have their slots
/// rebuilt afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReattributionPlan {
    /// The entries to move, deduplicated, in the order they were selected.
    pub entry_ids: Vec<WorklogEntryId>,
    /// The half-days whose slot projection the move invalidates, ascending
    /// (morning before afternoon).
    pub affected_half_days: Vec<AffectedHalfDay>,
}

impl ReattributionPlan {
    /// The local days the report names, ascending. Derived from the half-days rather
    /// than tracked separately, so the two can never disagree about which day was
    /// touched.
    pub fn affected_dates(&self) -> Vec<NaiveDate> {
        let mut dates: Vec<NaiveDate> = Vec::new();
        for unit in &self.affected_half_days {
            if !dates.contains(&unit.date) {
                dates.push(unit.date);
            }
        }
        dates
    }
}

/// Validate a selection and derive the days it touches.
///
/// Refusal order is deliberate: a source equal to the destination is nonsense
/// whatever the selection holds, so it is reported before an empty selection is —
/// otherwise `--from X --to X` on a quiet day would be reported as "nothing to
/// move" and the real mistake would stay hidden.
pub fn plan_reattribution(
    from: TaskId,
    to: TaskId,
    selected: &[EntryAttribution],
) -> Result<ReattributionPlan, ReattributionRefusal> {
    if from == to {
        return Err(ReattributionRefusal::SameTask);
    }
    if selected.is_empty() {
        return Err(ReattributionRefusal::EmptySelection);
    }

    let mut entry_ids: Vec<WorklogEntryId> = Vec::with_capacity(selected.len());
    let mut affected_half_days: Vec<AffectedHalfDay> = Vec::new();
    for entry in selected {
        if entry.task_id != from {
            return Err(ReattributionRefusal::ForeignEntry {
                entry: entry.id,
                expected: from,
                actual: entry.task_id,
            });
        }
        if !entry_ids.contains(&entry.id) {
            entry_ids.push(entry.id);
        }
        // The same rule the projection groups by, so a repair covers exactly the
        // half-days the rebuild will write to.
        let unit = AffectedHalfDay {
            date: entry.local_logged_at.date(),
            half_day: half_day_of(entry.local_logged_at.time().hour()),
        };
        if !affected_half_days.contains(&unit) {
            affected_half_days.push(unit);
        }
    }
    affected_half_days.sort_by_key(|unit| {
        (
            unit.date,
            matches!(unit.half_day, HalfDay::Afternoon),
        )
    });

    Ok(ReattributionPlan {
        entry_ids,
        affected_half_days,
    })
}

/// Hours a set of persisted slots accounts for.
///
/// Open slots (`end_time` is `None`) count for nothing and are not an error: a
/// running timer has no duration yet. Same rule as the activity report, so the
/// before/after figures a correction prints are the same figures the week view
/// shows.
///
/// `+ 0.0` normalises the sign of an empty sum, whose float identity is `-0.0` — see
/// [`crate::rules::worklog_time::total_block_hours`].
pub fn slot_hours(slots: &[ActivitySlot]) -> f64 {
    slots
        .iter()
        .filter_map(|slot| slot.end_time.map(|end| end - slot.start_time))
        .map(|span| span.num_minutes() as f64 / 60.0)
        .sum::<f64>()
        + 0.0
}

/// Is this slot one the reattribution repair may replace?
///
/// Only closed slots are. An open slot is a *running* activity: deleting it would
/// stop a timer nobody asked to stop, and it accounts for no hours yet anyway.
pub fn is_rebuildable(slot: &ActivitySlot) -> bool {
    slot.end_time.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::activity::SlotSource;
    use crate::types::common::HalfDay;
    use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
    use uuid::Uuid;

    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .expect("valid date")
            .and_hms_opt(h, min, 0)
            .expect("valid time")
    }

    fn attribution(task_id: TaskId, at: NaiveDateTime) -> EntryAttribution {
        EntryAttribution {
            id: Uuid::new_v4(),
            task_id,
            local_logged_at: at,
        }
    }

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0)
            .single()
            .expect("valid instant")
    }

    fn slot(
        task_id: Option<TaskId>,
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
    ) -> ActivitySlot {
        // Closed → as a flush would have left it (`Worklog`). Open → a running
        // timer, which `from_worklog` cannot produce (its `end_time` is not
        // optional), so `Manual` — the same distinction the constructors encode.
        let source = if end.is_some() {
            SlotSource::Worklog
        } else {
            SlotSource::Manual
        };
        ActivitySlot {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            task_id,
            start_time: start,
            end_time: end,
            half_day: HalfDay::Morning,
            date: start.date_naive(),
            created_at: start,
            session_id: None,
            source,
        }
    }

    // ─── Refusals ────────────────────────────────────────────────────────────

    #[test]
    fn moving_time_onto_the_same_task_is_refused() {
        let task = Uuid::new_v4();
        let entries = vec![attribution(task, local(2026, 8, 3, 9, 0))];
        assert_eq!(
            plan_reattribution(task, task, &entries),
            Err(ReattributionRefusal::SameTask)
        );
    }

    /// Reported *before* the empty selection, so `--from X --to X` on a day with no
    /// entries names the real mistake instead of the symptom.
    #[test]
    fn the_same_task_is_reported_even_when_the_selection_is_empty() {
        let task = Uuid::new_v4();
        assert_eq!(
            plan_reattribution(task, task, &[]),
            Err(ReattributionRefusal::SameTask)
        );
    }

    #[test]
    fn an_empty_selection_is_refused() {
        assert_eq!(
            plan_reattribution(Uuid::new_v4(), Uuid::new_v4(), &[]),
            Err(ReattributionRefusal::EmptySelection)
        );
    }

    /// The guard against correcting the wrong day's work: an entry id copied from
    /// another task's journal must not silently move time off a task the operator
    /// never named.
    #[test]
    fn an_entry_that_belongs_to_another_task_is_refused_and_named() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let elsewhere = Uuid::new_v4();
        let stranger = attribution(elsewhere, local(2026, 8, 3, 9, 0));
        let expected = Err(ReattributionRefusal::ForeignEntry {
            entry: stranger.id,
            expected: from,
            actual: elsewhere,
        });
        assert_eq!(plan_reattribution(from, to, &[stranger]), expected);
    }

    #[test]
    fn one_foreign_entry_refuses_the_whole_selection() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let mine = attribution(from, local(2026, 8, 3, 9, 0));
        let theirs = attribution(Uuid::new_v4(), local(2026, 8, 3, 10, 0));
        assert!(matches!(
            plan_reattribution(from, to, &[mine, theirs]),
            Err(ReattributionRefusal::ForeignEntry { .. })
        ));
    }

    // ─── The plan ────────────────────────────────────────────────────────────

    #[test]
    fn a_days_entries_yield_the_half_days_they_fall_in() {
        let from = Uuid::new_v4();
        let entries = vec![
            attribution(from, local(2026, 8, 3, 9, 0)),
            attribution(from, local(2026, 8, 3, 14, 30)),
        ];
        let plan = plan_reattribution(from, Uuid::new_v4(), &entries).expect("valid plan");
        assert_eq!(plan.entry_ids.len(), 2);
        assert_eq!(
            plan.affected_half_days,
            vec![
                AffectedHalfDay {
                    date: NaiveDate::from_ymd_opt(2026, 8, 3).expect("valid date"),
                    half_day: HalfDay::Morning,
                },
                AffectedHalfDay {
                    date: NaiveDate::from_ymd_opt(2026, 8, 3).expect("valid date"),
                    half_day: HalfDay::Afternoon,
                },
            ]
        );
        assert_eq!(
            plan.affected_dates(),
            vec![NaiveDate::from_ymd_opt(2026, 8, 3).expect("valid date")]
        );
    }

    /// An afternoon that moves must not put the morning in scope: the morning's slot
    /// belongs to entries that did not move, and rebuilding it is work the correction
    /// has no mandate for.
    #[test]
    fn a_move_confined_to_one_half_day_leaves_the_other_out_of_scope() {
        let from = Uuid::new_v4();
        let entries = vec![
            attribution(from, local(2026, 8, 3, 14, 0)),
            attribution(from, local(2026, 8, 3, 17, 30)),
        ];
        let plan = plan_reattribution(from, Uuid::new_v4(), &entries).expect("valid plan");
        assert_eq!(plan.affected_half_days.len(), 1);
        assert_eq!(plan.affected_half_days[0].half_day, HalfDay::Afternoon);
    }

    /// Only the half-days that actually carry a moved entry: a range selection that
    /// spans a quiet day must not send the repair to rebuild that day's slots.
    #[test]
    fn affected_half_days_are_sorted_deduplicated_and_hold_no_quiet_day() {
        let from = Uuid::new_v4();
        let entries = vec![
            attribution(from, local(2026, 8, 5, 9, 0)),
            attribution(from, local(2026, 8, 3, 16, 0)),
            attribution(from, local(2026, 8, 3, 9, 0)),
            attribution(from, local(2026, 8, 3, 10, 0)),
        ];
        let plan = plan_reattribution(from, Uuid::new_v4(), &entries).expect("valid plan");
        let rendered: Vec<(NaiveDate, HalfDay)> = plan
            .affected_half_days
            .iter()
            .map(|unit| (unit.date, unit.half_day))
            .collect();
        assert_eq!(
            rendered,
            vec![
                (
                    NaiveDate::from_ymd_opt(2026, 8, 3).expect("valid date"),
                    HalfDay::Morning
                ),
                (
                    NaiveDate::from_ymd_opt(2026, 8, 3).expect("valid date"),
                    HalfDay::Afternoon
                ),
                (
                    NaiveDate::from_ymd_opt(2026, 8, 5).expect("valid date"),
                    HalfDay::Morning
                ),
            ]
        );
        assert_eq!(
            plan.affected_dates(),
            vec![
                NaiveDate::from_ymd_opt(2026, 8, 3).expect("valid date"),
                NaiveDate::from_ymd_opt(2026, 8, 5).expect("valid date"),
            ]
        );
    }

    /// The same entry named twice (`--entry 7c1 --entry 7c1b` resolving to one row)
    /// must move once. Counting it twice would make the report claim more than
    /// happened.
    #[test]
    fn the_same_entry_named_twice_moves_once() {
        let from = Uuid::new_v4();
        let entry = attribution(from, local(2026, 8, 3, 9, 0));
        let plan = plan_reattribution(from, Uuid::new_v4(), &[entry.clone(), entry.clone()])
            .expect("valid plan");
        assert_eq!(plan.entry_ids, vec![entry.id]);
    }

    #[test]
    fn selection_order_is_preserved() {
        let from = Uuid::new_v4();
        let first = attribution(from, local(2026, 8, 3, 9, 0));
        let second = attribution(from, local(2026, 8, 3, 10, 0));
        let plan = plan_reattribution(from, Uuid::new_v4(), &[first.clone(), second.clone()])
            .expect("valid plan");
        assert_eq!(plan.entry_ids, vec![first.id, second.id]);
    }

    // ─── Hours ───────────────────────────────────────────────────────────────

    #[test]
    fn closed_slots_are_summed_to_the_minute() {
        let task = Uuid::new_v4();
        let slots = vec![
            slot(
                Some(task),
                utc(2026, 8, 3, 8, 0),
                Some(utc(2026, 8, 3, 10, 30)),
            ),
            slot(
                Some(task),
                utc(2026, 8, 3, 13, 0),
                Some(utc(2026, 8, 3, 14, 0)),
            ),
        ];
        assert_eq!(slot_hours(&slots), 3.5);
    }

    /// A running timer has no duration yet, and must not be counted as one — the
    /// same rule the activity report applies.
    #[test]
    fn an_open_slot_counts_for_nothing_and_is_never_rebuilt() {
        let running = slot(Some(Uuid::new_v4()), utc(2026, 8, 3, 8, 0), None);
        assert_eq!(slot_hours(std::slice::from_ref(&running)), 0.0);
        assert!(!is_rebuildable(&running));
    }

    #[test]
    fn a_closed_slot_is_rebuildable() {
        let closed = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 3, 8, 0),
            Some(utc(2026, 8, 3, 9, 0)),
        );
        assert!(is_rebuildable(&closed));
    }

    /// A task that ends the day with nothing left must report `0.0`, not `-0.0`: the
    /// two compare equal, so only the sign check catches it, and the JSON a caller
    /// reads shows the difference.
    #[test]
    fn no_slots_is_a_positive_zero() {
        assert_eq!(slot_hours(&[]), 0.0);
        assert!(slot_hours(&[]).is_sign_positive());
    }
}
