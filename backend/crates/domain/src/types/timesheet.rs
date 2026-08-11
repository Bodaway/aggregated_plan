use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::{Confidence, UserId};

pub type TimesheetDraftId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimesheetStatus {
    Draft,
    Validated,
    Submitted,
    DayOff,
}

impl TimesheetStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimesheetStatus::Draft => "draft",
            TimesheetStatus::Validated => "validated",
            TimesheetStatus::Submitted => "submitted",
            TimesheetStatus::DayOff => "day_off",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(TimesheetStatus::Draft),
            "validated" => Some(TimesheetStatus::Validated),
            "submitted" => Some(TimesheetStatus::Submitted),
            "day_off" => Some(TimesheetStatus::DayOff),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimesheetDraftLine {
    pub id: Uuid,
    pub gryzzly_project_id: Option<String>,
    pub project_name: Option<String>,
    pub hours: f64,
    pub is_pinned: bool,
    pub confidence: Confidence,
    pub source_refs: Vec<String>,
}

/// One persisted quarter share: what a single lane declares inside one quarter-day.
///
/// The persisted twin of [`crate::rules::quarters::Share`], plus the identity needed to
/// write it back. `presence_minutes` is stored, not recomputed on read, so a validated
/// day can still explain the weight its hours came from after the evidence has moved on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarterShareRow {
    pub id: Uuid,
    pub quarter_index: u8,
    /// `None` for a meeting lane, for the unattributed residue, and for a share whose
    /// task was deleted — the row survives that deletion on purpose.
    pub task_id: Option<Uuid>,
    pub lane_key: String,
    pub label: String,
    pub gryzzly_project_id: Option<String>,
    pub presence_minutes: i64,
    pub hours: f64,
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimesheetDraft {
    pub id: TimesheetDraftId,
    pub user_id: UserId,
    pub date: NaiveDate,
    pub status: TimesheetStatus,
    pub target_hours: f64,
    pub total_hours: f64,
    pub day_confidence: Confidence,
    pub blocks_json: Option<String>,
    pub unresolved_json: Option<String>,
    /// The concurrent evidence view, display only and tolerant of absence.
    pub lanes_json: Option<String>,
    pub lines: Vec<TimesheetDraftLine>,
    /// The day's arbitration. Owned by the draft — `upsert` replaces the whole set, the
    /// same contract as `lines`, so there is exactly one writer for the day's hours.
    pub shares: Vec<QuarterShareRow>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrips() {
        for s in [
            TimesheetStatus::Draft,
            TimesheetStatus::Validated,
            TimesheetStatus::Submitted,
            TimesheetStatus::DayOff,
        ] {
            assert_eq!(TimesheetStatus::from_str(s.as_str()), Some(s));
        }
    }
}
