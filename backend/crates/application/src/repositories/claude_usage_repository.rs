use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::rules::claude_usage::UsageRecord;

use crate::errors::RepositoryError;

/// Where the last pass stopped in one transcript.
///
/// A cache, never a ledger. Because `request_id` is the primary key of the request
/// table, replaying a file writes the same rows — so losing every cursor costs one
/// full rescan (about two seconds for the whole corpus) and never a wrong total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFile {
    pub path: String,
    pub size_bytes: i64,
    pub mtime: DateTime<Utc>,
    pub offset_bytes: i64,
    pub last_indexed_at: DateTime<Utc>,
}

/// Persistence for the Claude usage index.
#[async_trait]
pub trait ClaudeUsageRepository: Send + Sync {
    /// Write records, keyed by `request_id`. Idempotent by construction: the same
    /// batch applied twice leaves the same rows. Returns how many were written.
    async fn upsert_requests(&self, records: &[UsageRecord]) -> Result<usize, RepositoryError>;

    /// The cursor for one file, or `None` if it has never been read.
    async fn file_state(&self, path: &str) -> Result<Option<IndexedFile>, RepositoryError>;

    /// Store a file's cursor.
    async fn set_file_state(&self, state: &IndexedFile) -> Result<(), RepositoryError>;

    /// Every record at or after `since`, in no guaranteed order — the summary sorts
    /// what it needs. `since` is the older of the two horizons the block asks for
    /// (the rolling window and the sparkline), so one query serves both.
    async fn records_since(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<UsageRecord>, RepositoryError>;
}
