use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;

use super::common::*;

/// A Claude Code session id, exactly as the harness exports it
/// (`CLAUDE_CODE_SESSION_ID`).
///
/// A `String`, not a `Uuid`, on purpose: the value is minted by another program. If
/// the harness ever changes its format, parsing it here would turn every log call
/// into "this session does not exist" — a silent loss of worklog, which is the one
/// failure this whole feature exists to prevent. We store what we are given.
pub type SessionId = String;

/// How long a label may be before it is cut. It is a working directory shown in
/// `aplan sessions`, so length is a display concern, never a reason to fail a bind.
pub const SESSION_LABEL_MAX_LEN: usize = 200;

/// What a session was told to do with its worklog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionMode {
    /// Logging is on, against `Session::task_id`.
    Tracking,
    /// The user answered "ne pas tracker" for this session. Persisted precisely so a
    /// re-fired SessionStart hook reports the decision instead of re-deriving one
    /// from the human's pointer.
    Off,
}

impl SessionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionMode::Tracking => "tracking",
            SessionMode::Off => "off",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "tracking" => Ok(SessionMode::Tracking),
            "off" => Ok(SessionMode::Off),
            other => Err(DomainError::ValidationError(format!(
                "unknown session mode `{other}`"
            ))),
        }
    }
}

/// Why a session cannot be the implicit target of a logging verb.
///
/// Three distinct reasons rather than one boolean, because each one deserves its own
/// sentence at the terminal: "this session is not tracked" is a decision the user
/// made, "no task bound" is a setup step they still owe, and "session ended" means
/// they are looking at a stale id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTargetRefusal {
    Ended,
    NotTracked,
    NoTask,
}

/// One Claude Code session, and what it logs against.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub user_id: UserId,
    pub task_id: Option<TaskId>,
    pub mode: SessionMode,
    pub label: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    /// Up to when this session's time has already been materialized. `None` means
    /// "nothing yet", and the window then starts at `started_at`.
    pub last_flush_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl Session {
    /// A session that logs against `task_id`.
    pub fn tracking(
        id: SessionId,
        user_id: UserId,
        task_id: TaskId,
        label: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        Self::new(id, user_id, Some(task_id), SessionMode::Tracking, label, now)
    }

    /// A session the user opted out of tracking.
    pub fn off(
        id: SessionId,
        user_id: UserId,
        label: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        Self::new(id, user_id, None, SessionMode::Off, label, now)
    }

    fn new(
        id: SessionId,
        user_id: UserId,
        task_id: Option<TaskId>,
        mode: SessionMode,
        label: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        let id = id.trim().to_string();
        if id.is_empty() {
            return Err(DomainError::ValidationError(
                "session id cannot be empty".into(),
            ));
        }
        Ok(Self {
            id,
            user_id,
            task_id,
            mode,
            label: label.map(|l| l.chars().take(SESSION_LABEL_MAX_LEN).collect()),
            started_at: now,
            last_seen_at: now,
            last_flush_at: None,
            ended_at: None,
        })
    }

    pub fn is_open(&self) -> bool {
        self.ended_at.is_none()
    }

    /// The instant a flush should start looking from. Plan 2 uses this to pick the
    /// half-days it rebuilds; it never decides which entries count.
    pub fn flush_window_start(&self) -> DateTime<Utc> {
        self.last_flush_at.unwrap_or(self.started_at)
    }

    /// The task an implicit-target verb should write to, or why it must not write.
    ///
    /// Ended is checked first because it is the most specific state: a stale id is a
    /// different mistake from a deliberate opt-out, and telling the two apart is what
    /// keeps the message useful.
    pub fn target(&self) -> Result<TaskId, SessionTargetRefusal> {
        if self.ended_at.is_some() {
            return Err(SessionTargetRefusal::Ended);
        }
        if self.mode == SessionMode::Off {
            return Err(SessionTargetRefusal::NotTracked);
        }
        self.task_id.ok_or(SessionTargetRefusal::NoTask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn uid() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }
    fn tid() -> TaskId {
        Uuid::new_v4()
    }
    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-04T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn tracking_starts_open_on_its_task() {
        let s = Session::tracking("abc".into(), uid(), tid(), None, t0()).unwrap();
        assert_eq!(s.mode, SessionMode::Tracking);
        assert!(s.is_open());
        assert_eq!(s.started_at, t0());
        assert_eq!(s.last_seen_at, t0());
        assert!(s.last_flush_at.is_none());
    }

    #[test]
    fn an_empty_id_is_refused() {
        let err = Session::tracking("   ".into(), uid(), tid(), None, t0()).unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)));
    }

    #[test]
    fn the_id_is_trimmed_not_reformatted() {
        // The value is minted by another program: we normalise whitespace and keep
        // the rest verbatim, whatever shape it has.
        let s = Session::tracking("  not-a-uuid  ".into(), uid(), tid(), None, t0()).unwrap();
        assert_eq!(s.id, "not-a-uuid");
    }

    #[test]
    fn an_oversize_label_is_truncated_rather_than_refused() {
        // The label is a working directory, not user input. Failing a bind over it
        // would cost a session its worklog for a display string.
        let long = "x".repeat(SESSION_LABEL_MAX_LEN + 50);
        let s = Session::tracking("abc".into(), uid(), tid(), Some(long), t0()).unwrap();
        assert_eq!(s.label.unwrap().chars().count(), SESSION_LABEL_MAX_LEN);
    }

    #[test]
    fn a_tracking_session_targets_its_task() {
        let task = tid();
        let s = Session::tracking("abc".into(), uid(), task, None, t0()).unwrap();
        assert_eq!(s.target(), Ok(task));
    }

    #[test]
    fn an_off_session_refuses_a_target_instead_of_falling_back() {
        // The whole point of the feature: "ne pas tracker" must be a refusal the
        // caller has to handle, never a silent fallback onto the human's pointer.
        let s = Session::off("abc".into(), uid(), None, t0()).unwrap();
        assert_eq!(s.target(), Err(SessionTargetRefusal::NotTracked));
    }

    #[test]
    fn a_tracking_session_without_a_task_refuses_too() {
        let mut s = Session::tracking("abc".into(), uid(), tid(), None, t0()).unwrap();
        s.task_id = None;
        assert_eq!(s.target(), Err(SessionTargetRefusal::NoTask));
    }

    #[test]
    fn an_ended_session_refuses_before_anything_else() {
        let mut s = Session::tracking("abc".into(), uid(), tid(), None, t0()).unwrap();
        s.ended_at = Some(t0() + chrono::Duration::hours(2));
        assert!(!s.is_open());
        assert_eq!(s.target(), Err(SessionTargetRefusal::Ended));
    }

    #[test]
    fn the_flush_window_starts_at_the_last_flush_when_there_was_one() {
        let mut s = Session::tracking("abc".into(), uid(), tid(), None, t0()).unwrap();
        assert_eq!(s.flush_window_start(), t0(), "no flush yet → session start");

        let later = t0() + chrono::Duration::hours(3);
        s.last_flush_at = Some(later);
        assert_eq!(s.flush_window_start(), later);
    }

    #[test]
    fn mode_round_trips_through_its_wire_form() {
        for mode in [SessionMode::Tracking, SessionMode::Off] {
            assert_eq!(SessionMode::parse(mode.as_str()).unwrap(), mode);
        }
        assert!(SessionMode::parse("maybe").is_err());
    }
}
