use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type UserId = Uuid;
pub type TaskId = Uuid;
pub type MeetingId = Uuid;
pub type ProjectId = Uuid;
pub type ActivitySlotId = Uuid;
pub type AlertId = Uuid;
pub type TagId = Uuid;
pub type TaskLinkId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Source {
    Jira,
    Excel,
    Obsidian,
    Personal,
    Outlook,
    Gryzzly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
    Blocked,
    /// Used to mark a skipped occurrence of a recurring task.
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum UrgencyLevel {
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum ImpactLevel {
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HalfDay {
    Morning,
    Afternoon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertType {
    Deadline,
    Overload,
    Conflict,
    TimesheetReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AlertSeverity {
    Information,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectStatus {
    Active,
    Paused,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncSourceStatus {
    Idle,
    Syncing,
    Success,
    Error,

    /// The connector has no usable credentials, so no sync was attempted. Distinct
    /// from `Error`: nothing failed, there is simply nothing configured. Previously
    /// recorded as `Error` with the message "Not configured", which made the UI
    /// paint an unconfigured source as a failure.
    NotConfigured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Quadrant {
    UrgentImportant,
    Important,
    Urgent,
    Neither,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskLinkType {
    AutoMerged,
    ManualMerged,
    Rejected,
}

/// How much the reconstruction trusts an allocation / a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrackingState {
    #[default]
    Inbox,
    Followed,
    Dismissed,
}

impl std::fmt::Display for TrackingState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inbox => write!(f, "inbox"),
            Self::Followed => write!(f, "followed"),
            Self::Dismissed => write!(f, "dismissed"),
        }
    }
}

impl std::str::FromStr for TrackingState {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "inbox" => Ok(Self::Inbox),
            "followed" => Ok(Self::Followed),
            "dismissed" => Ok(Self::Dismissed),
            _ => Err(format!("Invalid tracking state: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_state_display_roundtrip() {
        let states = [TrackingState::Inbox, TrackingState::Followed, TrackingState::Dismissed];
        for state in &states {
            let s = state.to_string();
            let parsed: TrackingState = s.parse().unwrap();
            assert_eq!(&parsed, state);
        }
    }

    #[test]
    fn tracking_state_default_is_inbox() {
        assert_eq!(TrackingState::default(), TrackingState::Inbox);
    }

    #[test]
    fn tracking_state_invalid_string_errors() {
        let result: Result<TrackingState, _> = "invalid".parse();
        assert!(result.is_err());
    }
}
