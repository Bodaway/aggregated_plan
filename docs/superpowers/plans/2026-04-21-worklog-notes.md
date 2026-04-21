# Worklog Notes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add timestamped, task-scoped worklog entries, surface them inline in `TaskEditSheet`, provide a new `/worklog` page grouped by day, and redirect the activity-timer's quick note to create a worklog entry instead of appending to `tasks.notes`.

**Architecture:** Follows the existing DDD layering. New SQLite table `worklog_entries` + domain type `WorklogEntry` + `WorklogRepository` trait (application) + `SqliteWorklogRepository` (infrastructure) + async-graphql types/resolvers (api) + urql queries/mutations + React components (frontend). No FK to `ActivitySlot`. `tasks.notes` untouched; `appendTaskNotes` mutation remains registered for backward compat but is no longer called from the UI.

**Tech Stack:** Rust (Axum, async-graphql, sqlx, tokio), SQLite, React 18 + TS, urql, Tailwind, Vitest + Playwright.

**Spec:** `docs/superpowers/specs/2026-04-21-worklog-notes-design.md`.

---

## File Structure

**Backend:**
- Create: `migrations/sqlite/006_create_worklog_entries.sql`
- Create: `backend/crates/domain/src/types/worklog.rs`
- Modify: `backend/crates/domain/src/types/mod.rs` (add one `pub mod worklog;` + `pub use worklog::*;`)
- Modify: `backend/crates/domain/src/errors.rs` (only if new variants needed — we reuse `ValidationError`)
- Create: `backend/crates/application/src/repositories/worklog_repository.rs`
- Modify: `backend/crates/application/src/repositories/mod.rs` (wire in new module)
- Create: `backend/crates/application/src/use_cases/worklog.rs`
- Modify: `backend/crates/application/src/use_cases/mod.rs`
- Create: `backend/crates/infrastructure/src/database/worklog_repo.rs`
- Modify: `backend/crates/infrastructure/src/database/mod.rs`
- Create: `backend/crates/api/src/graphql/types/worklog_entry.rs`
- Modify: `backend/crates/api/src/graphql/types/mod.rs`
- Modify: `backend/crates/api/src/graphql/query.rs` (add `worklog_entries` resolver)
- Modify: `backend/crates/api/src/graphql/mutation.rs` (add three resolvers)
- Modify: `backend/crates/api/src/graphql/schema.rs` (new param, inject into Schema data)
- Modify: `backend/crates/api/src/main.rs` (construct repo and pass it through)

**Frontend:**
- Create: `frontend/src/graphql/queries/worklog.graphql`
- Create: `frontend/src/graphql/mutations/worklog.graphql`
- Create: `frontend/src/hooks/use-worklog.ts`
- Create: `frontend/src/components/worklog/WorklogSection.tsx` (inline in TaskEditSheet)
- Create: `frontend/src/components/worklog/WorklogEntryCard.tsx`
- Create: `frontend/src/components/worklog/WorklogEntryKebab.tsx`
- Create: `frontend/src/components/worklog/AddWorklogEntryForm.tsx`
- Create: `frontend/src/pages/WorklogPage.tsx`
- Modify: `frontend/src/components/task/TaskEditSheet.tsx` (mount `<WorklogSection>`)
- Modify: `frontend/src/App.tsx` (new route)
- Modify: `frontend/src/components/layout/Sidebar.tsx` (nav item)
- Modify: `frontend/src/hooks/use-activity.ts` (stop flow swap)
- Modify: `frontend/src/components/activity/ActivityTimer.tsx` (prop name stays, body swap only; could be no-change)

**Specs:**
- Modify: `SPEC_FONCTIONNELLE.md`
- Modify: `SPEC_TECHNIQUE.md`

**Tests (E2E):**
- Create: `frontend/e2e/worklog.spec.ts`

---

### Task 1: SQLite migration for `worklog_entries`

**Files:**
- Create: `migrations/sqlite/006_create_worklog_entries.sql`

- [ ] **Step 1: Create the migration file**

```sql
-- 006_create_worklog_entries.sql
-- Timestamped, task-scoped journal entries. Parallel to tasks.notes (unchanged).
CREATE TABLE worklog_entries (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    body       TEXT NOT NULL,
    logged_at  TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_worklog_entries_user_logged_at
    ON worklog_entries(user_id, logged_at DESC);
CREATE INDEX idx_worklog_entries_task_logged_at
    ON worklog_entries(task_id, logged_at DESC);
```

- [ ] **Step 2: Verify migration applies by building infrastructure tests (which run migrations in `:memory:` DBs)**

Run: `cd backend && cargo build -p infrastructure`
Expected: compiles successfully. (Migrations are only applied at pool-creation time, so we verify by building now and will truly exercise it in Task 7.)

- [ ] **Step 3: Commit**

```bash
git add migrations/sqlite/006_create_worklog_entries.sql
git commit -m "feat(db): add worklog_entries table"
```

---

### Task 2: `WorklogEntry` domain type

**Files:**
- Create: `backend/crates/domain/src/types/worklog.rs`
- Modify: `backend/crates/domain/src/types/mod.rs`

- [ ] **Step 1: Write the failing tests**

Create `backend/crates/domain/src/types/worklog.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::DomainError;

use super::common::*;

pub type WorklogEntryId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorklogEntry {
    pub id: WorklogEntryId,
    pub user_id: UserId,
    pub task_id: TaskId,
    pub body: String,
    pub logged_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub const WORKLOG_BODY_MAX_LEN: usize = 10_000;

impl WorklogEntry {
    /// Build a validated entry.
    /// - `body` must be non-empty after trimming.
    /// - `body` must not exceed `WORKLOG_BODY_MAX_LEN` characters.
    pub fn new(
        user_id: UserId,
        task_id: TaskId,
        body: String,
        logged_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        if body.trim().is_empty() {
            return Err(DomainError::ValidationError(
                "worklog body cannot be empty".into(),
            ));
        }
        if body.chars().count() > WORKLOG_BODY_MAX_LEN {
            return Err(DomainError::ValidationError(format!(
                "worklog body too long (max {} chars)",
                WORKLOG_BODY_MAX_LEN
            )));
        }
        Ok(Self {
            id: Uuid::new_v4(),
            user_id,
            task_id,
            body,
            logged_at,
            created_at: now,
            updated_at: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> UserId {
        Uuid::new_v4()
    }
    fn tid() -> TaskId {
        Uuid::new_v4()
    }
    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-04-21T10:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn new_rejects_empty_body() {
        let err = WorklogEntry::new(uid(), tid(), "".into(), t0(), t0()).unwrap_err();
        assert_eq!(
            err,
            DomainError::ValidationError("worklog body cannot be empty".into())
        );
    }

    #[test]
    fn new_rejects_whitespace_only_body() {
        let err = WorklogEntry::new(uid(), tid(), "   \n\t  ".into(), t0(), t0()).unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)));
    }

    #[test]
    fn new_rejects_oversize_body() {
        let big = "x".repeat(WORKLOG_BODY_MAX_LEN + 1);
        let err = WorklogEntry::new(uid(), tid(), big, t0(), t0()).unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)));
    }

    #[test]
    fn new_accepts_valid_body() {
        let entry = WorklogEntry::new(uid(), tid(), "done the thing".into(), t0(), t0()).unwrap();
        assert_eq!(entry.body, "done the thing");
        assert_eq!(entry.logged_at, t0());
        assert_eq!(entry.created_at, t0());
        assert_eq!(entry.updated_at, t0());
    }

    #[test]
    fn new_accepts_body_at_max_len() {
        let body = "a".repeat(WORKLOG_BODY_MAX_LEN);
        let entry = WorklogEntry::new(uid(), tid(), body.clone(), t0(), t0()).unwrap();
        assert_eq!(entry.body.chars().count(), WORKLOG_BODY_MAX_LEN);
    }
}
```

Update `backend/crates/domain/src/types/mod.rs` — add after the existing entries:

```rust
pub mod worklog;
```

and in the `pub use` block:

```rust
pub use worklog::*;
```

- [ ] **Step 2: Run tests (should fail because module not wired yet / or pass immediately)**

Run: `cd backend && cargo test -p domain worklog`
Expected: 5 tests pass (the module is wired in Step 1).

- [ ] **Step 3: Commit**

```bash
git add backend/crates/domain/src/types/worklog.rs backend/crates/domain/src/types/mod.rs
git commit -m "feat(domain): add WorklogEntry type with validation"
```

---

### Task 3: `WorklogRepository` trait + `WorklogFilter`

**Files:**
- Create: `backend/crates/application/src/repositories/worklog_repository.rs`
- Modify: `backend/crates/application/src/repositories/mod.rs`

