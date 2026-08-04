use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::DomainError;

use super::common::{ProjectId, TaskId, UserId};

pub type MemoryId = Uuid;

/// What kind of thing is remembered. `decision` and `commitment` outrank `fact`
/// and `preference` when answering "what had we decided?" (see `rules::recall`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryKind {
    Decision,
    Commitment,
    Fact,
    Preference,
}

impl MemoryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryKind::Decision => "decision",
            MemoryKind::Commitment => "commitment",
            MemoryKind::Fact => "fact",
            MemoryKind::Preference => "preference",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "decision" => Some(MemoryKind::Decision),
            "commitment" => Some(MemoryKind::Commitment),
            "fact" => Some(MemoryKind::Fact),
            "preference" => Some(MemoryKind::Preference),
            _ => None,
        }
    }
}

/// Where the memory came from. `dreaming` is reserved for the scheduled
/// consolidation session (later lot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemorySource {
    ClaudeSession,
    Manual,
    Dreaming,
}

impl MemorySource {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemorySource::ClaudeSession => "claude_session",
            MemorySource::Manual => "manual",
            MemorySource::Dreaming => "dreaming",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "claude_session" => Some(MemorySource::ClaudeSession),
            "manual" => Some(MemorySource::Manual),
            "dreaming" => Some(MemorySource::Dreaming),
            _ => None,
        }
    }
}

/// Validation-queue lifecycle. Distinct from the truth lifecycle carried by
/// `invalidated_at` / `superseded_by`: `rejected` is a tombstone that stops the
/// consolidation job from re-proposing the same candidate every evening.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryStatus {
    Pending,
    Active,
    Rejected,
}

impl MemoryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryStatus::Pending => "pending",
            MemoryStatus::Active => "active",
            MemoryStatus::Rejected => "rejected",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(MemoryStatus::Pending),
            "active" => Some(MemoryStatus::Active),
            "rejected" => Some(MemoryStatus::Rejected),
            _ => None,
        }
    }
}

pub const MEMORY_TITLE_MAX_LEN: usize = 500;
pub const MEMORY_BODY_MAX_LEN: usize = 10_000;

/// A semantic memory: what must be known, as opposed to what must be done
/// (`Task`) or what happened (`WorklogEntry`).
///
/// Deadlines never belong here — they live on the `Task` the memory points to,
/// otherwise two divergent reminder engines appear.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub id: MemoryId,
    pub user_id: UserId,
    pub kind: MemoryKind,
    pub title: String,
    pub body: Option<String>,

    /// When it was decided / promised.
    pub occurred_at: DateTime<Utc>,
    /// When aplan learned about it.
    pub recorded_at: DateTime<Utc>,
    /// `None` = still true. Written only by the supersede path.
    pub invalidated_at: Option<DateTime<Utc>>,
    pub superseded_by: Option<MemoryId>,

    /// The memory this candidate CLAIMS to contradict — a supersession proposed and
    /// not yet applied. `superseded_by` says a supersession happened; this says one
    /// was suggested and is waiting for a human verdict.
    ///
    /// Only a `Pending` row may carry it, and every queue verdict clears it (see
    /// `rules::memory_lifecycle`). That is what stops a stale claim from outliving
    /// the triage that answered it and misleading a later reader.
    pub proposed_supersedes: Option<MemoryId>,

    pub source: MemorySource,
    pub source_ref: Option<String>,
    pub status: MemoryStatus,

    pub project_id: Option<ProjectId>,
    pub task_id: Option<TaskId>,
    /// "Towards whom" / "with whom". Persisted in `memory_stakeholders`.
    pub stakeholders: Vec<String>,
}

/// Everything the caller decides when recording a memory. `recorded_at` is
/// always `now`, and `invalidated_at` / `superseded_by` are never settable here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMemory {
    pub kind: MemoryKind,
    pub title: String,
    pub body: Option<String>,
    /// Defaults to `now` when `None`.
    pub occurred_at: Option<DateTime<Utc>>,
    pub source: MemorySource,
    pub source_ref: Option<String>,
    pub status: MemoryStatus,
    /// The memory this candidate claims to contradict. Only legal alongside
    /// `status: Pending` — see [`Memory::proposed_supersedes`].
    pub proposed_supersedes: Option<MemoryId>,
    pub project_id: Option<ProjectId>,
    pub task_id: Option<TaskId>,
    pub stakeholders: Vec<String>,
}

