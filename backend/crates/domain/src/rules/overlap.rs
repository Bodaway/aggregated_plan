//! Finding where two different tasks' slots claim the same stretch of time.
//!
//! Several Claude Code sessions — and the human, working by hand — can log time
//! concurrently, so two tasks can legitimately claim the same hour. Nothing here
//! corrects that: each task keeps the time its own entries document, the
//! collision is only flagged, and the user arbitrates at the timesheet review
//! (Task 8 pairs an [`Overlap`] back to its slots to resolve titles; Task 9
//! shows it). `domain` stays free of I/O, so this returns slot ids, never a
//! task title.
//!
//! Pairs, not merged spans: a merged span would report "something overlapped
//! here" without saying which two tasks collided, and the user needs exactly
//! that to arbitrate. Three mutually overlapping slots therefore yield three
//! pairs, one per collision, never one span covering all of them.

use crate::types::activity::ActivitySlot;
use crate::types::common::ActivitySlotId;

/// Two different tasks' slots that claim an overlapping stretch of time, and how
/// many minutes of it they share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overlap {
    pub a: ActivitySlotId,
    pub b: ActivitySlotId,
    pub minutes: i64,
}

/// Every pair of closed, tagged slots on different tasks whose intervals share
/// more than an instant.
///
/// Three exclusions apply before a pair is even considered:
/// - **Open** slots (`end_time: None`) hold no measured hours yet.
/// - **Untagged** slots (`task_id: None`) are time attributed to nobody, so they
///   cannot be "two tasks claiming the same hour" — and there is no task name
///   for Task 9 to print for one anyway.
/// - **Same-task** pairs are not a collision: a task legitimately has several
///   stretches of work in a half-day.
///
/// Touching is not overlapping: intervals that meet exactly at a point (one's
/// `end_time` equal to the other's `start_time`) share zero duration, so a day
/// of back-to-back slots reports nothing.
///
/// Quadratic in the slot count, which is fine at the scale a personal
/// timesheet ever reaches.
pub fn find_overlaps(slots: &[ActivitySlot]) -> Vec<Overlap> {
    let mut overlaps = Vec::new();
    for (i, first) in slots.iter().enumerate() {
        let (first_task, first_end) = match (first.task_id, first.end_time) {
            (Some(task), Some(end)) => (task, end),
            _ => continue,
        };
        for second in &slots[i + 1..] {
            let (second_task, second_end) = match (second.task_id, second.end_time) {
                (Some(task), Some(end)) => (task, end),
                _ => continue,
            };
            if first_task == second_task {
                continue;
            }
            let overlap_start = first.start_time.max(second.start_time);
            let overlap_end = first_end.min(second_end);
            if overlap_end > overlap_start {
                overlaps.push(Overlap {
                    a: first.id,
                    b: second.id,
                    minutes: (overlap_end - overlap_start).num_minutes(),
                });
            }
        }
    }
    overlaps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::activity::SlotSource;
    use crate::types::common::{HalfDay, TaskId};
    use chrono::{DateTime, TimeZone, Utc};
    use uuid::Uuid;

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0)
            .single()
            .expect("valid instant")
    }

    fn slot(task_id: Option<TaskId>, start: DateTime<Utc>, end: Option<DateTime<Utc>>) -> ActivitySlot {
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
            source: SlotSource::Manual,
        }
    }

    /// Finds the reported pair naming both ids, whichever side each landed on.
    fn overlap_between(overlaps: &[Overlap], x: ActivitySlotId, y: ActivitySlotId) -> Overlap {
        *overlaps
            .iter()
            .find(|o| (o.a == x && o.b == y) || (o.a == y && o.b == x))
            .expect("expected pair not found among the reported overlaps")
    }

    #[test]
    fn two_disjoint_slots_do_not_overlap() {
        let first = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 4, 9, 0),
            Some(utc(2026, 8, 4, 10, 0)),
        );
        let second = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 4, 10, 30),
            Some(utc(2026, 8, 4, 11, 30)),
        );
        assert!(find_overlaps(&[first, second]).is_empty());
    }

    /// `end == start` shares zero duration: a day of back-to-back slots must
    /// report nothing.
    #[test]
    fn touching_slots_do_not_overlap() {
        let first = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 4, 9, 0),
            Some(utc(2026, 8, 4, 10, 0)),
        );
        let second = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 4, 10, 0),
            Some(utc(2026, 8, 4, 11, 0)),
        );
        assert!(find_overlaps(&[first, second]).is_empty());
    }

    #[test]
    fn a_partial_intersection_is_measured_in_minutes() {
        let first = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 4, 9, 0),
            Some(utc(2026, 8, 4, 10, 0)),
        );
        let second = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 4, 9, 30),
            Some(utc(2026, 8, 4, 11, 0)),
        );
        let overlaps = find_overlaps(&[first, second]);
        assert_eq!(overlaps.len(), 1);
        assert_eq!(overlaps[0].minutes, 30);
    }

    #[test]
    fn a_nested_slot_overlaps_by_its_own_length() {
        let outer = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 4, 9, 0),
            Some(utc(2026, 8, 4, 12, 0)),
        );
        let inner = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 4, 10, 0),
            Some(utc(2026, 8, 4, 10, 30)),
        );
        let overlaps = find_overlaps(&[outer, inner]);
        assert_eq!(overlaps.len(), 1);
        assert_eq!(overlaps[0].minutes, 30);
    }

    #[test]
    fn two_slots_on_the_same_task_are_not_an_overlap() {
        let task = Uuid::new_v4();
        let first = slot(
            Some(task),
            utc(2026, 8, 4, 9, 0),
            Some(utc(2026, 8, 4, 10, 0)),
        );
        let second = slot(
            Some(task),
            utc(2026, 8, 4, 9, 30),
            Some(utc(2026, 8, 4, 10, 30)),
        );
        assert!(find_overlaps(&[first, second]).is_empty());
    }

    /// A running timer holds no hours yet — even one whose start lands it
    /// squarely inside another task's closed slot.
    #[test]
    fn an_open_slot_is_never_an_overlap() {
        let closed = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 4, 9, 0),
            Some(utc(2026, 8, 4, 11, 0)),
        );
        let open = slot(Some(Uuid::new_v4()), utc(2026, 8, 4, 9, 30), None);
        assert!(find_overlaps(&[closed, open]).is_empty());
    }

    /// `task_id: None` is time attributed to nobody: it cannot be "two tasks
    /// claiming the same hour", however squarely its times sit inside a tagged
    /// slot.
    #[test]
    fn an_untagged_slot_is_never_an_overlap() {
        let tagged = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 4, 9, 0),
            Some(utc(2026, 8, 4, 11, 0)),
        );
        let untagged = slot(None, utc(2026, 8, 4, 9, 30), Some(utc(2026, 8, 4, 10, 0)));
        assert!(find_overlaps(&[tagged, untagged]).is_empty());
    }

    /// Pairs, not a merged span: each of the three collisions must be reported
    /// on its own, naming the two slots involved and the minutes they share.
    #[test]
    fn three_mutually_overlapping_slots_yield_three_pairs() {
        let a = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 4, 9, 0),
            Some(utc(2026, 8, 4, 11, 0)),
        );
        let b = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 4, 10, 0),
            Some(utc(2026, 8, 4, 12, 0)),
        );
        let c = slot(
            Some(Uuid::new_v4()),
            utc(2026, 8, 4, 9, 30),
            Some(utc(2026, 8, 4, 10, 30)),
        );

        let overlaps = find_overlaps(&[a.clone(), b.clone(), c.clone()]);
        assert_eq!(overlaps.len(), 3);

        assert_eq!(overlap_between(&overlaps, a.id, b.id).minutes, 60);
        assert_eq!(overlap_between(&overlaps, a.id, c.id).minutes, 60);
        assert_eq!(overlap_between(&overlaps, b.id, c.id).minutes, 30);
    }
}
