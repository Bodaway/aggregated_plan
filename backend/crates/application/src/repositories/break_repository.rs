use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::errors::RepositoryError;
use domain::types::*;

#[async_trait]
pub trait BreakRuleRepository: Send + Sync {
    /// Every rule, enabled or not, ordered by priority — what the settings screen shows.
    async fn list(&self, user_id: UserId) -> Result<Vec<BreakRule>, RepositoryError>;

    /// Only the enabled rules — what the tick evaluates.
    async fn list_enabled(&self, user_id: UserId) -> Result<Vec<BreakRule>, RepositoryError>;

    async fn get(&self, user_id: UserId, id: BreakRuleId)
        -> Result<Option<BreakRule>, RepositoryError>;

    async fn create(&self, rule: &BreakRule) -> Result<(), RepositoryError>;

    async fn update(&self, rule: &BreakRule) -> Result<(), RepositoryError>;

    async fn delete(&self, user_id: UserId, id: BreakRuleId) -> Result<(), RepositoryError>;
}

#[async_trait]
pub trait BreakEventRepository: Send + Sync {
    /// Events still awaiting resolution: deferred, or fired and unanswered.
    async fn list_open(&self, user_id: UserId) -> Result<Vec<BreakEvent>, RepositoryError>;

    async fn create(&self, event: &BreakEvent) -> Result<(), RepositoryError>;

    /// Resolve an event. `responded_at` is `None` for outcomes the user did not choose
    /// (absorbed, expired).
    async fn set_outcome(
        &self,
        id: BreakEventId,
        outcome: BreakOutcome,
        responded_at: Option<DateTime<Utc>>,
    ) -> Result<(), RepositoryError>;

    /// Arm or re-arm a deferral on an existing event.
    async fn set_deferral(
        &self,
        id: BreakEventId,
        until: DateTime<Utc>,
        reason: DeferReason,
        meeting_id: Option<&str>,
    ) -> Result<(), RepositoryError>;

    /// Stamp the moment the notification actually reached the daemon.
    async fn mark_fired(
        &self,
        id: BreakEventId,
        fired_at: DateTime<Utc>,
    ) -> Result<(), RepositoryError>;

    /// `(rule_id, outcome, count)` over `[from, to)`, for the stats panel.
    async fn counts_between(
        &self,
        user_id: UserId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<(BreakRuleId, BreakOutcome, i64)>, RepositoryError>;
}