impl Memory {
    /// Build a validated memory. Trims the title and every stakeholder, drops
    /// blank stakeholders, and de-duplicates them (the persisted table has a
    /// `(memory_id, person)` primary key, so duplicates would fail at insert).
    pub fn new(
        user_id: UserId,
        input: NewMemory,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        let title = input.title.trim().to_string();
        if title.is_empty() {
            return Err(DomainError::ValidationError(
                "memory title cannot be empty".into(),
            ));
        }
        if title.chars().count() > MEMORY_TITLE_MAX_LEN {
            return Err(DomainError::ValidationError(format!(
                "memory title too long (max {MEMORY_TITLE_MAX_LEN} chars)"
            )));
        }

        let body = match input.body {
            None => None,
            Some(b) => {
                let trimmed = b.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else if trimmed.chars().count() > MEMORY_BODY_MAX_LEN {
                    return Err(DomainError::ValidationError(format!(
                        "memory body too long (max {MEMORY_BODY_MAX_LEN} chars)"
                    )));
                } else {
                    Some(trimmed)
                }
            }
        };

        let mut stakeholders: Vec<String> = Vec::new();
        for person in input.stakeholders {
            let person = person.trim().to_string();
            if !person.is_empty() && !stakeholders.contains(&person) {
                stakeholders.push(person);
            }
        }

        // A supersession proposal is a question put to the validation queue, so it
        // is only meaningful on a row that is IN the queue. `--confirm` bypasses the
        // queue, and a claim there would never be answered — it would just keep
        // announcing a conflict. Refused rather than dropped: the caller meant
        // something, and `aplan memory supersede` is the verb that does it.
        if input.proposed_supersedes.is_some() && input.status != MemoryStatus::Pending {
            return Err(DomainError::ValidationError(format!(
                "a supersession proposal only applies to a pending candidate, not a {} memory; \
                 revise an established memory with `aplan memory supersede`",
                input.status.as_str()
            )));
        }

        Ok(Self {
            id: Uuid::new_v4(),
            user_id,
            kind: input.kind,
            title,
            body,
            occurred_at: input.occurred_at.unwrap_or(now),
            recorded_at: now,
            invalidated_at: None,
            superseded_by: None,
            proposed_supersedes: input.proposed_supersedes,
            source: input.source,
            source_ref: input.source_ref,
            status: input.status,
            project_id: input.project_id,
            task_id: input.task_id,
            stakeholders,
        })
    }

