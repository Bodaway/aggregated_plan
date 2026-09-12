use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::errors::AppError;

/// One transcript file on disk, as the walker found it.
///
/// `size_bytes` and `mtime` together are the whole change-detection story: a file
/// whose pair is unchanged since the last pass has nothing new to say, and a file
/// whose size has *dropped below* the recorded cursor was compacted or replaced, so
/// its byte offsets no longer point at line boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptFile {
    pub path: String,
    pub size_bytes: i64,
    pub mtime: DateTime<Utc>,
}

/// What one read returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptChunk {
    /// Complete lines only, in file order. A trailing partial line — the writer was
    /// mid-append — is left for the next pass rather than parsed half-formed.
    pub lines: Vec<String>,
    /// Byte offset just past the last complete line, to resume from.
    pub next_offset: i64,
}

/// Reads the Claude Code transcript tree (`~/.claude/projects/**/*.jsonl`).
///
/// READ-ONLY by contract: that tree belongs to the harness, which appends to it
/// continuously while this runs. Nothing in aplan may write there.
///
/// The tree is genuinely nested — measured on the real corpus, 166 files sit at
/// depth 2 and 495 deeper, under `<session>/subagents/` and below — so `list` walks
/// recursively. A single-level listing would miss three quarters of the corpus, which
/// is to say very nearly all of the subagent consumption.
#[async_trait]
pub trait ClaudeTranscriptSource: Send + Sync {
    /// Every `.jsonl` under the root, at any depth, in a stable order. An absent or
    /// unreadable root is an empty list, not an error: this machine may simply never
    /// have run Claude Code.
    async fn list(&self) -> Result<Vec<TranscriptFile>, AppError>;

    /// Complete lines of `path` from `from_offset` to the end of the file.
    async fn read_from(&self, path: &str, from_offset: i64) -> Result<TranscriptChunk, AppError>;
}
