//! Repair of activity slots the worklog projection owns but that lost their task.
//!
//! ## What an orphan is, and why one exists at all
//!
//! `activity_slots.task_id` is declared `ON DELETE SET NULL`. A latent defect wrote
//! tasks back with `INSERT OR REPLACE INTO tasks`, whose delete-then-insert fired that
//! clause: the task row came back identical — nothing in `tasks` looked wrong — while
//! every slot pointing at it silently lost its `task_id`. The result reads as "(no
//! task)" in `aplan journal`: hours that exist, are worth something, and belong to
//! nobody.
//!
//! ## Why such a slot cannot be repaired in place
//!
//! The slot no longer holds any trace of which task it was. The id is gone and no
//! other column names it, so re-pointing it means guessing, and guessing here bills
//! the wrong client. What *does* still name it is the worklog: the entries the slot
//! was projected from were never touched, and each one carries its own `task_id`. So
//! the repair drops the orphan and lets the projection rewrite the half-day from
//! those entries — the one source of truth that survived. Which is also why the unit
//! of repair is the (local day, half-day): that is the unit a slot is written in, so
//! it is the smallest scope a rebuild from the entries can be exact over.
//!
//! ## The line this module draws
//!
//! A slot is in scope only when it is **both** unattributed **and**
//! [`is_rebuildable`] — closed, and owned by the projection. That second half is not
//! a detail: a `Manual` slot with no task is not damage. It is a hand-run timer from
//! before migration `014` gave slots a `session_id` and a `source`, it never had a
//! task, and the worklog cannot reproduce it. Dropping one destroys time nothing can
//! rebuild, so the predicate is reused rather than re-derived here — a second copy of
//! "may this be replaced" is how the protected side of the line gets forgotten.

use crate::rules::reattribution::{is_rebuildable, AffectedHalfDay};
use crate::types::activity::ActivitySlot;
use crate::types::common::HalfDay;

/// Is this slot an unattributed projection the worklog can rebuild?
///
/// Both halves are load-bearing, and each rules out a different disaster — see the
/// module documentation, and [`is_rebuildable`] for the second half's own two
/// conditions.
pub fn is_repairable_orphan(slot: &ActivitySlot) -> bool {
    slot.task_id.is_none() && is_rebuildable(slot)
}

/// The (local day, half-day) units that hold a repairable orphan, ascending,
/// deduplicated.
///
/// Derived from the orphans rather than from the caller's date range: a range is how
/// an operator addresses the repair, but rebuilding a half-day that holds no orphan
/// would canonicalise slots nobody complained about — work this repair has no mandate
/// for. Sorted with the morning before the afternoon, the same order
/// [`crate::rules::reattribution::plan_reattribution`] reports its own half-days in,
/// so the two repairs read alike.
pub fn orphaned_half_days(slots: &[ActivitySlot]) -> Vec<AffectedHalfDay> {
    let mut units: Vec<AffectedHalfDay> = Vec::new();
    for slot in slots.iter().filter(|slot| is_repairable_orphan(slot)) {
        let unit = AffectedHalfDay {
            date: slot.date,
            half_day: slot.half_day,
        };
        if !units.contains(&unit) {
            units.push(unit);
        }
    }
    units.sort_by_key(|unit| (unit.date, matches!(unit.half_day, HalfDay::Afternoon)));
    units
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::activity::SlotSource;
    use crate::types::common::TaskId;
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};
    use uuid::Uuid;

    fn date(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, d).expect("valid date")
    }

    fn utc(d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, d, h, 0, 0)
            .single()
            .expect("valid instant")
    }

    /// A slot as a flush left it, then optionally stripped of its task the way
    /// `ON DELETE SET NULL` stripped the real ones.
    fn slot(
        task_id: Option<TaskId>,
        d: u32,
        half_day: HalfDay,
        source: SlotSource,
        end: Option<DateTime<Utc>>,
    ) -> ActivitySlot {
        ActivitySlot {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            task_id,
            start_time: utc(d, 9),
            end_time: end,
            half_day,
            date: date(d),
            created_at: utc(d, 9),
            session_id: None,
            source,
        }
    }

    fn worklog_orphan(d: u32, half_day: HalfDay) -> ActivitySlot {
        slot(None, d, half_day, SlotSource::Worklog, Some(utc(d, 11)))
    }

    #[test]
    fn a_closed_worklog_slot_with_no_task_is_the_shape_this_repair_fixes() {
        assert!(is_repairable_orphan(&worklog_orphan(4, HalfDay::Afternoon)));
    }

    /// The 95 rows this repair must not touch: hand-run timers that never had a task.
    #[test]
    fn a_manual_slot_with_no_task_is_not_damage_and_is_never_repairable() {
        let hand_made = slot(None, 4, HalfDay::Morning, SlotSource::Manual, Some(utc(4, 11)));
        assert!(!is_repairable_orphan(&hand_made));
    }

    /// An open slot is a running timer: it holds no hours and stopping it is not this
    /// repair's business.
    #[test]
    fn an_open_slot_with_no_task_is_never_repairable() {
        let running = slot(None, 4, HalfDay::Morning, SlotSource::Worklog, None);
        assert!(!is_repairable_orphan(&running));
    }

    #[test]
    fn a_slot_that_still_has_its_task_is_not_an_orphan() {
        let intact = slot(
            Some(Uuid::new_v4()),
            4,
            HalfDay::Morning,
            SlotSource::Worklog,
            Some(utc(4, 11)),
        );
        assert!(!is_repairable_orphan(&intact));
    }

    #[test]
    fn the_units_are_deduplicated_and_ordered_morning_first() {
        let slots = vec![
            worklog_orphan(10, HalfDay::Afternoon),
            worklog_orphan(4, HalfDay::Afternoon),
            worklog_orphan(10, HalfDay::Morning),
            worklog_orphan(10, HalfDay::Afternoon),
        ];
        let units: Vec<(NaiveDate, HalfDay)> = orphaned_half_days(&slots)
            .iter()
            .map(|unit| (unit.date, unit.half_day))
            .collect();
        assert_eq!(
            units,
            vec![
                (date(4), HalfDay::Afternoon),
                (date(10), HalfDay::Morning),
                (date(10), HalfDay::Afternoon),
            ]
        );
    }

    /// A half-day whose only unattributed slot is a hand-made one must not enter the
    /// repair's scope at all: naming it would send the rebuild to canonicalise slots
    /// that were never damaged.
    #[test]
    fn a_half_day_holding_only_protected_slots_is_not_in_scope() {
        let slots = vec![
            slot(None, 4, HalfDay::Morning, SlotSource::Manual, Some(utc(4, 11))),
            slot(
                Some(Uuid::new_v4()),
                4,
                HalfDay::Afternoon,
                SlotSource::Worklog,
                Some(utc(4, 15)),
            ),
        ];
        assert!(orphaned_half_days(&slots).is_empty());
    }
}
