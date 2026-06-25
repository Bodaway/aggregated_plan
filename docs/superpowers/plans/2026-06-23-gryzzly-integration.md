# Gryzzly Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pull the Gryzzly catalog (active projects + tasks) into the cockpit read-only and let each aplan task be assigned to a Gryzzly task, with the data model reserved so a future phase can upload tracked hours as declarations.

**Architecture:** Mirror the existing Jira connector pattern — a `GryzzlyClient` trait in `application`, a reqwest `HttpGryzzlyClient` + private mapper/types in `infrastructure`, a new `gryzzly_tasks` cache table with its own repository, a new `Source::Gryzzly` wired through the sync engine and `force_sync`, and two nullable columns on `tasks` (`gryzzly_task_id`, `gryzzly_project_id`) holding the assignment. The catalog refresh uses **soft-prune** (never the empty-wipe delete) and the assignment snapshots the project id for forward-compatibility.

**Tech Stack:** Rust (Axum 0.7, async-graphql 7, sqlx 0.8 runtime queries, reqwest 0.12, tokio, async_trait, thiserror); SQLite; TypeScript/React 18 + Vite, urql, shadcn/ui, Vitest.

## Global Constraints

- **DDD layering (strict):** domain = pure types, zero I/O; application = traits + use cases (depends on domain only); infrastructure = sqlx/reqwest impls; api = GraphQL/Axum. The api layer references connectors only via `Arc<dyn Trait>` + the concrete struct's `::new`.
- **Rust:** no `.unwrap()`/`.expect()` in production paths (reqwest `Client::builder().build()` is the one sanctioned `.expect`, matching `HttpJiraClient`); `thiserror` enums; `Result<T,E>` everywhere; `async_trait` for async traits; map `sqlx::Error` → `RepositoryError::Database(e.to_string())`; map connector failures to `ConnectorError` (`Http{status,message}` / `AuthFailed{service}` / `NetworkError` / `ParseError` — no other variants exist).
- **sqlx:** runtime queries (`sqlx::query(...)`, `.bind(...)`), NOT compile-time `sqlx::query!`.
- **Migrations:** `migrations/sqlite/NNN_snake_case.sql`, zero-padded; **next number is 009**. TEXT UUID ids; INTEGER booleans (`DEFAULT 0/1`); ISO-8601 TEXT timestamps; app supplies timestamps (no `DEFAULT (datetime('now'))` for new tables); `SQLite ADD COLUMN` cannot carry FK/UNIQUE/non-constant default — added columns are plain nullable TEXT.
- **`Source` enum fan-out is all-or-nothing:** adding a variant must touch every exhaustive site in the SAME commit, or `source_from_str` silently coerces unknown strings to `Source::Personal` (a silent-corruption trap). A round-trip test enforces it.
- **Catalog sync must never empty-wipe:** do NOT copy `delete_stale_by_source`'s "empty keep-list deletes all rows" behavior; use soft-disable + skip-prune-on-empty-fetch.
- **Spec maintenance:** update `SPEC_FONCTIONNELLE.md` / `SPEC_TECHNIQUE.md` (French) in the SAME commit as the behavior change (per CLAUDE.md).
- **Tests:** backend tests are inline `#[cfg(test)] mod tests`, integration via in-memory SQLite (`sqlite::memory:`). The `mcp` crate does not compile at HEAD — run the scoped command: `cd backend && cargo test -p domain -p application -p infrastructure -p api`.
- **Commits:** conventional-commit style matching the repo (`feat:`, `test:`, `docs:`…). NO `Co-Authored-By` / `Signed-off-by` footer. Stage only files relevant to the change (never `git add -A`).
- **Ports:** backend 3001, frontend 3000.

---

### Task 1: Migration — `gryzzly_tasks` catalog table + `tasks` assignment columns

**Files:**
- Create: `migrations/sqlite/009_create_gryzzly_catalog.sql`
- Test: `backend/crates/infrastructure/src/database/gryzzly_catalog_repo.rs` (created in Task 3 — the migration is exercised there; for this task, verify via a throwaway check below)

**Interfaces:**
- Produces: table `gryzzly_tasks(id, user_id, gryzzly_task_id, name, gryzzly_project_id, project_name, customer_name, is_active, last_synced_at)` with `UNIQUE(user_id, gryzzly_task_id)`; columns `tasks.gryzzly_task_id TEXT`, `tasks.gryzzly_project_id TEXT`.

- [ ] **Step 1: Write the migration file**

Create `migrations/sqlite/009_create_gryzzly_catalog.sql`:

```sql
-- Gryzzly read-only catalog cache (active projects + their tasks).
-- Refreshed by sync (Source::Gryzzly). Denormalized: the project is "just for info",
-- so project/customer names are copied onto each task row. gryzzly_project_id is kept
-- because a future hours-upload phase needs it to build declarations.
CREATE TABLE gryzzly_tasks (
    id                 TEXT PRIMARY KEY,
    user_id            TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    gryzzly_task_id    TEXT NOT NULL,
    name               TEXT NOT NULL,
    gryzzly_project_id TEXT NOT NULL,
    project_name       TEXT NOT NULL,
    customer_name      TEXT,
    is_active          INTEGER NOT NULL DEFAULT 1,
    last_synced_at     TEXT NOT NULL,
    UNIQUE(user_id, gryzzly_task_id)
);

CREATE INDEX idx_gryzzly_tasks_user_active_project
    ON gryzzly_tasks(user_id, is_active, project_name);

-- Assignment of an aplan task to a Gryzzly task. Both nullable, user-owned, never
-- overwritten by Jira/Excel sync. gryzzly_project_id is snapshotted at assign time so
-- a future declaration push never depends on a live catalog row.
ALTER TABLE tasks ADD COLUMN gryzzly_task_id TEXT;
ALTER TABLE tasks ADD COLUMN gryzzly_project_id TEXT;
```

- [ ] **Step 2: Verify the migration applies cleanly**

Run: `cd backend && cargo test -p infrastructure --no-run` then run any one existing infrastructure DB test that boots an in-memory pool, e.g. `cargo test -p infrastructure --lib -- database::task_repo 2>&1 | tail -20`
Expected: existing tests still PASS (migrations including 009 apply against `sqlite::memory:` with no SQL error). If 009 has a syntax error, the migration step panics and tests fail fast.

- [ ] **Step 3: Commit**

```bash
git add migrations/sqlite/009_create_gryzzly_catalog.sql
git commit -m "feat(db): add gryzzly_tasks catalog table and task assignment columns"
```

---

### Task 2: `GryzzlyClient` connector trait, DTOs, mapper, and HTTP client

**⚠️ PREREQUISITE (manual, do first):** The public Gryzzly API docs are login-gated. Using a real API key, confirm: base path (assumed `https://api.gryzzly.io/v1`), auth (assumed `Authorization: Bearer <key>`), the list-projects and list-tasks endpoint paths, the pagination mechanism (cursor vs offset/page), rate limits, and whether **tasks carry their own active/archived flag**. If active is project-level only, derive a task's `is_active` from *its project being active* in the mapper/sync. Adjust the endpoint paths and pagination loop in Step 5 accordingly. The trait, DTOs, mapper, and error-mapping below are stable regardless of the exact URLs.

**Files:**
- Create: `backend/crates/application/src/services/gryzzly_client.rs`
- Modify: `backend/crates/application/src/services/mod.rs`
- Create: `backend/crates/infrastructure/src/connectors/gryzzly/mod.rs`
- Create: `backend/crates/infrastructure/src/connectors/gryzzly/types.rs`
- Create: `backend/crates/infrastructure/src/connectors/gryzzly/mapper.rs`
- Create: `backend/crates/infrastructure/src/connectors/gryzzly/client.rs`
- Modify: `backend/crates/infrastructure/src/connectors/mod.rs`

**Interfaces:**
- Consumes: `ConnectorError` from `application::errors`.
- Produces:
  - `pub trait GryzzlyClient: Send + Sync` with `async fn fetch_projects(&self, active_only: bool) -> Result<Vec<GryzzlyProject>, ConnectorError>` and `async fn fetch_tasks(&self, project_ids: &[String]) -> Result<Vec<GryzzlyTask>, ConnectorError>`.
  - `pub struct GryzzlyProject { pub id: String, pub name: String, pub customer_name: Option<String>, pub is_active: bool }`
  - `pub struct GryzzlyTask { pub id: String, pub name: String, pub project_id: String, pub is_active: bool }`
  - `pub struct HttpGryzzlyClient` with `pub fn new(base_url: String, api_key: String) -> Self`.

- [ ] **Step 1: Write the trait + DTOs (application)**

Create `backend/crates/application/src/services/gryzzly_client.rs`:

