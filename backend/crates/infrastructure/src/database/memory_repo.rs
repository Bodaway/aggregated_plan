use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::{MemoryListFilter, MemoryRepository};
use application::services::{MemoryRetriever, RecallQuery};
use domain::rules::recall::{rank, ScoredMemory};
use domain::types::*;

/// How many FTS candidates are pulled per requested result before the domain
/// scorer re-ranks them. BM25 alone is not the final order — entity match and
/// recency can promote a row — so the SQL layer must hand up more than `limit`.
const CANDIDATE_OVERFETCH: u32 = 5;
/// Absolute cap on the candidate window, whatever the requested limit.
const CANDIDATE_MAX: u32 = 500;
/// Hard stop when walking `superseded_by`, so malformed data cannot hang a request.
const SUPERSESSION_CHAIN_MAX: usize = 100;

pub struct SqliteMemoryRepository {
    pool: SqlitePool,
}

impl SqliteMemoryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// FTS5/BM25-backed retriever. Separate from the repository because it answers a
/// different question ("what is relevant?" rather than "what is stored?").
pub struct SqliteMemoryRetriever {
    pool: SqlitePool,
}

impl SqliteMemoryRetriever {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RepositoryError::Database(format!("bad datetime '{s}': {e}")))
}

fn parse_opt_dt(s: Option<String>) -> Result<Option<DateTime<Utc>>, RepositoryError> {
    match s {
        None => Ok(None),
        Some(raw) => parse_dt(&raw).map(Some),
    }
}

fn parse_opt_uuid(s: Option<String>) -> Result<Option<Uuid>, RepositoryError> {
    match s {
        None => Ok(None),
        Some(raw) => Uuid::parse_str(&raw)
            .map(Some)
            .map_err(|e| RepositoryError::Database(format!("bad uuid '{raw}': {e}"))),
    }
}

/// Map a `memories` row. Stakeholders are attached separately by `attach_stakeholders`.
fn map_row(row: &SqliteRow) -> Result<Memory, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let user_id_str: String = Row::get(row, "user_id");
    let kind_str: String = Row::get(row, "kind");
    let source_str: String = Row::get(row, "source");
    let status_str: String = Row::get(row, "status");
    Ok(Memory {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        kind: MemoryKind::from_str(&kind_str)
            .ok_or_else(|| RepositoryError::Database(format!("bad memory kind '{kind_str}'")))?,
        title: Row::get(row, "title"),
        body: Row::get(row, "body"),
        occurred_at: parse_dt(&Row::get::<String, _>(row, "occurred_at"))?,
        recorded_at: parse_dt(&Row::get::<String, _>(row, "recorded_at"))?,
        invalidated_at: parse_opt_dt(Row::get(row, "invalidated_at"))?,
        superseded_by: parse_opt_uuid(Row::get(row, "superseded_by"))?,
        source: MemorySource::from_str(&source_str)
            .ok_or_else(|| RepositoryError::Database(format!("bad memory source '{source_str}'")))?,
        source_ref: Row::get(row, "source_ref"),
        status: MemoryStatus::from_str(&status_str)
            .ok_or_else(|| RepositoryError::Database(format!("bad memory status '{status_str}'")))?,
        project_id: parse_opt_uuid(Row::get(row, "project_id"))?,
        task_id: parse_opt_uuid(Row::get(row, "task_id"))?,
        stakeholders: vec![],
    })
}

/// Fill in `stakeholders` for a batch of memories with one round trip.
/// The entity bonus reads this field, so it must be populated before scoring.
async fn attach_stakeholders(
    pool: &SqlitePool,
    memories: &mut [Memory],
) -> Result<(), RepositoryError> {
    if memories.is_empty() {
        return Ok(());
    }
    let placeholders = vec!["?"; memories.len()].join(", ");
    let sql = format!(
        "SELECT memory_id, person FROM memory_stakeholders WHERE memory_id IN ({placeholders}) ORDER BY person"
    );
    let mut q = sqlx::query(&sql);
    for m in memories.iter() {
        q = q.bind(m.id.to_string());
    }
    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

    let mut by_memory: HashMap<String, Vec<String>> = HashMap::new();
    for row in &rows {
        let memory_id: String = Row::get(row, "memory_id");
        let person: String = Row::get(row, "person");
        by_memory.entry(memory_id).or_default().push(person);
    }
    for m in memories.iter_mut() {
        if let Some(people) = by_memory.remove(&m.id.to_string()) {
            m.stakeholders = people;
        }
    }
    Ok(())
}

