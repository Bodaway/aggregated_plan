use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::types::*;

use crate::errors::RepositoryError;

/// Filter for listing worklog entries belonging to one user.
#[derive(Debug, Clone, Default)]
pub struct WorklogFilter {
    /// If set, limit to entries whose task_id is in this list.
    pub task_ids: Option<Vec<TaskId>>,
    /// Inclusive lower bound on logged_at.
    pub from: Option<DateTime<Utc>>,
    /// Exclusive upper bound on logged_at.
    pub to: Option<DateTime<Utc>>,
    /// Max rows to return. Repositories enforce an absolute cap.
    pub limit: u32,
    /// Pagination offset.
    pub offset: u32,
}

pub const WORKLOG_FILTER_DEFAULT_LIMIT: u32 = 200;
pub const WORKLOG_FILTER_MAX_LIMIT: u32 = 1_000;

#[async_trait]
pub trait WorklogRepository: Send + Sync {
    async fn create(&self, entry: &WorklogEntry) -> Result<(), RepositoryError>;
    async fn update(&self, entry: &WorklogEntry) -> Result<(), RepositoryError>;
    async fn delete(&self, id: WorklogEntryId, user_id: UserId) -> Result<bool, RepositoryError>;
    async fn find_by_id(
        &self,
        id: WorklogEntryId,
        user_id: UserId,
    ) -> Result<Option<WorklogEntry>, RepositoryError>;
    async fn list(
        &self,
        user_id: UserId,
        filter: &WorklogFilter,
    ) -> Result<Vec<WorklogEntry>, RepositoryError>;
}