    /// True when the memory is both validated and still true — the only rows the
    /// default recall path is allowed to return.
    pub fn is_recallable(&self) -> bool {
        self.status == MemoryStatus::Active && self.invalidated_at.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> UserId {
        Uuid::new_v4()
    }

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-03T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn input(title: &str) -> NewMemory {
        NewMemory {
            kind: MemoryKind::Decision,
            title: title.into(),
            body: None,
            occurred_at: None,
            source: MemorySource::ClaudeSession,
            source_ref: None,
            status: MemoryStatus::Pending,
            proposed_supersedes: None,
            project_id: None,
            task_id: None,
            stakeholders: vec![],
        }
    }

    #[test]
    fn memory_kind_roundtrips_through_str() {
        for k in [
            MemoryKind::Decision,
            MemoryKind::Commitment,
            MemoryKind::Fact,
            MemoryKind::Preference,
        ] {
            assert_eq!(MemoryKind::from_str(k.as_str()), Some(k));
        }
        assert_eq!(MemoryKind::from_str("procedure"), None);
    }

    #[test]
    fn memory_source_roundtrips_through_str() {
        for s in [
            MemorySource::ClaudeSession,
            MemorySource::Manual,
            MemorySource::Dreaming,
        ] {
            assert_eq!(MemorySource::from_str(s.as_str()), Some(s));
        }
        assert_eq!(MemorySource::from_str("cron"), None);
    }

    #[test]
    fn memory_status_roundtrips_through_str() {
        for s in [
            MemoryStatus::Pending,
            MemoryStatus::Active,
            MemoryStatus::Rejected,
        ] {
            assert_eq!(MemoryStatus::from_str(s.as_str()), Some(s));
        }
        assert_eq!(MemoryStatus::from_str("archived"), None);
    }

    #[test]
    fn new_rejects_empty_or_blank_title() {
        assert!(matches!(
            Memory::new(uid(), input(""), t0()).unwrap_err(),
            DomainError::ValidationError(_)
        ));
        assert!(matches!(
            Memory::new(uid(), input("   \n "), t0()).unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[test]
    fn new_rejects_oversize_title() {
        let long = "x".repeat(MEMORY_TITLE_MAX_LEN + 1);
        assert!(matches!(
            Memory::new(uid(), input(&long), t0()).unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[test]
    fn new_rejects_oversize_body() {
        let mut i = input("Wave 0 limited to the Microsoft AI scope");
        i.body = Some("y".repeat(MEMORY_BODY_MAX_LEN + 1));
        assert!(matches!(
            Memory::new(uid(), i, t0()).unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[test]
    fn new_trims_title_and_normalizes_blank_body_to_none() {
        let mut i = input("  Wave 0 limited to the Microsoft AI scope  ");
        i.body = Some("   ".into());
        let m = Memory::new(uid(), i, t0()).unwrap();
        assert_eq!(m.title, "Wave 0 limited to the Microsoft AI scope");
        assert_eq!(m.body, None);
    }

    #[test]
    fn new_defaults_occurred_at_to_now_and_always_sets_recorded_at() {
        let m = Memory::new(uid(), input("a decision"), t0()).unwrap();
        assert_eq!(m.occurred_at, t0());
        assert_eq!(m.recorded_at, t0());
    }

    #[test]
    fn new_keeps_an_explicit_backdated_occurred_at() {
        let mut i = input("a decision");
        let earlier = DateTime::parse_from_rfc3339("2026-06-12T14:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        i.occurred_at = Some(earlier);
        let m = Memory::new(uid(), i, t0()).unwrap();
        assert_eq!(m.occurred_at, earlier);
        assert_eq!(m.recorded_at, t0(), "recorded_at is always now");
    }

    #[test]
    fn new_dedupes_and_trims_stakeholders_dropping_blanks() {
        let mut i = input("promise made");
        i.stakeholders = vec![
            " Pierre ".into(),
            "Pierre".into(),
            "".into(),
            "  ".into(),
            "Sophie".into(),
        ];
        let m = Memory::new(uid(), i, t0()).unwrap();
        assert_eq!(m.stakeholders, vec!["Pierre".to_string(), "Sophie".to_string()]);
    }

    #[test]
    fn new_never_sets_the_invalidation_columns() {
        let m = Memory::new(uid(), input("a decision"), t0()).unwrap();
        assert_eq!(m.invalidated_at, None);
        assert_eq!(m.superseded_by, None);
        assert_eq!(m.proposed_supersedes, None);
    }

    #[test]
    fn new_carries_a_supersession_proposal_on_a_candidate() {
        let older = Uuid::new_v4();
        let mut i = input("Wave 0 étendue à toute la plateforme");
        i.proposed_supersedes = Some(older);
        let m = Memory::new(uid(), i, t0()).unwrap();
        assert_eq!(m.proposed_supersedes, Some(older));
        assert_eq!(
            m.invalidated_at, None,
            "a proposal invalidates nothing on its own"
        );
    }

    /// The invariant: a proposal is a question asked of the triage, so it only
    /// means anything while the row is waiting for an answer.
    ///
    /// `--confirm` skips the queue, so a proposal attached to it would sit there
    /// forever, claiming a conflict nobody will ever be asked to settle. Refused
    /// loudly rather than dropped silently, because the caller meant something and
    /// the right verb for it exists (`aplan memory supersede`).
    #[test]
    fn new_refuses_a_proposal_on_a_memory_that_skips_the_queue() {
        for status in [MemoryStatus::Active, MemoryStatus::Rejected] {
            let mut i = input("Wave 0 étendue à toute la plateforme");
            i.status = status;
            i.proposed_supersedes = Some(Uuid::new_v4());
            assert!(
                matches!(
                    Memory::new(uid(), i, t0()).unwrap_err(),
                    DomainError::ValidationError(_)
                ),
                "a {} row must not carry a proposal",
                status.as_str()
            );
        }
    }

    #[test]
    fn is_recallable_requires_active_and_not_invalidated() {
        let mut m = Memory::new(uid(), input("a decision"), t0()).unwrap();
        assert!(!m.is_recallable(), "pending is not recallable");
        m.status = MemoryStatus::Active;
        assert!(m.is_recallable());
        m.invalidated_at = Some(t0());
        assert!(!m.is_recallable(), "invalidated is never recallable");
        m.invalidated_at = None;
        m.status = MemoryStatus::Rejected;
        assert!(!m.is_recallable());
    }
}