/// Insert the `memories` row. Transaction-scoped so callers can bundle it.
async fn insert_row(
    tx: &mut sqlx::SqliteConnection,
    memory: &Memory,
) -> Result<(), RepositoryError> {
    sqlx::query(
        "INSERT INTO memories
            (id, user_id, kind, title, body, occurred_at, recorded_at, invalidated_at,
             superseded_by, source, source_ref, status, project_id, task_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(memory.id.to_string())
    .bind(memory.user_id.to_string())
    .bind(memory.kind.as_str())
    .bind(&memory.title)
    .bind(&memory.body)
    .bind(memory.occurred_at.to_rfc3339())
    .bind(memory.recorded_at.to_rfc3339())
    .bind(memory.invalidated_at.map(|d| d.to_rfc3339()))
    .bind(memory.superseded_by.map(|id| id.to_string()))
    .bind(memory.source.as_str())
    .bind(&memory.source_ref)
    .bind(memory.status.as_str())
    .bind(memory.project_id.map(|id| id.to_string()))
    .bind(memory.task_id.map(|id| id.to_string()))
    .execute(&mut *tx)
    .await
    .map_err(|e| RepositoryError::Database(e.to_string()))?;
    Ok(())
}

/// Overwrite the mutable columns of an existing `memories` row.
async fn update_row(
    tx: &mut sqlx::SqliteConnection,
    memory: &Memory,
) -> Result<(), RepositoryError> {
    sqlx::query(
        "UPDATE memories SET
            kind = ?, title = ?, body = ?, occurred_at = ?, recorded_at = ?,
            invalidated_at = ?, superseded_by = ?, source = ?, source_ref = ?,
            status = ?, project_id = ?, task_id = ?
         WHERE id = ? AND user_id = ?",
    )
    .bind(memory.kind.as_str())
    .bind(&memory.title)
    .bind(&memory.body)
    .bind(memory.occurred_at.to_rfc3339())
    .bind(memory.recorded_at.to_rfc3339())
    .bind(memory.invalidated_at.map(|d| d.to_rfc3339()))
    .bind(memory.superseded_by.map(|id| id.to_string()))
    .bind(memory.source.as_str())
    .bind(&memory.source_ref)
    .bind(memory.status.as_str())
    .bind(memory.project_id.map(|id| id.to_string()))
    .bind(memory.task_id.map(|id| id.to_string()))
    .bind(memory.id.to_string())
    .bind(memory.user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| RepositoryError::Database(e.to_string()))?;
    Ok(())
}

/// Replace the stakeholder rows wholesale (the junction table has no other state).
async fn write_stakeholders(
    tx: &mut sqlx::SqliteConnection,
    memory: &Memory,
) -> Result<(), RepositoryError> {
    sqlx::query("DELETE FROM memory_stakeholders WHERE memory_id = ?")
        .bind(memory.id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
    for person in &memory.stakeholders {
        sqlx::query("INSERT INTO memory_stakeholders (memory_id, person) VALUES (?, ?)")
            .bind(memory.id.to_string())
            .bind(person)
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
    }
    Ok(())
}

/// Replace the FTS row. The table is standalone with no triggers, so every write
/// path must do this itself, inside the caller's transaction — a retitled memory
/// would otherwise stay searchable only under its old wording.
async fn write_fts(
    tx: &mut sqlx::SqliteConnection,
    memory: &Memory,
) -> Result<(), RepositoryError> {
    delete_fts(tx, memory.id).await?;
    sqlx::query("INSERT INTO memories_fts (memory_id, title, body) VALUES (?, ?, ?)")
        .bind(memory.id.to_string())
        .bind(&memory.title)
        .bind(&memory.body)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
    Ok(())
}

/// Drop the FTS row. No FK links it to `memories`, so deleting a memory does NOT
/// take its index entry with it — every delete path must call this.
async fn delete_fts(
    tx: &mut sqlx::SqliteConnection,
    id: MemoryId,
) -> Result<(), RepositoryError> {
    sqlx::query("DELETE FROM memories_fts WHERE memory_id = ?")
        .bind(id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
    Ok(())
}

/// Escape the LIKE metacharacters so a prefix is matched literally.
fn escape_like(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[async_trait]
impl MemoryRepository for SqliteMemoryRepository {
    /// One transaction for the three writes. `memories_fts` is a standalone FTS5
    /// table with no triggers: if its row is missing the memory is stored but can
    /// never be recalled, and `count(*)` would not reveal it.
    async fn create(&self, memory: &Memory) -> Result<(), RepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        insert_row(&mut tx, memory).await?;
        write_stakeholders(&mut tx, memory).await?;
        write_fts(&mut tx, memory).await?;

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(
        &self,
        id: MemoryId,
        user_id: UserId,
    ) -> Result<Option<Memory>, RepositoryError> {
        let rows = sqlx::query("SELECT * FROM memories WHERE id = ? AND user_id = ? LIMIT 1")
            .bind(id.to_string())
            .bind(user_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let mut found = vec![map_row(row)?];
        attach_stakeholders(&self.pool, &mut found).await?;
        Ok(found.into_iter().next())
    }

    async fn list(
        &self,
        user_id: UserId,
        filter: &MemoryListFilter,
    ) -> Result<Vec<Memory>, RepositoryError> {
        let mut sql = String::from("SELECT * FROM memories WHERE user_id = ?");
        if let Some(statuses) = &filter.status {
            if statuses.is_empty() {
                return Ok(vec![]);
            }
            let placeholders = vec!["?"; statuses.len()].join(", ");
            sql.push_str(&format!(" AND status IN ({placeholders})"));
        }
        if !filter.include_invalidated {
            sql.push_str(" AND invalidated_at IS NULL");
        }
        if filter.project_id.is_some() {
            sql.push_str(" AND project_id = ?");
        }
        sql.push_str(" ORDER BY occurred_at DESC LIMIT ? OFFSET ?");

        let mut q = sqlx::query(&sql).bind(user_id.to_string());
        if let Some(statuses) = &filter.status {
            for s in statuses {
                q = q.bind(s.as_str());
            }
        }
        if let Some(pid) = filter.project_id {
            q = q.bind(pid.to_string());
        }
        let rows = q
            .bind(filter.limit as i64)
            .bind(filter.offset as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        let mut memories: Vec<Memory> = rows
            .iter()
            .map(map_row)
            .collect::<Result<Vec<_>, _>>()?;
        attach_stakeholders(&self.pool, &mut memories).await?;
        Ok(memories)
    }

    async fn update(&self, memory: &Memory) -> Result<(), RepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        update_row(&mut tx, memory).await?;
        write_stakeholders(&mut tx, memory).await?;
        write_fts(&mut tx, memory).await?;

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn apply_merge(
        &self,
        survivor: &Memory,
        discarded: MemoryId,
        user_id: UserId,
    ) -> Result<(), RepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        update_row(&mut tx, survivor).await?;
        write_stakeholders(&mut tx, survivor).await?;
        write_fts(&mut tx, survivor).await?;

        // `memory_stakeholders` cascades with the row; the FTS entry does not.
        delete_fts(&mut tx, discarded).await?;
        sqlx::query("DELETE FROM memories WHERE id = ? AND user_id = ?")
            .bind(discarded.to_string())
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn apply_supersession(
        &self,
        invalidated: &Memory,
        successor: &Memory,
    ) -> Result<(), RepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        // The successor is written FIRST: `memories.superseded_by` is a foreign
        // key, so the row it points at must satisfy it within this transaction.
        update_row(&mut tx, successor).await?;
        write_fts(&mut tx, successor).await?;
        update_row(&mut tx, invalidated).await?;
        write_fts(&mut tx, invalidated).await?;

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn existing_source_refs(
        &self,
        user_id: UserId,
        prefix: &str,
    ) -> Result<Vec<String>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT source_ref FROM memories
             WHERE user_id = ? AND source_ref IS NOT NULL AND source_ref LIKE ? ESCAPE '\\'",
        )
        .bind(user_id.to_string())
        .bind(format!("{}%", escape_like(prefix)))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(rows
            .iter()
            .map(|row| Row::get::<String, _>(row, "source_ref"))
            .collect())
    }

    async fn supersession_chain(
        &self,
        user_id: UserId,
        from: MemoryId,
    ) -> Result<Vec<MemoryId>, RepositoryError> {
        let mut chain: Vec<MemoryId> = Vec::new();
        let mut cursor = from;
        // Bounded and visited-checked: a loop already in the data must not hang
        // the request, it must surface as a finite chain.
        while chain.len() < SUPERSESSION_CHAIN_MAX {
            let rows = sqlx::query(
                "SELECT superseded_by FROM memories WHERE id = ? AND user_id = ? LIMIT 1",
            )
            .bind(cursor.to_string())
            .bind(user_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

            let Some(row) = rows.first() else { break };
            let Some(next) = parse_opt_uuid(Row::get(row, "superseded_by"))? else {
                break;
            };
            if next == from || chain.contains(&next) {
                break;
            }
            chain.push(next);
            cursor = next;
        }
        Ok(chain)
    }
}

#[async_trait]
impl MemoryRetriever for SqliteMemoryRetriever {
    async fn search(
        &self,
        user_id: UserId,
        query: &RecallQuery,
        now: DateTime<Utc>,
    ) -> Result<Vec<ScoredMemory>, RepositoryError> {
        // `bm25()` is negative and more negative = better, so ASC is best-first.
        // This is only the candidate window; the final order comes from the domain.
        let mut sql = String::from(
            "SELECT m.*, bm25(memories_fts) AS bm25_score
             FROM memories_fts
             JOIN memories m ON m.id = memories_fts.memory_id
             WHERE memories_fts MATCH ? AND m.user_id = ?",
        );
        if !query.include_history {
            // Hard filter: recalling a superseded decision is worse than recalling nothing.
            sql.push_str(" AND m.invalidated_at IS NULL AND m.status = 'active'");
        }
        sql.push_str(" ORDER BY bm25_score ASC LIMIT ?");

        let candidate_limit = query
            .limit
            .saturating_mul(CANDIDATE_OVERFETCH)
            .min(CANDIDATE_MAX);

        let rows = sqlx::query(&sql)
            .bind(&query.match_query)
            .bind(user_id.to_string())
            .bind(candidate_limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        let mut memories: Vec<Memory> = rows
            .iter()
            .map(map_row)
            .collect::<Result<Vec<_>, _>>()?;
        attach_stakeholders(&self.pool, &mut memories).await?;

        let candidates: Vec<(Memory, f64)> = memories
            .into_iter()
            .zip(rows.iter().map(|r| Row::get::<f64, _>(r, "bm25_score")))
            .collect();

        let mut ranked = rank(candidates, &query.context, now, &query.weights);
        ranked.truncate(query.limit as usize);
        Ok(ranked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::repositories::MEMORY_LIST_DEFAULT_LIMIT;
    use application::services::RECALL_DEFAULT_LIMIT;
    use domain::rules::recall::{build_match_query, RecallContext, RecallWeights};
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    /// In-memory pool with `foreign_keys(true)`, like the production pool. A bare
    /// `SqlitePool::connect("sqlite::memory:")` leaves FK enforcement OFF, which
    /// keeps TDD green on a violation that only surfaces at runtime.
    /// `max_connections(1)` keeps every statement on the same in-memory database.
    async fn test_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("valid in-memory url")
            .foreign_keys(true);
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("in-memory pool")
    }

    async fn migrated_pool() -> SqlitePool {
        let pool = test_pool().await;
        sqlx::migrate!("../../../migrations/sqlite")
            .run(&pool)
            .await
            .expect("migrations run");
        pool
    }

    async fn pool_with_user() -> (SqlitePool, Uuid) {
        let pool = migrated_pool().await;
        let uid = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, name, email, created_at) VALUES (?, 'T', ?, ?)")
            .bind(uid.to_string())
            .bind(format!("{uid}@example.test"))
            .bind(Utc::now().to_rfc3339())
            .execute(&pool)
            .await
            .expect("seed user");
        (pool, uid)
    }

    async fn seed_project(pool: &SqlitePool, uid: Uuid, name: &str) -> Uuid {
        let pid = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO projects (id, user_id, name, source, status) VALUES (?, ?, ?, 'personal', 'active')",
        )
        .bind(pid.to_string())
        .bind(uid.to_string())
        .bind(name)
        .execute(pool)
        .await
        .expect("seed project");
        pid
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-03T09:00:00+00:00")
            .expect("valid fixture")
            .with_timezone(&Utc)
    }

    fn memory(uid: Uuid, kind: MemoryKind, title: &str, body: Option<&str>) -> Memory {
        Memory::new(
            uid,
            NewMemory {
                kind,
                title: title.into(),
                body: body.map(String::from),
                occurred_at: None,
                source: MemorySource::ClaudeSession,
                source_ref: None,
                status: MemoryStatus::Active,
                project_id: None,
                task_id: None,
                stakeholders: vec![],
            },
            now(),
        )
        .expect("valid fixture")
    }

    fn recall(raw: &str) -> RecallQuery {
        RecallQuery {
            match_query: build_match_query(raw).expect("buildable query"),
            context: RecallContext::default(),
            include_history: false,
            weights: RecallWeights::default(),
            limit: RECALL_DEFAULT_LIMIT,
        }
    }

    fn list_all() -> MemoryListFilter {
        MemoryListFilter {
            status: None,
            include_invalidated: true,
            project_id: None,
            limit: MEMORY_LIST_DEFAULT_LIMIT,
            offset: 0,
        }
    }

    // ─── Schema ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn migration_creates_the_memory_tables_and_the_worklog_watermark() {
        let pool = migrated_pool().await;
        for table in ["memories", "memory_stakeholders", "memories_fts"] {
            let row: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("query sqlite_master");
            assert_eq!(row.0, 1, "table {table} should exist after migration");
        }
        // The consolidation watermark is per-entry, not a global cursor.
        sqlx::query("SELECT consolidated_at FROM worklog_entries LIMIT 1")
            .fetch_all(&pool)
            .await
            .expect("worklog_entries.consolidated_at should exist");
    }

    // ─── Repository ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn create_then_find_by_id_roundtrips_every_column() {
        let (pool, uid) = pool_with_user().await;
        let pid = seed_project(&pool, uid, "Pernod").await;
        let repo = SqliteMemoryRepository::new(pool);

        let mut m = memory(
            uid,
            MemoryKind::Decision,
            "Wave 0 limitée au périmètre AI Microsoft",
            Some("Pierre veut un livrable avant septembre"),
        );
        m.project_id = Some(pid);
        m.stakeholders = vec!["Pierre".into(), "Sophie".into()];
        m.source_ref = Some("session-42".into());
        repo.create(&m).await.expect("create");

        let got = repo
            .find_by_id(m.id, uid)
            .await
            .expect("find")
            .expect("row exists");
        assert_eq!(got.title, m.title);
        assert_eq!(got.body, m.body);
        assert_eq!(got.kind, MemoryKind::Decision);
        assert_eq!(got.source, MemorySource::ClaudeSession);
        assert_eq!(got.status, MemoryStatus::Active);
        assert_eq!(got.project_id, Some(pid));
        assert_eq!(got.source_ref.as_deref(), Some("session-42"));
        assert_eq!(got.occurred_at, m.occurred_at);
        assert_eq!(got.recorded_at, m.recorded_at);
        assert_eq!(got.invalidated_at, None);
        assert_eq!(got.superseded_by, None);
        assert_eq!(got.stakeholders, vec!["Pierre".to_string(), "Sophie".to_string()]);
    }

    #[tokio::test]
    async fn find_by_id_is_scoped_to_its_user() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool);
        let m = memory(uid, MemoryKind::Fact, "the mcp crate does not compile", None);
        repo.create(&m).await.expect("create");
        assert!(repo
            .find_by_id(m.id, Uuid::new_v4())
            .await
            .expect("find")
            .is_none());
    }

    #[tokio::test]
    async fn create_rejects_an_unknown_user_because_foreign_keys_are_on() {
        // Guards the test pool itself: with FKs off this insert would succeed.
        let pool = migrated_pool().await;
        let repo = SqliteMemoryRepository::new(pool);
        let orphan = memory(Uuid::new_v4(), MemoryKind::Fact, "orphan", None);
        let err = repo
            .create(&orphan)
            .await
            .expect_err("FK on users(id) must be enforced");
        assert!(matches!(err, RepositoryError::Database(_)));
    }

    #[tokio::test]
    async fn deleting_the_task_keeps_the_memory_and_nulls_the_link() {
        let (pool, uid) = pool_with_user().await;
        let task_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tasks (id, user_id, title, source, status) VALUES (?, ?, 'a task', 'personal', 'todo')",
        )
        .bind(task_id.to_string())
        .bind(uid.to_string())
        .execute(&pool)
        .await
        .expect("seed task");

        let repo = SqliteMemoryRepository::new(pool.clone());
        let mut m = memory(uid, MemoryKind::Commitment, "answer Pierre on the architecture", None);
        m.task_id = Some(task_id);
        repo.create(&m).await.expect("create");

        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(task_id.to_string())
            .execute(&pool)
            .await
            .expect("delete task");

        let got = repo
            .find_by_id(m.id, uid)
            .await
            .expect("find")
            .expect("the memory must survive its task");
        assert_eq!(got.task_id, None, "ON DELETE SET NULL, never CASCADE");
    }

    #[tokio::test]
    async fn deleting_a_memory_cascades_its_stakeholders() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let mut m = memory(uid, MemoryKind::Commitment, "answer Pierre", None);
        m.stakeholders = vec!["Pierre".into()];
        repo.create(&m).await.expect("create");

        sqlx::query("DELETE FROM memories WHERE id = ?")
            .bind(m.id.to_string())
            .execute(&pool)
            .await
            .expect("delete memory");

        let left: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memory_stakeholders WHERE memory_id = ?")
            .bind(m.id.to_string())
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(left.0, 0);
    }

    #[tokio::test]
    async fn list_filters_by_status_and_orders_newest_first() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool);

        let mut old = memory(uid, MemoryKind::Decision, "older decision", None);
        old.occurred_at = now() - chrono::Duration::days(30);
        old.status = MemoryStatus::Pending;
        let mut recent = memory(uid, MemoryKind::Decision, "recent decision", None);
        recent.occurred_at = now() - chrono::Duration::days(1);
        recent.status = MemoryStatus::Pending;
        let mut active = memory(uid, MemoryKind::Fact, "an active fact", None);
        active.occurred_at = now();
        repo.create(&old).await.expect("create");
        repo.create(&recent).await.expect("create");
        repo.create(&active).await.expect("create");

        let pending = repo
            .list(
                uid,
                &MemoryListFilter {
                    status: Some(vec![MemoryStatus::Pending]),
                    ..list_all()
                },
            )
            .await
            .expect("list");
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].title, "recent decision", "occurred_at DESC");
        assert_eq!(pending[1].title, "older decision");
    }

    #[tokio::test]
    async fn list_hides_invalidated_rows_unless_asked() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let m = memory(uid, MemoryKind::Decision, "a superseded decision", None);
        repo.create(&m).await.expect("create");
        sqlx::query("UPDATE memories SET invalidated_at = ? WHERE id = ?")
            .bind(now().to_rfc3339())
            .bind(m.id.to_string())
            .execute(&pool)
            .await
            .expect("invalidate");

        let hidden = repo
            .list(
                uid,
                &MemoryListFilter {
                    include_invalidated: false,
                    ..list_all()
                },
            )
            .await
            .expect("list");
        assert!(hidden.is_empty());

        let shown = repo.list(uid, &list_all()).await.expect("list");
        assert_eq!(shown.len(), 1);
        assert!(shown[0].invalidated_at.is_some());
    }

    #[tokio::test]
    async fn list_filters_by_project_and_paginates() {
        let (pool, uid) = pool_with_user().await;
        let pid = seed_project(&pool, uid, "Cartier").await;
        let repo = SqliteMemoryRepository::new(pool);
        for i in 0..3 {
            let mut m = memory(uid, MemoryKind::Fact, &format!("fact {i}"), None);
            m.project_id = Some(pid);
            m.occurred_at = now() - chrono::Duration::days(i);
            repo.create(&m).await.expect("create");
        }
        let mut elsewhere = memory(uid, MemoryKind::Fact, "unrelated", None);
        elsewhere.occurred_at = now();
        repo.create(&elsewhere).await.expect("create");

        let scoped = repo
            .list(
                uid,
                &MemoryListFilter {
                    project_id: Some(pid),
                    ..list_all()
                },
            )
            .await
            .expect("list");
        assert_eq!(scoped.len(), 3);

        let page = repo
            .list(
                uid,
                &MemoryListFilter {
                    project_id: Some(pid),
                    limit: 1,
                    offset: 1,
                    ..list_all()
                },
            )
            .await
            .expect("list");
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].title, "fact 1");
    }

    /// `limit: 0` means "use the default", and the trait says the repository is
    /// what applies it. Bound straight through it emits `LIMIT 0`, which returns
    /// an empty set with no error — an inbox that looks legitimately empty.
    #[tokio::test]
    async fn a_default_constructed_filter_still_returns_rows() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool);
        repo.create(&memory(uid, MemoryKind::Fact, "a stored fact", None))
            .await
            .expect("create");

        let rows = repo
            .list(uid, &MemoryListFilter::default())
            .await
            .expect("list");
        assert_eq!(rows.len(), 1, "`limit: 0` must mean the default, not LIMIT 0");
    }

    // ─── Retriever ───────────────────────────────────────────────────────────

    /// The FTS row is written in the same transaction, so an inserted memory is
    /// immediately findable. Asserted through `MATCH` — never `count(*)`, which
    /// reports 1 even when the index is unusable.
    #[tokio::test]
    async fn a_created_memory_is_immediately_findable_by_match() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        repo.create(&memory(
            uid,
            MemoryKind::Decision,
            "Wave 0 limitée au périmètre AI Microsoft",
            Some("Pierre veut un livrable avant septembre"),
        ))
        .await
        .expect("create");

        let hits = retriever
            .search(uid, &recall("wave 0"), now())
            .await
            .expect("search");
        assert_eq!(hits.len(), 1, "the FTS row must be written with the memory");
        assert!(hits[0].memory.title.starts_with("Wave 0"));
    }

    #[tokio::test]
    async fn the_body_is_searchable_too() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        repo.create(&memory(
            uid,
            MemoryKind::Decision,
            "Wave 0 scope",
            Some("Pierre veut un livrable avant septembre"),
        ))
        .await
        .expect("create");
        let hits = retriever
            .search(uid, &recall("livrable"), now())
            .await
            .expect("search");
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn a_jira_key_query_does_not_error_and_finds_its_memory() {
        // Raw `MATCH 'AP-1234'` raises `no such column: 1234`.
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        repo.create(&memory(
            uid,
            MemoryKind::Commitment,
            "AP-1234 doit passer en revue avant vendredi",
            None,
        ))
        .await
        .expect("create");
        let hits = retriever
            .search(uid, &recall("AP-1234"), now())
            .await
            .expect("search must not error on a Jira key");
        assert_eq!(hits.len(), 1);
    }

    /// The Jira key is ONE phrase, so `AP` must be immediately followed by `1234`.
    /// An unpositioned AND (`"AP" "1234"`) would match this decoy.
    #[tokio::test]
    async fn a_jira_key_query_requires_adjacency() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        repo.create(&memory(
            uid,
            MemoryKind::Fact,
            "AP porte le budget alors que le ticket voisin AP-9999 chiffre 1234 euros",
            None,
        ))
        .await
        .expect("create");
        let hits = retriever
            .search(uid, &recall("AP-1234"), now())
            .await
            .expect("search");
        assert!(
            hits.is_empty(),
            "`AP` and `1234` far apart must not match the phrase `AP-1234`"
        );
    }

    #[tokio::test]
    async fn an_embedded_quote_in_the_query_does_not_error() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        repo.create(&memory(uid, MemoryKind::Fact, "wave scope", None))
            .await
            .expect("create");
        // `"` is doubled by the query builder; SQLite must accept the escape.
        let hits = retriever
            .search(uid, &recall("say \"wave\" now"), now())
            .await
            .expect("an escaped quote must not break the parser");
        assert!(hits.is_empty(), "the decoy words are not in the memory");
    }

    #[tokio::test]
    async fn a_client_colon_subject_query_does_not_error() {
        // Raw `MATCH 'Cartier: certificat'` raises `no such column: Cartier`.
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        repo.create(&memory(
            uid,
            MemoryKind::Fact,
            "Cartier : certificat à renouveler",
            None,
        ))
        .await
        .expect("create");
        let hits = retriever
            .search(uid, &recall("Cartier : certificat"), now())
            .await
            .expect("search must not error on a `Client : subject` label");
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn a_diacritic_free_query_matches_an_accented_memory() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        repo.create(&memory(uid, MemoryKind::Decision, "Wave 0 limitée", None))
            .await
            .expect("create");
        let hits = retriever
            .search(uid, &recall("limitee"), now())
            .await
            .expect("search");
        assert_eq!(hits.len(), 1, "`remove_diacritics 2` must fold the accent");
    }

    /// Prefix expansion covers the singular query against a plural document.
    #[tokio::test]
    async fn a_singular_query_reaches_a_plural_memory() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        repo.create(&memory(
            uid,
            MemoryKind::Commitment,
            "engagements pris envers Pierre",
            None,
        ))
        .await
        .expect("create");
        let hits = retriever
            .search(uid, &recall("engagement"), now())
            .await
            .expect("search");
        assert_eq!(hits.len(), 1);
    }

    /// The other direction, which `*` alone cannot reach: the de-pluralized OR
    /// branch is what makes the plural query find the singular document.
    #[tokio::test]
    async fn a_plural_query_reaches_a_singular_memory() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        repo.create(&memory(
            uid,
            MemoryKind::Commitment,
            "engagement pris envers Pierre",
            None,
        ))
        .await
        .expect("create");
        let hits = retriever
            .search(uid, &recall("engagements"), now())
            .await
            .expect("search");
        assert_eq!(
            hits.len(),
            1,
            "`engagements*` alone returns 0 rows here — the OR branch carries it"
        );
    }

    /// The OR branch must not widen an AND across groups.
    #[tokio::test]
    async fn a_depluralized_branch_still_requires_the_other_groups() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        repo.create(&memory(uid, MemoryKind::Commitment, "engagement pris", None))
            .await
            .expect("create");
        let hits = retriever
            .search(uid, &recall("cartier engagements"), now())
            .await
            .expect("search");
        assert!(
            hits.is_empty(),
            "`cartier` is absent, so the parenthesized OR must not rescue the match"
        );
    }

    #[tokio::test]
    async fn the_hard_filter_hides_pending_and_invalidated_memories() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool.clone());

        let mut pending = memory(uid, MemoryKind::Decision, "wave 0 pending candidate", None);
        pending.status = MemoryStatus::Pending;
        repo.create(&pending).await.expect("create");

        let superseded = memory(uid, MemoryKind::Decision, "wave 0 superseded decision", None);
        repo.create(&superseded).await.expect("create");
        sqlx::query("UPDATE memories SET invalidated_at = ? WHERE id = ?")
            .bind(now().to_rfc3339())
            .bind(superseded.id.to_string())
            .execute(&pool)
            .await
            .expect("invalidate");

        let active = memory(uid, MemoryKind::Decision, "wave 0 active decision", None);
        repo.create(&active).await.expect("create");

        let hits = retriever
            .search(uid, &recall("wave"), now())
            .await
            .expect("search");
        assert_eq!(hits.len(), 1, "only the active, still-true row may be recalled");
        assert_eq!(hits[0].memory.id, active.id);
    }

    #[tokio::test]
    async fn include_history_lifts_the_hard_filter() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool.clone());
        let superseded = memory(uid, MemoryKind::Decision, "wave 0 superseded decision", None);
        repo.create(&superseded).await.expect("create");
        sqlx::query("UPDATE memories SET invalidated_at = ? WHERE id = ?")
            .bind(now().to_rfc3339())
            .bind(superseded.id.to_string())
            .execute(&pool)
            .await
            .expect("invalidate");

        let mut query = recall("wave");
        query.include_history = true;
        let hits = retriever.search(uid, &query, now()).await.expect("search");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].memory.invalidated_at.is_some());
    }

    #[tokio::test]
    async fn search_is_scoped_to_its_user() {
        let (pool, uid) = pool_with_user().await;
        let other = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, name, email, created_at) VALUES (?, 'O', ?, ?)")
            .bind(other.to_string())
            .bind(format!("{other}@example.test"))
            .bind(Utc::now().to_rfc3339())
            .execute(&pool)
            .await
            .expect("seed other user");

        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        repo.create(&memory(other, MemoryKind::Decision, "wave 0 decision", None))
            .await
            .expect("create");
        assert!(retriever
            .search(uid, &recall("wave"), now())
            .await
            .expect("search")
            .is_empty());
    }

    #[tokio::test]
    async fn search_returns_a_positive_score_and_honours_the_limit() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        for i in 0..3 {
            repo.create(&memory(
                uid,
                MemoryKind::Decision,
                &format!("wave scope decision {i}"),
                None,
            ))
            .await
            .expect("create");
        }
        let mut query = recall("wave");
        query.limit = 2;
        let hits = retriever.search(uid, &query, now()).await.expect("search");
        assert_eq!(hits.len(), 2);
        assert!(hits[0].score > 0.0);
        assert!(hits[0].score >= hits[1].score, "best first");
    }

    /// Pins `ORDER BY bm25_score ASC`. `bm25()` is negative and MORE negative
    /// means a better match, so ASC is best-first and DESC would hand the domain
    /// the WORST candidates. A fixture smaller than the candidate window cannot
    /// catch the flip — every row fits either way — so this one deliberately
    /// overflows it: 6 matching rows against a window of `1 * CANDIDATE_OVERFETCH`,
    /// where the densest match is exactly the row DESC drops.
    #[tokio::test]
    async fn the_candidate_window_is_ordered_best_bm25_first() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);

        let densest = memory(uid, MemoryKind::Decision, "wave wave wave wave", None);
        repo.create(&densest).await.expect("create");
        for i in 0..5 {
            repo.create(&memory(
                uid,
                MemoryKind::Decision,
                &format!("wave decision {i}"),
                Some("filler filler filler filler filler filler filler filler filler filler"),
            ))
            .await
            .expect("create");
        }

        let mut query = recall("wave");
        query.limit = 1;
        let hits = retriever.search(uid, &query, now()).await.expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].memory.id, densest.id,
            "the window must keep the most negative bm25 rows, so ORDER BY must be ASC"
        );
    }

    #[tokio::test]
    async fn the_entity_bonus_promotes_the_memory_of_the_current_project() {
        let (pool, uid) = pool_with_user().await;
        let pid = seed_project(&pool, uid, "Pernod").await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);

        let unattached = memory(uid, MemoryKind::Decision, "wave scope decision A", None);
        repo.create(&unattached).await.expect("create");
        let mut attached = memory(uid, MemoryKind::Decision, "wave scope decision B", None);
        attached.project_id = Some(pid);
        repo.create(&attached).await.expect("create");

        let mut query = recall("wave");
        query.context = RecallContext {
            project_id: Some(pid),
            ..RecallContext::default()
        };
        let hits = retriever.search(uid, &query, now()).await.expect("search");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].memory.id, attached.id);
    }

    #[tokio::test]
    async fn stakeholders_are_loaded_before_scoring() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        let mut m = memory(uid, MemoryKind::Commitment, "answer on the wave scope", None);
        m.stakeholders = vec!["Pierre".into()];
        repo.create(&m).await.expect("create");

        let mut query = recall("wave");
        query.context = RecallContext {
            stakeholders: vec!["Pierre".into()],
            ..RecallContext::default()
        };
        let hits = retriever.search(uid, &query, now()).await.expect("search");
        assert_eq!(hits[0].memory.stakeholders, vec!["Pierre".to_string()]);

        let baseline = retriever
            .search(uid, &recall("wave"), now())
            .await
            .expect("search");
        assert!(
            hits[0].score > baseline[0].score,
            "a stakeholder match must raise the score"
        );
    }

    #[tokio::test]
    async fn a_query_matching_nothing_returns_no_rows_and_no_error() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        repo.create(&memory(uid, MemoryKind::Fact, "something else", None))
            .await
            .expect("create");
        assert!(retriever
            .search(uid, &recall("wave"), now())
            .await
            .expect("search")
            .is_empty());
    }

    // ─── Write paths for the queue and invalidation (lot 3) ─────────────────

    /// A retitled memory must stop matching its old wording and start matching
    /// the new one — the FTS row is rewritten inside `update`'s transaction.
    #[tokio::test]
    async fn update_rewrites_the_fts_row() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);
        let mut m = memory(uid, MemoryKind::Decision, "wave scope decision", None);
        repo.create(&m).await.expect("create");

        m.title = "cartier certificat renouvele".into();
        m.body = Some("nouveau contexte".into());
        repo.update(&m).await.expect("update");

        assert!(
            retriever
                .search(uid, &recall("wave"), now())
                .await
                .expect("search")
                .is_empty(),
            "the old wording must stop matching"
        );
        assert_eq!(
            retriever
                .search(uid, &recall("cartier"), now())
                .await
                .expect("search")
                .len(),
            1,
            "the new wording must match"
        );
    }

    #[tokio::test]
    async fn update_replaces_stakeholders_rather_than_appending() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool);
        let mut m = memory(uid, MemoryKind::Commitment, "answer Pierre", None);
        m.stakeholders = vec!["Pierre".into()];
        repo.create(&m).await.expect("create");

        m.stakeholders = vec!["Sophie".into()];
        repo.update(&m).await.expect("update");

        let got = repo.find_by_id(m.id, uid).await.expect("find").expect("row");
        assert_eq!(got.stakeholders, vec!["Sophie".to_string()]);
    }

    #[tokio::test]
    async fn apply_merge_deletes_the_discarded_row_and_its_index_entry() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);

        let target = memory(uid, MemoryKind::Decision, "wave scope short", None);
        repo.create(&target).await.expect("create");
        let mut candidate = memory(uid, MemoryKind::Decision, "wave scope reformulated fully", None);
        candidate.status = MemoryStatus::Pending;
        repo.create(&candidate).await.expect("create");

        let mut survivor = target.clone();
        survivor.title = candidate.title.clone();
        repo.apply_merge(&survivor, candidate.id, uid)
            .await
            .expect("merge");

        assert!(repo
            .find_by_id(candidate.id, uid)
            .await
            .expect("find")
            .is_none());
        let hits = retriever
            .search(uid, &recall("reformulated"), now())
            .await
            .expect("search");
        assert_eq!(
            hits.len(),
            1,
            "exactly one row answers the new wording — the discarded index entry is gone"
        );
        assert_eq!(hits[0].memory.id, target.id);
    }

    /// The keystone: `invalidated_at` is now written, so the hard filter of §7.1
    /// finally guards something. Proven through the real `MATCH`, not a stub.
    #[tokio::test]
    async fn a_superseded_memory_leaves_recall_and_returns_under_history() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let retriever = SqliteMemoryRetriever::new(pool);

        let old = memory(uid, MemoryKind::Decision, "wave scope is Microsoft only", None);
        repo.create(&old).await.expect("create");
        let mut new = memory(uid, MemoryKind::Decision, "wave scope is the whole platform", None);
        new.status = MemoryStatus::Pending;
        repo.create(&new).await.expect("create");

        let mut invalidated = old.clone();
        invalidated.invalidated_at = Some(now());
        invalidated.superseded_by = Some(new.id);
        let mut successor = new.clone();
        successor.status = MemoryStatus::Active;
        repo.apply_supersession(&invalidated, &successor)
            .await
            .expect("supersede");

        let hits = retriever
            .search(uid, &recall("wave scope"), now())
            .await
            .expect("search");
        assert_eq!(hits.len(), 1, "the superseded fact must vanish from recall");
        assert_eq!(hits[0].memory.id, new.id);

        let mut history = recall("wave scope");
        history.include_history = true;
        let all = retriever.search(uid, &history, now()).await.expect("search");
        assert_eq!(all.len(), 2, "--history brings the old truth back");
        let revived = all
            .iter()
            .find(|h| h.memory.id == old.id)
            .expect("the old row survives");
        assert_eq!(revived.memory.superseded_by, Some(new.id));
        assert!(revived.memory.invalidated_at.is_some());
    }

    /// `superseded_by` is a real foreign key, so the successor row must satisfy it
    /// within the same transaction. Writing the old row first would fail.
    #[tokio::test]
    async fn apply_supersession_satisfies_the_superseded_by_foreign_key() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool.clone());
        let old = memory(uid, MemoryKind::Decision, "scope is X", None);
        repo.create(&old).await.expect("create");
        let new = memory(uid, MemoryKind::Decision, "scope is Y", None);
        repo.create(&new).await.expect("create");

        let mut invalidated = old.clone();
        invalidated.invalidated_at = Some(now());
        invalidated.superseded_by = Some(new.id);
        repo.apply_supersession(&invalidated, &new)
            .await
            .expect("the FK must be satisfiable");

        let stored = repo.find_by_id(old.id, uid).await.expect("find").expect("row");
        assert_eq!(stored.superseded_by, Some(new.id));
    }

    #[tokio::test]
    async fn a_dangling_superseded_by_is_refused_by_the_foreign_key() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool);
        let old = memory(uid, MemoryKind::Decision, "scope is X", None);
        repo.create(&old).await.expect("create");

        let mut invalidated = old.clone();
        invalidated.superseded_by = Some(Uuid::new_v4());
        assert!(
            repo.update(&invalidated).await.is_err(),
            "superseded_by must point at a real memory"
        );
    }

    #[tokio::test]
    async fn supersession_chain_is_walked_in_order() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool);
        let a = memory(uid, MemoryKind::Decision, "scope is X", None);
        let b = memory(uid, MemoryKind::Decision, "scope is Y", None);
        let c = memory(uid, MemoryKind::Decision, "scope is Z", None);
        for m in [&a, &b, &c] {
            repo.create(m).await.expect("create");
        }

        let mut a_dead = a.clone();
        a_dead.invalidated_at = Some(now());
        a_dead.superseded_by = Some(b.id);
        repo.apply_supersession(&a_dead, &b).await.expect("A by B");
        let mut b_dead = b.clone();
        b_dead.invalidated_at = Some(now());
        b_dead.superseded_by = Some(c.id);
        repo.apply_supersession(&b_dead, &c).await.expect("B by C");

        assert_eq!(
            repo.supersession_chain(uid, a.id).await.expect("chain"),
            vec![b.id, c.id],
            "nearest successor first, `from` excluded"
        );
        assert!(repo
            .supersession_chain(uid, c.id)
            .await
            .expect("chain")
            .is_empty(), "the head has no successor");
    }

    #[tokio::test]
    async fn existing_source_refs_filters_by_prefix() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool);
        for (title, source_ref) in [
            ("imported one", Some("memory-file:note-a")),
            ("imported two", Some("memory-file:note-b")),
            ("from a session", Some("session:42")),
            ("no provenance", None),
        ] {
            let mut m = memory(uid, MemoryKind::Fact, title, None);
            m.source_ref = source_ref.map(String::from);
            repo.create(&m).await.expect("create");
        }

        let mut refs = repo
            .existing_source_refs(uid, "memory-file:")
            .await
            .expect("refs");
        refs.sort();
        assert_eq!(refs, vec!["memory-file:note-a", "memory-file:note-b"]);
        assert!(repo
            .existing_source_refs(Uuid::new_v4(), "memory-file:")
            .await
            .expect("refs")
            .is_empty(), "scoped to its user");
    }

    /// `_` and `%` are LIKE wildcards; a prefix carrying them must still match
    /// literally.
    #[tokio::test]
    async fn existing_source_refs_escapes_like_wildcards() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteMemoryRepository::new(pool);
        let mut wanted = memory(uid, MemoryKind::Fact, "wanted", None);
        wanted.source_ref = Some("a_b:one".into());
        repo.create(&wanted).await.expect("create");
        let mut decoy = memory(uid, MemoryKind::Fact, "decoy", None);
        decoy.source_ref = Some("axb:two".into());
        repo.create(&decoy).await.expect("create");

        let refs = repo.existing_source_refs(uid, "a_b:").await.expect("refs");
        assert_eq!(refs, vec!["a_b:one"], "`_` must not act as a wildcard");
    }

    /// `NOT` raw raises `fts5: syntax error near "NOT"`. Quoted by the domain
    /// builder it must reach SQLite harmlessly.
    #[tokio::test]
    async fn hostile_input_reaches_sqlite_without_erroring() {
        let (pool, uid) = pool_with_user().await;
        let retriever = SqliteMemoryRetriever::new(pool);
        for raw in [
            "NOT",
            "OR",
            "AND",
            "AP-1234",
            "Cartier : certificat",
            "Cartier: certificat",
            "NEAR(a b)",
            "^x",
            "100%",
            "a\"b",
            "\" OR x:y \"",
            "engagements travaux",
            "col:value",
        ] {
            retriever
                .search(uid, &recall(raw), now())
                .await
                .unwrap_or_else(|e| panic!("query {raw:?} must be safe, got {e}"));
        }
    }
}

