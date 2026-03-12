use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::types::*;

use crate::errors::RepositoryError;

/// Represents the synchronization status of an external source for a user.
pub struct SyncStatus {
    pub source: Source,
    pub user_id: UserId,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub status: SyncSourceStatus,
    pub error_message: Option<String>,
}

/// Repository trait for persisting and querying sync status records.
#[async_trait]
pub trait SyncStatusRepository: Send + Sync {
    /// Find all sync statuses for a user.
    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<SyncStatus>, RepositoryError>;

    /// Insert or update a sync status record.
    async fn upsert(&self, status: &SyncStatus) -> Result<(), RepositoryError>;
}
