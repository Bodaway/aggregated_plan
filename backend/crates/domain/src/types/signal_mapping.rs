use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::UserId;

pub type SignalMappingId = Uuid;

/// The kind of signal a mapping rule matches against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MappingKind {
    RepoPath,
    Branch,
    MeetingSubject,
    MeetingOrganizer,
    InternalProject,
}

impl MappingKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MappingKind::RepoPath => "repo_path",
            MappingKind::Branch => "branch",
            MappingKind::MeetingSubject => "meeting_subject",
            MappingKind::MeetingOrganizer => "meeting_organizer",
            MappingKind::InternalProject => "internal_project",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "repo_path" => Some(MappingKind::RepoPath),
            "branch" => Some(MappingKind::Branch),
            "meeting_subject" => Some(MappingKind::MeetingSubject),
            "meeting_organizer" => Some(MappingKind::MeetingOrganizer),
            "internal_project" => Some(MappingKind::InternalProject),
            _ => None,
        }
    }
}

/// A learned rule mapping a raw signal to a Gryzzly project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalMapping {
    pub id: SignalMappingId,
    pub user_id: UserId,
    pub kind: MappingKind,
    pub pattern: String,
    pub branch_pattern: Option<String>,
    pub gryzzly_project_id: String,
    pub gryzzly_project_name: Option<String>,
    pub is_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_kind_roundtrips_through_str() {
        for k in [
            MappingKind::RepoPath,
            MappingKind::Branch,
            MappingKind::MeetingSubject,
            MappingKind::MeetingOrganizer,
            MappingKind::InternalProject,
        ] {
            assert_eq!(MappingKind::from_str(k.as_str()), Some(k));
        }
        assert_eq!(MappingKind::from_str("bogus"), None);
    }
}
