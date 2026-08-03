use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::types::*;
use domain::types::recurrence::RecurrenceTemplateId;

use crate::errors::RepositoryError;

/// Filter for listing worklog entries belonging to one user.
#[derive(Debug, Clone, Default)]
pub struct WorklogFilter {
    /// If set, limit to entries whose task_id is in this list.
    pub task_ids: Option<Vec<TaskId>>,
    /// Inclusive lower bound on logged_at.
    pub from: Option<DateTime<Utc>>,
    /// Exclusive upper bound on logged_at.
    pub to: Option<DateTime<Utc>>,
    /// Max rows to return. `0` means "use the default"; repositories enforce an
    /// absolute cap. Never bind this field into SQL — bind
    /// [`WorklogFilter::effective_limit`].
    pub limit: u32,
    /// Pagination offset.
    pub offset: u32,
}

pub const WORKLOG_FILTER_DEFAULT_LIMIT: u32 = 200;
pub const WORKLOG_FILTER_MAX_LIMIT: u32 = 1_000;

impl WorklogFilter {
    /// The row count an implementation must actually apply: `0` (which the derived
    /// `Default` produces) resolves to [`WORKLOG_FILTER_DEFAULT_LIMIT`], and
    /// anything above [`WORKLOG_FILTER_MAX_LIMIT`] is capped.
    ///
    /// Beside the constants so every implementation shares one rule. Binding
    /// `limit` raw emits `LIMIT 0` for a default-constructed filter and returns an
    /// empty list with no error at all — the exact failure the consolidation job
    /// could not survive, since an empty list is indistinguishable from "nothing
    /// left to consolidate".
    pub fn effective_limit(&self) -> u32 {
        match self.limit {
            0 => WORKLOG_FILTER_DEFAULT_LIMIT,
            n => n.min(WORKLOG_FILTER_MAX_LIMIT),
        }
    }
}

#[async_trait]
pub trait WorklogRepository: Send + Sync {
    async fn create(&self, entry: &WorklogEntry) -> Result<(), RepositoryError>;
    async fn update(&self, entry: &WorklogEntry) -> Result<(), RepositoryError>;
    async fn delete(&self, id: WorklogEntryId, user_id: UserId) -> Result<bool, RepositoryError>;
    async fn find_by_id(
        &self,
        id: WorklogEntryId,
        user_id: UserId,
    ) -> Result<Option<WorklogEntry>, RepositoryError>;
    async fn list(
        &self,
        user_id: UserId,
        filter: &WorklogFilter,
    ) -> Result<Vec<WorklogEntry>, RepositoryError>;
    /// List all worklog entries whose task belongs to the given recurrence template.
    /// Results are ordered by `logged_at DESC`.
    async fn find_by_recurrence(
        &self,
        user_id: UserId,
        template_id: RecurrenceTemplateId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<WorklogEntry>, RepositoryError>;

    /// The entries the consolidation job has not read yet: `consolidated_at IS
    /// NULL`, scoped to the user, **oldest first**.
    ///
    /// Oldest first because the job is a catch-up: after a day off the backlog is
    /// read in the order it happened, and a `limit` that truncates leaves the most
    /// recent entries for the next run rather than the ones already overdue.
    ///
    /// This is a per-entry watermark, deliberately **not** a timestamp cursor
    /// (§6.2 of the design): an entry inserted later with an earlier `logged_at`
    /// would be permanently skipped by a cursor, and there is no way to notice.
    ///
    /// Implementations MUST bind [`WorklogFilter::effective_limit`], never
    /// `filter.limit`.
    ///
    /// The default fails loudly instead of returning an empty list, so a
    /// repository double that does not implement it cannot make the job believe
    /// there is nothing to consolidate.
    async fn list_unconsolidated(
        &self,
        _user_id: UserId,
        _filter: &WorklogFilter,
    ) -> Result<Vec<WorklogEntry>, RepositoryError> {
        Err(RepositoryError::Database(
            "list_unconsolidated is not implemented by this repository".into(),
        ))
    }

    /// Stamp `consolidated_at` on the given entries. Returns how many rows moved.
    ///
    /// Only rows that belong to `user_id` **and** are still unmarked are touched:
    /// the first marking wins, so a re-run cannot rewrite the date on which an
    /// entry was actually consolidated.
    ///
    /// The caller must invoke this **after** the memories the entries produced are
    /// persisted (§6.2): a duplicate memory is recoverable through the rejection
    /// tombstones, an entry skipped forever is not.
    ///
    /// Same loud default, same reason: a silent `0` would look like "already done".
    async fn mark_consolidated(
        &self,
        _user_id: UserId,
        _ids: &[WorklogEntryId],
        _at: DateTime<Utc>,
    ) -> Result<u64, RepositoryError> {
        Err(RepositoryError::Database(
            "mark_consolidated is not implemented by this repository".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The derived `Default` leaves `limit` at `0`, which is exactly the value that
    /// would silently emit `LIMIT 0` — and for the consolidation job, an empty page
    /// reads as "nothing left to do".
    #[test]
    fn a_default_filter_resolves_to_the_default_limit() {
        assert_eq!(
            WorklogFilter::default().effective_limit(),
            WORKLOG_FILTER_DEFAULT_LIMIT
        );
    }

    #[test]
    fn an_oversized_limit_is_capped() {
        let greedy = WorklogFilter {
            limit: u32::MAX,
            ..WorklogFilter::default()
        };
        assert_eq!(greedy.effective_limit(), WORKLOG_FILTER_MAX_LIMIT);
    }

    #[test]
    fn a_reasonable_limit_is_honoured_as_typed() {
        let asked = WorklogFilter {
            limit: 7,
            ..WorklogFilter::default()
        };
        assert_eq!(asked.effective_limit(), 7);
    }
}
