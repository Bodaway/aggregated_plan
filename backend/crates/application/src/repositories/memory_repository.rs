use async_trait::async_trait;
use domain::types::*;

use crate::errors::RepositoryError;

/// Filter for listing memories of one user. Listing is the inbox/history path;
/// searching goes through `MemoryRetriever`.
#[derive(Debug, Clone, Default)]
pub struct MemoryListFilter {
    /// Keep only these validation-queue statuses. `None` = any status.
    pub status: Option<Vec<MemoryStatus>>,
    /// When false, rows carrying `invalidated_at` are excluded.
    pub include_invalidated: bool,
    pub project_id: Option<ProjectId>,
    /// Max rows. `0` means "use the default"; repositories enforce an absolute
    /// cap. Never bind this field into SQL — bind [`MemoryListFilter::effective_limit`].
    pub limit: u32,
    pub offset: u32,
}

pub const MEMORY_LIST_DEFAULT_LIMIT: u32 = 50;
pub const MEMORY_LIST_MAX_LIMIT: u32 = 500;

impl MemoryListFilter {
    /// The row count an implementation must actually apply: `0` (which the
    /// derived `Default` produces) resolves to [`MEMORY_LIST_DEFAULT_LIMIT`],
    /// and anything above [`MEMORY_LIST_MAX_LIMIT`] is capped.
    ///
    /// This lives here, beside the constants, so every implementation shares one
    /// rule. Binding `limit` raw would emit `LIMIT 0` for a default filter and
    /// return an empty list with no error at all.
    pub fn effective_limit(&self) -> u32 {
        match self.limit {
            0 => MEMORY_LIST_DEFAULT_LIMIT,
            n => n.min(MEMORY_LIST_MAX_LIMIT),
        }
    }
}

/// Persists semantic memories and their stakeholders.
///
/// Implementations MUST write the `memories_fts` row in the same transaction as
/// the `memories` row: the FTS table is standalone (no `content=`, no triggers),
/// so a partial write leaves a memory that can never be recalled.
#[async_trait]
pub trait MemoryRepository: Send + Sync {
    /// Insert a memory, its stakeholders and its FTS row atomically.
    async fn create(&self, memory: &Memory) -> Result<(), RepositoryError>;

    async fn find_by_id(
        &self,
        id: MemoryId,
        user_id: UserId,
    ) -> Result<Option<Memory>, RepositoryError>;

    /// Newest-first (`occurred_at DESC`) list of the user's memories.
    /// Implementations MUST bind [`MemoryListFilter::effective_limit`], never
    /// `filter.limit`.
    async fn list(
        &self,
        user_id: UserId,
        filter: &MemoryListFilter,
    ) -> Result<Vec<Memory>, RepositoryError>;

    /// Overwrite a memory. Must rewrite its FTS row and stakeholders in the same
    /// transaction, or a retitled memory stays searchable under its old wording.
    async fn update(&self, memory: &Memory) -> Result<(), RepositoryError>;

    /// Apply a merge atomically: the survivor is updated and the discarded row
    /// deleted in ONE transaction. A partial merge would leave the candidate in
    /// the queue with its wording already copied over.
    async fn apply_merge(
        &self,
        survivor: &Memory,
        discarded: MemoryId,
        user_id: UserId,
    ) -> Result<(), RepositoryError>;

    /// Apply a supersession atomically: both rows are updated in ONE transaction.
    /// Half of it would either hide a fact with no successor, or leave two active
    /// contradictory truths.
    async fn apply_supersession(
        &self,
        invalidated: &Memory,
        successor: &Memory,
    ) -> Result<(), RepositoryError>;

    /// The `source_ref` values already stored for this user that start with
    /// `prefix`. Backs import idempotency without loading whole memories.
    async fn existing_source_refs(
        &self,
        user_id: UserId,
        prefix: &str,
    ) -> Result<Vec<String>, RepositoryError>;

    /// Ids reachable from `from` by following `superseded_by`, nearest first and
    /// excluding `from` itself. Feeds the domain's cycle check. Implementations
    /// must terminate even if the stored data already holds a loop.
    async fn supersession_chain(
        &self,
        user_id: UserId,
        from: MemoryId,
    ) -> Result<Vec<MemoryId>, RepositoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The derived `Default` leaves `limit` at `0`, which is exactly the value
    /// that would silently emit `LIMIT 0`.
    #[test]
    fn a_default_filter_resolves_to_the_default_limit() {
        assert_eq!(
            MemoryListFilter::default().effective_limit(),
            MEMORY_LIST_DEFAULT_LIMIT
        );
    }

    #[test]
    fn an_oversized_limit_is_capped() {
        let greedy = MemoryListFilter {
            limit: u32::MAX,
            ..MemoryListFilter::default()
        };
        assert_eq!(greedy.effective_limit(), MEMORY_LIST_MAX_LIMIT);
    }

    #[test]
    fn a_reasonable_limit_is_honoured_as_typed() {
        let asked = MemoryListFilter {
            limit: 7,
            ..MemoryListFilter::default()
        };
        assert_eq!(asked.effective_limit(), 7);
    }
}