- [ ] **Step 1: Create the trait file**

```rust
// backend/crates/application/src/repositories/worklog_repository.rs
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::types::*;

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
    /// Max rows to return. Repositories MUST enforce an absolute cap.
    pub limit: u32,
    /// Pagination offset.
    pub offset: u32,
}

pub const WORKLOG_FILTER_DEFAULT_LIMIT: u32 = 200;
pub const WORKLOG_FILTER_MAX_LIMIT: u32 = 1_000;

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
}
```

Update `backend/crates/application/src/repositories/mod.rs` — append:

```rust
pub mod worklog_repository;
pub use worklog_repository::*;
```

- [ ] **Step 2: Build the application crate**

Run: `cd backend && cargo build -p application`
Expected: compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add backend/crates/application/src/repositories/worklog_repository.rs backend/crates/application/src/repositories/mod.rs
git commit -m "feat(application): add WorklogRepository trait and WorklogFilter"
```

---

### Task 4: Application use cases — add / update / delete / list

**Files:**
- Create: `backend/crates/application/src/use_cases/worklog.rs`
- Modify: `backend/crates/application/src/use_cases/mod.rs`

- [ ] **Step 1: Write the failing tests + use-case file**

Create `backend/crates/application/src/use_cases/worklog.rs`:

```rust
use chrono::{DateTime, Utc};
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::{
    WorklogFilter, WorklogRepository, WORKLOG_FILTER_DEFAULT_LIMIT, WORKLOG_FILTER_MAX_LIMIT,
};

