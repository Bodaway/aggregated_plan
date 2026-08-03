use async_trait::async_trait;
use chrono::{DateTime, Utc};

use application::errors::AppError;
use application::services::memory_file_source::{MemoryFile, MemoryFileSource};

/// Reads the harness memory directory from the local filesystem. Suitable for a
/// single-user local deployment where the backend can see the user's home.
///
/// READ-ONLY by design: that directory has another writer (the harness
/// auto-memory), and two writers on a generated file diverge. Nothing here opens
/// a file for writing.
pub struct FsMemoryFileSource;

impl FsMemoryFileSource {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsMemoryFileSource {
    fn default() -> Self {
        Self::new()
    }
}

fn io_error(directory: &str, error: std::io::Error) -> AppError {
    match error.kind() {
        std::io::ErrorKind::NotFound => {
            AppError::NotFound(format!("memory directory {directory}"))
        }
        _ => AppError::Configuration(format!("cannot read memory directory {directory}: {error}")),
    }
}

#[async_trait]
impl MemoryFileSource for FsMemoryFileSource {
    async fn list(&self, directory: &str) -> Result<Vec<MemoryFile>, AppError> {
        let mut entries = tokio::fs::read_dir(directory)
            .await
            .map_err(|e| io_error(directory, e))?;

        let mut files: Vec<MemoryFile> = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| io_error(directory, e))?
        {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let metadata = entry.metadata().await.ok();
            if metadata.as_ref().is_some_and(|m| m.is_dir()) {
                continue;
            }
            // A file that cannot be read is skipped rather than failing the whole
            // import: one unreadable note must not block the other four.
            let Ok(contents) = tokio::fs::read_to_string(&path).await else {
                tracing::warn!("skipping unreadable memory file {}", path.display());
                continue;
            };
            let file_name = entry.file_name().to_string_lossy().to_string();
            let modified_at = metadata
                .and_then(|m| m.modified().ok())
                .map(DateTime::<Utc>::from);
            files.push(MemoryFile {
                file_name,
                contents,
                modified_at,
            });
        }

        // `read_dir` order is filesystem-defined; a stable order keeps the import
        // report readable and the tests deterministic.
        files.sort_by(|a, b| a.file_name.cmp(&b.file_name));
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Unique scratch directory, removed by `Scratch`'s drop.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("aplan-memfiles-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).expect("create scratch dir");
            Self { path }
        }

        fn write(&self, name: &str, contents: &str) {
            std::fs::write(self.path.join(name), contents).expect("write scratch file");
        }

        fn as_str(&self) -> String {
            self.path.to_string_lossy().to_string()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[tokio::test]
    async fn lists_markdown_files_in_a_stable_order() {
        let dir = Scratch::new();
        dir.write("b_note.md", "---\nname: b\n---\nbody b");
        dir.write("a_note.md", "---\nname: a\n---\nbody a");
        dir.write("notes.txt", "not markdown");

        let files = FsMemoryFileSource::new()
            .list(&dir.as_str())
            .await
            .expect("lists");
        let names: Vec<&str> = files.iter().map(|f| f.file_name.as_str()).collect();
        assert_eq!(names, vec!["a_note.md", "b_note.md"], "sorted, .txt excluded");
        assert!(files[0].contents.contains("body a"));
        assert!(files[0].modified_at.is_some(), "the mtime is the date fallback");
    }

    #[tokio::test]
    async fn includes_the_index_file_so_the_caller_can_skip_it() {
        // MEMORY.md has no frontmatter; the decision to skip belongs to the
        // import rule, not to the reader.
        let dir = Scratch::new();
        dir.write("MEMORY.md", "- [a](a.md) — hook");
        let files = FsMemoryFileSource::new()
            .list(&dir.as_str())
            .await
            .expect("lists");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name, "MEMORY.md");
    }

    #[tokio::test]
    async fn an_empty_directory_yields_nothing() {
        let dir = Scratch::new();
        assert!(FsMemoryFileSource::new()
            .list(&dir.as_str())
            .await
            .expect("lists")
            .is_empty());
    }

    #[tokio::test]
    async fn a_missing_directory_is_reported_as_not_found() {
        let missing = std::env::temp_dir().join(format!("aplan-absent-{}", uuid::Uuid::new_v4()));
        let err = FsMemoryFileSource::new()
            .list(&missing.to_string_lossy())
            .await
            .expect_err("must not succeed");
        assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn subdirectories_are_ignored() {
        let dir = Scratch::new();
        std::fs::create_dir_all(dir.path.join("nested.md")).expect("create dir named like a file");
        dir.write("real.md", "---\nname: r\n---\nbody");
        let files = FsMemoryFileSource::new()
            .list(&dir.as_str())
            .await
            .expect("lists");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name, "real.md");
    }
}
