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

    /// The break currently being served, if any: `pending` with a `started_at` on it.
    ///
    /// There can only ever be one. The scheduler is a single loop, and no break can
    /// ring while another is running — which is what lets the HUD ask "what is on
    /// screen right now" without a key.
    async fn find_active(&self, user_id: UserId) -> Result<Option<BreakEvent>, RepositoryError>;

    /// Open the session: the user pressed the button. The outcome stays `pending` —
    /// `taken` is only earned at the deadline — and `ends_at` is frozen here rather
    /// than recomputed later, so retuning the rule cannot lengthen a running break.
    async fn start_session(
        &self,
        id: BreakEventId,
        started_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Result<(), RepositoryError>;

    /// Cut the running break short — "J'y retourne" — and report whether this call is
    /// the one that closed it.
    ///
    /// A compare-and-swap, not a read followed by an update, because the tick writes
    /// `taken` the instant the countdown runs out and that write has to stand: it is
    /// the one that saw the break through to its end. A press arriving a hundredth of
    /// a second later finds a row that is no longer `pending`, matches nothing, and
    /// answers `false`. The same conjunction makes the mutation idempotent for free —
    /// a second press, or one carrying the id of a break that has since ended, matches
    /// nothing either.
    ///
    /// The tick's own `taken` write stays unconditional, and that asymmetry is the
    /// point: reaching the deadline is the authority on how the break ended.
    async fn abandon_if_running(
        &self,
        user_id: UserId,
        id: BreakEventId,
        responded_at: DateTime<Utc>,
    ) -> Result<bool, RepositoryError>;

    /// `(rule_id, outcome, count)` over `[from, to)`, for the stats panel.
    async fn counts_between(
        &self,
        user_id: UserId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<(BreakRuleId, BreakOutcome, i64)>, RepositoryError>;
}