```rust
use async_trait::async_trait;

use crate::errors::ConnectorError;

/// A Gryzzly project (read-only catalog). The project is contextual info only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GryzzlyProject {
    pub id: String,
    pub name: String,
    pub customer_name: Option<String>,
    pub is_active: bool,
}

/// A Gryzzly task — a category of billable work within a project (NOT an aplan task).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GryzzlyTask {
    pub id: String,
    pub name: String,
    pub project_id: String,
    pub is_active: bool,
}

/// Read-only client for the Gryzzly v1 REST API. Named generically (not `…ReadClient`)
/// so a future `push_declaration(...)` write method can be added without renaming.
#[async_trait]
pub trait GryzzlyClient: Send + Sync {
    /// List projects. When `active_only`, archived/closed projects are excluded.
    async fn fetch_projects(&self, active_only: bool) -> Result<Vec<GryzzlyProject>, ConnectorError>;

    /// List tasks belonging to the given project ids.
    async fn fetch_tasks(&self, project_ids: &[String]) -> Result<Vec<GryzzlyTask>, ConnectorError>;
}
```

- [ ] **Step 2: Export the module**

In `backend/crates/application/src/services/mod.rs`, add alongside the existing `jira_client` lines:

```rust
pub mod gryzzly_client;
pub use gryzzly_client::*;
```

- [ ] **Step 3: Write the mapper failing test (infrastructure)**

Create `backend/crates/infrastructure/src/connectors/gryzzly/mapper.rs`:

```rust
use application::services::{GryzzlyProject, GryzzlyTask};

use super::types::{RawGryzzlyProject, RawGryzzlyTask};

/// Map a raw API project DTO into the application-layer DTO.
pub fn map_project(raw: RawGryzzlyProject) -> GryzzlyProject {
    GryzzlyProject {
        id: raw.id,
        name: raw.name,
        customer_name: raw.customer_name,
        // If the API has no per-project archived flag, default to active.
        is_active: !raw.archived.unwrap_or(false),
    }
}

/// Map a raw API task DTO into the application-layer DTO.
/// `project_active` lets the caller fold project-level activeness in when the
/// task API exposes no per-task flag.
pub fn map_task(raw: RawGryzzlyTask, project_active: bool) -> GryzzlyTask {
    GryzzlyTask {
        id: raw.id,
        name: raw.name,
        project_id: raw.project_id,
        is_active: project_active && !raw.archived.unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::gryzzly::types::{RawGryzzlyProject, RawGryzzlyTask};

    #[test]
    fn maps_active_project() {
        let raw = RawGryzzlyProject { id: "p1".into(), name: "Website".into(), customer_name: Some("Acme".into()), archived: Some(false) };
        let p = map_project(raw);
        assert_eq!(p.id, "p1");
        assert_eq!(p.name, "Website");
        assert_eq!(p.customer_name.as_deref(), Some("Acme"));
        assert!(p.is_active);
    }

    #[test]
    fn archived_project_is_inactive() {
        let raw = RawGryzzlyProject { id: "p2".into(), name: "Old".into(), customer_name: None, archived: Some(true) };
        assert!(!map_project(raw).is_active);
    }

    #[test]
    fn task_inactive_when_project_inactive() {
        let raw = RawGryzzlyTask { id: "t1".into(), name: "Dev".into(), project_id: "p1".into(), archived: Some(false) };
        assert!(!map_task(raw, false).is_active);
    }
}
```

- [ ] **Step 4: Write the raw API DTOs (private to the connector)**

Create `backend/crates/infrastructure/src/connectors/gryzzly/types.rs`. **Adjust field names + `#[serde(rename_all)]` to the real API after the prerequisite probe.**

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RawGryzzlyProject {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub customer_name: Option<String>,
    #[serde(default)]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RawGryzzlyTask {
    pub id: String,
    pub name: String,
    pub project_id: String,
    #[serde(default)]
    pub archived: Option<bool>,
}

/// Envelope for a paginated list response. Replace with the real pagination shape
/// (cursor token vs offset) confirmed in the prerequisite probe.
#[derive(Debug, Clone, Deserialize)]
pub struct RawList<T> {
    pub data: Vec<T>,
}
```

- [ ] **Step 5: Run the mapper test to confirm it passes**

Run: `cd backend && cargo test -p infrastructure --lib -- connectors::gryzzly::mapper`
Expected: 3 tests PASS.

- [ ] **Step 6: Write the HTTP client**

Create `backend/crates/infrastructure/src/connectors/gryzzly/client.rs`. Model the reqwest setup and status→error mapping on `infrastructure/src/connectors/jira/client.rs`. **Replace endpoint paths + pagination with the real ones from the prerequisite probe.**

```rust
use std::time::Duration;

use application::errors::ConnectorError;
use application::services::{GryzzlyClient, GryzzlyProject, GryzzlyTask};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};

use super::mapper::{map_project, map_task};
use super::types::{RawGryzzlyProject, RawGryzzlyTask, RawList};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const SERVICE: &str = "gryzzly";

pub struct HttpGryzzlyClient {
    http: Client,
    base_url: String,
    api_key: String,
}

impl HttpGryzzlyClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("failed to build reqwest client");
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, ConnectorError> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| ConnectorError::NetworkError(e.to_string()))?;

        match resp.status() {
            s if s.is_success() => resp
                .json::<T>()
                .await
                .map_err(|e| ConnectorError::ParseError(e.to_string())),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Err(ConnectorError::AuthFailed { service: SERVICE.to_string() })
            }
            // Rate limited: surface as Http{429,..}. A bounded Retry-After-aware
            // retry can be added here once the real limit is known.
            status => {
                let code = status.as_u16();
                let body = resp.text().await.unwrap_or_default();
                Err(ConnectorError::Http { status: code, message: body })
            }
        }
    }
}

#[async_trait]
impl GryzzlyClient for HttpGryzzlyClient {
    async fn fetch_projects(&self, active_only: bool) -> Result<Vec<GryzzlyProject>, ConnectorError> {
        // Endpoint + pagination are placeholders pending the prerequisite probe.
        let page: RawList<RawGryzzlyProject> = self.get_json("projects?limit=1000").await?;
        let mut projects: Vec<GryzzlyProject> = page.data.into_iter().map(map_project).collect();
        if active_only {
            projects.retain(|p| p.is_active);
        }
        Ok(projects)
    }

    async fn fetch_tasks(&self, project_ids: &[String]) -> Result<Vec<GryzzlyTask>, ConnectorError> {
        let mut out = Vec::new();
        for project_id in project_ids {
            let page: RawList<RawGryzzlyTask> =
                self.get_json(&format!("tasks?project_id={}&limit=1000", project_id)).await?;
            // project_active is true here because callers pass only active project ids;
            // if the task API has its own flag, map_task already ANDs it in.
            out.extend(page.data.into_iter().map(|t| map_task(t, true)));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_trims_trailing_slash_and_keeps_key() {
        let c = HttpGryzzlyClient::new("https://api.gryzzly.io/v1/".into(), "secret".into());
        assert_eq!(c.base_url, "https://api.gryzzly.io/v1");
        assert_eq!(c.api_key, "secret");
    }
}
```

- [ ] **Step 7: Wire the connector modules**

Create `backend/crates/infrastructure/src/connectors/gryzzly/mod.rs`:

```rust
pub mod client;
pub mod mapper;
pub mod types;

pub use client::HttpGryzzlyClient;
```

In `backend/crates/infrastructure/src/connectors/mod.rs`, add `pub mod gryzzly;` next to the existing connector modules.

- [ ] **Step 8: Build + run connector tests**

Run: `cd backend && cargo test -p application -p infrastructure --lib -- gryzzly`
Expected: mapper (3) + client (1) tests PASS; both crates compile.

- [ ] **Step 9: Commit**

```bash
git add backend/crates/application/src/services/gryzzly_client.rs \
        backend/crates/application/src/services/mod.rs \
        backend/crates/infrastructure/src/connectors/gryzzly/ \
        backend/crates/infrastructure/src/connectors/mod.rs
git commit -m "feat(gryzzly): add GryzzlyClient trait, DTOs, mapper and HTTP client"
```

---

### Task 3: `GryzzlyCatalogRepository` trait + SQLite implementation

**Files:**
- Create: `backend/crates/domain/src/types/gryzzly.rs`
- Modify: `backend/crates/domain/src/types/mod.rs`
- Create: `backend/crates/application/src/repositories/gryzzly_catalog_repository.rs`
- Modify: `backend/crates/application/src/repositories/mod.rs`
- Create: `backend/crates/infrastructure/src/database/gryzzly_catalog_repo.rs`
- Modify: `backend/crates/infrastructure/src/database/mod.rs`

**Interfaces:**
- Consumes: migration from Task 1; `UserId`, `RepositoryError`.
- Produces:
  - `pub struct GryzzlyCatalogEntry { pub id: Uuid, pub user_id: UserId, pub gryzzly_task_id: String, pub name: String, pub gryzzly_project_id: String, pub project_name: String, pub customer_name: Option<String>, pub is_active: bool, pub last_synced_at: DateTime<Utc> }`
  - `pub trait GryzzlyCatalogRepository: Send + Sync` with `upsert`, `soft_prune_missing`, `list_active`, `find_by_gryzzly_task_id` (signatures in Step 2).

- [ ] **Step 1: Define the domain entity**

Create `backend/crates/domain/src/types/gryzzly.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::UserId;

/// One row of the Gryzzly catalog cache (a Gryzzly task + denormalized project info).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GryzzlyCatalogEntry {
    pub id: Uuid,
    pub user_id: UserId,
    pub gryzzly_task_id: String,
    pub name: String,
    pub gryzzly_project_id: String,
    pub project_name: String,
    pub customer_name: Option<String>,
    pub is_active: bool,
    pub last_synced_at: DateTime<Utc>,
}
```

Add `pub mod gryzzly;` and `pub use gryzzly::*;` to `backend/crates/domain/src/types/mod.rs` (match how existing type modules are declared there).

- [ ] **Step 2: Define the repository trait**

Create `backend/crates/application/src/repositories/gryzzly_catalog_repository.rs`:

```rust
use async_trait::async_trait;
use domain::types::{GryzzlyCatalogEntry, UserId};

use crate::errors::RepositoryError;

#[async_trait]
pub trait GryzzlyCatalogRepository: Send + Sync {
    /// Insert or update one catalog row, keyed on (user_id, gryzzly_task_id).
    /// Re-activates a previously soft-disabled row.
    async fn upsert(&self, entry: &GryzzlyCatalogEntry) -> Result<(), RepositoryError>;

