use async_trait::async_trait;
use domain::types::*;

use crate::errors::RepositoryError;

/// Repository trait for persisting and querying tags.
#[async_trait]
pub trait TagRepository: Send + Sync {
    /// Find all tags for a user.
    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<Tag>, RepositoryError>;

    /// Find a tag by its unique identifier.
    async fn find_by_id(&self, id: TagId) -> Result<Option<Tag>, RepositoryError>;

    /// Save a new tag.
    async fn save(&self, tag: &Tag) -> Result<(), RepositoryError>;

    /// Update an existing tag.
    async fn update(&self, tag: &Tag) -> Result<(), RepositoryError>;

    /// Delete a tag by its identifier.
    async fn delete(&self, id: TagId) -> Result<(), RepositoryError>;
}