/// Environment guard. FTS5 is compiled into the SQLite that `sqlx` embeds, and
/// `bm25()` is negative — both are load-bearing assumptions of this module.
/// A `sqlx` upgrade that drops FTS5 or changes the sign must fail here, loudly,
/// instead of silently emptying every recall.
#[cfg(test)]
mod fts5_environment_guard {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::SqlitePool;
    use std::str::FromStr;

    async fn bare_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("valid in-memory url")
            .foreign_keys(true);
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("in-memory pool")
    }

    #[tokio::test]
    async fn fts5_is_creatable_with_the_chosen_tokenizer() {
        let pool = bare_pool().await;
        sqlx::query(
            "CREATE VIRTUAL TABLE guard USING fts5(title, body, tokenize = 'unicode61 remove_diacritics 2')",
        )
        .execute(&pool)
        .await
        .expect("FTS5 must be available in the embedded SQLite");

        sqlx::query("INSERT INTO guard (title, body) VALUES ('Wave 0 limitée', 'engagement pris')")
            .execute(&pool)
            .await
            .expect("insert");

        // Accents are folded by the tokenizer.
        let folded: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM guard WHERE guard MATCH ?")
            .bind("\"limitee\"")
            .fetch_one(&pool)
            .await
            .expect("match");
        assert_eq!(folded.0, 1, "`remove_diacritics 2` must fold accents");

        // No lemmatization: the exact plural misses, the prefix form hits.
        let exact: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM guard WHERE guard MATCH ?")
            .bind("\"engagements\"")
            .fetch_one(&pool)
            .await
            .expect("match");
        assert_eq!(exact.0, 0, "the tokenizer does not lemmatize");
        let prefixed: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM guard WHERE guard MATCH ?")
            .bind("\"engagement\"*")
            .fetch_one(&pool)
            .await
            .expect("match");
        assert_eq!(prefixed.0, 1, "prefix expansion must be supported");
    }

    #[tokio::test]
    async fn bm25_is_negative_and_more_negative_means_a_better_match() {
        let pool = bare_pool().await;
        sqlx::query("CREATE VIRTUAL TABLE guard USING fts5(body)")
            .execute(&pool)
            .await
            .expect("create fts5");
        for body in ["wave wave wave scope", "wave scope filler filler filler filler filler"] {
            sqlx::query("INSERT INTO guard (body) VALUES (?)")
                .bind(body)
                .execute(&pool)
                .await
                .expect("insert");
        }

        let scores: Vec<(f64,)> = sqlx::query_as(
            "SELECT bm25(guard) FROM guard WHERE guard MATCH ? ORDER BY bm25(guard) ASC",
        )
        .bind("\"wave\"")
        .fetch_all(&pool)
        .await
        .expect("bm25");

        assert_eq!(scores.len(), 2);
        for (score,) in &scores {
            assert!(*score < 0.0, "bm25() must be negative, got {score}");
        }
        assert!(
            scores[0].0 < scores[1].0,
            "ASC order must put the denser match first: {scores:?}"
        );
    }

    /// Why `memories_fts` is standalone rather than `content='memories'`: without
    /// triggers, an external-content table answers MATCH with 0 rows while
    /// `count(*)` still reports 1 — so a `count(*)`-based test hides the outage.
    #[tokio::test]
    async fn an_external_content_fts_table_without_triggers_is_silently_empty() {
        let pool = bare_pool().await;
        sqlx::query("CREATE TABLE src (id INTEGER PRIMARY KEY, title TEXT)")
            .execute(&pool)
            .await
            .expect("create src");
        sqlx::query("CREATE VIRTUAL TABLE src_fts USING fts5(title, content='src', content_rowid='id')")
            .execute(&pool)
            .await
            .expect("create external-content fts5");
        sqlx::query("INSERT INTO src (title) VALUES ('wave scope')")
            .execute(&pool)
            .await
            .expect("insert");

        let matched: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM src_fts WHERE src_fts MATCH ?")
            .bind("\"wave\"")
            .fetch_one(&pool)
            .await
            .expect("match");
        let naive: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM src_fts")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(matched.0, 0, "MATCH is blind without triggers");
        assert_eq!(naive.0, 1, "count(*) hides it — never assert on count(*)");
    }
}
