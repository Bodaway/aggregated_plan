use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::errors::AppError;

/// One memory file as read from disk. `contents` is the raw markdown, parsed by
/// `domain::rules::memory_import`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryFile {
    pub file_name: String,
    pub contents: String,
    /// Filesystem modification time, used as the fallback `occurred_at` when the
    /// frontmatter carries no `modified`.
    pub modified_at: Option<DateTime<Utc>>,
}

/// Reads the harness memory directory. READ-ONLY by contract: that directory has
/// another writer (the harness auto-memory), and two writers on a generated file
/// diverge (§7.2 of the design). Nothing in aplan may write there.
#[async_trait]
pub trait MemoryFileSource: Send + Sync {
    /// Markdown files directly inside `directory`, in a stable order. Whatever is
    /// there — the set grows over time and must not be hardcoded.
    async fn list(&self, directory: &str) -> Result<Vec<MemoryFile>, AppError>;
}