    /// Soft-disable (is_active = 0) every row for the user whose gryzzly_task_id is
    /// NOT in `keep_ids`. NEVER hard-deletes. Returns the number of rows disabled.
    async fn soft_prune_missing(&self, user_id: UserId, keep_ids: &[String]) -> Result<u64, RepositoryError>;

    /// Active rows for the picker, optionally filtered by a name/project search and a
    /// project-name filter, ordered by project_name then name, capped at `limit`.
    async fn list_active(
        &self,
        user_id: UserId,
        search: Option<&str>,
        project_filter: Option<&str>,
        limit: i64,
    ) -> Result<Vec<GryzzlyCatalogEntry>, RepositoryError>;

    /// Look up one row by gryzzly_task_id regardless of is_active (so a stale/disabled
    /// assignment still resolves for display + future push). None if absent.
    async fn find_by_gryzzly_task_id(
        &self,
        user_id: UserId,
        gryzzly_task_id: &str,
    ) -> Result<Option<GryzzlyCatalogEntry>, RepositoryError>;
}
```

Add `pub mod gryzzly_catalog_repository;` and `pub use gryzzly_catalog_repository::*;` to `backend/crates/application/src/repositories/mod.rs`.

- [ ] **Step 3: Write the failing repo integration tests**

Create `backend/crates/infrastructure/src/database/gryzzly_catalog_repo.rs` with the test module first (model the test harness on the existing `task_repo.rs` tests — same in-memory pool + migrate helper). Use the existing test helper that the other `*_repo.rs` files use to build a migrated `SqlitePool` (find it in `task_repo.rs` tests, e.g. `setup_pool()` / `test_pool()`), plus seeding a `users` row (FK).

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    // Reuse the same in-memory-pool + user-seed helpers the other repo tests use.
    // (Mirror task_repo.rs's test setup: create pool, run migrations, insert a user.)

    fn entry(user_id: Uuid, gid: &str, active: bool) -> GryzzlyCatalogEntry {
        GryzzlyCatalogEntry {
            id: Uuid::new_v4(),
            user_id,
            gryzzly_task_id: gid.to_string(),
            name: format!("Task {gid}"),
            gryzzly_project_id: "proj-1".to_string(),
            project_name: "Website".to_string(),
            customer_name: Some("Acme".to_string()),
            is_active: active,
            last_synced_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn upsert_creates_then_updates() {
        let (pool, user_id) = setup_with_user().await;
        let repo = SqliteGryzzlyCatalogRepository::new(pool);
        repo.upsert(&entry(user_id, "g1", true)).await.unwrap();

        let mut e = entry(user_id, "g1", true);
        e.name = "Renamed".into();
        repo.upsert(&e).await.unwrap();

        let found = repo.find_by_gryzzly_task_id(user_id, "g1").await.unwrap().unwrap();
        assert_eq!(found.name, "Renamed");
        let active = repo.list_active(user_id, None, None, 100).await.unwrap();
        assert_eq!(active.len(), 1, "upsert must not duplicate on (user_id, gryzzly_task_id)");
    }

    #[tokio::test]
    async fn soft_prune_disables_missing_but_keeps_row() {
        let (pool, user_id) = setup_with_user().await;
        let repo = SqliteGryzzlyCatalogRepository::new(pool);
        repo.upsert(&entry(user_id, "g1", true)).await.unwrap();
        repo.upsert(&entry(user_id, "g2", true)).await.unwrap();

        let disabled = repo.soft_prune_missing(user_id, &["g1".to_string()]).await.unwrap();
        assert_eq!(disabled, 1);

        // g2 disabled but still present (so an assignment to it still resolves).
        assert!(repo.list_active(user_id, None, None, 100).await.unwrap().iter().all(|e| e.gryzzly_task_id == "g1"));
        let g2 = repo.find_by_gryzzly_task_id(user_id, "g2").await.unwrap().unwrap();
        assert!(!g2.is_active);
    }

    #[tokio::test]
    async fn list_active_filters_by_search() {
        let (pool, user_id) = setup_with_user().await;
        let repo = SqliteGryzzlyCatalogRepository::new(pool);
        repo.upsert(&entry(user_id, "g1", true)).await.unwrap();
        let hits = repo.list_active(user_id, Some("Task g1"), None, 100).await.unwrap();
        assert_eq!(hits.len(), 1);
        let misses = repo.list_active(user_id, Some("zzz"), None, 100).await.unwrap();
        assert!(misses.is_empty());
    }
}
```

> Replace `setup_with_user()` with the project's actual in-memory-pool helper found in `task_repo.rs`'s test module; it must run `sqlx::migrate!` (so table `gryzzly_tasks` exists) and insert a `users` row whose id is returned.

- [ ] **Step 4: Run tests to confirm they fail (struct/impl not defined)**

Run: `cd backend && cargo test -p infrastructure --lib -- gryzzly_catalog_repo`
Expected: FAIL to compile — `SqliteGryzzlyCatalogRepository` undefined.

- [ ] **Step 5: Implement the repository**

Prepend the implementation above the test module in `backend/crates/infrastructure/src/database/gryzzly_catalog_repo.rs`:

```rust
use application::errors::RepositoryError;
use application::repositories::GryzzlyCatalogRepository;
use async_trait::async_trait;
use domain::types::{GryzzlyCatalogEntry, UserId};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct SqliteGryzzlyCatalogRepository {
    pool: SqlitePool,
}

impl SqliteGryzzlyCatalogRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn map_row(row: &sqlx::sqlite::SqliteRow) -> Result<GryzzlyCatalogEntry, RepositoryError> {
    let id: String = row.try_get("id").map_err(|e| RepositoryError::Database(e.to_string()))?;
    let user_id: String = row.try_get("user_id").map_err(|e| RepositoryError::Database(e.to_string()))?;
    let last_synced_at: String = row.try_get("last_synced_at").map_err(|e| RepositoryError::Database(e.to_string()))?;
    let is_active: i64 = row.try_get("is_active").map_err(|e| RepositoryError::Database(e.to_string()))?;
    Ok(GryzzlyCatalogEntry {
        id: Uuid::parse_str(&id).map_err(|e| RepositoryError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id).map_err(|e| RepositoryError::Database(e.to_string()))?,
        gryzzly_task_id: row.try_get("gryzzly_task_id").map_err(|e| RepositoryError::Database(e.to_string()))?,
        name: row.try_get("name").map_err(|e| RepositoryError::Database(e.to_string()))?,
        gryzzly_project_id: row.try_get("gryzzly_project_id").map_err(|e| RepositoryError::Database(e.to_string()))?,
        project_name: row.try_get("project_name").map_err(|e| RepositoryError::Database(e.to_string()))?,
        customer_name: row.try_get("customer_name").ok(),
        is_active: is_active != 0,
        last_synced_at: chrono::DateTime::parse_from_rfc3339(&last_synced_at)
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .with_timezone(&chrono::Utc),
    })
}

#[async_trait]
impl GryzzlyCatalogRepository for SqliteGryzzlyCatalogRepository {
    async fn upsert(&self, entry: &GryzzlyCatalogEntry) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO gryzzly_tasks
                (id, user_id, gryzzly_task_id, name, gryzzly_project_id, project_name, customer_name, is_active, last_synced_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_id, gryzzly_task_id) DO UPDATE SET
                name = excluded.name,
                gryzzly_project_id = excluded.gryzzly_project_id,
                project_name = excluded.project_name,
                customer_name = excluded.customer_name,
                is_active = excluded.is_active,
                last_synced_at = excluded.last_synced_at",
        )
        .bind(entry.id.to_string())
        .bind(entry.user_id.to_string())
        .bind(&entry.gryzzly_task_id)
        .bind(&entry.name)
        .bind(&entry.gryzzly_project_id)
        .bind(&entry.project_name)
        .bind(&entry.customer_name)
        .bind(if entry.is_active { 1i64 } else { 0i64 })
        .bind(entry.last_synced_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn soft_prune_missing(&self, user_id: UserId, keep_ids: &[String]) -> Result<u64, RepositoryError> {
        if keep_ids.is_empty() {
            // Defensive: never mass-disable on an empty keep-list. The sync use case
            // already skips pruning on an empty fetch; this is a second guard.
            return Ok(0);
        }
        let placeholders = keep_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "UPDATE gryzzly_tasks SET is_active = 0
             WHERE user_id = ? AND is_active = 1 AND gryzzly_task_id NOT IN ({placeholders})"
        );
        let mut q = sqlx::query(&sql).bind(user_id.to_string());
        for id in keep_ids {
            q = q.bind(id);
        }
        let res = q.execute(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(res.rows_affected())
    }

    async fn list_active(
        &self,
        user_id: UserId,
        search: Option<&str>,
        project_filter: Option<&str>,
        limit: i64,
    ) -> Result<Vec<GryzzlyCatalogEntry>, RepositoryError> {
        let mut sql = String::from(
            "SELECT * FROM gryzzly_tasks WHERE user_id = ? AND is_active = 1",
        );
        if search.is_some() {
            sql.push_str(" AND (name LIKE ? OR project_name LIKE ?)");
        }
        if project_filter.is_some() {
            sql.push_str(" AND project_name = ?");
        }
        sql.push_str(" ORDER BY project_name, name LIMIT ?");

        let mut q = sqlx::query(&sql).bind(user_id.to_string());
        if let Some(s) = search {
            let pat = format!("%{s}%");
            q = q.bind(pat.clone()).bind(pat);
        }
        if let Some(p) = project_filter {
            q = q.bind(p.to_string());
        }
        q = q.bind(limit);

        let rows = q.fetch_all(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string()))?;
        rows.iter().map(map_row).collect()
    }

    async fn find_by_gryzzly_task_id(
        &self,
        user_id: UserId,
        gryzzly_task_id: &str,
    ) -> Result<Option<GryzzlyCatalogEntry>, RepositoryError> {
        let row = sqlx::query("SELECT * FROM gryzzly_tasks WHERE user_id = ? AND gryzzly_task_id = ?")
            .bind(user_id.to_string())
            .bind(gryzzly_task_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        row.as_ref().map(map_row).transpose()
    }
}
```

Add `pub mod gryzzly_catalog_repo;` to `backend/crates/infrastructure/src/database/mod.rs` and re-export `SqliteGryzzlyCatalogRepository` matching the pattern used for the other repos there.

- [ ] **Step 6: Run tests to confirm they pass**

Run: `cd backend && cargo test -p domain -p application -p infrastructure --lib -- gryzzly`
Expected: all Task 2 + Task 3 tests PASS.

- [ ] **Step 7: Commit**

```bash
git add backend/crates/domain/src/types/gryzzly.rs backend/crates/domain/src/types/mod.rs \
        backend/crates/application/src/repositories/gryzzly_catalog_repository.rs \
        backend/crates/application/src/repositories/mod.rs \
        backend/crates/infrastructure/src/database/gryzzly_catalog_repo.rs \
        backend/crates/infrastructure/src/database/mod.rs
git commit -m "feat(gryzzly): add catalog entity and GryzzlyCatalogRepository with soft-prune"
```

---

### Task 4: Add `Source::Gryzzly` across every enum site (+ round-trip test)

This is the all-or-nothing fan-out. The `sync_source` match gets a temporary **no-op arm** (matching the existing `Obsidian`/`Personal` no-op arms); Task 5 replaces it with the real dispatch.

**Files:**
- Modify: `backend/crates/domain/src/types/common.rs:14-20`
- Modify: `backend/crates/infrastructure/src/database/conversions.rs:5-24`
- Modify: `backend/crates/api/src/graphql/types/enums.rs:6-36`
- Modify: `backend/crates/application/src/use_cases/sync.rs` (the `sync_source` match)
- Modify: `backend/crates/cli/src/cli.rs` (`SourceArg`)
- Modify: `backend/crates/cli/src/commands.rs` (`SourceArg`→`SourceGql` map)
- Modify: `backend/crates/cli/graphql/schema.graphql` (`enum SourceGql`)
- Modify: `backend/crates/mcp/src/server.rs` (`parse_source` — for completeness; mcp is out of scope to build)

**Interfaces:**
- Produces: `Source::Gryzzly`, `SourceGql::Gryzzly`, DB string `"gryzzly"`, CLI `--source gryzzly`.

- [ ] **Step 1: Write the round-trip failing test**

Add to `backend/crates/infrastructure/src/database/conversions.rs` (inside a `#[cfg(test)] mod tests`, creating it if absent):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use domain::types::Source;

