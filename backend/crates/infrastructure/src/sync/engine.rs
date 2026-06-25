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
    gryzzly_client: Option<Arc<dyn GryzzlyClient>>,
    gryzzly_catalog_repo: Arc<dyn GryzzlyCatalogRepository>,
}

pub struct SyncEngineDeps {
    pub task_repo: Arc<dyn TaskRepository>,
    pub meeting_repo: Arc<dyn MeetingRepository>,
    pub project_repo: Arc<dyn ProjectRepository>,
    pub sync_repo: Arc<dyn SyncStatusRepository>,
    pub config_repo: Arc<dyn ConfigRepository>,
    pub jira_client: Option<Arc<dyn JiraClient>>,
    pub outlook_client: Option<Arc<dyn OutlookClient>>,
    pub excel_client: Option<Arc<dyn ExcelClient>>,
    pub gryzzly_client: Option<Arc<dyn GryzzlyClient>>,
    pub gryzzly_catalog_repo: Arc<dyn GryzzlyCatalogRepository>,
}

impl SyncEngine {
    pub fn new(deps: SyncEngineDeps) -> Self {
        Self {
            task_repo: deps.task_repo,
            meeting_repo: deps.meeting_repo,
            project_repo: deps.project_repo,
            sync_repo: deps.sync_repo,
            config_repo: deps.config_repo,
            jira_client: deps.jira_client,
            outlook_client: deps.outlook_client,
            excel_client: deps.excel_client,
            gryzzly_client: deps.gryzzly_client,
            gryzzly_catalog_repo: deps.gryzzly_catalog_repo,
        }
    }

    /// Synchronize a single source for the given user.
    pub async fn sync_source(
        &self,
        source: Source,
        user_id: UserId,
    ) -> Result<SyncStatus, AppError> {
        let ctx = sync::SyncContext {
            task_repo: self.task_repo.as_ref(),
            meeting_repo: self.meeting_repo.as_ref(),
            project_repo: self.project_repo.as_ref(),
            sync_repo: self.sync_repo.as_ref(),
            config_repo: self.config_repo.as_ref(),
            jira_client: self.jira_client.as_deref(),
            outlook_client: self.outlook_client.as_deref(),
            excel_client: self.excel_client.as_deref(),
            gryzzly_client: self.gryzzly_client.as_deref(),
            gryzzly_catalog_repo: self.gryzzly_catalog_repo.as_ref(),
        };
        sync::sync_source(&ctx, source, user_id).await
    }

    /// Synchronize all configured sources for the given user.
    pub async fn sync_all(
        &self,
        user_id: UserId,
    ) -> Result<Vec<sync::SyncResult>, AppError> {
        let ctx = sync::SyncContext {
            task_repo: self.task_repo.as_ref(),
            meeting_repo: self.meeting_repo.as_ref(),
            project_repo: self.project_repo.as_ref(),
            sync_repo: self.sync_repo.as_ref(),
            config_repo: self.config_repo.as_ref(),
            jira_client: self.jira_client.as_deref(),
            outlook_client: self.outlook_client.as_deref(),
            excel_client: self.excel_client.as_deref(),
            gryzzly_client: self.gryzzly_client.as_deref(),
            gryzzly_catalog_repo: self.gryzzly_catalog_repo.as_ref(),
        };
        sync::sync_all(&ctx, user_id).await
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
