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
            _ => None,
        }
    }

    /// Whether this outcome describes a break the user was actually shown.
    pub fn counts_towards_adherence(&self) -> bool {
        matches!(
            self,
            BreakOutcome::Taken
                | BreakOutcome::Snoozed
                | BreakOutcome::Skipped
                | BreakOutcome::Ignored
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
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