    #[test]
    fn source_round_trips_every_variant() {
        for s in [Source::Jira, Source::Excel, Source::Obsidian, Source::Personal, Source::Outlook, Source::Gryzzly] {
            assert_eq!(source_from_str(source_to_str(s)), s, "round-trip failed for {s:?}");
        }
    }

    #[test]
    fn gryzzly_maps_to_its_own_string() {
        assert_eq!(source_to_str(Source::Gryzzly), "gryzzly");
        assert_eq!(source_from_str("gryzzly"), Source::Gryzzly);
    }
}
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cd backend && cargo test -p infrastructure --lib -- conversions::tests`
Expected: FAIL to compile — `Source::Gryzzly` does not exist yet.

- [ ] **Step 3: Add the enum variant (domain)**

In `backend/crates/domain/src/types/common.rs`, add `Gryzzly` to `enum Source`:

```rust
pub enum Source {
    Jira,
    Excel,
    Obsidian,
    Personal,
    Outlook,
    Gryzzly,
}
```

- [ ] **Step 4: Add both conversion arms**

In `backend/crates/infrastructure/src/database/conversions.rs`, add to `source_to_str` `Source::Gryzzly => "gryzzly",` and to `source_from_str` `"gryzzly" => Source::Gryzzly,` (before the `_ => Source::Personal` fallback).

- [ ] **Step 5: Add the GraphQL enum + both From arms**

In `backend/crates/api/src/graphql/types/enums.rs`: add `Gryzzly,` to `SourceGql`; add `types::Source::Gryzzly => SourceGql::Gryzzly,` to the first `From`; add `SourceGql::Gryzzly => types::Source::Gryzzly,` to the second.

- [ ] **Step 6: Add the no-op sync arm (temporary seam)**

In `backend/crates/application/src/use_cases/sync.rs`, in the exhaustive `sync_source` match, add an arm mirroring the existing `Obsidian`/`Personal` no-op arms (return their same no-op `SyncStatus`/`SyncResult`). Copy the exact shape of the `Source::Obsidian =>` arm and label it `Source::Gryzzly`. *(Task 5 replaces this.)*

- [ ] **Step 7: Update the CLI source enum + map + schema**

- `backend/crates/cli/src/cli.rs`: add `Gryzzly` to `enum SourceArg`.
- `backend/crates/cli/src/commands.rs`: add `SourceArg::Gryzzly => force_sync::SourceGql::GRYZZLY,` to the match.
- `backend/crates/cli/graphql/schema.graphql`: add `GRYZZLY` to `enum SourceGql`.
- `backend/crates/mcp/src/server.rs`: add `"gryzzly" => Ok(Source::Gryzzly),` to `parse_source` (the wildcard makes it compile either way; add for correctness).

- [ ] **Step 8: Regenerate CLI graphql-client + build**

Run: `cd backend && cargo build -p domain -p application -p infrastructure -p api -p cli 2>&1 | tail -30`
Expected: all five crates compile. The CLI's generated `force_sync::SourceGql::GRYZZLY` resolves from the updated `schema.graphql` (graphql-client codegen runs at build time via the `#[derive(GraphQLQuery)]` macro). If `GRYZZLY` is reported missing, the schema edit didn't take — re-check `schema.graphql`.

