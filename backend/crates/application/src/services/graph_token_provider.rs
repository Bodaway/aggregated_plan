use async_trait::async_trait;
use domain::types::UserId;

use crate::errors::AppError;

/// Provides a currently-valid Microsoft Graph access token, refreshing it transparently.
#[async_trait]
pub trait GraphTokenProvider: Send + Sync {
    /// Return a valid access token for the user, refreshing via the stored refresh token
    /// if the cached access token is missing or near expiry.
    async fn valid_access_token(&self, user_id: UserId) -> Result<String, AppError>;
}
