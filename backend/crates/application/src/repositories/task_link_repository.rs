use async_trait::async_trait;
use domain::types::*;

use crate::errors::RepositoryError;

/// Repository trait for persisting and querying task links (deduplication).
#[async_trait]
pub trait TaskLinkRepository: Send + Sync {
    /// Find all task links for a user.
    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<TaskLink>, RepositoryError>;

    /// Find all rejected pairs for a user (to avoid re-suggesting).
    async fn find_rejected_pairs(
        &self,
        user_id: UserId,
    ) -> Result<Vec<(TaskId, TaskId)>, RepositoryError>;

    /// Save a new task link.
    async fn save(&self, link: &TaskLink) -> Result<(), RepositoryError>;

    /// Delete a task link by its identifier.
    async fn delete(&self, id: TaskLinkId) -> Result<(), RepositoryError>;
}