/// Add a new worklog entry. `logged_at` defaults to `now` when `None`.
pub async fn add_worklog_entry(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    task_id: TaskId,
    body: String,
    logged_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<WorklogEntry, AppError> {
    let entry = WorklogEntry::new(user_id, task_id, body, logged_at.unwrap_or(now), now)?;
    worklog_repo.create(&entry).await?;
    Ok(entry)
}

/// Partially update an existing entry. Re-validates body if provided.
/// Only touches fields the caller passed.
pub async fn update_worklog_entry(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    id: WorklogEntryId,
    body: Option<String>,
    logged_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<WorklogEntry, AppError> {
    let mut entry = worklog_repo
        .find_by_id(id, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("worklog entry {id}")))?;

    if let Some(new_body) = body {
        // Round-trip through WorklogEntry::new to reuse validation (then copy fields).
        let validated = WorklogEntry::new(
            entry.user_id,
            entry.task_id,
            new_body,
            entry.logged_at,
            now,
        )?;
        entry.body = validated.body;
    }
    if let Some(lat) = logged_at {
        entry.logged_at = lat;
    }
    entry.updated_at = now;
    worklog_repo.update(&entry).await?;
    Ok(entry)
}

/// Delete an entry owned by the user. Returns true if a row was removed.
pub async fn delete_worklog_entry(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    id: WorklogEntryId,
) -> Result<bool, AppError> {
    Ok(worklog_repo.delete(id, user_id).await?)
}

/// List entries for a user, clamping `limit` to the repository's max.
pub async fn list_worklog_entries(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    mut filter: WorklogFilter,
) -> Result<Vec<WorklogEntry>, AppError> {
    if filter.limit == 0 {
        filter.limit = WORKLOG_FILTER_DEFAULT_LIMIT;
    }
    if filter.limit > WORKLOG_FILTER_MAX_LIMIT {
        filter.limit = WORKLOG_FILTER_MAX_LIMIT;
    }
    Ok(worklog_repo.list(user_id, &filter).await?)
}

// ---------------------------------------------------------------------------
// Tests use an in-memory fake repository to keep dependencies minimal.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    use crate::errors::RepositoryError;

    #[derive(Default)]
    struct FakeRepo {
        entries: Mutex<Vec<WorklogEntry>>,
    }

    #[async_trait]
    impl WorklogRepository for FakeRepo {
        async fn create(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
            self.entries.lock().unwrap().push(entry.clone());
            Ok(())
        }
        async fn update(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
            let mut v = self.entries.lock().unwrap();
            if let Some(slot) = v.iter_mut().find(|e| e.id == entry.id) {
                *slot = entry.clone();
            }
            Ok(())
        }
        async fn delete(
            &self,
            id: WorklogEntryId,
            user_id: UserId,
        ) -> Result<bool, RepositoryError> {
            let mut v = self.entries.lock().unwrap();
            let before = v.len();
            v.retain(|e| !(e.id == id && e.user_id == user_id));
            Ok(v.len() < before)
        }
        async fn find_by_id(
            &self,
            id: WorklogEntryId,
            user_id: UserId,
        ) -> Result<Option<WorklogEntry>, RepositoryError> {
            Ok(self
                .entries
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.id == id && e.user_id == user_id)
                .cloned())
        }
        async fn list(
            &self,
            user_id: UserId,
            filter: &WorklogFilter,
        ) -> Result<Vec<WorklogEntry>, RepositoryError> {
            let v = self.entries.lock().unwrap();
            let mut out: Vec<WorklogEntry> = v
                .iter()
                .filter(|e| e.user_id == user_id)
                .filter(|e| match &filter.task_ids {
                    Some(ids) => ids.contains(&e.task_id),
                    None => true,
                })
                .filter(|e| match filter.from {
                    Some(f) => e.logged_at >= f,
                    None => true,
                })
                .filter(|e| match filter.to {
                    Some(t) => e.logged_at < t,
                    None => true,
                })
                .cloned()
                .collect();
            out.sort_by(|a, b| b.logged_at.cmp(&a.logged_at));
            let start = filter.offset as usize;
            let end = (start + filter.limit as usize).min(out.len());
            if start >= out.len() {
                Ok(vec![])
            } else {
                Ok(out[start..end].to_vec())
            }
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-04-21T10:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[tokio::test]
    async fn add_uses_now_when_logged_at_is_none() {
        let repo = FakeRepo::default();
        let uid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let entry = add_worklog_entry(&repo, uid, tid, "x".into(), None, now())
            .await
            .unwrap();
        assert_eq!(entry.logged_at, now());
    }

    #[tokio::test]
    async fn add_uses_override_when_logged_at_is_some() {
        let repo = FakeRepo::default();
        let uid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let earlier = DateTime::parse_from_rfc3339("2026-04-20T08:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let entry = add_worklog_entry(&repo, uid, tid, "x".into(), Some(earlier), now())
            .await
            .unwrap();
        assert_eq!(entry.logged_at, earlier);
    }

    #[tokio::test]
    async fn update_rejects_other_users_entry() {
        let repo = FakeRepo::default();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let entry = add_worklog_entry(&repo, a, tid, "orig".into(), None, now())
            .await
            .unwrap();
        let err = update_worklog_entry(&repo, b, entry.id, Some("hax".into()), None, now())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn update_changes_body_and_touches_updated_at() {
        let repo = FakeRepo::default();
        let uid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let entry = add_worklog_entry(&repo, uid, tid, "v1".into(), None, now())
            .await
            .unwrap();
        let later = now() + chrono::Duration::seconds(30);
        let updated =
            update_worklog_entry(&repo, uid, entry.id, Some("v2".into()), None, later)
                .await
                .unwrap();
        assert_eq!(updated.body, "v2");
        assert_eq!(updated.updated_at, later);
    }

    #[tokio::test]
    async fn delete_removes_owned_entry_and_ignores_others() {
        let repo = FakeRepo::default();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let entry = add_worklog_entry(&repo, a, tid, "x".into(), None, now())
            .await
            .unwrap();
        assert!(!delete_worklog_entry(&repo, b, entry.id).await.unwrap());
        assert!(delete_worklog_entry(&repo, a, entry.id).await.unwrap());
    }

    #[tokio::test]
    async fn list_clamps_limit_to_default_when_zero() {
        let repo = FakeRepo::default();
        let uid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        add_worklog_entry(&repo, uid, tid, "one".into(), None, now())
            .await
            .unwrap();
        let out = list_worklog_entries(&repo, uid, WorklogFilter::default())
            .await
            .unwrap();
        assert_eq!(out.len(), 1);
    }
}
```

Update `backend/crates/application/src/use_cases/mod.rs` — append a line:

```rust
pub mod worklog;
```

- [ ] **Step 2: Run tests**

Run: `cd backend && cargo test -p application worklog`
Expected: 6 tests pass.

- [ ] **Step 3: Commit**

```bash
git add backend/crates/application/src/use_cases/worklog.rs backend/crates/application/src/use_cases/mod.rs
git commit -m "feat(application): worklog use cases (add/update/delete/list)"
```

---

### Task 5: `SqliteWorklogRepository` — struct + create + find_by_id

**Files:**
- Create: `backend/crates/infrastructure/src/database/worklog_repo.rs`
- Modify: `backend/crates/infrastructure/src/database/mod.rs`

- [ ] **Step 1: Write the failing tests + partial impl**

Create `backend/crates/infrastructure/src/database/worklog_repo.rs`:

```rust
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::{WorklogFilter, WorklogRepository};
use domain::types::*;

pub struct SqliteWorklogRepository {
    pool: SqlitePool,
}

impl SqliteWorklogRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RepositoryError::Database(format!("Failed to parse datetime '{s}': {e}")))
}

fn map_row(row: &SqliteRow) -> Result<WorklogEntry, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let user_id_str: String = Row::get(row, "user_id");
    let task_id_str: String = Row::get(row, "task_id");
    let body: String = Row::get(row, "body");
    let logged_at_str: String = Row::get(row, "logged_at");
    let created_at_str: String = Row::get(row, "created_at");
    let updated_at_str: String = Row::get(row, "updated_at");

    Ok(WorklogEntry {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        task_id: Uuid::parse_str(&task_id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        body,
        logged_at: parse_datetime(&logged_at_str)?,
        created_at: parse_datetime(&created_at_str)?,
        updated_at: parse_datetime(&updated_at_str)?,
    })
}

#[async_trait]
impl WorklogRepository for SqliteWorklogRepository {
    async fn create(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO worklog_entries (id, user_id, task_id, body, logged_at, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(entry.id.to_string())
        .bind(entry.user_id.to_string())
        .bind(entry.task_id.to_string())
        .bind(&entry.body)
        .bind(entry.logged_at.to_rfc3339())
        .bind(entry.created_at.to_rfc3339())
        .bind(entry.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn update(&self, entry: &WorklogEntry) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE worklog_entries
                SET body = ?, logged_at = ?, updated_at = ?
              WHERE id = ? AND user_id = ?",
        )
        .bind(&entry.body)
        .bind(entry.logged_at.to_rfc3339())
        .bind(entry.updated_at.to_rfc3339())
        .bind(entry.id.to_string())
        .bind(entry.user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(
        &self,
        id: WorklogEntryId,
        user_id: UserId,
    ) -> Result<bool, RepositoryError> {
        let result = sqlx::query("DELETE FROM worklog_entries WHERE id = ? AND user_id = ?")
            .bind(id.to_string())
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_by_id(
        &self,
        id: WorklogEntryId,
        user_id: UserId,
    ) -> Result<Option<WorklogEntry>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM worklog_entries WHERE id = ? AND user_id = ? LIMIT 1",
        )
        .bind(id.to_string())
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => Ok(Some(map_row(row)?)),
            None => Ok(None),
        }
    }

    async fn list(
        &self,
        user_id: UserId,
        filter: &WorklogFilter,
    ) -> Result<Vec<WorklogEntry>, RepositoryError> {
        let mut sql =
            String::from("SELECT * FROM worklog_entries WHERE user_id = ?");
        if let Some(ids) = &filter.task_ids {
            if ids.is_empty() {
                return Ok(vec![]);
            }
            sql.push_str(" AND task_id IN (");
            sql.push_str(&vec!["?"; ids.len()].join(","));
            sql.push(')');
        }
        if filter.from.is_some() {
            sql.push_str(" AND logged_at >= ?");
        }
        if filter.to.is_some() {
            sql.push_str(" AND logged_at < ?");
        }
        sql.push_str(" ORDER BY logged_at DESC, created_at DESC LIMIT ? OFFSET ?");

        let mut q = sqlx::query(&sql).bind(user_id.to_string());
        if let Some(ids) = &filter.task_ids {
            for tid in ids {
                q = q.bind(tid.to_string());
            }
        }
        if let Some(from) = filter.from {
            q = q.bind(from.to_rfc3339());
        }
        if let Some(to) = filter.to {
            q = q.bind(to.to_rfc3339());
        }
        let rows = q
            .bind(filter.limit as i64)
            .bind(filter.offset as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(map_row).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::connection::create_sqlite_pool;

    const USER_ID: &str = "00000000-0000-0000-0000-000000000001";

    async fn setup() -> SqlitePool {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        // User is seeded by create_sqlite_pool. Insert a task we can reference.
        sqlx::query(
            "INSERT INTO tasks (id, user_id, title, source, status, impact, urgency, created_at, updated_at, tracking_state)
             VALUES (?, ?, 'T', 'personal', 'todo', 1, 1, ?, ?, 'followed')",
        )
        .bind("11111111-1111-1111-1111-111111111111")
        .bind(USER_ID)
        .bind("2026-04-21T00:00:00+00:00")
        .bind("2026-04-21T00:00:00+00:00")
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn uid() -> Uuid {
        Uuid::parse_str(USER_ID).unwrap()
    }
    fn tid() -> Uuid {
        Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()
    }
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-04-21T10:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[tokio::test]
    async fn create_then_find_by_id_roundtrips() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        let entry = WorklogEntry::new(uid(), tid(), "hello".into(), now(), now()).unwrap();
        repo.create(&entry).await.unwrap();
        let found = repo.find_by_id(entry.id, uid()).await.unwrap().unwrap();
        assert_eq!(found.id, entry.id);
        assert_eq!(found.body, "hello");
        assert_eq!(found.logged_at, now());
    }

    #[tokio::test]
    async fn find_by_id_respects_user_scoping() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        let entry = WorklogEntry::new(uid(), tid(), "x".into(), now(), now()).unwrap();
        repo.create(&entry).await.unwrap();
        let other = Uuid::new_v4();
        assert!(repo.find_by_id(entry.id, other).await.unwrap().is_none());
    }
}
```

Update `backend/crates/infrastructure/src/database/mod.rs` — append:

```rust
pub mod worklog_repo;
pub use worklog_repo::SqliteWorklogRepository;
```

- [ ] **Step 2: Run tests**

Run: `cd backend && cargo test -p infrastructure worklog_repo::tests::create_then_find_by_id_roundtrips worklog_repo::tests::find_by_id_respects_user_scoping`
Expected: both tests pass.

- [ ] **Step 3: Commit**

```bash
git add backend/crates/infrastructure/src/database/worklog_repo.rs backend/crates/infrastructure/src/database/mod.rs
git commit -m "feat(infrastructure): SqliteWorklogRepository (create/find/update/delete/list)"
```

---

### Task 6: `SqliteWorklogRepository` — list/update/delete + cascade tests

**Files:**
- Modify: `backend/crates/infrastructure/src/database/worklog_repo.rs` (tests only)

- [ ] **Step 1: Add more integration tests to the existing `tests` module**

Append to the `tests` module in `backend/crates/infrastructure/src/database/worklog_repo.rs`:

```rust
    #[tokio::test]
    async fn list_orders_by_logged_at_desc() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        let t1 = DateTime::parse_from_rfc3339("2026-04-20T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let t2 = DateTime::parse_from_rfc3339("2026-04-21T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let e1 = WorklogEntry::new(uid(), tid(), "older".into(), t1, t1).unwrap();
        let e2 = WorklogEntry::new(uid(), tid(), "newer".into(), t2, t2).unwrap();
        repo.create(&e1).await.unwrap();
        repo.create(&e2).await.unwrap();

        let out = repo
            .list(
                uid(),
                &WorklogFilter {
                    limit: 10,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].body, "newer");
        assert_eq!(out[1].body, "older");
    }

    #[tokio::test]
    async fn list_filters_by_date_range() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        let t1 = DateTime::parse_from_rfc3339("2026-04-20T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let t2 = DateTime::parse_from_rfc3339("2026-04-21T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        repo.create(&WorklogEntry::new(uid(), tid(), "a".into(), t1, t1).unwrap())
            .await
            .unwrap();
        repo.create(&WorklogEntry::new(uid(), tid(), "b".into(), t2, t2).unwrap())
            .await
            .unwrap();

        let only_21 = repo
            .list(
                uid(),
                &WorklogFilter {
                    from: Some(t2),
                    limit: 10,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(only_21.len(), 1);
        assert_eq!(only_21[0].body, "b");
    }

    #[tokio::test]
    async fn list_respects_limit_and_offset() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        for i in 0..5 {
            let at = DateTime::parse_from_rfc3339(&format!(
                "2026-04-{:02}T09:00:00+00:00",
                20 + i
            ))
            .unwrap()
            .with_timezone(&Utc);
            repo.create(&WorklogEntry::new(uid(), tid(), format!("n{i}"), at, at).unwrap())
                .await
                .unwrap();
        }
        let page1 = repo
            .list(
                uid(),
                &WorklogFilter {
                    limit: 2,
                    offset: 0,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(page1.len(), 2);
        let page2 = repo
            .list(
                uid(),
                &WorklogFilter {
                    limit: 2,
                    offset: 2,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);
        assert_ne!(page1[0].id, page2[0].id);
    }

    #[tokio::test]
    async fn update_persists_new_body() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        let mut entry = WorklogEntry::new(uid(), tid(), "v1".into(), now(), now()).unwrap();
        repo.create(&entry).await.unwrap();
        entry.body = "v2".into();
        entry.updated_at = now() + chrono::Duration::seconds(30);
        repo.update(&entry).await.unwrap();
        let found = repo.find_by_id(entry.id, uid()).await.unwrap().unwrap();
        assert_eq!(found.body, "v2");
    }

    #[tokio::test]
    async fn delete_returns_true_on_hit_false_on_miss() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool);
        let entry = WorklogEntry::new(uid(), tid(), "x".into(), now(), now()).unwrap();
        repo.create(&entry).await.unwrap();
        assert!(repo.delete(entry.id, uid()).await.unwrap());
        assert!(!repo.delete(entry.id, uid()).await.unwrap());
    }

    #[tokio::test]
    async fn deleting_task_cascades_entries() {
        let pool = setup().await;
        let repo = SqliteWorklogRepository::new(pool.clone());
        let entry = WorklogEntry::new(uid(), tid(), "x".into(), now(), now()).unwrap();
        repo.create(&entry).await.unwrap();
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(tid().to_string())
            .execute(&pool)
            .await
            .unwrap();
        assert!(repo.find_by_id(entry.id, uid()).await.unwrap().is_none());
    }
```

- [ ] **Step 2: Run tests**

Run: `cd backend && cargo test -p infrastructure worklog_repo`
Expected: 8 tests pass (the 2 from Task 5 plus 6 new ones).

- [ ] **Step 3: Commit**

```bash
git add backend/crates/infrastructure/src/database/worklog_repo.rs
git commit -m "test(infrastructure): worklog list/update/delete + cascade coverage"
```

---

### Task 7: GraphQL types — `WorklogEntryGql` + input types

**Files:**
- Create: `backend/crates/api/src/graphql/types/worklog_entry.rs`
- Modify: `backend/crates/api/src/graphql/types/mod.rs`

- [ ] **Step 1: Create the GraphQL type file**

```rust
// backend/crates/api/src/graphql/types/worklog_entry.rs
use std::sync::Arc;

use async_graphql::{Context, InputObject, Object, ID};
use chrono::{DateTime, Utc};

use application::repositories::TaskRepository;
use domain::types::WorklogEntry;

use super::task::TaskGql;

/// GraphQL wrapper for the domain WorklogEntry entity.
pub struct WorklogEntryGql(pub WorklogEntry);

#[Object]
impl WorklogEntryGql {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }

    async fn task_id(&self) -> ID {
        ID(self.0.task_id.to_string())
    }

    /// Hydrated task. Returns null if the task was deleted (shouldn't happen
    /// under normal conditions thanks to the FK cascade).
    async fn task(&self, ctx: &Context<'_>) -> Option<TaskGql> {
        let repo = ctx.data::<Arc<dyn TaskRepository>>().ok()?;
        let task = repo.find_by_id(self.0.task_id).await.ok()??;
        Some(TaskGql(task))
    }

    async fn body(&self) -> &str {
        &self.0.body
    }

    async fn logged_at(&self) -> DateTime<Utc> {
        self.0.logged_at
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.0.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.0.updated_at
    }
}

/// Filter input for `worklogEntries`.
#[derive(InputObject, Debug, Default)]
pub struct WorklogEntryFilterInput {
    pub task_ids: Option<Vec<ID>>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}
```

Update `backend/crates/api/src/graphql/types/mod.rs` — append:

```rust
pub mod worklog_entry;
pub use worklog_entry::*;
```

- [ ] **Step 2: Build**

Run: `cd backend && cargo build -p api`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add backend/crates/api/src/graphql/types/worklog_entry.rs backend/crates/api/src/graphql/types/mod.rs
git commit -m "feat(api): WorklogEntry GraphQL type + filter input"
```

---

### Task 8: GraphQL query `worklogEntries` + wire repo through `build_schema`

**Files:**
- Modify: `backend/crates/api/src/graphql/query.rs`
- Modify: `backend/crates/api/src/graphql/schema.rs`
- Modify: `backend/crates/api/src/main.rs`

- [ ] **Step 1: Add the `worklogEntries` resolver**

In `backend/crates/api/src/graphql/query.rs`, add imports (if missing) and a new `#[Object]` method. Look for the `TaskRepository` import and add:

```rust
use application::repositories::{WorklogFilter, WorklogRepository};
use domain::types::UserId;
use crate::graphql::types::{WorklogEntryFilterInput, WorklogEntryGql};
```

Then inside the `impl QueryRoot` block (somewhere alongside other task-related queries), add:

```rust
    /// List worklog entries for the authenticated user.
    async fn worklog_entries(
        &self,
        ctx: &Context<'_>,
        filter: Option<WorklogEntryFilterInput>,
    ) -> Result<Vec<WorklogEntryGql>> {
        let repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let user_id = *ctx.data::<UserId>()?;
        let f = filter.unwrap_or_default();
        let wf = WorklogFilter {
            task_ids: f
                .task_ids
                .map(|ids| {
                    ids.iter()
                        .map(|i| Uuid::parse_str(i).map_err(|e| async_graphql::Error::new(e.to_string())))
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?,
            from: f.from,
            to: f.to,
            limit: f.limit.unwrap_or(0).max(0) as u32,
            offset: f.offset.unwrap_or(0).max(0) as u32,
        };
        let entries = application::use_cases::worklog::list_worklog_entries(repo.as_ref(), user_id, wf)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(entries.into_iter().map(WorklogEntryGql).collect())
    }
```

Ensure `use std::sync::Arc;` and `use uuid::Uuid;` are present at the top of `query.rs` (they are already used elsewhere in the file).

- [ ] **Step 2: Wire `WorklogRepository` into `build_schema`**

In `backend/crates/api/src/graphql/schema.rs`, add a new parameter at the end of `build_schema` and `.data(...)` it:

```rust
pub fn build_schema(
    task_repo: Arc<dyn TaskRepository>,
    meeting_repo: Arc<dyn MeetingRepository>,
    project_repo: Arc<dyn ProjectRepository>,
    activity_repo: Arc<dyn ActivitySlotRepository>,
    alert_repo: Arc<dyn AlertRepository>,
    tag_repo: Arc<dyn TagRepository>,
    task_link_repo: Arc<dyn TaskLinkRepository>,
    sync_repo: Arc<dyn SyncStatusRepository>,
    config_repo: Arc<dyn ConfigRepository>,
    worklog_repo: Arc<dyn WorklogRepository>,
) -> AppSchema {
    // ... existing body ...
    Schema::build(
        CombinedQuery(QueryRoot),
        CombinedMutation(MutationRoot),
        EmptySubscription,
    )
    .data(task_repo)
    .data(meeting_repo)
    .data(project_repo)
    .data(activity_repo)
    .data(alert_repo)
    .data(tag_repo)
    .data(task_link_repo)
    .data(sync_repo)
    .data(config_repo)
    .data(worklog_repo)
    .data(default_user_id)
    .finish()
}
```

- [ ] **Step 3: Construct `SqliteWorklogRepository` in `main.rs` and pass it to `build_schema`**

In `backend/crates/api/src/main.rs`, after the `config_repo` construction and before the `build_schema` call, add:

```rust
    let worklog_repo: Arc<dyn application::repositories::WorklogRepository> =
        Arc::new(infrastructure::database::SqliteWorklogRepository::new(db_pool.clone()));
```

Then update the `build_schema` call to pass `worklog_repo`:

```rust
    let schema = graphql::schema::build_schema(
        task_repo,
        meeting_repo,
        project_repo,
        activity_repo,
        alert_repo,
        tag_repo,
        task_link_repo,
        sync_repo,
        config_repo,
        worklog_repo,
    );
```

- [ ] **Step 4: Build**

Run: `cd backend && cargo build -p api`
Expected: compiles.

- [ ] **Step 5: Smoke-test the query**

Restart the service:

```bash
systemctl --user restart aggregated-plan.service
```

Wait ~10s for rebuild, then run:

```bash
curl -s -X POST http://127.0.0.1:3001/graphql \
  -H 'Content-Type: application/json' \
  -d '{"query":"{ worklogEntries { id body loggedAt } }"}'
```

Expected: `{"data":{"worklogEntries":[]}}` (empty list; no entries exist yet).

- [ ] **Step 6: Commit**

```bash
git add backend/crates/api/src/graphql/query.rs backend/crates/api/src/graphql/schema.rs backend/crates/api/src/main.rs
git commit -m "feat(api): worklogEntries GraphQL query"
```

---

### Task 9: GraphQL mutations — add / update / delete

**Files:**
- Modify: `backend/crates/api/src/graphql/mutation.rs`

- [ ] **Step 1: Add the three mutations**

In `backend/crates/api/src/graphql/mutation.rs`, add imports near the other application imports:

```rust
use application::repositories::WorklogRepository;
use application::use_cases::worklog as worklog_uc;
use crate::graphql::types::WorklogEntryGql;
```

Inside the `#[Object] impl MutationRoot` block (near the other task-adjacent mutations, e.g. after `append_task_notes`), add:

```rust
    /// Add a timestamped worklog entry to a task.
    async fn add_worklog_entry(
        &self,
        ctx: &Context<'_>,
        task_id: ID,
        body: String,
        logged_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<WorklogEntryGql> {
        let repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let user_id = *ctx.data::<UserId>()?;
        let tid = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {e}")))?;
        let entry = worklog_uc::add_worklog_entry(
            repo.as_ref(),
            user_id,
            tid,
            body,
            logged_at,
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(WorklogEntryGql(entry))
    }

    /// Update a worklog entry's body and/or logged_at. Only provided fields are changed.
    async fn update_worklog_entry(
        &self,
        ctx: &Context<'_>,
        id: ID,
        body: Option<String>,
        logged_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<WorklogEntryGql> {
        let repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let user_id = *ctx.data::<UserId>()?;
        let eid = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid entry ID: {e}")))?;
        let entry = worklog_uc::update_worklog_entry(
            repo.as_ref(),
            user_id,
            eid,
            body,
            logged_at,
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(WorklogEntryGql(entry))
    }

    /// Delete a worklog entry. Returns true if it existed and was removed.
    async fn delete_worklog_entry(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        let repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let user_id = *ctx.data::<UserId>()?;
        let eid = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid entry ID: {e}")))?;
        worklog_uc::delete_worklog_entry(repo.as_ref(), user_id, eid)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))
    }
```

- [ ] **Step 2: Build and smoke-test**

Run: `cd backend && cargo build -p api`
Expected: compiles.

Restart the service:

```bash
systemctl --user restart aggregated-plan.service
```

Wait ~10s. Then create a task via existing `createTask` mutation, grab its id, and add an entry:

```bash
# Pick any existing task id (query tasks):
TID=$(curl -s -X POST http://127.0.0.1:3001/graphql -H 'Content-Type: application/json' \
  -d '{"query":"{ tasks(filter:{}) { edges { node { id } } } }"}' \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["tasks"]["edges"][0]["node"]["id"])')
echo "task: $TID"

curl -s -X POST http://127.0.0.1:3001/graphql \
  -H 'Content-Type: application/json' \
  -d "{\"query\":\"mutation{ addWorklogEntry(taskId:\\\"$TID\\\", body:\\\"first entry\\\"){ id body loggedAt } }\"}"
```

Expected: a `{"data":{"addWorklogEntry":{"id":"...","body":"first entry","loggedAt":"..."}}}` payload.

- [ ] **Step 3: Commit**

```bash
git add backend/crates/api/src/graphql/mutation.rs
git commit -m "feat(api): addWorklogEntry / updateWorklogEntry / deleteWorklogEntry"
```

---

### Task 10: Frontend GraphQL operation files

**Files:**
- Create: `frontend/src/graphql/queries/worklog.graphql`
- Create: `frontend/src/graphql/mutations/worklog.graphql`

- [ ] **Step 1: Create the query**

```graphql
# frontend/src/graphql/queries/worklog.graphql
query WorklogEntries($filter: WorklogEntryFilterInput) {
  worklogEntries(filter: $filter) {
    id
    taskId
    task {
      id
      title
    }
    body
    loggedAt
    createdAt
    updatedAt
  }
}
```

- [ ] **Step 2: Create the mutations**

```graphql
# frontend/src/graphql/mutations/worklog.graphql
mutation AddWorklogEntry($taskId: ID!, $body: String!, $loggedAt: DateTime) {
  addWorklogEntry(taskId: $taskId, body: $body, loggedAt: $loggedAt) {
    id
    taskId
    task { id title }
    body
    loggedAt
    createdAt
    updatedAt
  }
}

mutation UpdateWorklogEntry($id: ID!, $body: String, $loggedAt: DateTime) {
  updateWorklogEntry(id: $id, body: $body, loggedAt: $loggedAt) {
    id
    taskId
    task { id title }
    body
    loggedAt
    createdAt
    updatedAt
  }
}

mutation DeleteWorklogEntry($id: ID!) {
  deleteWorklogEntry(id: $id)
}
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/graphql/queries/worklog.graphql frontend/src/graphql/mutations/worklog.graphql
git commit -m "feat(frontend): worklog GraphQL operations"
```

---

### Task 11: `useWorklog` hook

**Files:**
- Create: `frontend/src/hooks/use-worklog.ts`

- [ ] **Step 1: Write the hook**

```typescript
// frontend/src/hooks/use-worklog.ts
import { useCallback, useMemo } from 'react';
import { useQuery, useMutation } from 'urql';

const WORKLOG_QUERY = `
  query WorklogEntries($filter: WorklogEntryFilterInput) {
    worklogEntries(filter: $filter) {
      id
      taskId
      task { id title }
      body
      loggedAt
      createdAt
      updatedAt
    }
  }
`;

const ADD = `
  mutation AddWorklogEntry($taskId: ID!, $body: String!, $loggedAt: DateTime) {
    addWorklogEntry(taskId: $taskId, body: $body, loggedAt: $loggedAt) {
      id taskId task { id title } body loggedAt createdAt updatedAt
    }
  }
`;

const UPDATE = `
  mutation UpdateWorklogEntry($id: ID!, $body: String, $loggedAt: DateTime) {
    updateWorklogEntry(id: $id, body: $body, loggedAt: $loggedAt) {
      id taskId task { id title } body loggedAt createdAt updatedAt
    }
  }
`;

const DELETE = `
  mutation DeleteWorklogEntry($id: ID!) { deleteWorklogEntry(id: $id) }
`;

export type WorklogEntry = {
  id: string;
  taskId: string;
  task: { id: string; title: string } | null;
  body: string;
  loggedAt: string;
  createdAt: string;
  updatedAt: string;
};

export type WorklogFilter = {
  taskIds?: string[];
  from?: string;
  to?: string;
  limit?: number;
  offset?: number;
};

export function useWorklog(filter: WorklogFilter = {}) {
  const variables = useMemo(() => ({ filter }), [filter]);
  const [result, reexecute] = useQuery<{ worklogEntries: WorklogEntry[] }>({
    query: WORKLOG_QUERY,
    variables,
    requestPolicy: 'cache-and-network',
  });

  const [, executeAdd] = useMutation<{ addWorklogEntry: WorklogEntry }>(ADD);
  const [, executeUpdate] = useMutation<{ updateWorklogEntry: WorklogEntry }>(UPDATE);
  const [, executeDelete] = useMutation<{ deleteWorklogEntry: boolean }>(DELETE);

  const refetch = useCallback(
    () => reexecute({ requestPolicy: 'network-only' }),
    [reexecute]
  );

  const addEntry = useCallback(
    async (input: { taskId: string; body: string; loggedAt?: string }) => {
      const res = await executeAdd(input);
      if (res.error) throw res.error;
      refetch();
      return res.data?.addWorklogEntry;
    },
    [executeAdd, refetch]
  );

  const updateEntry = useCallback(
    async (input: { id: string; body?: string; loggedAt?: string }) => {
      const res = await executeUpdate(input);
      if (res.error) throw res.error;
      refetch();
      return res.data?.updateWorklogEntry;
    },
    [executeUpdate, refetch]
  );

  const deleteEntry = useCallback(
    async (id: string) => {
      const res = await executeDelete({ id });
      if (res.error) throw res.error;
      refetch();
      return res.data?.deleteWorklogEntry ?? false;
    },
    [executeDelete, refetch]
  );

  return {
    entries: result.data?.worklogEntries ?? [],
    loading: result.fetching,
    error: result.error ?? null,
    addEntry,
    updateEntry,
    deleteEntry,
    refetch,
  };
}
```

- [ ] **Step 2: Type-check**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/use-worklog.ts
git commit -m "feat(frontend): useWorklog hook"
```

---

### Task 12: `WorklogEntryCard` + `WorklogEntryKebab` + `AddWorklogEntryForm`

**Files:**
- Create: `frontend/src/components/worklog/WorklogEntryCard.tsx`
- Create: `frontend/src/components/worklog/WorklogEntryKebab.tsx`
- Create: `frontend/src/components/worklog/AddWorklogEntryForm.tsx`

- [ ] **Step 1: Add the form component**

```tsx
// frontend/src/components/worklog/AddWorklogEntryForm.tsx
import { useState, useCallback, useRef } from 'react';

interface Props {
  readonly onSubmit: (body: string) => Promise<void>;
  readonly placeholder?: string;
}

export function AddWorklogEntryForm({ onSubmit, placeholder }: Props) {
  const [value, setValue] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const submit = useCallback(async () => {
    const trimmed = value.trim();
    if (!trimmed || submitting) return;
    setSubmitting(true);
    try {
      await onSubmit(trimmed);
      setValue('');
      textareaRef.current?.focus();
    } finally {
      setSubmitting(false);
    }
  }, [value, submitting, onSubmit]);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        e.preventDefault();
        void submit();
      }
    },
    [submit]
  );

  return (
    <div className="space-y-2">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder ?? 'Log an entry… (Ctrl+Enter to submit)'}
        rows={3}
        className="w-full rounded-md border border-gray-300 p-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
        disabled={submitting}
      />
      <div className="flex justify-end">
        <button
          type="button"
          onClick={submit}
          disabled={!value.trim() || submitting}
          className="rounded-md bg-blue-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-700 disabled:bg-gray-300"
        >
          {submitting ? 'Logging…' : 'Log entry'}
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Add the kebab menu**

```tsx
// frontend/src/components/worklog/WorklogEntryKebab.tsx
import { useState, useRef, useEffect } from 'react';

interface Props {
  readonly onEdit: () => void;
  readonly onDelete: () => void;
  readonly onEditTimestamp: () => void;
}

export function WorklogEntryKebab({ onEdit, onDelete, onEditTimestamp }: Props) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, [open]);

  return (
    <div ref={containerRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-700"
        aria-label="Entry actions"
      >
        <svg className="h-4 w-4" fill="currentColor" viewBox="0 0 20 20">
          <path d="M10 6a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm0 5.5a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm0 5.5a1.5 1.5 0 110-3 1.5 1.5 0 010 3z" />
        </svg>
      </button>
      {open && (
        <div className="absolute right-0 top-full z-20 mt-1 w-40 rounded-md border border-gray-200 bg-white py-1 shadow-lg">
          <button
            type="button"
            onClick={() => { setOpen(false); onEdit(); }}
            className="block w-full px-3 py-1.5 text-left text-sm hover:bg-gray-50"
          >
            Edit
          </button>
          <button
            type="button"
            onClick={() => { setOpen(false); onEditTimestamp(); }}
            className="block w-full px-3 py-1.5 text-left text-sm hover:bg-gray-50"
          >
            Edit timestamp…
          </button>
          <button
            type="button"
            onClick={() => { setOpen(false); onDelete(); }}
            className="block w-full px-3 py-1.5 text-left text-sm text-red-600 hover:bg-red-50"
          >
            Delete
          </button>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Add the card**

```tsx
// frontend/src/components/worklog/WorklogEntryCard.tsx
import { useState, useCallback } from 'react';
import type { WorklogEntry } from '@/hooks/use-worklog';
import { WorklogEntryKebab } from './WorklogEntryKebab';

interface Props {
  readonly entry: WorklogEntry;
  readonly showTaskChip?: boolean;
  readonly onTaskClick?: (taskId: string) => void;
  readonly onSave: (patch: { body?: string; loggedAt?: string }) => Promise<void>;
  readonly onDelete: () => Promise<void>;
}

function formatTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}

