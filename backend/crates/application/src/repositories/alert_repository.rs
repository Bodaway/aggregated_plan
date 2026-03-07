use async_trait::async_trait;
use domain::types::*;

use crate::errors::RepositoryError;

/// Repository trait for persisting and querying alerts.
#[async_trait]
pub trait AlertRepository: Send + Sync {
    /// Find all unresolved alerts for a user.
    async fn find_unresolved(&self, user_id: UserId) -> Result<Vec<Alert>, RepositoryError>;

    /// Find alerts for a user, optionally filtering by resolved status.
    async fn find_by_user(
        &self,
        user_id: UserId,
        resolved: Option<bool>,
    ) -> Result<Vec<Alert>, RepositoryError>;

    /// Save a new alert.
    async fn save(&self, alert: &Alert) -> Result<(), RepositoryError>;

    /// Save multiple alerts in a single batch operation.
    async fn save_batch(&self, alerts: &[Alert]) -> Result<(), RepositoryError>;

    /// Update an existing alert.
    async fn update(&self, alert: &Alert) -> Result<(), RepositoryError>;

    /// Delete all resolved alerts for a user. Returns the number of deleted records.
    async fn delete_resolved(&self, user_id: UserId) -> Result<u64, RepositoryError>;
}
