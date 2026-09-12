use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

use application::errors::AppError;
use application::services::claude_transcript_source::{
    ClaudeTranscriptSource, TranscriptChunk, TranscriptFile,
};

/// Reads the Claude Code transcript tree from the local filesystem.
///
/// READ-ONLY by contract: the harness appends to these files continuously while
/// this runs, and nothing here opens one for writing.
pub struct FsClaudeTranscriptSource {
    root: PathBuf,
}

impl FsClaudeTranscriptSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `~/.claude/projects`, the harness's own location.
    pub fn from_home() -> Option<Self> {
        std::env::var_os("HOME").map(|home| {
            Self::new(Path::new(&home).join(".claude").join("projects"))
        })
    }
}

/// Recursive walk, iterative so a pathological depth cannot blow the stack and so
/// no `Box<dyn Future>` is needed in an async fn.
///
/// Recursion is not optional here: measured on the real corpus, 166 transcripts sit
/// directly under a project directory and 495 deeper — subagent transcripts live in
/// `<session>/subagents/` and nest further. A single-level listing would miss three
/// quarters of the files, which is very nearly all of the subagent consumption.
async fn walk_jsonl(root: &Path) -> Vec<TranscriptFile> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];

    while let Some(directory) = pending.pop() {
        // An unreadable directory is skipped, never fatal: the tree belongs to
        // another program and may hold anything.
        let Ok(mut entries) = tokio::fs::read_dir(&directory).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(path_str) = path.to_str() else { continue };
            found.push(TranscriptFile {
                path: path_str.to_string(),
                size_bytes: metadata.len() as i64,
                mtime: metadata
                    .modified()
                    .map(DateTime::<Utc>::from)
                    .unwrap_or_else(|_| Utc::now()),
            });
        }
    }

    // Stable order, so two passes see the same tree in the same sequence and a
    // failure is reproducible.
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

#[async_trait]
impl ClaudeTranscriptSource for FsClaudeTranscriptSource {
    /// An absent root is an empty list rather than an error: this machine may
    /// simply never have run Claude Code, and a background job must not shout
    /// about a normal state.
    async fn list(&self) -> Result<Vec<TranscriptFile>, AppError> {
        Ok(walk_jsonl(&self.root).await)
    }

    async fn read_from(&self, path: &str, from_offset: i64) -> Result<TranscriptChunk, AppError> {
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|e| AppError::Configuration(format!("cannot open {path}: {e}")))?;

        if from_offset > 0 {
            file.seek(SeekFrom::Start(from_offset as u64))
                .await
                .map_err(|e| AppError::Configuration(format!("cannot seek {path}: {e}")))?;
        }

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .await
            .map_err(|e| AppError::Configuration(format!("cannot read {path}: {e}")))?;

        // Complete lines only. The harness is very likely appending to this file
        // right now, so the tail can be half a line — parsed, it would count as a
        // decode failure, and skipped without rewinding the cursor it would be lost
        // for good. Stopping at the last newline leaves it for the next pass.
        let last_newline = buffer.iter().rposition(|byte| *byte == b'\n');
        let Some(end) = last_newline else {
            return Ok(TranscriptChunk {
                lines: Vec::new(),
                next_offset: from_offset,
            });
        };

        let complete = &buffer[..=end];
        let lines = String::from_utf8_lossy(complete)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.to_string())
            .collect();

        Ok(TranscriptChunk {
            lines,
            next_offset: from_offset + complete.len() as i64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
    }

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("aplan-transcripts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[tokio::test]
    async fn finds_subagent_transcripts_nested_under_a_session() {
        // Three quarters of the real corpus lives below the first level. A walker
        // that stopped there would drop almost all of the subagent consumption.
        let root = temp_root("nested");
        write(&root.join("proj/a.jsonl"), "{}\n");
        write(&root.join("proj/session-1/subagents/agent-x.jsonl"), "{}\n");
        write(&root.join("proj/session-1/subagents/deeper/agent-y.jsonl"), "{}\n");
        write(&root.join("proj/notes.txt"), "ignored\n");

        let files = FsClaudeTranscriptSource::new(&root).list().await.unwrap();

        let names: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(names.len(), 3, "the .txt is not a transcript: {names:?}");
        assert!(names.iter().any(|p| p.ends_with("agent-x.jsonl")));
        assert!(names.iter().any(|p| p.ends_with("agent-y.jsonl")));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn an_absent_root_lists_nothing_rather_than_failing() {
        // A machine that has never run Claude Code is a normal state, not an error
        // for a background job to shout about.
        let source = FsClaudeTranscriptSource::new("/nonexistent/aplan/transcripts");

        assert_eq!(source.list().await.unwrap(), Vec::new());
    }

    #[tokio::test]
    async fn reads_from_an_offset_and_reports_where_it_stopped() {
        let root = temp_root("offset");
        let path = root.join("a.jsonl");
        write(&path, "line-one\nline-two\n");
        let source = FsClaudeTranscriptSource::new(&root);
        let path_str = path.to_str().unwrap();

        let whole = source.read_from(path_str, 0).await.unwrap();
        assert_eq!(whole.lines, vec!["line-one", "line-two"]);
        assert_eq!(whole.next_offset, 18);

        let tail = source.read_from(path_str, 9).await.unwrap();
        assert_eq!(tail.lines, vec!["line-two"]);
        assert_eq!(tail.next_offset, 18);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn a_half_written_trailing_line_is_left_for_the_next_pass() {
        // The harness appends while this reads. A partial tail parsed now would
        // count as a decode failure; skipped without rewinding the cursor it would
        // be lost. The cursor stops at the last newline instead.
        let root = temp_root("partial");
        let path = root.join("a.jsonl");
        write(&path, "complete\npartial-so-f");
        let source = FsClaudeTranscriptSource::new(&root);
        let path_str = path.to_str().unwrap();

        let chunk = source.read_from(path_str, 0).await.unwrap();

        assert_eq!(chunk.lines, vec!["complete"]);
        assert_eq!(chunk.next_offset, 9, "just past the newline, not past the tail");

        // The rest arrives once the writer finishes the line.
        write(&path, "complete\npartial-so-far\n");
        let rest = source.read_from(path_str, chunk.next_offset).await.unwrap();
        assert_eq!(rest.lines, vec!["partial-so-far"]);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn a_file_with_no_newline_at_all_advances_nothing() {
        let root = temp_root("noline");
        let path = root.join("a.jsonl");
        write(&path, "still-being-written");
        let source = FsClaudeTranscriptSource::new(&root);

        let chunk = source.read_from(path.to_str().unwrap(), 0).await.unwrap();

        assert!(chunk.lines.is_empty());
        assert_eq!(chunk.next_offset, 0, "so the line is read whole next time");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn reports_size_and_mtime_for_the_change_check() {
        let root = temp_root("stat");
        write(&root.join("a.jsonl"), "0123456789\n");

        let files = FsClaudeTranscriptSource::new(&root).list().await.unwrap();

        assert_eq!(files[0].size_bytes, 11);
        assert!(files[0].mtime <= Utc::now());

        std::fs::remove_dir_all(&root).unwrap();
    }
}
