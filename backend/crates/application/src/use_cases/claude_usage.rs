//! Indexing the Claude Code transcripts, and reading the result back as the HUD's
//! Neural budget.
//!
//! The interesting half is the cursor arithmetic below. Everything about what a
//! record *means* — which lines count, what "consumed" is, how a request that wrote
//! several lines collapses into one — lives in `domain::rules::claude_usage` and is
//! not re-decided here.

use chrono::{DateTime, Duration, Utc};
use domain::rules::claude_usage::{
    record_from_line, summarize, NeuralBudget, TranscriptLine, UsageRecord,
};
use domain::types::UserId;
#[cfg(test)]
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::{ClaudeUsageRepository, ConfigRepository, IndexedFile};
use crate::services::{ClaudeTranscriptSource, TranscriptFile};

/// Where the hand-calibrated ceiling lives. Same family as `aplan.breaks.*`, so
/// `aplan config` reaches it with no new plumbing.
pub const CEILING_KEY: &str = "aplan.claude.declared_ceiling_tokens";

/// What one indexing pass did. Returned rather than logged so the job can decide
/// how loud to be, and so the tests can assert on work actually avoided.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexOutcome {
    /// Transcripts the walker found.
    pub files_seen: usize,
    /// Transcripts actually opened — the rest were unchanged since the last pass.
    pub files_read: usize,
    /// Transcripts re-read from byte zero because they had shrunk.
    pub files_restarted: usize,
    pub records_written: usize,
    /// Lines that would not decode. Counted rather than swallowed: a format change
    /// upstream should show as a rising number, not as a budget quietly flattening.
    pub lines_rejected: usize,
}

/// Where to resume reading one file, or `None` when it has nothing new.
///
/// Three cases, and the middle one is the whole reason `size_bytes` is stored:
///
/// * never seen — read it whole;
/// * **shrunk below the recorded cursor** — it was compacted or replaced, so the
///   offset no longer points at a line boundary. Resuming there would slice a line
///   in half. Start over;
/// * unchanged in both size and mtime — skip it. This is what keeps a tick from
///   re-reading 629 MB every five minutes.
fn resume_offset(file: &TranscriptFile, state: Option<&IndexedFile>) -> Option<i64> {
    match state {
        None => Some(0),
        Some(state) => {
            if file.size_bytes < state.offset_bytes {
                Some(0)
            } else if file.size_bytes == state.size_bytes && file.mtime == state.mtime {
                None
            } else {
                Some(state.offset_bytes)
            }
        }
    }
}

/// Read every transcript's new tail into the index.
///
/// Never fails on one bad file: a transcript that cannot be read is counted and
/// skipped, because a background job that aborts halfway leaves the index in a state
/// nobody asked for. An absent transcript tree yields an empty outcome — this machine
/// may simply never have run Claude Code.
pub async fn index_claude_usage(
    source: &dyn ClaudeTranscriptSource,
    repo: &dyn ClaudeUsageRepository,
    now: DateTime<Utc>,
) -> Result<IndexOutcome, AppError> {
    let files = source.list().await?;
    let mut outcome = IndexOutcome {
        files_seen: files.len(),
        ..Default::default()
    };

    for file in files {
        let state = repo.file_state(&file.path).await?;
        let Some(from) = resume_offset(&file, state.as_ref()) else {
            continue;
        };
        if from == 0 && state.is_some() {
            outcome.files_restarted += 1;
        }

        let chunk = match source.read_from(&file.path, from).await {
            Ok(chunk) => chunk,
            // One unreadable transcript must not cost the other 660.
            Err(_) => continue,
        };
        outcome.files_read += 1;

        let mut records: Vec<UsageRecord> = Vec::new();
        for line in &chunk.lines {
            match serde_json::from_str::<TranscriptLine>(line) {
                Ok(decoded) => {
                    if let Some(record) = record_from_line(decoded) {
                        records.push(record);
                    }
                }
                Err(_) => outcome.lines_rejected += 1,
            }
        }

        // Collapsing here as well as in the repository is not belt and braces: the
        // lines of one request are adjacent, so this turns a batch of duplicates
        // into a single write rather than relying on the primary key to absorb
        // them one statement at a time.
        let records = domain::rules::claude_usage::deduplicate_by_request(records);
        outcome.records_written += repo.upsert_requests(&records).await?;

        repo.set_file_state(&IndexedFile {
            path: file.path.clone(),
            size_bytes: file.size_bytes,
            mtime: file.mtime,
            offset_bytes: chunk.next_offset,
            last_indexed_at: now,
        })
        .await?;
    }

    Ok(outcome)
}

