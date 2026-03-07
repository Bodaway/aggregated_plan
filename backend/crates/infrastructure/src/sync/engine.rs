use std::sync::Arc;

use domain::types::*;

use application::errors::AppError;
use application::repositories::*;
use application::services::*;
use application::use_cases::sync;

/// Sync engine orchestrates pulling data from external sources (Jira, Outlook, Excel)
/// and writing it to local repositories.
pub struct SyncEngine {
    task_repo: Arc<dyn TaskRepository>,
    meeting_repo: Arc<dyn MeetingRepository>,
    project_repo: Arc<dyn ProjectRepository>,
    sync_repo: Arc<dyn SyncStatusRepository>,
    config_repo: Arc<dyn ConfigRepository>,
    jira_client: Option<Arc<dyn JiraClient>>,
    outlook_client: Option<Arc<dyn OutlookClient>>,
    excel_client: Option<Arc<dyn ExcelClient>>,
}

impl SyncEngine {
    /// Create a new sync engine with all required repositories and optional clients.
    pub fn new(
        task_repo: Arc<dyn TaskRepository>,
        meeting_repo: Arc<dyn MeetingRepository>,
        project_repo: Arc<dyn ProjectRepository>,
        sync_repo: Arc<dyn SyncStatusRepository>,
        config_repo: Arc<dyn ConfigRepository>,
        jira_client: Option<Arc<dyn JiraClient>>,
        outlook_client: Option<Arc<dyn OutlookClient>>,
        excel_client: Option<Arc<dyn ExcelClient>>,
    ) -> Self {
        Self {
            task_repo,
            meeting_repo,
            project_repo,
            sync_repo,
            config_repo,
            jira_client,
            outlook_client,
            excel_client,
        }
    }

    /// Synchronize a single source for the given user.
    pub async fn sync_source(
        &self,
        source: Source,
        user_id: UserId,
    ) -> Result<SyncStatus, AppError> {
        sync::sync_source(
            source,
            self.task_repo.as_ref(),
            self.meeting_repo.as_ref(),
            self.project_repo.as_ref(),
            self.sync_repo.as_ref(),
            self.jira_client.as_deref(),
            self.outlook_client.as_deref(),
            self.excel_client.as_deref(),
            self.config_repo.as_ref(),
            user_id,
        )
        .await
    }

    /// Synchronize all configured sources for the given user.
    pub async fn sync_all(
        &self,
        user_id: UserId,
    ) -> Result<Vec<sync::SyncResult>, AppError> {
        sync::sync_all(
            self.jira_client.as_deref(),
            self.outlook_client.as_deref(),
            self.excel_client.as_deref(),
            self.task_repo.as_ref(),
            self.meeting_repo.as_ref(),
            self.project_repo.as_ref(),
            self.sync_repo.as_ref(),
            self.config_repo.as_ref(),
            user_id,
        )
        .await
    }

    /// Get current sync statuses for a user.
    pub async fn get_statuses(
        &self,
        user_id: UserId,
    ) -> Result<Vec<SyncStatus>, AppError> {
        let statuses = self.sync_repo.find_by_user(user_id).await?;
        Ok(statuses)
    }
}