function toLocalInputValue(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => n.toString().padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function WorklogEntryCard({ entry, showTaskChip, onTaskClick, onSave, onDelete }: Props) {
  const [mode, setMode] = useState<'view' | 'edit-body' | 'edit-ts'>('view');
  const [body, setBody] = useState(entry.body);
  const [tsInput, setTsInput] = useState(toLocalInputValue(entry.loggedAt));

  const saveBody = useCallback(async () => {
    const trimmed = body.trim();
    if (!trimmed || trimmed === entry.body) {
      setMode('view');
      setBody(entry.body);
      return;
    }
    await onSave({ body: trimmed });
    setMode('view');
  }, [body, entry.body, onSave]);

  const saveTs = useCallback(async () => {
    const dt = new Date(tsInput);
    if (isNaN(dt.getTime())) {
      setMode('view');
      return;
    }
    await onSave({ loggedAt: dt.toISOString() });
    setMode('view');
  }, [tsInput, onSave]);

  return (
    <div className="group rounded-md border border-gray-200 bg-white p-3">
      <div className="flex items-start gap-3">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 text-xs text-gray-500">
            <span>{formatTime(entry.loggedAt)}</span>
            {showTaskChip && entry.task && (
              <button
                type="button"
                onClick={() => onTaskClick?.(entry.task!.id)}
                className="truncate rounded bg-blue-50 px-2 py-0.5 text-xs text-blue-700 hover:bg-blue-100"
              >
                {entry.task.title}
              </button>
            )}
          </div>
          {mode === 'edit-body' ? (
            <div className="mt-2 space-y-1">
              <textarea
                value={body}
                onChange={(e) => setBody(e.target.value)}
                rows={3}
                className="w-full rounded-md border border-gray-300 p-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                autoFocus
              />
              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  onClick={() => { setBody(entry.body); setMode('view'); }}
                  className="rounded border border-gray-300 px-2 py-1 text-xs hover:bg-gray-50"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  onClick={saveBody}
                  className="rounded bg-blue-600 px-2 py-1 text-xs text-white hover:bg-blue-700"
                >
                  Save
                </button>
              </div>
            </div>
          ) : mode === 'edit-ts' ? (
            <div className="mt-2 flex items-center gap-2">
              <input
                type="datetime-local"
                value={tsInput}
                onChange={(e) => setTsInput(e.target.value)}
                className="rounded-md border border-gray-300 px-2 py-1 text-xs"
              />
              <button
                type="button"
                onClick={() => setMode('view')}
                className="rounded border border-gray-300 px-2 py-1 text-xs hover:bg-gray-50"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={saveTs}
                className="rounded bg-blue-600 px-2 py-1 text-xs text-white hover:bg-blue-700"
              >
                Save
              </button>
            </div>
          ) : (
            <div className="mt-1 whitespace-pre-wrap break-words text-sm text-gray-800">
              {entry.body}
            </div>
          )}
        </div>
        {mode === 'view' && (
          <WorklogEntryKebab
            onEdit={() => setMode('edit-body')}
            onDelete={onDelete}
            onEditTimestamp={() => setMode('edit-ts')}
          />
        )}
      </div>
    </div>
  );
}
```

Note on markdown: the spec says entries are markdown. This phase renders as `whitespace-pre-wrap` plain-text-ish; a later polish can swap to a markdown renderer if the project already ships one (grep for existing markdown rendering; if none, keep as-is — links still clickable if we wrap later).

- [ ] **Step 4: Create the folder if needed and type-check**

Run:
```bash
mkdir -p frontend/src/components/worklog
cd frontend && pnpm exec tsc --noEmit
```
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/worklog/
git commit -m "feat(frontend): worklog entry card, kebab and add-entry form"
```

---

### Task 13: `WorklogSection` inline component + wire into `TaskEditSheet`

**Files:**
- Create: `frontend/src/components/worklog/WorklogSection.tsx`
- Modify: `frontend/src/components/task/TaskEditSheet.tsx`

- [ ] **Step 1: Build the section**

```tsx
// frontend/src/components/worklog/WorklogSection.tsx
import { useMemo } from 'react';
import { useWorklog } from '@/hooks/use-worklog';
import { AddWorklogEntryForm } from './AddWorklogEntryForm';
import { WorklogEntryCard } from './WorklogEntryCard';

interface Props {
  readonly taskId: string;
}

export function WorklogSection({ taskId }: Props) {
  const filter = useMemo(() => ({ taskIds: [taskId], limit: 50 }), [taskId]);
  const { entries, loading, error, addEntry, updateEntry, deleteEntry } = useWorklog(filter);

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold uppercase tracking-wider text-gray-700">
          Worklog
        </h3>
        <span className="text-xs text-gray-400">
          {entries.length} entr{entries.length === 1 ? 'y' : 'ies'}
        </span>
      </div>

      <AddWorklogEntryForm
        onSubmit={(body) => addEntry({ taskId, body }).then(() => undefined)}
      />

      {error && (
        <div className="rounded-md border border-red-200 bg-red-50 p-2 text-xs text-red-700">
          {error.message}
        </div>
      )}

      {loading && entries.length === 0 ? (
        <p className="text-xs text-gray-400">Loading…</p>
      ) : entries.length === 0 ? (
        <p className="py-4 text-center text-xs text-gray-400">No entries yet.</p>
      ) : (
        <ul className="space-y-2">
          {entries.map((e) => (
            <li key={e.id}>
              <WorklogEntryCard
                entry={e}
                onSave={(patch) => updateEntry({ id: e.id, ...patch }).then(() => undefined)}
                onDelete={() => deleteEntry(e.id).then(() => undefined)}
              />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Mount in `TaskEditSheet`**

In `frontend/src/components/task/TaskEditSheet.tsx`, locate the `notes` field (around lines 60–80 per the earlier scan — confirm with grep). Directly after the element rendering the `notes` textarea, add:

```tsx
import { WorklogSection } from '@/components/worklog/WorklogSection';

// … inside the form, right after the <textarea /> for notes:
{task?.id && (
  <div className="mt-6 border-t border-gray-200 pt-4">
    <WorklogSection taskId={task.id} />
  </div>
)}
```

Use the actual variable that holds the current task (grep shows `task` or similar) — match whichever pattern the existing file uses. If the sheet is open for creating a task (no id yet), skip the section — the `{task?.id && …}` guard handles it.

- [ ] **Step 3: Type-check**

Run: `cd frontend && pnpm exec tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Smoke-test in the browser**

Visit http://localhost:3000/dashboard, open any task card, confirm the "Worklog" section appears below the notes textarea and a new entry can be added.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/worklog/WorklogSection.tsx frontend/src/components/task/TaskEditSheet.tsx
git commit -m "feat(frontend): inline WorklogSection in TaskEditSheet"
```

---

### Task 14: `WorklogPage` + route + sidebar nav

**Files:**
- Create: `frontend/src/pages/WorklogPage.tsx`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/components/layout/Sidebar.tsx`

- [ ] **Step 1: Build the page**

```tsx
// frontend/src/pages/WorklogPage.tsx
import { useMemo, useState, useCallback } from 'react';
import { useWorklog, type WorklogEntry } from '@/hooks/use-worklog';
import { WorklogEntryCard } from '@/components/worklog/WorklogEntryCard';

type Preset = 'today' | '7d' | 'week' | 'month' | 'custom';

function startOfDay(d: Date): Date {
  const n = new Date(d);
  n.setHours(0, 0, 0, 0);
  return n;
}

function addDays(d: Date, n: number): Date {
  const r = new Date(d);
  r.setDate(r.getDate() + n);
  return r;
}

function startOfWeek(d: Date): Date {
  const n = startOfDay(d);
  const day = n.getDay(); // 0 Sun .. 6 Sat
  // ISO-ish: week starts Monday
  const diff = day === 0 ? -6 : 1 - day;
  return addDays(n, diff);
}

function rangeForPreset(p: Preset, customFrom?: string, customTo?: string): { from?: string; to?: string } {
  const today = startOfDay(new Date());
  switch (p) {
    case 'today':
      return { from: today.toISOString(), to: addDays(today, 1).toISOString() };
    case '7d':
      return { from: addDays(today, -6).toISOString(), to: addDays(today, 1).toISOString() };
    case 'week':
      return { from: startOfWeek(today).toISOString(), to: addDays(startOfWeek(today), 7).toISOString() };
    case 'month': {
      const first = new Date(today.getFullYear(), today.getMonth(), 1);
      const nextFirst = new Date(today.getFullYear(), today.getMonth() + 1, 1);
      return { from: first.toISOString(), to: nextFirst.toISOString() };
    }
    case 'custom': {
      const from = customFrom ? new Date(customFrom).toISOString() : undefined;
      const to = customTo ? addDays(new Date(customTo), 1).toISOString() : undefined;
      return { from, to };
    }
  }
}

function formatDayHeader(dayKey: string): string {
  const d = new Date(dayKey);
  return d.toLocaleDateString(undefined, {
    weekday: 'long',
    day: '2-digit',
    month: 'short',
    year: 'numeric',
  });
}

function groupByDay(entries: WorklogEntry[]): Array<{ dayKey: string; items: WorklogEntry[] }> {
  const map = new Map<string, WorklogEntry[]>();
  for (const e of entries) {
    const d = new Date(e.loggedAt);
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
    const arr = map.get(key) ?? [];
    arr.push(e);
    map.set(key, arr);
  }
  return Array.from(map.entries())
    .sort((a, b) => (a[0] < b[0] ? 1 : -1))
    .map(([dayKey, items]) => ({
      dayKey,
      items: items.sort((a, b) => (a.loggedAt < b.loggedAt ? 1 : -1)),
    }));
}

const PRESETS: ReadonlyArray<{ value: Preset; label: string }> = [
  { value: 'today', label: 'Today' },
  { value: '7d', label: 'Last 7 days' },
  { value: 'week', label: 'This week' },
  { value: 'month', label: 'This month' },
  { value: 'custom', label: 'Custom…' },
];

export function WorklogPage() {
  const [preset, setPreset] = useState<Preset>('7d');
  const [customFrom, setCustomFrom] = useState('');
  const [customTo, setCustomTo] = useState('');

  const filter = useMemo(() => {
    const { from, to } = rangeForPreset(preset, customFrom, customTo);
    return { from, to, limit: 500 };
  }, [preset, customFrom, customTo]);

  const { entries, loading, error, updateEntry, deleteEntry } = useWorklog(filter);
  const grouped = useMemo(() => groupByDay(entries), [entries]);

  const openTask = useCallback((taskId: string) => {
    // Dispatches the same event TaskCard uses to open TaskEditSheet, if available.
    // Fallback: navigate to /dashboard with a query param the host recognizes.
    window.dispatchEvent(new CustomEvent('task:open', { detail: { taskId } }));
  }, []);

  return (
    <div className="space-y-4 max-w-4xl">
      <div className="flex flex-wrap items-center gap-2 rounded-md border border-gray-200 bg-white p-3">
        {PRESETS.map((p) => (
          <button
            key={p.value}
            type="button"
            onClick={() => setPreset(p.value)}
            className={`rounded-full px-3 py-1 text-xs font-medium ${
              preset === p.value ? 'bg-blue-600 text-white' : 'bg-gray-100 text-gray-700 hover:bg-gray-200'
            }`}
          >
            {p.label}
          </button>
        ))}
        {preset === 'custom' && (
          <div className="ml-2 flex items-center gap-2">
            <input
              type="date"
              value={customFrom}
              onChange={(e) => setCustomFrom(e.target.value)}
              className="rounded-md border border-gray-300 px-2 py-1 text-xs"
            />
            <span className="text-xs text-gray-500">to</span>
            <input
              type="date"
              value={customTo}
              onChange={(e) => setCustomTo(e.target.value)}
              className="rounded-md border border-gray-300 px-2 py-1 text-xs"
            />
          </div>
        )}
      </div>

      {error && (
        <div className="rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          {error.message}
        </div>
      )}

      {loading && entries.length === 0 ? (
        <p className="text-sm text-gray-500">Loading…</p>
      ) : grouped.length === 0 ? (
        <p className="rounded-md border border-gray-200 bg-white p-6 text-center text-sm text-gray-500">
          No entries for this range.
        </p>
      ) : (
        <div className="space-y-6">
          {grouped.map(({ dayKey, items }) => (
            <section key={dayKey}>
              <h2 className="mb-2 text-sm font-semibold text-gray-700">
                {formatDayHeader(dayKey)} — {items.length} {items.length === 1 ? 'entry' : 'entries'}
              </h2>
              <ul className="space-y-2">
                {items.map((e) => (
                  <li key={e.id}>
                    <WorklogEntryCard
                      entry={e}
                      showTaskChip
                      onTaskClick={openTask}
                      onSave={(patch) => updateEntry({ id: e.id, ...patch }).then(() => undefined)}
                      onDelete={() => deleteEntry(e.id).then(() => undefined)}
                    />
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Add route in `frontend/src/App.tsx`**

Add import near the top:

```tsx
import { WorklogPage } from '@/pages/WorklogPage';
```

Insert a new `<Route>` **after** the existing `/activity` route (i.e., right before the `/dedup` route):

```tsx
<Route
  path="/worklog"
  element={
    <PageLayout title="Worklog">
      <WorklogPage />
    </PageLayout>
  }
/>
```

- [ ] **Step 3: Add sidebar nav item**

In `frontend/src/components/layout/Sidebar.tsx`, find the `navItems` array (around lines 9–56). Insert, between the `/activity` entry and the `/dedup` entry:

```tsx
{
  path: '/worklog',
  label: 'Worklog',
  iconPath:
    'M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z',
},
```

(Pick any existing iconPath style from the file — the above is a calendar/book icon. Match whatever SVG path format the existing entries use.)

- [ ] **Step 4: Type-check + manual check**

```bash
cd frontend && pnpm exec tsc --noEmit
```
Expected: no errors.

Visit http://localhost:3000/worklog in the browser and confirm the page renders, filters toggle, and the empty state appears when the range has no entries.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/pages/WorklogPage.tsx frontend/src/App.tsx frontend/src/components/layout/Sidebar.tsx
git commit -m "feat(frontend): Worklog page with day grouping + sidebar nav"
```

---

### Task 15: Swap activity-timer stop flow to `addWorklogEntry`

**Files:**
- Modify: `frontend/src/hooks/use-activity.ts`

- [ ] **Step 1: Replace the `APPEND_TASK_NOTES_MUTATION` usage with `ADD_WORKLOG_ENTRY_MUTATION`**

In `frontend/src/hooks/use-activity.ts`, around line 84–91, change the mutation constant:

```typescript
const ADD_WORKLOG_ENTRY_MUTATION = `
  mutation AddWorklogEntryFromTimer($taskId: ID!, $body: String!) {
    addWorklogEntry(taskId: $taskId, body: $body) {
      id
    }
  }
`;
```

Around line 191–202, replace the body of `appendTaskNote`:

```typescript
const appendTaskNote = useCallback(
  async (taskId: string, text: string) => {
    const res = await executeAppendNotes({ taskId, body: text });
    if (res.error) {
      throw res.error;
    }
    return res;
  },
  [executeAppendNotes]
);
```

And update the `useMutation` call this hook sits on (the `executeAppendNotes` binding) to reference the new query constant. Grep the file for the existing `useMutation(APPEND_TASK_NOTES_MUTATION)` and replace the argument with `ADD_WORKLOG_ENTRY_MUTATION`.

The public API of the hook stays the same: the `appendTaskNote(taskId, text)` callback keeps its signature, only the underlying mutation changed. Delete the now-unused `APPEND_TASK_NOTES_MUTATION` constant.

- [ ] **Step 2: Type-check + manual check**

```bash
cd frontend && pnpm exec tsc --noEmit
```
Expected: no errors.

In the browser: start the activity timer on a task, type a quick note, stop the timer or press the "Log note" button. Then open the task's Worklog section and confirm the entry appears.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/use-activity.ts
git commit -m "feat(frontend): activity timer quick note creates a worklog entry"
```

---

### Task 16: Update specs

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md`
- Modify: `SPEC_TECHNIQUE.md`

- [ ] **Step 1: Add a "Journal de bord (Worklog)" section to `SPEC_FONCTIONNELLE.md`**

Find an appropriate location (after the section covering tasks or activity tracking) and add:

```markdown
### Journal de bord (Worklog)

- **R-WL-01** : une entrée de worklog est toujours attachée à une tâche (pas d'entrée orpheline).
- **R-WL-02** : l'horodatage (`loggedAt`) est fixé automatiquement à l'instant de création. Il reste modifiable via une action secondaire (menu kebab → « Edit timestamp »).
- **R-WL-03** : le corps d'une entrée est en markdown, non vide après trim, et ne dépasse pas 10 000 caractères.
- **R-WL-04** : la vue `/worklog` est filtrable par plage de dates (presets : aujourd'hui, 7 derniers jours, cette semaine, ce mois, personnalisé) et par tâche/projet. Les entrées sont regroupées par jour, ordre anti-chronologique.
- **R-WL-05** : supprimer une tâche supprime toutes ses entrées de worklog (cascade).
- **R-WL-06** : arrêter le timer d'activité avec une note rapide crée une entrée de worklog associée à la tâche courante (et n'écrit plus dans `tasks.notes`).
```

- [ ] **Step 2: Add a subsection to `SPEC_TECHNIQUE.md`**

Under the data-model or API section, add:

```markdown
### Worklog entries

Table: `worklog_entries`
- `id TEXT PRIMARY KEY` (UUID)
- `user_id TEXT NOT NULL REFERENCES users(id)`
- `task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE`
- `body TEXT NOT NULL`
- `logged_at TEXT NOT NULL` (ISO 8601 UTC)
- `created_at TEXT NOT NULL`
- `updated_at TEXT NOT NULL`

Indexes:
- `idx_worklog_entries_user_logged_at (user_id, logged_at DESC)`
- `idx_worklog_entries_task_logged_at (task_id, logged_at DESC)`

GraphQL surface:
- Query `worklogEntries(filter: WorklogEntryFilterInput): [WorklogEntry!]!`
- Mutation `addWorklogEntry(taskId: ID!, body: String!, loggedAt: DateTime): WorklogEntry!`
- Mutation `updateWorklogEntry(id: ID!, body: String, loggedAt: DateTime): WorklogEntry!`
- Mutation `deleteWorklogEntry(id: ID!): Boolean!`

Limits: default 200 rows, max 1000 per query. Validation: body required, trimmed, max 10 000 characters.

`appendTaskNotes` mutation reste disponible mais n'est plus appelée par le front-end (rétrocompatibilité).
```

- [ ] **Step 3: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "docs(spec): document worklog notes feature"
```

---

### Task 17: Playwright E2E — happy path

**Files:**
- Create: `frontend/e2e/worklog.spec.ts`

- [ ] **Step 1: Write the E2E**

Follow the pattern of existing E2E tests in `frontend/e2e/` (inspect one first). Typical shape:

```typescript
// frontend/e2e/worklog.spec.ts
import { test, expect } from '@playwright/test';

test.describe('Worklog feature', () => {
  test('add an entry from a task and see it on the Worklog page', async ({ page }) => {
    await page.goto('/dashboard');

    // Open the first task
    const firstTask = page.locator('[data-testid="task-card"]').first();
    await firstTask.click();

    // Wait for the task edit sheet
    await expect(page.getByRole('heading', { name: 'Worklog' })).toBeVisible();

    // Log an entry
    const textarea = page.locator('textarea[placeholder*="Log an entry"]');
    await textarea.fill('E2E worklog smoke test');
    await page.getByRole('button', { name: /log entry/i }).click();

    // Confirm it renders in-sheet
    await expect(page.getByText('E2E worklog smoke test')).toBeVisible();

    // Navigate to /worklog and confirm it appears there too
    await page.goto('/worklog');
    await expect(page.getByText('E2E worklog smoke test')).toBeVisible();
  });
});
```

If existing E2E tests add `data-testid` attributes or use different selectors, match that style. Grep `frontend/e2e/*.spec.ts` for conventions before writing.

- [ ] **Step 2: Run**

Run: `cd frontend && pnpm test:e2e worklog`
Expected: test passes against the running backend + frontend.

- [ ] **Step 3: Commit**

```bash
git add frontend/e2e/worklog.spec.ts
git commit -m "test(frontend): E2E for worklog happy path"
```

---

## Self-Review

**Spec coverage:**

| Spec section | Tasks |
|---|---|
| §3 Data model (migration, indexes, cascade) | Tasks 1, 6 (cascade test) |
| §3 Domain type + validation | Task 2 |
| §4 Application repo trait + filter | Task 3 |
| §4 Use cases | Task 4 |
| §5 Infrastructure repo | Tasks 5, 6 |
| §6 GraphQL surface (types, queries, mutations, wiring) | Tasks 7, 8, 9 |
| §7 TaskEditSheet inline section | Tasks 11, 12, 13 |
| §8 Worklog tab/page | Tasks 11, 12, 14 |
| §9 Activity-timer redirect | Task 15 |
| §10 Spec updates | Task 16 |
| §11 Testing (unit + integration + E2E) | Tasks 2, 4, 5, 6, 17 |

**Placeholders:** none — every step shows code, commands, or file content.

**Type consistency:** `WorklogEntry` fields match across domain → repo → GraphQL → frontend hook. `WorklogFilter` (Rust) ↔ `WorklogEntryFilterInput` (GraphQL) ↔ `WorklogFilter` (TS). Mutation names consistent: `addWorklogEntry`, `updateWorklogEntry`, `deleteWorklogEntry`.

---

## Execution

Per user direction ("write a plan and implement without asking"), execution will proceed inline in this session using `superpowers:executing-plans` — no pause for approval.
