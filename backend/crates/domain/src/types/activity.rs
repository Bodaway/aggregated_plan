use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::*;
use super::session::SessionId;

/// Where a slot came from, and therefore whether anything may replace it.
///
/// `activity_slots` are a projection of worklog timestamps, and the flush rebuilds
/// that projection by dropping what it wrote before. Without this distinction the
/// rebuild has no way to tell its own output from a slot the user created by hand,
/// and the automatic path would silently delete the manual one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotSource {
    /// Written by the worklog projection. A rebuild owns it.
    Worklog,
    /// Anything else: a live timer, a hand-made slot, a row whose provenance is
    /// unknown. Never rebuilt.
    Manual,
}

impl SlotSource {
    /// Is this slot one the worklog projection owns?
    ///
    /// Not the whole answer to "may a rebuild delete and rewrite this slot" — that
    /// is [`crate::rules::reattribution::is_rebuildable`], which also checks the slot
    /// is closed. This method is one input to that question: a rebuild deletes what
    /// the projection wrote and must never delete anything else.
    pub fn is_projection(&self) -> bool {
        matches!(self, SlotSource::Worklog)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySlot {
    pub id: ActivitySlotId,
    pub user_id: UserId,
    pub task_id: Option<TaskId>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub half_day: HalfDay,
    pub date: NaiveDate,
    pub created_at: DateTime<Utc>,
    /// Who produced the time. `None` is the human, working by hand.
    pub session_id: Option<SessionId>,
    pub source: SlotSource,
}

impl ActivitySlot {
    /// A closed slot the worklog projection owns.
    #[allow(clippy::too_many_arguments)]
    pub fn from_worklog(
        user_id: UserId,
        task_id: TaskId,
        session_id: Option<SessionId>,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        half_day: HalfDay,
        date: NaiveDate,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            task_id: Some(task_id),
            start_time,
            end_time: Some(end_time),
            half_day,
            date,
            created_at: now,
            session_id,
            source: SlotSource::Worklog,
        }
    }

    /// A slot no rebuild may touch — including an open one, which is a running timer.
    pub fn manual(
        user_id: UserId,
        task_id: Option<TaskId>,
        start_time: DateTime<Utc>,
        end_time: Option<DateTime<Utc>>,
        half_day: HalfDay,
        date: NaiveDate,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            task_id,
            start_time,
            end_time,
            half_day,
            date,
            created_at: now,
            session_id: None,
            source: SlotSource::Manual,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn uid() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }
    fn t(h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, h, 0, 0).unwrap()
    }

    #[test]
    fn a_worklog_slot_is_rebuildable_and_carries_its_author() {
        let task = Uuid::new_v4();
        let slot = ActivitySlot::from_worklog(
            uid(),
            task,
            Some("sess-1".into()),
            t(9),
            t(11),
            HalfDay::Morning,
            t(9).date_naive(),
            t(11),
        );
        assert_eq!(slot.task_id, Some(task));
        assert_eq!(slot.end_time, Some(t(11)));
        assert_eq!(slot.source, SlotSource::Worklog);
        assert_eq!(slot.session_id.as_deref(), Some("sess-1"));
        assert!(slot.source.is_projection());
    }

    #[test]
    fn a_manual_slot_is_never_rebuildable() {
        // The regression this field exists to prevent: today's flush only ever
        // appends, so nothing protects a hand-made slot from a rebuild that
        // canonicalises the half-day it sits in.
        let slot = ActivitySlot::manual(
            uid(),
            None,
            t(14),
            None,
            HalfDay::Afternoon,
            t(14).date_naive(),
            t(14),
        );
        assert_eq!(slot.source, SlotSource::Manual);
        assert!(slot.session_id.is_none());
        assert!(!slot.source.is_projection());
    }
}
