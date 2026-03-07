use domain::types::UserId;

use application::errors::AppError;
use application::repositories::*;
use application::use_cases::deduplication::{self, DeduplicationSuggestion};

/// Deduplication engine scans tasks and identifies potential duplicates
/// using domain rules (Jira key matching, similarity scoring).
pub struct DedupEngine;

impl DedupEngine {
    /// Run the deduplication engine for a user, returning suggestions.
    pub async fn run(
        task_repo: &dyn TaskRepository,
        task_link_repo: &dyn TaskLinkRepository,
        user_id: UserId,
    ) -> Result<Vec<DeduplicationSuggestion>, AppError> {
        deduplication::find_suggestions(task_repo, task_link_repo, user_id).await
    }
}
