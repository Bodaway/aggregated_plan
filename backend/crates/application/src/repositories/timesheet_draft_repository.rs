use async_trait::async_trait;
use chrono::NaiveDate;
use domain::types::*;

use crate::errors::RepositoryError;

/// Persists the reconstructed daily timesheet draft (header + per-project lines).
#[async_trait]
pub trait TimesheetDraftRepository: Send + Sync {
    /// Insert or replace the whole draft for (user, date). Replaces all lines.
    async fn upsert(&self, draft: &TimesheetDraft) -> Result<(), RepositoryError>;

    /// Load the draft (with its lines) for a user + local date.
    async fn find_by_user_and_date(
        &self,
        user_id: UserId,
        date: NaiveDate,
    ) -> Result<Option<TimesheetDraft>, RepositoryError>;

    /// Change only the status of an existing draft.
    async fn set_status(
        &self,
        user_id: UserId,
        date: NaiveDate,
        status: TimesheetStatus,
    ) -> Result<(), RepositoryError>;
}
