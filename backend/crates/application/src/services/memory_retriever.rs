use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::rules::recall::{RecallContext, RecallWeights, ScoredMemory};
use domain::types::UserId;

use crate::errors::RepositoryError;

pub const RECALL_DEFAULT_LIMIT: u32 = 10;
pub const RECALL_MAX_LIMIT: u32 = 100;

/// A search request against the memory store.
#[derive(Debug, Clone)]
pub struct RecallQuery {
    /// An FTS5 `MATCH` expression already built by
    /// `domain::rules::recall::build_match_query`. Raw user input must never
    /// reach here.
    pub match_query: String,
    /// Entities in focus, used for the entity bonus.
    pub context: RecallContext,
    /// Lift the hard filter `invalidated_at IS NULL AND status = 'active'`.
    /// Off by default: recalling a superseded decision is the worst failure mode.
    pub include_history: bool,
    pub weights: RecallWeights,
    /// `0` means "use `RECALL_DEFAULT_LIMIT`"; implementations cap at
    /// `RECALL_MAX_LIMIT`. Never size a query off this field — use
    /// [`RecallQuery::effective_limit`].
    pub limit: u32,
}

impl RecallQuery {
    /// The result count an implementation must actually apply: `0` resolves to
    /// [`RECALL_DEFAULT_LIMIT`] and anything above [`RECALL_MAX_LIMIT`] is
    /// capped. Sizing the candidate window off a raw `0` would fetch nothing.
    pub fn effective_limit(&self) -> u32 {
        match self.limit {
            0 => RECALL_DEFAULT_LIMIT,
            n => n.min(RECALL_MAX_LIMIT),
        }
    }

    /// A query with the default weights, the default limit and the hard filter on.
    pub fn new(match_query: String) -> Self {
        Self {
            match_query,
            context: RecallContext::default(),
            include_history: false,
            weights: RecallWeights::default(),
            limit: RECALL_DEFAULT_LIMIT,
        }
    }
}

/// Retrieves memories by relevance. Backed by FTS5/BM25 plus the pure scoring
/// rules in `domain::rules::recall` — no vectors in v1.
#[async_trait]
pub trait MemoryRetriever: Send + Sync {
    /// Best-first memories matching `query`. `now` is injected so recency decay
    /// stays deterministic and testable.
    async fn search(
        &self,
        user_id: UserId,
        query: &RecallQuery,
        now: DateTime<Utc>,
    ) -> Result<Vec<ScoredMemory>, RepositoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query_with_limit(limit: u32) -> RecallQuery {
        RecallQuery {
            limit,
            ..RecallQuery::new("\"wave\"*".into())
        }
    }

    #[test]
    fn a_zero_limit_resolves_to_the_default() {
        assert_eq!(query_with_limit(0).effective_limit(), RECALL_DEFAULT_LIMIT);
    }

    #[test]
    fn an_oversized_limit_is_capped() {
        assert_eq!(
            query_with_limit(u32::MAX).effective_limit(),
            RECALL_MAX_LIMIT
        );
    }

    #[test]
    fn a_reasonable_limit_is_honoured_as_typed() {
        assert_eq!(query_with_limit(3).effective_limit(), 3);
    }
}
