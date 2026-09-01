use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{BreakEventId, BreakRuleId, UserId};

/// What became of one due slot.
///
/// `Skipped` and `Ignored` are kept apart on purpose: systematically *ignoring* the
/// 20-minute break says the cadence is wrong, while explicitly *skipping* says the
/// timing was wrong. Those are two different fixes, and collapsing them would erase
/// the only signal that tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakOutcome {
    /// Created, unresolved: either deferred, or fired and awaiting an answer.
    Pending,
    Taken,
    Snoozed,
    Skipped,
    /// Fired, closed without a choice.
    Ignored,
    /// Collapsed by coalescing. The user never saw it.
    Absorbed,
    /// Could no longer usefully fire.
    Expired,
    /// The break was opened and cut short before its deadline.
    Abandoned,
}

impl BreakOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            BreakOutcome::Pending => "pending",
            BreakOutcome::Taken => "taken",
            BreakOutcome::Snoozed => "snoozed",
            BreakOutcome::Skipped => "skipped",
            BreakOutcome::Ignored => "ignored",
            BreakOutcome::Absorbed => "absorbed",
            BreakOutcome::Expired => "expired",
            BreakOutcome::Abandoned => "abandoned",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(BreakOutcome::Pending),
            "taken" => Some(BreakOutcome::Taken),
            "snoozed" => Some(BreakOutcome::Snoozed),
            "skipped" => Some(BreakOutcome::Skipped),
            "ignored" => Some(BreakOutcome::Ignored),
            "absorbed" => Some(BreakOutcome::Absorbed),
            "expired" => Some(BreakOutcome::Expired),
            "abandoned" => Some(BreakOutcome::Abandoned),
            _ => None,
        }
    }

    /// Whether this outcome describes a break the user was actually shown.
    ///
    /// `Abandoned` belongs here: the notification was seen, the user answered it, and
    /// the break did not run to its end. That is a measured failure, not the
    /// scheduling noise `Absorbed` and `Expired` describe.
    pub fn counts_towards_adherence(&self) -> bool {
        matches!(
            self,
            BreakOutcome::Taken
                | BreakOutcome::Snoozed
                | BreakOutcome::Skipped
                | BreakOutcome::Ignored
                | BreakOutcome::Abandoned
        )
    }
}

/// Why a slot is waiting instead of firing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeferReason {
    Meeting,
    Snooze,
}

impl DeferReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeferReason::Meeting => "meeting",
            DeferReason::Snooze => "snooze",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "meeting" => Some(DeferReason::Meeting),
            "snooze" => Some(DeferReason::Snooze),
            _ => None,
        }
    }
}

/// One due slot and its fate. Persisting this is what makes deferral survive an
/// API restart, and what makes adherence measurable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakEvent {
    pub id: BreakEventId,
    pub user_id: UserId,
    pub rule_id: BreakRuleId,
    /// The instant the cadence designated.
    pub due_at: DateTime<Utc>,
    /// When the notification actually went out. `None` while deferred, and also
    /// after a delivery failure.
    pub fired_at: Option<DateTime<Utc>>,
    pub deferred_until: Option<DateTime<Utc>>,
    pub defer_reason: Option<DeferReason>,
    /// Audit trail for "why didn't it fire".
    pub suppressed_by_meeting_id: Option<String>,
    pub outcome: BreakOutcome,
    pub responded_at: Option<DateTime<Utc>>,
    /// When the user opened the break. `None` until they press the button.
    pub started_at: Option<DateTime<Utc>>,
    /// The deadline, frozen at `started_at + rule.duration_seconds` when the session
    /// opens. Frozen rather than recomputed from the rule so that retuning
    /// `duration_seconds` in the settings screen cannot lengthen a break already
    /// under way, and so that backend and HUD read one absolute instant instead of
    /// two counters that can drift apart.
    pub ends_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl BreakEvent {
    /// Whether a break is currently being served on this row. The end-of-day sweep
    /// reads it to leave such a row alone: its owner will close it.
    pub fn session_is_running(&self, now: DateTime<Utc>) -> bool {
        self.started_at.is_some() && self.ends_at.is_some_and(|e| e > now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, h, m, 0).unwrap()
    }

    fn event(
        started_at: Option<DateTime<Utc>>,
        ends_at: Option<DateTime<Utc>>,
    ) -> BreakEvent {
        BreakEvent {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            rule_id: Uuid::new_v4(),
            due_at: at(16, 58),
            fired_at: Some(at(16, 58)),
            deferred_until: None,
            defer_reason: None,
            suppressed_by_meeting_id: None,
            outcome: BreakOutcome::Pending,
            responded_at: None,
            started_at,
            ends_at,
            created_at: at(16, 58),
        }
    }

    #[test]
    fn outcome_round_trips_through_its_storage_string() {
        for o in [
            BreakOutcome::Pending,
            BreakOutcome::Taken,
            BreakOutcome::Snoozed,
            BreakOutcome::Skipped,
            BreakOutcome::Ignored,
            BreakOutcome::Absorbed,
            BreakOutcome::Expired,
            BreakOutcome::Abandoned,
        ] {
            assert_eq!(BreakOutcome::from_str(o.as_str()), Some(o));
        }
        assert_eq!(BreakOutcome::from_str("dismissed"), None);
    }

    #[test]
    fn defer_reason_round_trips_through_its_storage_string() {
        for r in [DeferReason::Meeting, DeferReason::Snooze] {
            assert_eq!(DeferReason::from_str(r.as_str()), Some(r));
        }
    }

    /// An abandoned break is a *measured failure*, not scheduling noise: the
    /// notification was seen, the user answered it, and the break simply did not run
    /// to its end. That is the opposite of `absorbed` and `expired`, which never
    /// reached a screen — so it belongs on both sides of the ratio.
    #[test]
    fn abandoned_counts_towards_adherence() {
        assert_eq!(BreakOutcome::from_str("abandoned"), Some(BreakOutcome::Abandoned));
        assert_eq!(BreakOutcome::Abandoned.as_str(), "abandoned");
        assert!(BreakOutcome::Abandoned.counts_towards_adherence());
    }

    /// A running session is what lets a break outlive the end-of-day sweep, so the
    /// predicate has to be exact on both halves: no start means no session at all, and
    /// a deadline already past means the session is over rather than running.
    #[test]
    fn a_session_runs_only_between_its_start_and_its_deadline() {
        let now = at(17, 0);
        assert!(event(Some(at(16, 58)), Some(at(17, 3))).session_is_running(now));
        assert!(!event(Some(at(16, 50)), Some(at(16, 55))).session_is_running(now));
        assert!(!event(None, None).session_is_running(now));
        // Started, but no deadline was ever frozen: nothing says when it would end,
        // so nothing is running.
        assert!(!event(Some(at(16, 58)), None).session_is_running(now));
    }

    /// Adherence counts what the user actually saw. `absorbed` never reached a
    /// screen, so it must count neither for nor against.
    #[test]
    fn only_seen_outcomes_count_towards_adherence() {
        assert!(BreakOutcome::Taken.counts_towards_adherence());
        assert!(BreakOutcome::Skipped.counts_towards_adherence());
        assert!(BreakOutcome::Ignored.counts_towards_adherence());
        assert!(BreakOutcome::Snoozed.counts_towards_adherence());
        assert!(!BreakOutcome::Absorbed.counts_towards_adherence());
        assert!(!BreakOutcome::Expired.counts_towards_adherence());
        assert!(!BreakOutcome::Pending.counts_towards_adherence());
    }
}
