use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{BreakRuleId, UserId};

/// What a break is for. Drives the notification icon and the seeded copy; it is
/// deliberately a closed set so the UI can render one control per kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakKind {
    Visual,
    Posture,
    Long,
    Strength,
}

impl BreakKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            BreakKind::Visual => "visual",
            BreakKind::Posture => "posture",
            BreakKind::Long => "long",
            BreakKind::Strength => "strength",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "visual" => Some(BreakKind::Visual),
            "posture" => Some(BreakKind::Posture),
            "long" => Some(BreakKind::Long),
            "strength" => Some(BreakKind::Strength),
            _ => None,
        }
    }
}

/// Passed straight through to the notification daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakUrgency {
    Low,
    Normal,
    Critical,
}

impl BreakUrgency {
    pub fn as_str(&self) -> &'static str {
        match self {
            BreakUrgency::Low => "low",
            BreakUrgency::Normal => "normal",
            BreakUrgency::Critical => "critical",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "low" => Some(BreakUrgency::Low),
            "normal" => Some(BreakUrgency::Normal),
            "critical" => Some(BreakUrgency::Critical),
            _ => None,
        }
    }
}

/// How often a rule comes due.
///
/// Modelled as a sum type rather than two nullable fields so the interval-XOR-daily
/// invariant cannot be violated in memory. Storage keeps two nullable columns plus a
/// cross-column CHECK, and the repository is the only place the two shapes meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakCadence {
    /// Anchored on each working window's start, never on the last fire.
    Interval { minutes: u32 },
    /// A wall-clock time in the user's timezone, resolved to UTC by the application.
    Daily { at: NaiveTime },
}

impl BreakCadence {
    pub fn interval_minutes(&self) -> Option<u32> {
        match self {
            BreakCadence::Interval { minutes } => Some(*minutes),
            BreakCadence::Daily { .. } => None,
        }
    }

    pub fn at_time(&self) -> Option<NaiveTime> {
        match self {
            BreakCadence::Interval { .. } => None,
            BreakCadence::Daily { at } => Some(*at),
        }
    }
}

/// One rhythm of the routine. This is what the settings screen edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakRule {
    pub id: BreakRuleId,
    pub user_id: UserId,
    pub kind: BreakKind,
    /// Notification title.
    pub label: String,
    /// Notification body: what to actually do.
    pub body: String,
    pub cadence: BreakCadence,
    pub duration_seconds: u32,
    /// Breaks collision ties when several rules come due in the same tick, and
    /// orders the settings list. Higher wins.
    pub priority: i32,
    pub enabled: bool,
    pub urgency: BreakUrgency,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_round_trips_through_its_storage_string() {
        for k in [BreakKind::Visual, BreakKind::Posture, BreakKind::Long, BreakKind::Strength] {
            assert_eq!(BreakKind::from_str(k.as_str()), Some(k));
        }
        assert_eq!(BreakKind::from_str("nope"), None);
    }

    #[test]
    fn urgency_round_trips_through_its_storage_string() {
        for u in [BreakUrgency::Low, BreakUrgency::Normal, BreakUrgency::Critical] {
            assert_eq!(BreakUrgency::from_str(u.as_str()), Some(u));
        }
        assert_eq!(BreakUrgency::from_str(""), None);
    }

    /// The cadence enum is what enforces interval-XOR-daily in the type system;
    /// the database CHECK (Task 2) enforces the same thing in storage.
    #[test]
    fn cadence_carries_exactly_one_shape() {
        let i = BreakCadence::Interval { minutes: 20 };
        let d = BreakCadence::Daily { at: NaiveTime::from_hms_opt(14, 0, 0).unwrap() };
        assert_eq!(i.interval_minutes(), Some(20));
        assert_eq!(i.at_time(), None);
        assert_eq!(d.interval_minutes(), None);
        assert_eq!(d.at_time(), Some(NaiveTime::from_hms_opt(14, 0, 0).unwrap()));
    }
}
