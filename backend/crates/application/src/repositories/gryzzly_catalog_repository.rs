use async_trait::async_trait;
use domain::types::{GryzzlyCatalogEntry, UserId};

use crate::errors::RepositoryError;

#[async_trait]
pub trait GryzzlyCatalogRepository: Send + Sync {
    /// Insert or update one catalog row, keyed on (user_id, gryzzly_task_id).
    /// Re-activates a previously soft-disabled row.
    async fn upsert(&self, entry: &GryzzlyCatalogEntry) -> Result<(), RepositoryError>;

    /// Soft-disable (is_active = 0) every row for the user whose gryzzly_task_id is
    /// NOT in `keep_ids`. NEVER hard-deletes. Returns the number of rows disabled.
    async fn soft_prune_missing(&self, user_id: UserId, keep_ids: &[String]) -> Result<u64, RepositoryError>;

    /// Active rows for the picker, optionally filtered by a name/project search and a
    /// project-name filter, ordered by project_name then name, capped at `limit`.
    async fn list_active(
        &self,
        user_id: UserId,
        search: Option<&str>,
        project_filter: Option<&str>,
        limit: i64,
    ) -> Result<Vec<GryzzlyCatalogEntry>, RepositoryError>;

    /// Look up one row by gryzzly_task_id regardless of is_active (so a stale/disabled
    /// assignment still resolves for display + future push). None if absent.
    async fn find_by_gryzzly_task_id(
        &self,
        user_id: UserId,
        gryzzly_task_id: &str,
    ) -> Result<Option<GryzzlyCatalogEntry>, RepositoryError>;
}
