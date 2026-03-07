use async_trait::async_trait;
use chrono::NaiveDate;
use domain::types::*;

use crate::errors::RepositoryError;

/// Repository trait for persisting and querying meetings.
#[async_trait]
pub trait MeetingRepository: Send + Sync {
    /// Find all meetings for a user on a specific date.
    async fn find_by_user_and_date(
        &self,
        user_id: UserId,
        date: NaiveDate,
    ) -> Result<Vec<Meeting>, RepositoryError>;

    /// Find all meetings for a user within a date range.
    async fn find_by_user_and_range(
        &self,
        user_id: UserId,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Meeting>, RepositoryError>;

    /// Insert or update meetings in batch (upsert by outlook_id).
    async fn upsert_batch(&self, meetings: &[Meeting]) -> Result<(), RepositoryError>;

    /// Delete meetings whose outlook_id is not in the provided list.
    /// Returns the number of deleted records.
    async fn delete_stale(
        &self,
        user_id: UserId,
        current_outlook_ids: &[String],
    ) -> Result<u64, RepositoryError>;

    /// Find all meetings associated with a specific project.
    async fn find_by_project(
        &self,
        user_id: UserId,
        project_id: ProjectId,
    ) -> Result<Vec<Meeting>, RepositoryError>;
}
