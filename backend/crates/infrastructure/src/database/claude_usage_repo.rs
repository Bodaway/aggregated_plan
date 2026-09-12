use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use application::errors::RepositoryError;
use application::repositories::{ClaudeUsageRepository, IndexedFile};
use domain::rules::claude_usage::UsageRecord;

/// How many requests go into one `INSERT`. SQLite's default parameter ceiling is
/// 999 and each row binds eleven columns, so 80 rows (880 parameters) is the
/// largest safe batch — and a first full pass of the corpus is ~29 000 rows, which
/// is 360 statements instead of 29 000.
const BATCH_ROWS: usize = 80;

pub struct SqliteClaudeUsageRepository {
    pool: SqlitePool,
}

impl SqliteClaudeUsageRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn row_to_record(row: &sqlx::sqlite::SqliteRow) -> Result<UsageRecord, RepositoryError> {
    let occurred_at: String = Row::get(row, "occurred_at");
    let occurred_at = DateTime::parse_from_rfc3339(&occurred_at)
        .map_err(|e| RepositoryError::Serialization(e.to_string()))?
        .with_timezone(&Utc);

    Ok(UsageRecord {
        request_id: Row::get(row, "request_id"),
        occurred_at,
        model: Row::get(row, "model"),
        session_id: Row::get(row, "session_id"),
        project_path: Row::get(row, "project_path"),
        is_sidechain: Row::get::<i64, _>(row, "is_sidechain") != 0,
        input_tokens: Row::get(row, "input_tokens"),
        output_tokens: Row::get(row, "output_tokens"),
        cache_creation_input_tokens: Row::get(row, "cache_creation_input_tokens"),
        cache_read_input_tokens: Row::get(row, "cache_read_input_tokens"),
        thinking_tokens: Row::get(row, "thinking_tokens"),
    })
}

#[async_trait]
impl ClaudeUsageRepository for SqliteClaudeUsageRepository {
    /// `INSERT OR REPLACE` on the `request_id` primary key. That is the whole
    /// idempotence story: the lines of one request repeat the same usage object, a
    /// chunk boundary can split them across two passes, and a rescan replays entire
    /// files — all three land on the same row with the same values.
    async fn upsert_requests(&self, records: &[UsageRecord]) -> Result<usize, RepositoryError> {
        if records.is_empty() {
            return Ok(0);
        }

        let mut written = 0usize;
        for batch in records.chunks(BATCH_ROWS) {
            let placeholders = std::iter::repeat("(?,?,?,?,?,?,?,?,?,?,?)")
                .take(batch.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "INSERT OR REPLACE INTO claude_usage_requests
                   (request_id, occurred_at, model, session_id, project_path, is_sidechain,
                    input_tokens, output_tokens, cache_creation_input_tokens,
                    cache_read_input_tokens, thinking_tokens)
                 VALUES {placeholders}"
            );

            let mut query = sqlx::query(&sql);
            for record in batch {
                query = query
                    .bind(&record.request_id)
                    .bind(record.occurred_at.to_rfc3339())
                    .bind(&record.model)
                    .bind(&record.session_id)
                    .bind(&record.project_path)
                    .bind(i64::from(record.is_sidechain))
                    .bind(record.input_tokens)
                    .bind(record.output_tokens)
                    .bind(record.cache_creation_input_tokens)
                    .bind(record.cache_read_input_tokens)
                    .bind(record.thinking_tokens);
            }

            query
                .execute(&self.pool)
                .await
                .map_err(|e| RepositoryError::Database(e.to_string()))?;
            written += batch.len();
        }

        Ok(written)
    }

    async fn file_state(&self, path: &str) -> Result<Option<IndexedFile>, RepositoryError> {
        let row = sqlx::query(
            "SELECT path, size_bytes, mtime, offset_bytes, last_indexed_at
             FROM claude_usage_files WHERE path = ?",
        )
        .bind(path)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        let Some(row) = row else { return Ok(None) };
        let parse = |column: &str| -> Result<DateTime<Utc>, RepositoryError> {
            let raw: String = Row::get(&row, column);
            Ok(DateTime::parse_from_rfc3339(&raw)
                .map_err(|e| RepositoryError::Serialization(e.to_string()))?
                .with_timezone(&Utc))
        };

        Ok(Some(IndexedFile {
            path: Row::get(&row, "path"),
            size_bytes: Row::get(&row, "size_bytes"),
            mtime: parse("mtime")?,
            offset_bytes: Row::get(&row, "offset_bytes"),
            last_indexed_at: parse("last_indexed_at")?,
        }))
    }

