use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::DomainError;

use super::common::*;

pub type WorklogEntryId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorklogEntry {
    pub id: WorklogEntryId,
    pub user_id: UserId,
    pub task_id: TaskId,
    pub body: String,
    pub logged_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub const WORKLOG_BODY_MAX_LEN: usize = 10_000;

impl WorklogEntry {
    pub fn new(
        user_id: UserId,
        task_id: TaskId,
        body: String,
        logged_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        if body.trim().is_empty() {
            return Err(DomainError::ValidationError(
                "worklog body cannot be empty".into(),
            ));
        }
        if body.chars().count() > WORKLOG_BODY_MAX_LEN {
            return Err(DomainError::ValidationError(format!(
                "worklog body too long (max {} chars)",
                WORKLOG_BODY_MAX_LEN
            )));
        }
        Ok(Self {
            id: Uuid::new_v4(),
            user_id,
            task_id,
            body,
            logged_at,
            created_at: now,
            updated_at: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> UserId {
        Uuid::new_v4()
    }
    fn tid() -> TaskId {
        Uuid::new_v4()
    }
    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-04-21T10:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn new_rejects_empty_body() {
        let err = WorklogEntry::new(uid(), tid(), "".into(), t0(), t0()).unwrap_err();
        assert_eq!(
            err,
            DomainError::ValidationError("worklog body cannot be empty".into())
        );
    }

    #[test]
    fn new_rejects_whitespace_only_body() {
        let err = WorklogEntry::new(uid(), tid(), "   \n\t  ".into(), t0(), t0()).unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)));
    }

    #[test]
    fn new_rejects_oversize_body() {
        let big = "x".repeat(WORKLOG_BODY_MAX_LEN + 1);
        let err = WorklogEntry::new(uid(), tid(), big, t0(), t0()).unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)));
    }

    #[test]
    fn new_accepts_valid_body() {
        let entry = WorklogEntry::new(uid(), tid(), "done the thing".into(), t0(), t0()).unwrap();
        assert_eq!(entry.body, "done the thing");
        assert_eq!(entry.logged_at, t0());
        assert_eq!(entry.created_at, t0());
        assert_eq!(entry.updated_at, t0());
    }

    #[test]
    fn new_accepts_body_at_max_len() {
        let body = "a".repeat(WORKLOG_BODY_MAX_LEN);
        let entry = WorklogEntry::new(uid(), tid(), body.clone(), t0(), t0()).unwrap();
        assert_eq!(entry.body.chars().count(), WORKLOG_BODY_MAX_LEN);
    }
}
