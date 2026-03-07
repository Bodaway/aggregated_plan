use async_trait::async_trait;
use domain::types::*;

use crate::errors::RepositoryError;

/// Repository trait for persisting and querying user configuration key-value pairs.
#[async_trait]
pub trait ConfigRepository: Send + Sync {
    /// Get a single configuration value by key.
    async fn get(&self, user_id: UserId, key: &str) -> Result<Option<String>, RepositoryError>;

    /// Get all configuration key-value pairs for a user.
    async fn get_all(&self, user_id: UserId) -> Result<Vec<(String, String)>, RepositoryError>;

    /// Set a configuration value for a user.
    async fn set(&self, user_id: UserId, key: &str, value: &str) -> Result<(), RepositoryError>;
}
