use async_trait::async_trait;
use tokio::process::Command;

use application::errors::AppError;
use application::services::{Notification, NotificationOutcome, Notifier};

/// Build the `notify-send` argv for a notification.
///
/// Split out as a pure function on purpose: this is the part with actual logic, and
/// it can be tested without a session bus. The spawn below is a three-line shell.
pub fn command_args(n: &Notification) -> Vec<String> {
    let mut args = vec![
        "--app-name=aplan".to_string(),
        format!("--urgency={}", n.urgency.as_str()),
        format!("--expire-time={}", n.expire_after.as_millis()),
    ];
    if let Some(icon) = &n.icon {
        args.push(format!("--icon={icon}"));
    }
    for (key, label) in &n.actions {
        args.push(format!("--action={key}={label}"));
    }
    args.push(n.title.clone());
    args.push(n.body.clone());
    args
}

/// `notify-send` writes the chosen action's key to stdout, and writes nothing when the
/// notification was closed without a choice.
pub fn parse_outcome(stdout: &str) -> NotificationOutcome {
    let key = stdout.trim();
    if key.is_empty() {
        NotificationOutcome::Dismissed
    } else {
        NotificationOutcome::Action(key.to_string())
    }
}

/// Delivers through `notify-send`, which `--action` puts into `--wait` mode: the
/// process stays alive until the user answers or the notification expires.
pub struct NotifySendNotifier;

impl NotifySendNotifier {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NotifySendNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Notifier for NotifySendNotifier {
    async fn notify(&self, n: Notification) -> Result<NotificationOutcome, AppError> {
        let output = Command::new("notify-send")
            .args(command_args(&n))
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("notify-send failed to run: {e}")))?;
        if !output.status.success() {
            return Err(AppError::Internal(format!(
                "notify-send exited with {}",
                output.status
            )));
        }
        Ok(parse_outcome(&String::from_utf8_lossy(&output.stdout)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::types::BreakUrgency;
    use std::time::Duration;

    fn sample() -> Notification {
        Notification {
            title: "Pause visuelle".into(),
            body: "Regarde au loin 20 s.".into(),
            urgency: BreakUrgency::Low,
            icon: Some("appointment-soon".into()),
            expire_after: Duration::from_secs(90),
            actions: vec![
                ("taken".into(), "Pris".into()),
                ("snoozed".into(), "Plus tard".into()),
                ("skipped".into(), "Passer".into()),
            ],
        }
    }

    #[test]
    fn args_carry_app_name_urgency_icon_and_every_action() {
        let args = command_args(&sample());
        assert!(args.contains(&"--app-name=aplan".to_string()));
        assert!(args.contains(&"--urgency=low".to_string()));
        assert!(args.contains(&"--icon=appointment-soon".to_string()));
        assert!(args.contains(&"--action=taken=Pris".to_string()));
        assert!(args.contains(&"--action=snoozed=Plus tard".to_string()));
        assert!(args.contains(&"--action=skipped=Passer".to_string()));
        // Title and body are positional and must come last, in that order.
        assert_eq!(args[args.len() - 2], "Pause visuelle");
        assert_eq!(args[args.len() - 1], "Regarde au loin 20 s.");
    }

    #[test]
    fn args_omit_the_icon_when_there_is_none() {
        let mut n = sample();
        n.icon = None;
        assert!(!command_args(&n).iter().any(|a| a.starts_with("--icon")));
    }

    /// `notify-send` prints the chosen action key on stdout, and nothing at all when
    /// the notification is dismissed.
    #[test]
    fn stdout_maps_to_an_outcome() {
        assert_eq!(parse_outcome("taken\n"), NotificationOutcome::Action("taken".into()));
        assert_eq!(parse_outcome("snoozed"), NotificationOutcome::Action("snoozed".into()));
        assert_eq!(parse_outcome(""), NotificationOutcome::Dismissed);
        assert_eq!(parse_outcome("   \n"), NotificationOutcome::Dismissed);
    }
}
