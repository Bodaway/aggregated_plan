use async_trait::async_trait;
use domain::types::*;

use crate::errors::RepositoryError;

/// Persists learned signal→Gryzzly-project mapping rules (user-scoped).
#[async_trait]
pub trait SignalMappingRepository: Send + Sync {
    /// All enabled rules for the user (the resolver filters by kind in memory).
    async fn list_enabled(&self, user_id: UserId) -> Result<Vec<SignalMapping>, RepositoryError>;

    /// Insert or update a rule (idempotent on (user_id, kind, pattern)).
    async fn upsert(&self, mapping: &SignalMapping) -> Result<(), RepositoryError>;

    /// Enable/disable a rule without deleting it.
    async fn set_enabled(&self, id: SignalMappingId, enabled: bool) -> Result<(), RepositoryError>;

    /// Hard-delete a rule.
    async fn delete(&self, id: SignalMappingId) -> Result<(), RepositoryError>;
}
