use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;

use application::jobs::{
    humanize_duration, AttemptOutcome, JobHealth, LogEntry, LogLevel, RetryPolicy,
};
use application::repositories::{
    AlertRepository, ConfigRepository, GryzzlyCatalogRepository, MeetingRepository,
    SignalMappingRepository, TaskRepository, TimesheetDraftRepository, WorklogRepository,
};
use application::services::git_connector::GitConnector;
use application::use_cases::timesheet::run_eod_pass;
use domain::types::UserId;

/// Dependencies the end-of-day scheduler needs (Arc clones of the app's repos).
pub struct EodDeps {
    pub worklog_repo: Arc<dyn WorklogRepository>,
    pub meeting_repo: Arc<dyn MeetingRepository>,
    pub task_repo: Arc<dyn TaskRepository>,
    pub catalog_repo: Arc<dyn GryzzlyCatalogRepository>,
    pub mapping_repo: Arc<dyn SignalMappingRepository>,
    pub config_repo: Arc<dyn ConfigRepository>,
    pub git: Arc<dyn GitConnector>,
    pub draft_repo: Arc<dyn TimesheetDraftRepository>,
    pub alert_repo: Arc<dyn AlertRepository>,
}

/// Long-lived background task: run one end-of-day pass for `user_id`, then wait as long
/// as `application::jobs::RetryPolicy` says to before the next one — 5 minutes while
/// healthy, backing off to 30 minutes while it is not. Errors are logged, never fatal.
/// Idempotent via the last_auto_run watermark, which is what actually decides when work
/// is due; the tick only decides how often we ask.
///
/// This function owns exactly two things the policy cannot: the sleeping and the
/// `tracing` calls.
pub async fn run_eod_scheduler(deps: EodDeps, user_id: UserId) {
    let policy = RetryPolicy::end_of_day();
    let mut health = JobHealth::default();
    loop {
        let attempt = run_eod_pass(
            deps.worklog_repo.as_ref(),
            deps.meeting_repo.as_ref(),
            deps.task_repo.as_ref(),
            deps.catalog_repo.as_ref(),
            deps.mapping_repo.as_ref(),
            deps.config_repo.as_ref(),
            deps.git.as_ref(),
            deps.draft_repo.as_ref(),
            deps.alert_repo.as_ref(),
            user_id,
            Utc::now(),
        )
        .await;

        // A pass that kept its work but had to tolerate a broken step is not a success:
        // whatever it tolerated is still unfixed, so it must feed the back-off too.
        let failure = match &attempt {
            Ok(outcome) if outcome.degraded.is_empty() => None,
            Ok(outcome) => Some(outcome.degradation_signature()),
            Err(e) => Some(e.to_string()),
        };
        if let Ok(outcome) = &attempt {
            if !outcome.processed.is_empty() {
                tracing::info!(
                    dates = ?outcome.processed,
                    "end-of-day timesheet reconstruction completed"
                );
            }
        }

        let observed = match &failure {
            Some(signature) => AttemptOutcome::Failed { signature },
            None => AttemptOutcome::Succeeded,
        };
        let (next_health, decision) = health.observe(observed, Utc::now(), &policy);
        health = next_health;
        report(decision.log, failure.as_deref(), decision.retry_in);

        tokio::time::sleep(decision.retry_in).await;
    }
}

/// Turn the policy's verdict into a journal line. The policy already decided whether to
/// speak and how loudly; this only decides the wording.
fn report(entry: Option<LogEntry>, error: Option<&str>, retry_in: Duration) {
    let Some(entry) = entry else {
        return;
    };
    match entry {
        LogEntry::Failure { level, consecutive_failures, failing_for, suppressed_repeats } => {
            let error = error.unwrap_or("unknown");
            let failing_for = humanize_duration(failing_for);
            match level {
                LogLevel::Warn => tracing::warn!(
                    error,
                    consecutive_failures,
                    failing_for = %failing_for,
                    retry_in_s = retry_in.as_secs(),
                    "end-of-day timesheet reconstruction failed"
                ),
                // The line a three-week-old outage needs: not "failed" for the
                // thousandth time, but for how long and how many times.
                LogLevel::Error => tracing::error!(
                    error,
                    consecutive_failures,
                    failing_for = %failing_for,
                    suppressed_repeats,
                    retry_in_s = retry_in.as_secs(),
                    "end-of-day timesheet reconstruction has been failing for {failing_for} \
                     ({consecutive_failures} consecutive attempts) -- it will not fix itself"
                ),
            }
        }
        LogEntry::Recovered { after_failures, was_failing_for } => tracing::info!(
            after_failures,
            was_failing_for = %humanize_duration(was_failing_for),
            "end-of-day timesheet reconstruction recovered"
        ),
    }
}
