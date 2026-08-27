use async_trait::async_trait;
use std::time::Duration;

use crate::errors::AppError;
use domain::types::BreakUrgency;

/// A desktop notification with optional buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub urgency: BreakUrgency,
    pub icon: Option<String>,
    /// How long to wait for an answer before giving up and closing.
    pub expire_after: Duration,
    /// `(key, label)` — the key is what comes back in `NotificationOutcome::Action`.
    pub actions: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationOutcome {
    /// The user pressed a button; carries its key.
    Action(String),
    /// Closed without choosing.
    Dismissed,
    /// Never answered within `expire_after`.
    Expired,
}

/// Delivers a notification and waits for what the user does about it.
///
/// Implementations block for as long as the notification is on screen, so callers must
/// treat `notify` as long-running and never hold a tick open on it.
#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, n: Notification) -> Result<NotificationOutcome, AppError>;
}

/// Records nothing, shows nothing, always reports a dismissal.
///
/// Used in tests, and selected at wiring time when no session bus is reachable: a
/// headless API must still keep its books, and must not spam the log every 30 seconds
/// with a failure it cannot fix.
pub struct NullNotifier;

#[async_trait]
impl Notifier for NullNotifier {
    async fn notify(&self, _n: Notification) -> Result<NotificationOutcome, AppError> {
        Ok(NotificationOutcome::Dismissed)
    }
}