- [ ] **Step 9: Run the round-trip test**

Run: `cd backend && cargo test -p infrastructure --lib -- conversions::tests`
Expected: both tests PASS.

- [ ] **Step 10: Commit**

```bash
git add backend/crates/domain/src/types/common.rs \
        backend/crates/infrastructure/src/database/conversions.rs \
        backend/crates/api/src/graphql/types/enums.rs \
        backend/crates/application/src/use_cases/sync.rs \
        backend/crates/cli/src/cli.rs backend/crates/cli/src/commands.rs \
        backend/crates/cli/graphql/schema.graphql backend/crates/mcp/src/server.rs
git commit -m "feat(gryzzly): register Source::Gryzzly across enum, conversions, GraphQL and CLI"
```

---

### Task 5: `sync_gryzzly` use case + SyncContext wiring + force_sync client build

**Files:**
- Modify: `backend/crates/application/src/use_cases/sync.rs` (add `gryzzly_client` to `SyncContext`; add `sync_gryzzly`; replace the Task 4 no-op arm; add `sync_all` dispatch)
- Modify: `backend/crates/api/src/graphql/mutation.rs` (build the client in `force_sync`, pass into `SyncContext`)
- Modify: `SPEC_FONCTIONNELLE.md`, `SPEC_TECHNIQUE.md` (French — new source + catalog)

**Interfaces:**
- Consumes: `GryzzlyClient` (Task 2), `GryzzlyCatalogRepository` (Task 3), `Source::Gryzzly` (Task 4), `SyncStatusRepository`, `ConfigRepository`.
- Produces: `pub async fn sync_gryzzly(client: &dyn GryzzlyClient, catalog_repo: &dyn GryzzlyCatalogRepository, sync_repo: &dyn SyncStatusRepository, user_id: UserId, now: DateTime<Utc>) -> Result<SyncResult, AppError>`; `SyncContext.gryzzly_client: Option<&dyn GryzzlyClient>` + `catalog_repo` access.

> **Note on repo access:** `SyncContext` (sync.rs:11) holds `&dyn` repos. Add `pub gryzzly_catalog_repo: &'a dyn GryzzlyCatalogRepository` AND `pub gryzzly_client: Option<&'a dyn GryzzlyClient>`. Both must be added to every `SyncContext { .. }` literal (the one in `force_sync`, plus any in tests) — the compiler lists them all.

- [ ] **Step 1: Write the failing `sync_gryzzly` tests**

