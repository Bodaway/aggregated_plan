use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::types::*;

use crate::errors::RepositoryError;

/// Persistence for the session actors.
///
/// Note what is absent: no `delete`. A session is history — which entries it wrote,
/// which half-days its flush owns — and history that can vanish is history the
/// reattribution repair cannot reason about. Sessions end; they do not disappear.
#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn find_by_id(
        &self,
        id: &str,
        user_id: UserId,
    ) -> Result<Option<Session>, RepositoryError>;

    /// Insert, or overwrite the mutable columns of an existing row: `task_id`,
    /// `mode`, `label`, `last_seen_at`. `started_at` is never rewritten — a session
    /// that rebinds is the same session, and plan 2's flush window is anchored on it.
    async fn upsert(&self, session: &Session) -> Result<(), RepositoryError>;

    /// Open sessions, most recently seen first. What `aplan sessions` prints.
    async fn list_open(&self, user_id: UserId) -> Result<Vec<Session>, RepositoryError>;

    /// Open sessions whose `last_seen_at` is older than `idle_before`, oldest first.
    /// What the reaper reads.
    async fn list_idle_open(
        &self,
        user_id: UserId,
        idle_before: DateTime<Utc>,
    ) -> Result<Vec<Session>, RepositoryError>;

    /// Bump `last_seen_at`. Returns false when no open session has that id.
    async fn touch(
        &self,
        id: &str,
        user_id: UserId,
        at: DateTime<Utc>,
    ) -> Result<bool, RepositoryError>;

    /// Advance the flush watermark of one session. Plan 2's flush calls this.
    async fn set_last_flush(
        &self,
        id: &str,
        user_id: UserId,
        at: DateTime<Utc>,
    ) -> Result<bool, RepositoryError>;

    /// Close the session. Idempotent: an already-ended session keeps its first
    /// `ended_at`, because that is when the work actually stopped.
    async fn end(
        &self,
        id: &str,
        user_id: UserId,
        at: DateTime<Utc>,
    ) -> Result<bool, RepositoryError>;
}