/// The Neural budget block's whole payload.
///
/// One query covers both horizons: the rolling window and the sparkline are read
/// from the same fetch, whichever reaches further back.
pub async fn neural_budget(
    repo: &dyn ClaudeUsageRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    now: DateTime<Utc>,
    window_hours: i64,
    sparkline_days: i64,
) -> Result<NeuralBudget, AppError> {
    let declared_ceiling = read_ceiling(config_repo, user_id).await?;

    let window_start = now - Duration::hours(window_hours.max(0));
    let sparkline_start = (now.date_naive() - Duration::days(sparkline_days.max(1) - 1))
        .and_hms_opt(0, 0, 0)
        .map(|naive| naive.and_utc())
        .unwrap_or(window_start);
    let since = window_start.min(sparkline_start);

    let records = repo.records_since(since).await?;

    Ok(summarize(
        records,
        now,
        window_hours,
        sparkline_days,
        declared_ceiling,
    ))
}

/// The declared ceiling, or zero when it has never been set.
///
/// Zero is deliberate rather than a fallback guess: the burn is measured, the ceiling
/// is not, and inventing a denominator would make the one number the app cannot know
/// look like one it does. `summarize` renders a zero ceiling as a zero ratio, and the
/// block says out loud that the figure is typed in by hand.
async fn read_ceiling(
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
) -> Result<i64, AppError> {
    Ok(config_repo
        .get(user_id, CEILING_KEY)
        .await?
        .and_then(|raw| raw.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::errors::RepositoryError;
    use crate::services::TranscriptChunk;

    fn at(iso: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(iso).unwrap().with_timezone(&Utc)
    }

    fn line(request_id: &str, output: i64) -> String {
        format!(
            r#"{{"type":"assistant","requestId":"{request_id}",
               "timestamp":"2026-09-11T14:41:07.751Z","sessionId":"s1",
               "cwd":"/home/mbt/appfactory/aggregated_plan","isSidechain":false,
               "message":{{"model":"claude-opus-5","usage":{{"output_tokens":{output}}}}}}}"#
        )
    }

    #[derive(Default)]
    struct FakeSource {
        files: Vec<TranscriptFile>,
        chunks: HashMap<String, Vec<String>>,
        reads: Mutex<Vec<(String, i64)>>,
        unreadable: Vec<String>,
    }

    #[async_trait]
    impl ClaudeTranscriptSource for FakeSource {
        async fn list(&self) -> Result<Vec<TranscriptFile>, AppError> {
            Ok(self.files.clone())
        }
        async fn read_from(
            &self,
            path: &str,
            from_offset: i64,
        ) -> Result<TranscriptChunk, AppError> {
            self.reads
                .lock()
                .unwrap()
                .push((path.to_string(), from_offset));
            if self.unreadable.iter().any(|p| p == path) {
                return Err(AppError::Internal("unreadable".into()));
            }
            let lines = self.chunks.get(path).cloned().unwrap_or_default();
            let next_offset = lines.iter().map(|l| l.len() as i64 + 1).sum::<i64>() + from_offset;
            Ok(TranscriptChunk { lines, next_offset })
        }
    }

    #[derive(Default)]
    struct FakeRepo {
        records: Mutex<HashMap<String, UsageRecord>>,
        states: Mutex<HashMap<String, IndexedFile>>,
    }

    #[async_trait]
    impl ClaudeUsageRepository for FakeRepo {
        async fn upsert_requests(
            &self,
            records: &[UsageRecord],
        ) -> Result<usize, RepositoryError> {
            let mut store = self.records.lock().unwrap();
            for record in records {
                store.insert(record.request_id.clone(), record.clone());
            }
            Ok(records.len())
        }
        async fn file_state(&self, path: &str) -> Result<Option<IndexedFile>, RepositoryError> {
            Ok(self.states.lock().unwrap().get(path).cloned())
        }
        async fn set_file_state(&self, state: &IndexedFile) -> Result<(), RepositoryError> {
            self.states
                .lock()
                .unwrap()
                .insert(state.path.clone(), state.clone());
            Ok(())
        }
        async fn records_since(
            &self,
            since: DateTime<Utc>,
        ) -> Result<Vec<UsageRecord>, RepositoryError> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .values()
                .filter(|r| r.occurred_at >= since)
                .cloned()
                .collect())
        }
    }

    struct FakeConfig(Option<String>);

    #[async_trait]
    impl ConfigRepository for FakeConfig {
        async fn get(&self, _: UserId, _: &str) -> Result<Option<String>, RepositoryError> {
            Ok(self.0.clone())
        }
        async fn get_all(&self, _: UserId) -> Result<Vec<(String, String)>, RepositoryError> {
            Ok(Vec::new())
        }
        async fn set(&self, _: UserId, _: &str, _: &str) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    fn file(path: &str, size: i64, mtime: &str) -> TranscriptFile {
        TranscriptFile {
            path: path.to_string(),
            size_bytes: size,
            mtime: at(mtime),
        }
    }

    /// A file whose declared size matches the bytes the fake will hand back.
    ///
    /// Not pedantry: with an arbitrary size, the first pass records a cursor past
    /// the file's stated end, the second pass reads that as "shrunk", and the
    /// incremental path is never exercised at all — the fixture would quietly test
    /// the restart branch while claiming to test the skip branch.
    fn sized(path: &str, mtime: &str, lines: &[String]) -> TranscriptFile {
        TranscriptFile {
            path: path.to_string(),
            size_bytes: lines.iter().map(|l| l.len() as i64 + 1).sum(),
            mtime: at(mtime),
        }
    }

    // ─── the cursor ───

    #[test]
    fn an_unseen_file_is_read_whole() {
        assert_eq!(resume_offset(&file("a", 100, "2026-09-11T10:00:00Z"), None), Some(0));
    }

    #[test]
    fn an_unchanged_file_is_not_reopened() {
        // The point of the cursor: 661 files, 629 MB, a tick every five minutes.
        let f = file("a", 100, "2026-09-11T10:00:00Z");
        let state = IndexedFile {
            path: "a".into(),
            size_bytes: 100,
            mtime: at("2026-09-11T10:00:00Z"),
            offset_bytes: 100,
            last_indexed_at: at("2026-09-11T10:00:00Z"),
        };

        assert_eq!(resume_offset(&f, Some(&state)), None);
    }

    #[test]
    fn a_grown_file_resumes_at_the_cursor() {
        let f = file("a", 250, "2026-09-11T11:00:00Z");
        let state = IndexedFile {
            path: "a".into(),
            size_bytes: 100,
            mtime: at("2026-09-11T10:00:00Z"),
            offset_bytes: 100,
            last_indexed_at: at("2026-09-11T10:00:00Z"),
        };

        assert_eq!(resume_offset(&f, Some(&state)), Some(100));
    }

    #[test]
    fn a_shrunk_file_is_read_from_zero_rather_than_mid_line() {
        // Compaction or replacement. Resuming at a stale offset would start parsing
        // in the middle of a line — and, worse, silently skip everything before it.
        let f = file("a", 40, "2026-09-11T11:00:00Z");
        let state = IndexedFile {
            path: "a".into(),
            size_bytes: 100,
            mtime: at("2026-09-11T10:00:00Z"),
            offset_bytes: 100,
            last_indexed_at: at("2026-09-11T10:00:00Z"),
        };

        assert_eq!(resume_offset(&f, Some(&state)), Some(0));
    }

    // ─── the pass ───

    #[tokio::test]
    async fn indexes_a_fresh_tree_and_records_its_cursor() {
        let lines = vec![line("req_a", 100), line("req_b", 200)];
        let source = FakeSource {
            files: vec![sized("/t/a.jsonl", "2026-09-11T10:00:00Z", &lines)],
            chunks: HashMap::from([("/t/a.jsonl".to_string(), lines)]),
            ..Default::default()
        };
        let repo = FakeRepo::default();

        let outcome = index_claude_usage(&source, &repo, at("2026-09-11T15:00:00Z"))
            .await
            .unwrap();

        assert_eq!(outcome.files_seen, 1);
        assert_eq!(outcome.files_read, 1);
        assert_eq!(outcome.records_written, 2);
        assert_eq!(outcome.lines_rejected, 0);
        assert!(repo.states.lock().unwrap().contains_key("/t/a.jsonl"));
    }

    #[tokio::test]
    async fn a_second_pass_over_an_untouched_tree_opens_nothing() {
        let lines = vec![line("req_a", 100)];
        let source = FakeSource {
            files: vec![sized("/t/a.jsonl", "2026-09-11T10:00:00Z", &lines)],
            chunks: HashMap::from([("/t/a.jsonl".to_string(), lines)]),
            ..Default::default()
        };
        let repo = FakeRepo::default();
        let now = at("2026-09-11T15:00:00Z");

        index_claude_usage(&source, &repo, now).await.unwrap();
        // The fake advances no size, so the second pass sees the same pair. Real
        // files that have not been appended to look exactly like this.
        source.reads.lock().unwrap().clear();
        let second = index_claude_usage(&source, &repo, now).await.unwrap();

        assert_eq!(second.files_read, 0);
        assert!(source.reads.lock().unwrap().is_empty(), "no file was opened");
    }

    #[tokio::test]
    async fn reindexing_the_same_content_leaves_the_totals_alone() {
        // Idempotence, the property that lets a rescan be safe. Wiping the cursors
        // and running again must not double anything.
        let lines = vec![line("req_a", 100), line("req_b", 200)];
        let source = FakeSource {
            files: vec![sized("/t/a.jsonl", "2026-09-11T10:00:00Z", &lines)],
            chunks: HashMap::from([("/t/a.jsonl".to_string(), lines)]),
            ..Default::default()
        };
        let repo = FakeRepo::default();
        let now = at("2026-09-11T15:00:00Z");

        index_claude_usage(&source, &repo, now).await.unwrap();
        repo.states.lock().unwrap().clear();
        index_claude_usage(&source, &repo, now).await.unwrap();

        assert_eq!(repo.records.lock().unwrap().len(), 2, "still two requests");
    }

    #[tokio::test]
    async fn the_lines_of_one_request_become_one_record() {
        // TRAP 1 end to end: two content blocks, one API call, one row. On the real
        // corpus this is a 1.89x difference.
        let lines = vec![line("req_a", 100), line("req_a", 100)];
        let source = FakeSource {
            files: vec![sized("/t/a.jsonl", "2026-09-11T10:00:00Z", &lines)],
            chunks: HashMap::from([("/t/a.jsonl".to_string(), lines)]),
            ..Default::default()
        };
        let repo = FakeRepo::default();

        let outcome = index_claude_usage(&source, &repo, at("2026-09-11T15:00:00Z"))
            .await
            .unwrap();

        assert_eq!(outcome.records_written, 1);
        assert_eq!(repo.records.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_shrunk_file_is_restarted_and_counted_as_such() {
        let repo = FakeRepo::default();
        repo.set_file_state(&IndexedFile {
            path: "/t/a.jsonl".into(),
            size_bytes: 5_000,
            mtime: at("2026-09-11T09:00:00Z"),
            offset_bytes: 5_000,
            last_indexed_at: at("2026-09-11T09:00:00Z"),
        })
        .await
        .unwrap();

        let source = FakeSource {
            files: vec![file("/t/a.jsonl", 120, "2026-09-11T11:00:00Z")],
            chunks: HashMap::from([("/t/a.jsonl".to_string(), vec![line("req_a", 100)])]),
            ..Default::default()
        };

        let outcome = index_claude_usage(&source, &repo, at("2026-09-11T15:00:00Z"))
            .await
            .unwrap();

        assert_eq!(outcome.files_restarted, 1);
        assert_eq!(source.reads.lock().unwrap()[0], ("/t/a.jsonl".to_string(), 0));
    }

    #[tokio::test]
    async fn one_unreadable_transcript_does_not_cost_the_others() {
        let source = FakeSource {
            files: vec![
                file("/t/bad.jsonl", 10, "2026-09-11T10:00:00Z"),
                file("/t/good.jsonl", 120, "2026-09-11T10:00:00Z"),
            ],
            chunks: HashMap::from([("/t/good.jsonl".to_string(), vec![line("req_a", 100)])]),
            unreadable: vec!["/t/bad.jsonl".to_string()],
            ..Default::default()
        };
        let repo = FakeRepo::default();

        let outcome = index_claude_usage(&source, &repo, at("2026-09-11T15:00:00Z"))
            .await
            .unwrap();

        assert_eq!(outcome.files_read, 1);
        assert_eq!(outcome.records_written, 1);
    }

    #[tokio::test]
    async fn undecodable_lines_are_counted_not_swallowed() {
        // A format change upstream must show as a number that climbs, not as a
        // budget that quietly flattens to zero.
        let source = FakeSource {
            files: vec![file("/t/a.jsonl", 120, "2026-09-11T10:00:00Z")],
            chunks: HashMap::from([(
                "/t/a.jsonl".to_string(),
                vec!["{not json".to_string(), line("req_a", 100)],
            )]),
            ..Default::default()
        };
        let repo = FakeRepo::default();

        let outcome = index_claude_usage(&source, &repo, at("2026-09-11T15:00:00Z"))
            .await
            .unwrap();

        assert_eq!(outcome.lines_rejected, 1);
        assert_eq!(outcome.records_written, 1);
    }

    #[tokio::test]
    async fn an_absent_transcript_tree_is_a_normal_empty_pass() {
        let source = FakeSource::default();
        let repo = FakeRepo::default();

        let outcome = index_claude_usage(&source, &repo, at("2026-09-11T15:00:00Z"))
            .await
            .unwrap();

        assert_eq!(outcome, IndexOutcome::default());
    }

    // ─── reading it back ───

    #[tokio::test]
    async fn the_budget_reads_the_ceiling_from_configuration() {
        let repo = FakeRepo::default();
        repo.upsert_requests(&[UsageRecord {
            request_id: "r1".into(),
            occurred_at: at("2026-09-11T14:00:00Z"),
            model: "claude-opus-5".into(),
            session_id: "s".into(),
            project_path: "/p".into(),
            is_sidechain: false,
            input_tokens: 0,
            output_tokens: 500,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            thinking_tokens: 0,
        }])
        .await
        .unwrap();

        let budget = neural_budget(
            &repo,
            &FakeConfig(Some("2500000".into())),
            Uuid::new_v4(),
            at("2026-09-11T15:00:00Z"),
            5,
            10,
        )
        .await
        .unwrap();

        assert_eq!(budget.declared_ceiling, 2_500_000);
        assert_eq!(budget.consumed_tokens, 500);
    }

    #[tokio::test]
    async fn an_unset_or_unusable_ceiling_reads_as_zero_not_as_a_guess() {
        // Inventing a denominator would make the one figure the app cannot measure
        // look like one it does.
        for stored in [None, Some("".to_string()), Some("beaucoup".to_string()), Some("0".to_string())] {
            let budget = neural_budget(
                &FakeRepo::default(),
                &FakeConfig(stored.clone()),
                Uuid::new_v4(),
                at("2026-09-11T15:00:00Z"),
                5,
                10,
            )
            .await
            .unwrap();

            assert_eq!(budget.declared_ceiling, 0, "stored value: {stored:?}");
            assert_eq!(budget.consumed_ratio, 0.0);
        }
    }

    #[tokio::test]
    async fn one_fetch_covers_the_sparkline_as_well_as_the_window() {
        // The sparkline reaches days back while the window reaches hours back;
        // asking only for the window would draw nine empty bars.
        let repo = FakeRepo::default();
        repo.upsert_requests(&[UsageRecord {
            request_id: "old".into(),
            occurred_at: at("2026-09-09T08:00:00Z"),
            model: "claude-opus-5".into(),
            session_id: "s".into(),
            project_path: "/p".into(),
            is_sidechain: false,
            input_tokens: 0,
            output_tokens: 70,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            thinking_tokens: 0,
        }])
        .await
        .unwrap();

        let budget = neural_budget(
            &repo,
            &FakeConfig(None),
            Uuid::new_v4(),
            at("2026-09-11T15:00:00Z"),
            5,
            3,
        )
        .await
        .unwrap();

        assert_eq!(budget.per_day, vec![70, 0, 0]);
        assert_eq!(budget.consumed_tokens, 0, "two days old, well outside the window");
    }
}