In `backend/crates/application/src/use_cases/sync.rs` test module, add (use a small in-test fake `GryzzlyClient` and the in-memory catalog repo via the infrastructure test helper if reachable; if the application crate can't depend on infrastructure in tests, assert against a mock catalog repo implementing the trait inline):

```rust
#[cfg(test)]
mod gryzzly_tests {
    use super::*;
    // A fake client whose fetches are programmable.
    struct FakeGryzzly { projects: Vec<GryzzlyProject>, tasks: Vec<GryzzlyTask> }
    #[async_trait::async_trait]
    impl GryzzlyClient for FakeGryzzly {
        async fn fetch_projects(&self, _active_only: bool) -> Result<Vec<GryzzlyProject>, ConnectorError> { Ok(self.projects.clone()) }
        async fn fetch_tasks(&self, _ids: &[String]) -> Result<Vec<GryzzlyTask>, ConnectorError> { Ok(self.tasks.clone()) }
    }

    #[tokio::test]
    async fn empty_fetch_skips_prune() {
        // catalog has g1 active; a fetch returning zero tasks must NOT disable g1.
        // Arrange an in-memory catalog repo seeded with g1, an empty FakeGryzzly,
        // call sync_gryzzly, then assert g1 is still active.
        // (Use the infrastructure SqliteGryzzlyCatalogRepository test helper.)
    }

    #[tokio::test]
    async fn upserts_and_soft_prunes() {
        // catalog seeded with g1,g2 active; fetch returns only g1 (in one active project);
        // after sync: g1 active+updated, g2 soft-disabled (still present).
    }
}
```

> Fill the two test bodies using `SqliteGryzzlyCatalogRepository` + the in-memory pool helper (same as Task 3) and an in-memory `SyncStatusRepository`. Assert via `find_by_gryzzly_task_id`. Keep them concrete — no `todo!()`.

- [ ] **Step 2: Run to confirm they fail**

Run: `cd backend && cargo test -p application --lib -- gryzzly_tests`
Expected: FAIL — `sync_gryzzly` undefined.

- [ ] **Step 3: Implement `sync_gryzzly`**

Add to `sync.rs`, modeled structurally on `sync_jira` (sync.rs:42 — mark Syncing → fetch → write → mark Success), but writing to the catalog repo with the empty-fetch guard:

```rust
pub async fn sync_gryzzly(
    client: &dyn GryzzlyClient,
    catalog_repo: &dyn GryzzlyCatalogRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    now: DateTime<Utc>,
) -> Result<SyncResult, AppError> {
    // Mark Syncing (mirror sync_jira's status handling).
    mark_syncing(sync_repo, Source::Gryzzly, user_id).await?;

    let projects = match client.fetch_projects(true).await {
        Ok(p) => p,
        Err(e) => {
            update_sync_error(sync_repo, Source::Gryzzly, user_id, &e.to_string()).await?;
            return Err(AppError::Connector { connector_source: Source::Gryzzly, message: e.to_string() });
        }
    };
    let project_ids: Vec<String> = projects.iter().map(|p| p.id.clone()).collect();
    let tasks = match client.fetch_tasks(&project_ids).await {
        Ok(t) => t,
        Err(e) => {
            update_sync_error(sync_repo, Source::Gryzzly, user_id, &e.to_string()).await?;
            return Err(AppError::Connector { connector_source: Source::Gryzzly, message: e.to_string() });
        }
    };

    // EMPTY-FETCH GUARD: never prune the catalog on an empty fetch (transient API
    // hiccup must not wipe assignments' lookup rows).
    if tasks.is_empty() {
        update_sync_error(sync_repo, Source::Gryzzly, user_id, "empty catalog fetch — skipping prune").await?;
        return Ok(SyncResult::default());
    }

    let by_project: std::collections::HashMap<&str, &GryzzlyProject> =
        projects.iter().map(|p| (p.id.as_str(), p)).collect();

    let mut keep_ids = Vec::with_capacity(tasks.len());
    let mut upserted = 0u32;
    for t in &tasks {
        let proj = by_project.get(t.project_id.as_str());
        let entry = GryzzlyCatalogEntry {
            id: Uuid::new_v4(),
            user_id,
            gryzzly_task_id: t.id.clone(),
            name: t.name.clone(),
            gryzzly_project_id: t.project_id.clone(),
            project_name: proj.map(|p| p.name.clone()).unwrap_or_default(),
            customer_name: proj.and_then(|p| p.customer_name.clone()),
            is_active: t.is_active,
            last_synced_at: now,
        };
        catalog_repo.upsert(&entry).await.map_err(AppError::from)?;
        keep_ids.push(t.id.clone());
        upserted += 1;
    }
    let pruned = catalog_repo.soft_prune_missing(user_id, &keep_ids).await.map_err(AppError::from)? as u32;

    mark_success(sync_repo, Source::Gryzzly, user_id, now).await?;
    Ok(SyncResult { tasks_created: upserted, tasks_removed: pruned, ..SyncResult::default() })
}
```

> Use the EXACT helper names that `sync_jira` uses for marking Syncing/Success/error (read sync.rs:42-207 and reuse them — the names above are illustrative). `SyncResult`'s field names: reuse `tasks_created`/`tasks_removed` to count catalog upserts/prunes for this source, and add a code comment that for `Source::Gryzzly` these count catalog rows, not aplan tasks.

- [ ] **Step 4: Wire SyncContext + replace the no-op arm + sync_all**

- Add to `struct SyncContext<'a>`: `pub gryzzly_client: Option<&'a dyn GryzzlyClient>,` and `pub gryzzly_catalog_repo: &'a dyn GryzzlyCatalogRepository,`.
- Replace the Task-4 no-op `Source::Gryzzly` arm in `sync_source` with:

```rust
Source::Gryzzly => match ctx.gryzzly_client {
    Some(client) => sync_gryzzly(client, ctx.gryzzly_catalog_repo, ctx.sync_repo, user_id, Utc::now()).await.map(status_from_result),
    None => {
        update_sync_error(ctx.sync_repo, Source::Gryzzly, user_id, "Not configured").await?;
        Ok(/* the Not-configured SyncStatus, matching how sync_source returns it for other unconfigured sources */)
    }
},
```

- In `sync_all`, add a Gryzzly dispatch block alongside the others (call `sync_source(ctx, Source::Gryzzly, user_id)` or the same per-source helper the file uses).

> Match the exact return type/shape that the surrounding `sync_source` arms produce (`SyncStatus` vs `SyncResult`). Read the neighbouring `Source::Jira` arm and mirror its return precisely.

- [ ] **Step 5: Build the client in `force_sync` + extend SyncContext literal**

In `backend/crates/api/src/graphql/mutation.rs`, near the existing jira-client build (~line 349), add (mirroring the `Option<Arc<dyn JiraClient>>` pattern):

```rust
use infrastructure::connectors::gryzzly::HttpGryzzlyClient; // near line 14

// inside force_sync, alongside the jira_client build:
let gryzzly_api_key = config_repo.get(*user_id, "gryzzly.api_key").await.ok().flatten();
let gryzzly_base_url = config_repo
    .get(*user_id, "gryzzly.base_url").await.ok().flatten()
    .unwrap_or_else(|| "https://api.gryzzly.io/v1".to_string());
let gryzzly_client: Option<Arc<dyn GryzzlyClient>> = match gryzzly_api_key {
    Some(k) if !k.is_empty() => Some(Arc::new(HttpGryzzlyClient::new(gryzzly_base_url, k))),
    _ => None,
};
```

Construct the concrete `SqliteGryzzlyCatalogRepository` from the pool (same place the other repos are built) and add `gryzzly_client: gryzzly_client.as_deref(), gryzzly_catalog_repo: &gryzzly_catalog_repo,` to the `SyncContext { .. }` literal (~line 369-378). The compiler will flag any other `SyncContext` literal (tests) — add the fields there too (pass `None` / a stub repo).

- [ ] **Step 6: Update the French specs**

Add a section to `SPEC_TECHNIQUE.md` (data model + sync source `gryzzly`, the `gryzzly_tasks` table, config keys `gryzzly.api_key`/`gryzzly.base_url`) and to `SPEC_FONCTIONNELLE.md` (the cockpit can synchronise le catalogue Gryzzly — projets actifs + tâches — en lecture seule). Keep it concise and in French.

- [ ] **Step 7: Run tests + full scoped build**

Run: `cd backend && cargo test -p domain -p application -p infrastructure -p api --lib -- gryzzly`
Then: `cd backend && cargo build -p api -p cli 2>&1 | tail -20`
Expected: `sync_gryzzly` tests PASS (empty-fetch keeps rows; soft-prune disables-but-keeps); api + cli compile.

- [ ] **Step 8: Commit**

```bash
git add backend/crates/application/src/use_cases/sync.rs \
        backend/crates/api/src/graphql/mutation.rs \
        SPEC_TECHNIQUE.md SPEC_FONCTIONNELLE.md
git commit -m "feat(gryzzly): sync the catalog via force_sync with empty-fetch-safe soft-prune"
```

---

### Task 6: Add `gryzzly_task_id` + `gryzzly_project_id` to the Task domain type & repo

**Files:**
- Modify: `backend/crates/domain/src/types/task.rs:8-45` (struct + any `make_test_task` fixture)
- Modify: `backend/crates/infrastructure/src/database/task_repo.rs` (`map_task_row` + `save` at 404-440 + test fixtures)
- (Other `Task { .. }` literal sites the compiler flags — sync/dedup/dashboard tests)

**Interfaces:**
- Produces: `Task.gryzzly_task_id: Option<String>`, `Task.gryzzly_project_id: Option<String>`, persisted to/from the `tasks` columns added in Task 1.

- [ ] **Step 1: Write the failing round-trip test**

In `task_repo.rs` test module add:

```rust
#[tokio::test]
async fn task_persists_gryzzly_assignment() {
    let (pool, user_id) = setup_with_user().await; // existing helper
    let repo = SqliteTaskRepository::new(pool);
    let mut t = make_task(user_id); // existing fixture
    t.gryzzly_task_id = Some("g-123".into());
    t.gryzzly_project_id = Some("p-9".into());
    repo.save(&t).await.unwrap();

    let loaded = repo.find_by_id(t.id).await.unwrap().unwrap();
    assert_eq!(loaded.gryzzly_task_id.as_deref(), Some("g-123"));
    assert_eq!(loaded.gryzzly_project_id.as_deref(), Some("p-9"));
}
```

- [ ] **Step 2: Run to confirm it fails**

Run: `cd backend && cargo test -p infrastructure --lib -- task_persists_gryzzly_assignment`
Expected: FAIL to compile — no such fields.

- [ ] **Step 3: Add the struct fields**

In `backend/crates/domain/src/types/task.rs`, after `pub occurrence_date: Option<NaiveDate>,` (line 42) add:

```rust
    /// Assigned Gryzzly task id (user-owned; never overwritten by sync). Optional.
    pub gryzzly_task_id: Option<String>,
    /// Snapshot of the assigned Gryzzly task's project id, captured at assign time so a
    /// future hours-upload can build a declaration without a live catalog row.
    pub gryzzly_project_id: Option<String>,
```

Update the in-file `make_test_task()` fixture (and any other `Task { .. }` in this file's tests) to set both to `None`.

- [ ] **Step 4: Extend `map_task_row` and `save`**

In `task_repo.rs` `map_task_row`, add (use the tolerant accessor like `notes`/`delegated_to`):

```rust
        gryzzly_task_id: row.try_get("gryzzly_task_id").ok().flatten(),
        gryzzly_project_id: row.try_get("gryzzly_project_id").ok().flatten(),
```

In `save` (line 406-437): append `gryzzly_task_id, gryzzly_project_id` to the INSERT column list, add two more `?` to the VALUES list, and add the two binds at the matching position (append them just before `created_at` in BOTH the column list and the binds so positions stay aligned):

```rust
        // ...after .bind(task.occurrence_date.map(...))
        .bind(&task.gryzzly_task_id)
        .bind(&task.gryzzly_project_id)
        // then .bind(task.created_at...) .bind(task.updated_at...)
```

Column list becomes `... recurrence_id, occurrence_date, gryzzly_task_id, gryzzly_project_id, created_at, updated_at)` with the placeholder count raised from 29 to 31. Update both `make_task()` (line ~592) and the inline `Task` fixture (line ~968) to set the two new fields to `None`.

- [ ] **Step 5: Fix all other `Task { .. }` literals the compiler flags**

Run: `cd backend && cargo build -p domain -p infrastructure -p application -p api 2>&1 | grep -A2 "missing field" | head -40`
Add `gryzzly_task_id: None, gryzzly_project_id: None,` to each flagged literal (sync mapper that builds Tasks from Jira, dedup tests, dashboard tests, etc.).

- [ ] **Step 6: Run the round-trip test + scoped build**

Run: `cd backend && cargo test -p infrastructure --lib -- task_persists_gryzzly_assignment` then `cargo build -p domain -p application -p infrastructure -p api 2>&1 | tail -10`
Expected: test PASSES; all four crates compile.

- [ ] **Step 7: Commit**

```bash
git add backend/crates/domain/src/types/task.rs \
        backend/crates/infrastructure/src/database/task_repo.rs
# plus any other files touched in Step 5:
git add -p   # stage only the gryzzly-field additions in flagged literal sites
git commit -m "feat(gryzzly): add gryzzly_task_id/gryzzly_project_id to Task and persistence"
```

---

### Task 7: `assignGryzzlyTask` — application use case + GraphQL mutation

**Files:**
- Create: `backend/crates/application/src/use_cases/gryzzly_assignment.rs`
- Modify: `backend/crates/application/src/use_cases/mod.rs`
- Modify: `backend/crates/api/src/graphql/mutation.rs` (new resolver)
- Modify: `SPEC_FONCTIONNELLE.md`, `SPEC_TECHNIQUE.md` (French — assignation)

**Interfaces:**
- Consumes: `TaskRepository`, `GryzzlyCatalogRepository`, `Task` fields (Task 6).
- Produces: `pub async fn assign_gryzzly_task(task_repo: &dyn TaskRepository, catalog_repo: &dyn GryzzlyCatalogRepository, task_id: TaskId, gryzzly_task_id: Option<String>) -> Result<Task, AppError>`.

- [ ] **Step 1: Write the failing use-case tests**

Create `backend/crates/application/src/use_cases/gryzzly_assignment.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn assign_snapshots_project_id() {
        // Seed catalog g1 -> project p1; seed a task; assign g1.
        // Expect task.gryzzly_task_id == Some("g1") AND task.gryzzly_project_id == Some("p1").
    }

    #[tokio::test]
    async fn assign_unknown_task_is_rejected() {
        // Assigning a gryzzly_task_id absent from the catalog returns an AppError (not a silent set).
    }

    #[tokio::test]
    async fn clearing_assignment_nulls_both_fields() {
        // assign_gryzzly_task(.., None) sets both gryzzly_task_id and gryzzly_project_id to None.
    }
}
```

> Fill bodies with the in-memory `SqliteTaskRepository` + `SqliteGryzzlyCatalogRepository` helpers. Concrete asserts, no `todo!()`.

- [ ] **Step 2: Run to confirm failure**

Run: `cd backend && cargo test -p application --lib -- gryzzly_assignment`
Expected: FAIL — function undefined.

- [ ] **Step 3: Implement the use case**

```rust
use domain::types::{Task, TaskId};

use crate::errors::AppError;
use crate::repositories::{GryzzlyCatalogRepository, TaskRepository};

/// Assign (or clear, with `None`) the Gryzzly task for an aplan task. On assign, the
/// project id is snapshotted from the catalog so a future push never needs a live row.
pub async fn assign_gryzzly_task(
    task_repo: &dyn TaskRepository,
    catalog_repo: &dyn GryzzlyCatalogRepository,
    task_id: TaskId,
    gryzzly_task_id: Option<String>,
) -> Result<Task, AppError> {
    let mut task = task_repo
        .find_by_id(task_id)
        .await?
        .ok_or_else(|| AppError::NotFound { entity: "task".to_string(), id: task_id.to_string() })?;

    match gryzzly_task_id {
        Some(gid) => {
            let entry = catalog_repo
                .find_by_gryzzly_task_id(task.user_id, &gid)
                .await?
                .ok_or_else(|| AppError::Validation { message: format!("unknown gryzzly task: {gid}") })?;
            task.gryzzly_task_id = Some(entry.gryzzly_task_id);
            task.gryzzly_project_id = Some(entry.gryzzly_project_id);
        }
        None => {
            task.gryzzly_task_id = None;
            task.gryzzly_project_id = None;
        }
    }
    task_repo.save(&task).await?;
    Ok(task)
}
```

> Use the actual `AppError` variant names present in `application/src/errors.rs` (e.g. `NotFound`, `Validation` — confirm and substitute). Add `pub mod gryzzly_assignment; pub use gryzzly_assignment::*;` to `use_cases/mod.rs`.

- [ ] **Step 4: Add the GraphQL mutation**

In `backend/crates/api/src/graphql/mutation.rs`, add a resolver mirroring how `update_task` obtains repos from `ctx.data`:

```rust
async fn assign_gryzzly_task(
    &self,
    ctx: &Context<'_>,
    task_id: ID,
    gryzzly_task_id: Option<ID>,
) -> async_graphql::Result<TaskGql> {
    let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
    let catalog_repo = ctx.data::<Arc<dyn GryzzlyCatalogRepository>>()?;
    let tid = Uuid::parse_str(task_id.as_str())?;
    let gid = gryzzly_task_id.map(|g| g.to_string());
    let task = gryzzly_assignment::assign_gryzzly_task(task_repo.as_ref(), catalog_repo.as_ref(), tid, gid).await?;
    Ok(TaskGql::from(task))
}
```

> Ensure `Arc<dyn GryzzlyCatalogRepository>` is registered in the schema `Data` in `backend/crates/api/src/main.rs` (where the other repos are added with `.data(...)`). Add it there.

- [ ] **Step 5: Update French specs**

Document `assignGryzzlyTask(taskId, gryzzlyTaskId)` in `SPEC_TECHNIQUE.md` and the "assigner une tâche à une tâche Gryzzly" behaviour in `SPEC_FONCTIONNELLE.md`.

- [ ] **Step 6: Run tests + build**

Run: `cd backend && cargo test -p application --lib -- gryzzly_assignment` then `cargo build -p api 2>&1 | tail -10`
Expected: 3 use-case tests PASS; api compiles.

- [ ] **Step 7: Commit**

```bash
git add backend/crates/application/src/use_cases/gryzzly_assignment.rs \
        backend/crates/application/src/use_cases/mod.rs \
        backend/crates/api/src/graphql/mutation.rs backend/crates/api/src/main.rs \
        SPEC_TECHNIQUE.md SPEC_FONCTIONNELLE.md
git commit -m "feat(gryzzly): add assignGryzzlyTask mutation with project-id snapshot"
```

---

### Task 8: `gryzzlyTasks` query + Task GraphQL field with stale states

**Files:**
- Modify: `backend/crates/api/src/graphql/query.rs` (or the query resolver module — confirm the path used by existing queries like `tasks`)
- Create/Modify: `backend/crates/api/src/graphql/types/gryzzly.rs` (GraphQL output types)
- Modify: `backend/crates/api/src/graphql/types/task.rs` (assigned-task field)

**Interfaces:**
- Consumes: `GryzzlyCatalogRepository` (Task 3), `Task.gryzzly_task_id` (Task 6).
- Produces:
  - Query `gryzzlyTasks(search: String, projectFilter: String, limit: Int = 100): [GryzzlyTaskGql!]!`
  - Field on `TaskGql`: `gryzzlyTask: AssignedGryzzlyTaskGql` (nullable) — `{ gryzzlyTaskId, name, projectName, stale }`.

- [ ] **Step 1: Define GraphQL output types**

Create `backend/crates/api/src/graphql/types/gryzzly.rs`:

```rust
use async_graphql::SimpleObject;
use domain::types::GryzzlyCatalogEntry;

#[derive(SimpleObject)]
pub struct GryzzlyTaskGql {
    pub gryzzly_task_id: String,
    pub name: String,
    pub gryzzly_project_id: String,
    pub project_name: String,
    pub customer_name: Option<String>,
}

impl From<GryzzlyCatalogEntry> for GryzzlyTaskGql {
    fn from(e: GryzzlyCatalogEntry) -> Self {
        Self {
            gryzzly_task_id: e.gryzzly_task_id,
            name: e.name,
            gryzzly_project_id: e.gryzzly_project_id,
            project_name: e.project_name,
            customer_name: e.customer_name,
        }
    }
}

/// The assignment as seen on a Task. `stale` is true when the catalog row is
/// inactive (state 2) or missing entirely (state 3 — name/project_name null).
#[derive(SimpleObject)]
pub struct AssignedGryzzlyTaskGql {
    pub gryzzly_task_id: String,
    pub name: Option<String>,
    pub project_name: Option<String>,
    pub stale: bool,
}
```

Register `pub mod gryzzly;` in the graphql `types/mod.rs`.

- [ ] **Step 2: Write the failing Task-field resolver test**

Add an integration test (in the api crate's test module that drives the schema, or a focused resolver unit test) asserting the three states. If the api crate lacks a schema-test harness, assert the pure mapping in a small helper instead and test that:

```rust
// helper under test: resolves (assignment id, Option<catalog entry>) -> AssignedGryzzlyTaskGql
#[test]
fn stale_states() {
    // state 1: active entry -> stale=false, name Some
    // state 2: inactive entry -> stale=true, name Some
    // state 3: None entry     -> stale=true, name None
}
```

- [ ] **Step 3: Implement the Task field**

In `backend/crates/api/src/graphql/types/task.rs`, add a resolver to the `#[Object] impl TaskGql`:

```rust
/// The assigned Gryzzly task (with project context), or null if unassigned.
async fn gryzzly_task(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<AssignedGryzzlyTaskGql>> {
    let Some(gid) = self.0.gryzzly_task_id.clone() else { return Ok(None) };
    let repo = ctx.data::<Arc<dyn GryzzlyCatalogRepository>>()?;
    let entry = repo.find_by_gryzzly_task_id(self.0.user_id, &gid).await?;
    Ok(Some(match entry {
        Some(e) if e.is_active => AssignedGryzzlyTaskGql { gryzzly_task_id: gid, name: Some(e.name), project_name: Some(e.project_name), stale: false },
        Some(e) => AssignedGryzzlyTaskGql { gryzzly_task_id: gid, name: Some(e.name), project_name: Some(e.project_name), stale: true },
        None => AssignedGryzzlyTaskGql { gryzzly_task_id: gid, name: None, project_name: None, stale: true },
    }))
}
```

> Factor the `match entry { .. }` into a small free function `resolve_assigned(gid, Option<GryzzlyCatalogEntry>) -> AssignedGryzzlyTaskGql` so the Step-2 test can call it directly without a live DB.

- [ ] **Step 4: Implement the `gryzzlyTasks` query**

In the query resolver module, add:

```rust
async fn gryzzly_tasks(
    &self,
    ctx: &Context<'_>,
    search: Option<String>,
    project_filter: Option<String>,
    #[graphql(default = 100)] limit: i32,
) -> async_graphql::Result<Vec<GryzzlyTaskGql>> {
    let repo = ctx.data::<Arc<dyn GryzzlyCatalogRepository>>()?;
    let user_id = /* obtain current user id the same way `tasks` query does */;
    let rows = repo.list_active(user_id, search.as_deref(), project_filter.as_deref(), limit as i64).await?;
    Ok(rows.into_iter().map(GryzzlyTaskGql::from).collect())
}
```

> Obtain `user_id` exactly how the existing `tasks` query obtains it (auth middleware injected default user). Reuse that helper.

- [ ] **Step 5: Run tests + build**

Run: `cd backend && cargo test -p api --lib -- gryzzly` then `cargo build -p api 2>&1 | tail -10`
Expected: stale-state test PASSES; api compiles. Optionally smoke-test via the running server: `cargo run -p api` then a GraphQL query `{ gryzzlyTasks(limit:5){ name projectName } }`.

- [ ] **Step 6: Commit**

```bash
git add backend/crates/api/src/graphql/types/gryzzly.rs \
        backend/crates/api/src/graphql/types/task.rs \
        backend/crates/api/src/graphql/types/mod.rs \
        backend/crates/api/src/graphql/query.rs
git commit -m "feat(gryzzly): expose gryzzlyTasks query and assigned-task field with stale states"
```

---

### Task 9: Frontend — Gryzzly task assignment picker

**Files:**
- Create: `frontend/src/lib/gryzzly-picker-options.ts`
- Create: `frontend/src/lib/gryzzly-picker-options.test.ts`
- Create: `frontend/src/hooks/use-gryzzly-tasks.ts`
- Create: `frontend/src/components/gryzzly/GryzzlyTaskPicker.tsx`
- Modify: the task detail/edit component (locate via `rg -l "updateTask|UpdateTask" frontend/src/components`) to mount `<GryzzlyTaskPicker>`

**Interfaces:**
- Consumes: `gryzzlyTasks` query + `assignGryzzlyTask` mutation (Tasks 7-8); the task's `gryzzlyTask { gryzzlyTaskId name projectName stale }` field.
- Produces: a picker that lists active Gryzzly tasks grouped by project, always including the currently-assigned (possibly stale) task so it can be cleared.

- [ ] **Step 1: Write the failing merge-logic test**

The repo already uses pure-util + Vitest (`task-picker-sort.ts`/`.test.ts`). Mirror it. Create `frontend/src/lib/gryzzly-picker-options.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { buildPickerOptions } from "./gryzzly-picker-options";

const active = [
  { gryzzlyTaskId: "g1", name: "Dev", projectName: "Website" },
  { gryzzlyTaskId: "g2", name: "Specs", projectName: "Website" },
];

describe("buildPickerOptions", () => {
  it("returns active options grouped/sorted by project then name", () => {
    const opts = buildPickerOptions(active, null);
    expect(opts.map((o) => o.gryzzlyTaskId)).toEqual(["g1", "g2"]);
  });

  it("includes a stale assigned task not present in the active list", () => {
    const assigned = { gryzzlyTaskId: "g9", name: "Old", projectName: "Archived", stale: true };
    const opts = buildPickerOptions(active, assigned);
    const g9 = opts.find((o) => o.gryzzlyTaskId === "g9");
    expect(g9).toBeTruthy();
    expect(g9?.stale).toBe(true);
  });

  it("does not duplicate the assigned task when it is already active", () => {
    const assigned = { gryzzlyTaskId: "g1", name: "Dev", projectName: "Website", stale: false };
    const opts = buildPickerOptions(active, assigned);
    expect(opts.filter((o) => o.gryzzlyTaskId === "g1")).toHaveLength(1);
  });
});
```

- [ ] **Step 2: Run to confirm failure**

Run: `cd frontend && pnpm test -- gryzzly-picker-options`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the pure util**

Create `frontend/src/lib/gryzzly-picker-options.ts`:

```ts
export interface GryzzlyOption {
  gryzzlyTaskId: string;
  name: string;
  projectName: string;
  stale?: boolean;
}

export interface AssignedGryzzlyTask {
  gryzzlyTaskId: string;
  name: string | null;
  projectName: string | null;
  stale: boolean;
}

/** Active options sorted by project then name, plus the currently-assigned task
 *  (even if inactive/missing from the active list) so the user can see & clear it. */
export function buildPickerOptions(
  active: GryzzlyOption[],
  assigned: AssignedGryzzlyTask | null,
): GryzzlyOption[] {
  const sorted = [...active].sort(
    (a, b) => a.projectName.localeCompare(b.projectName) || a.name.localeCompare(b.name),
  );
  if (!assigned) return sorted;
  if (sorted.some((o) => o.gryzzlyTaskId === assigned.gryzzlyTaskId)) return sorted;
  return [
    ...sorted,
    {
      gryzzlyTaskId: assigned.gryzzlyTaskId,
      name: assigned.name ?? "(unknown Gryzzly task)",
      projectName: assigned.projectName ?? "(archived)",
      stale: true,
    },
  ];
}
```

- [ ] **Step 4: Run to confirm pass**

Run: `cd frontend && pnpm test -- gryzzly-picker-options`
Expected: 3 tests PASS.

- [ ] **Step 5: Add the urql hook + component**

Create `frontend/src/hooks/use-gryzzly-tasks.ts` (urql `useQuery` for `gryzzlyTasks`, returning `{ options, fetching }`), and `frontend/src/components/gryzzly/GryzzlyTaskPicker.tsx` — a shadcn `Command`/`Popover` combobox that: takes `taskId` + current `assigned`, calls `buildPickerOptions`, renders grouped by `projectName` with a stale badge, runs `assignGryzzlyTask` mutation on select, and has a "Clear assignment" row that calls the mutation with `gryzzlyTaskId: null`. Match the urql `useMutation` + shadcn usage of an existing control (e.g. the delegated_to or status control in the task edit component).

```tsx
// GryzzlyTaskPicker.tsx — shape (match existing shadcn combobox usage in the codebase):
// const [{ data }] = useGryzzlyTasks(search);
// const options = buildPickerOptions(data?.gryzzlyTasks ?? [], assigned);
// const [, assign] = useMutation(ASSIGN_GRYZZLY_TASK);
// onSelect(id) => assign({ taskId, gryzzlyTaskId: id });
// onClear()    => assign({ taskId, gryzzlyTaskId: null });
```

- [ ] **Step 6: Mount it in the task detail/edit surface**

Locate the task edit component (`cd frontend && rg -l "delegated_to|delegatedTo|UpdateTaskInput" src/components`) and add `<GryzzlyTaskPicker taskId={task.id} assigned={task.gryzzlyTask ?? null} />` near the existing metadata controls. Add `gryzzlyTask { gryzzlyTaskId name projectName stale }` to the task GraphQL fragment/query the component uses.

- [ ] **Step 7: Verify build + tests**

Run: `cd frontend && pnpm test -- gryzzly && pnpm build 2>&1 | tail -20`
Expected: util tests PASS; production build succeeds with no type errors.

- [ ] **Step 8: Commit**

```bash
git add frontend/src/lib/gryzzly-picker-options.ts frontend/src/lib/gryzzly-picker-options.test.ts \
        frontend/src/hooks/use-gryzzly-tasks.ts frontend/src/components/gryzzly/GryzzlyTaskPicker.tsx
git add -p   # stage the task-edit component mount + fragment change
git commit -m "feat(gryzzly): add task assignment picker (active + stale-assigned options)"
```

---

## Final verification

- [ ] Backend scoped tests green: `cd backend && cargo test -p domain -p application -p infrastructure -p api`
- [ ] Backend lint: `cd backend && cargo clippy -p domain -p application -p infrastructure -p api 2>&1 | tail -20`
- [ ] CLI builds (graphql codegen): `cd backend && cargo build -p cli`
- [ ] Frontend: `cd frontend && pnpm test && pnpm build`
- [ ] Manual: set `gryzzly.api_key` in Settings → `aplan sync --source gryzzly` → assign a Gryzzly task to an aplan task in the UI → reload and confirm the assignment + project show; archive that Gryzzly task in Gryzzly, re-sync, confirm the assignment shows as **stale** (not vanished).
- [ ] Specs (French) updated for the source, catalog table, config keys, and assignment field.
```