    async fn set_file_state(&self, state: &IndexedFile) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO claude_usage_files
               (path, size_bytes, mtime, offset_bytes, last_indexed_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(path) DO UPDATE SET
               size_bytes = excluded.size_bytes,
               mtime = excluded.mtime,
               offset_bytes = excluded.offset_bytes,
               last_indexed_at = excluded.last_indexed_at",
        )
        .bind(&state.path)
        .bind(state.size_bytes)
        .bind(state.mtime.to_rfc3339())
        .bind(state.offset_bytes)
        .bind(state.last_indexed_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn records_since(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<UsageRecord>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT request_id, occurred_at, model, session_id, project_path, is_sidechain,
                    input_tokens, output_tokens, cache_creation_input_tokens,
                    cache_read_input_tokens, thinking_tokens
             FROM claude_usage_requests
             WHERE occurred_at >= ?
             ORDER BY occurred_at",
        )
        .bind(since.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(row_to_record).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::connection::create_sqlite_pool;

    fn at(iso: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(iso).unwrap().with_timezone(&Utc)
    }

    fn record(request_id: &str, occurred_at: &str, output: i64) -> UsageRecord {
        UsageRecord {
            request_id: request_id.to_string(),
            occurred_at: at(occurred_at),
            model: "claude-opus-5".to_string(),
            session_id: "s1".to_string(),
            project_path: "/home/mbt/appfactory/aggregated_plan".to_string(),
            is_sidechain: false,
            input_tokens: 2,
            output_tokens: output,
            cache_creation_input_tokens: 25_177,
            cache_read_input_tokens: 32_883,
            thinking_tokens: 0,
        }
    }

    async fn repo() -> SqliteClaudeUsageRepository {
        SqliteClaudeUsageRepository::new(create_sqlite_pool("sqlite::memory:").await.unwrap())
    }

    #[tokio::test]
    async fn writes_and_reads_a_record_whole() {
        let repo = repo().await;
        repo.upsert_requests(&[record("req_a", "2026-09-11T14:41:07.751Z", 228)])
            .await
            .unwrap();

        let back = repo.records_since(at("2026-09-11T00:00:00Z")).await.unwrap();

        assert_eq!(back.len(), 1);
        assert_eq!(back[0], record("req_a", "2026-09-11T14:41:07.751Z", 228));
    }

    #[tokio::test]
    async fn writing_the_same_request_twice_leaves_one_row() {
        // The property the whole index rests on. A chunk boundary between the two
        // lines of one request, or a full rescan, must not double a total.
        let repo = repo().await;
        let batch = [record("req_a", "2026-09-11T14:41:07.751Z", 228)];

        repo.upsert_requests(&batch).await.unwrap();
        repo.upsert_requests(&batch).await.unwrap();

        let back = repo.records_since(at("2026-09-11T00:00:00Z")).await.unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].output_tokens, 228);
    }

    #[tokio::test]
    async fn records_since_excludes_what_is_older_and_includes_the_boundary() {
        let repo = repo().await;
        repo.upsert_requests(&[
            record("old", "2026-09-10T23:59:59Z", 1),
            record("edge", "2026-09-11T00:00:00Z", 10),
            record("new", "2026-09-11T14:00:00Z", 100),
        ])
        .await
        .unwrap();

        let back = repo.records_since(at("2026-09-11T00:00:00Z")).await.unwrap();

        assert_eq!(back.len(), 2);
        assert_eq!(back[0].request_id, "edge");
        assert_eq!(back[1].request_id, "new");
    }

    #[tokio::test]
    async fn a_batch_larger_than_one_statement_is_written_whole() {
        // The parameter ceiling is real: eleven columns times more than ninety rows
        // exceeds SQLite's 999. A first full pass is ~29 000 rows.
        let repo = repo().await;
        let many: Vec<UsageRecord> = (0..250)
            .map(|i| record(&format!("req_{i}"), "2026-09-11T14:00:00Z", 10))
            .collect();

        let written = repo.upsert_requests(&many).await.unwrap();

        assert_eq!(written, 250);
        assert_eq!(
            repo.records_since(at("2026-09-11T00:00:00Z")).await.unwrap().len(),
            250
        );
    }

    #[tokio::test]
    async fn an_empty_batch_touches_nothing() {
        let repo = repo().await;
        assert_eq!(repo.upsert_requests(&[]).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn a_file_cursor_round_trips_and_then_overwrites_itself() {
        let repo = repo().await;
        assert_eq!(repo.file_state("/t/a.jsonl").await.unwrap(), None);

        let first = IndexedFile {
            path: "/t/a.jsonl".into(),
            size_bytes: 100,
            mtime: at("2026-09-11T10:00:00Z"),
            offset_bytes: 100,
            last_indexed_at: at("2026-09-11T10:00:01Z"),
        };
        repo.set_file_state(&first).await.unwrap();
        assert_eq!(repo.file_state("/t/a.jsonl").await.unwrap(), Some(first));

        let second = IndexedFile {
            path: "/t/a.jsonl".into(),
            size_bytes: 250,
            mtime: at("2026-09-11T11:00:00Z"),
            offset_bytes: 250,
            last_indexed_at: at("2026-09-11T11:00:01Z"),
        };
        repo.set_file_state(&second).await.unwrap();

        assert_eq!(repo.file_state("/t/a.jsonl").await.unwrap(), Some(second));
    }

    #[tokio::test]
    async fn the_sidechain_flag_survives_the_round_trip() {
        // Half the corpus is subagent turns; a flag silently lost on the way back
        // would take "how much do my subagents cost" with it.
        let repo = repo().await;
        let mut sub = record("req_sub", "2026-09-11T14:00:00Z", 400);
        sub.is_sidechain = true;
        repo.upsert_requests(&[sub]).await.unwrap();

        let back = repo.records_since(at("2026-09-11T00:00:00Z")).await.unwrap();

        assert!(back[0].is_sidechain);
    }
}
