use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;

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

/// Long-lived background task: every 60s, run one end-of-day pass for `user_id`.
/// Errors are logged, never fatal. Idempotent via the last_auto_run watermark.
pub async fn run_eod_scheduler(deps: EodDeps, user_id: UserId) {
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    loop {
        ticker.tick().await;
        match run_eod_pass(
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
        .await
        {
            Ok(dates) if !dates.is_empty() => {
                tracing::info!(?dates, "end-of-day timesheet reconstruction completed")
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "end-of-day timesheet reconstruction failed"),
        }
    }
}
