# Gryzzly Sync State + Terminated Projects Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the sync indicator tell the truth about Gryzzly (never-configured vs expired session vs real failure), and carry terminated Gryzzly projects into the catalog marked with a `[terminé]` badge instead of letting them vanish.

**Architecture:** `SyncSourceStatus` gains a real `NotConfigured` variant so an unconfigured connector stops masquerading as a failure. `gryzzly_tasks` gains a `project_status` column, fed from the project map `sync_gryzzly` already builds; `map_task` stops folding project activeness into `is_active` so a closed project and a deleted task become distinguishable. One migration carries both schema changes.

**Tech Stack:** Rust (domain/application/infrastructure/api), sqlx 0.8 + SQLite, async-graphql 7, React 18 + Vite, Vitest + React Testing Library, Tailwind.

**Spec:** `docs/superpowers/specs/2026-08-10-gryzzly-sync-state-and-terminated-projects-design.md`

**Branch:** continues `gryzzly-internal-api-auth` (migration 015 already there; this adds 016).

## Global Constraints

- **Read-only integration.** No writes to Gryzzly, ever. Only `view/projects.list`, `expandedProjectMetrics.get`, `self.getIdentity` may be called.
- **`project_status` values:** `active` | `done`, from the API verbatim. **NULL means unknown → treat as active.** A row written by the old `scripts/gryzzly/import_catalog.py` predates the column and must never render as terminated.
- **No CHECK constraint on `project_status`.** The API owns its vocabulary; migrations 013 and 015 are the record of what enumerating an external vocabulary in a CHECK costs.
- **`is_active` on a catalog row means the task's own liveness only** — `completed_at.is_none() && deleted_at.is_none()`. It no longer folds the project's state.
- **Badge copy is exactly `terminé`** (lowercase, accented), muted grey styling, mirroring the existing `stale` badge idiom in `GryzzlyTaskPicker`.
- **The badge goes on the picker's project *group header*, not on every row** — the picker groups by project, so one badge per group. Plus the collapsed trigger line when the assigned task's project is done.
- **`stale` and `terminé` are independent** and can both show: `stale` = row gone/disabled in the catalog, `terminé` = project closed in Gryzzly.
- DDD layering: traits in `application`, impls in `infrastructure`. No `.unwrap()` in production paths.
- Backend tests inline `#[cfg(test)] mod tests`. Frontend tests colocated `*.test.tsx`. TDD: failing test first.
- Run scoped: `cargo test -p domain -p application -p infrastructure -p api`; frontend `cd frontend && pnpm test`.
- Commit messages: plain imperative subject, no `Co-Authored-By`, no `Signed-off-by`.

## File Structure

| File | Responsibility |
|---|---|
| `migrations/sqlite/016_add_project_status_and_not_configured.sql` | **Create** — column add + `sync_status.status` CHECK rebuild |
| `backend/crates/domain/src/types/common.rs` | **Modify** — `SyncSourceStatus::NotConfigured` |
| `backend/crates/domain/src/types/gryzzly.rs` | **Modify** — `GryzzlyCatalogEntry.project_status` |
| `backend/crates/infrastructure/src/database/conversions.rs` | **Modify** — `not_configured` string mapping |
| `backend/crates/infrastructure/src/database/connection.rs` | **Modify** — status-variant regression test |
| `backend/crates/infrastructure/src/database/gryzzly_catalog_repo.rs` | **Modify** — `map_row` + `upsert` carry `project_status` |
| `backend/crates/application/src/services/gryzzly_client.rs` | **Modify** — `GryzzlyProject.status` |
| `backend/crates/application/src/use_cases/sync.rs` | **Modify** — `NotConfigured` branches; fetch all projects; write `project_status` |
| `backend/crates/infrastructure/src/connectors/gryzzly/mapper.rs` | **Modify** — `map_project` carries status; `map_task` unfolds |
| `backend/crates/infrastructure/src/connectors/gryzzly/client.rs` | **Modify** — exclude deleted projects; drop `project_active` arg |
| `backend/crates/api/src/graphql/types/enums.rs` | **Modify** — `SyncSourceStatusGql::NotConfigured` |
| `backend/crates/api/src/graphql/types/gryzzly.rs` | **Modify** — `projectStatus` on both GQL types + `resolve_assigned` |
| `backend/crates/cli/graphql/schema.graphql` | **Modify** — hand-edit the two additions |
| `frontend/src/components/gryzzly/TerminatedBadge.tsx` | **Create** — the shared `[terminé]` pill |
| `frontend/src/lib/gryzzly-picker-options.ts` | **Modify** — `projectStatus` passthrough |
| `frontend/src/hooks/use-gryzzly-tasks.ts` | **Modify** — select `projectStatus` |
| `frontend/src/components/gryzzly/GryzzlyTaskPicker.tsx` | **Modify** — badge on group header + trigger |
| `frontend/src/components/sync/SyncStatusBar.tsx` | **Modify** — not-configured state, inline reason, Reconnecter |
| `frontend/src/components/sync/SyncStatusBar.test.tsx` | **Create** — none exists today |
| `SPEC_TECHNIQUE.md`, `SPEC_FONCTIONNELLE.md` | **Modify** — document both changes |

---

### Task 1: Migration 016 and the `NotConfigured` status

The migration must land with (or before) the code that writes `not_configured`, or the CHECK rejects it. Both schema changes ship in one file per the spec.

**Files:**
- Create: `migrations/sqlite/016_add_project_status_and_not_configured.sql`
- Modify: `backend/crates/domain/src/types/common.rs` (`SyncSourceStatus`, ~line 80)
- Modify: `backend/crates/infrastructure/src/database/conversions.rs` (~lines 228-247)
- Modify: `backend/crates/api/src/graphql/types/enums.rs` (~lines 262-280)
- Modify: `backend/crates/application/src/use_cases/sync.rs` (the four `else` branches + a new helper)
- Modify: `backend/crates/infrastructure/src/database/connection.rs` (regression test)

**Interfaces:**
- Consumes: nothing
- Produces:
  - `domain::types::SyncSourceStatus::NotConfigured`
  - `sync_status_to_str(NotConfigured) == "not_configured"` and the reverse
  - `api::graphql::types::enums::SyncSourceStatusGql::NotConfigured` (SDL: `NOT_CONFIGURED`)
  - `gryzzly_tasks.project_status` column (used by Task 2)

- [ ] **Step 1: Write the migration**

Create `migrations/sqlite/016_add_project_status_and_not_configured.sql`:

```sql
-- Two changes, one migration, both needed by the terminated-project work.
--
--   1. `gryzzly_tasks.project_status` — lets a task on a CLOSED project be told apart
--      from a task DELETED in Gryzzly. Before this column both rendered identically
--      as `stale`, because `sync_gryzzly` fetched active projects only and the rest
--      were soft-pruned out of sight.
--   2. `sync_status.status` must admit `not_configured`.

-- ── 1. project_status ───────────────────────────────────────────────────────────
--
-- Values come from the Gryzzly API verbatim: `active` or `done`. Deliberately NO
-- CHECK: 013 and 015 are this repo's record of what enumerating someone else's
-- vocabulary in a CHECK costs, and the API is free to add a status tomorrow.
--
-- NULL for every pre-existing row, and NULL reads as "unknown, treat as active".
-- Rows imported by scripts/gryzzly/import_catalog.py predate the column and must
-- not suddenly render as terminated.
ALTER TABLE gryzzly_tasks ADD COLUMN project_status TEXT;

-- ── 2. sync_status.status must admit `not_configured` ───────────────────────────
--
-- `update_sync_error(..., "Not configured")` recorded an unconfigured connector as
-- `status = error` with the state carried as prose, so the UI painted a red Error
-- dot for something merely unconfigured — indistinguishable from a real failure.
-- `SyncSourceStatus` gains a fifth variant and this CHECK has to follow.
--
-- THIRD instance of this bug class in this schema: `alerts.alert_type` (013),
-- `sync_status.source` (015), and now `sync_status.status`. The pair of tests in
-- `database::connection::migration_tests` now enumerates BOTH columns' enums.
--
-- SQLite cannot ALTER a CHECK, so this is the documented rebuild
-- (https://sqlite.org/lang_altertable.html#otherxform), identical in shape to 015:
--
--   * steps 2 and 11 (BEGIN / COMMIT) belong to sqlx's per-migration transaction;
--   * steps 1 and 12 (`PRAGMA foreign_keys` off/on) are absent — a documented no-op
--     inside a transaction, and `sync_status` is a child table only (nothing has a
--     foreign key TO it), so neither the DROP nor the RENAME can break one;
--   * step 3 (inventory): one table, no explicit index (only the PRIMARY KEY and
--     UNIQUE autoindexes, which the new table recreates itself), no trigger, no
--     view — steps 8 and 9 are empty;
--   * step 10 (`PRAGMA foreign_key_check`) cannot fail a migration from inside SQL,
--     so it stays asserted in the test suite.
--
-- The `source` CHECK keeps 015's six values. Column lists are written out on both
-- sides of the INSERT rather than `SELECT *`.
CREATE TABLE new_sync_status (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source TEXT NOT NULL
        CHECK (source IN ('jira', 'outlook', 'excel', 'obsidian', 'personal', 'gryzzly')),
    last_sync_at TEXT,
    status TEXT NOT NULL DEFAULT 'idle'
        CHECK (status IN ('idle', 'syncing', 'success', 'error', 'not_configured')),
    error_message TEXT,
    UNIQUE(user_id, source)
);

INSERT INTO new_sync_status
    (id, user_id, source, last_sync_at, status, error_message)
SELECT
     id, user_id, source, last_sync_at, status, error_message
FROM sync_status;

DROP TABLE sync_status;

ALTER TABLE new_sync_status RENAME TO sync_status;
```

- [ ] **Step 2: Write the failing tests**

In `backend/crates/infrastructure/src/database/connection.rs`, inside `mod migration_tests`, add:

```rust
    /// Sibling of `sync_status_accepts_every_source_variant`. That test guards the
    /// `source` column only — `status` carries the same enumerated-CHECK trap, and
    /// this codebase has now hit that trap three times (alerts.alert_type in 013,
    /// sync_status.source in 015, sync_status.status in 016). Both tests must exist
    /// or the next added variant ships broken.
    #[tokio::test]
    async fn sync_status_accepts_every_status_variant() {
        use domain::types::SyncSourceStatus;

        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT INTO users (id, email, name) VALUES ('u1', 'u@example.com', 'U')")
            .execute(&pool)
            .await
            .unwrap();

        // Exhaustive by construction: adding a variant without listing it here is a
        // compile error, and listing it without widening the CHECK fails below.
        let all = [
            SyncSourceStatus::Idle,
            SyncSourceStatus::Syncing,
            SyncSourceStatus::Success,
            SyncSourceStatus::Error,
            SyncSourceStatus::NotConfigured,
        ];
        for (i, status) in all.into_iter().enumerate() {
            let as_str = crate::database::conversions::sync_status_to_str(status);
            let res = sqlx::query(
                "INSERT INTO sync_status (id, user_id, source, status) VALUES (?, 'u1', ?, ?)",
            )
            .bind(format!("st-{i}"))
            // A distinct source per row: (user_id, source) is UNIQUE.
            .bind(["jira", "outlook", "excel", "obsidian", "personal"][i])
            .bind(as_str)
            .execute(&pool)
            .await;
            assert!(
                res.is_ok(),
                "sync_status.status rejects {as_str:?}: {:?}",
                res.err()
            );
        }
    }

    /// The new column must exist and default to NULL, since NULL is the documented
    /// "unknown, treat as active" state for rows predating it.
    #[tokio::test]
    async fn gryzzly_tasks_has_a_nullable_project_status() {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM pragma_table_info('gryzzly_tasks') WHERE name = 'project_status'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, 1, "project_status column missing");

        let notnull: (i64,) = sqlx::query_as(
            "SELECT \"notnull\" FROM pragma_table_info('gryzzly_tasks') WHERE name = 'project_status'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(notnull.0, 0, "project_status must be nullable");
    }
```

In `backend/crates/infrastructure/src/database/conversions.rs`, inside its `mod tests`, add:

```rust
    #[test]
    fn not_configured_status_round_trips() {
        assert_eq!(sync_status_to_str(SyncSourceStatus::NotConfigured), "not_configured");
        assert_eq!(sync_status_from_str("not_configured"), SyncSourceStatus::NotConfigured);
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd backend && cargo test -p infrastructure sync_status_accepts_every_status_variant not_configured_status_round_trips gryzzly_tasks_has_a_nullable 2>&1 | tail -20`
Expected: FAIL to compile — `no variant named 'NotConfigured' found for enum 'SyncSourceStatus'`

- [ ] **Step 4: Add the domain variant**

In `backend/crates/domain/src/types/common.rs`, in `pub enum SyncSourceStatus`, after `Error`:

```rust
    /// The connector has no usable credentials, so no sync was attempted. Distinct
    /// from `Error`: nothing failed, there is simply nothing configured. Previously
    /// recorded as `Error` with the message "Not configured", which made the UI
    /// paint an unconfigured source as a failure.
    NotConfigured,
```

- [ ] **Step 5: Map it to and from its DB string**

In `backend/crates/infrastructure/src/database/conversions.rs`, add to `sync_status_to_str`:

```rust
        SyncSourceStatus::NotConfigured => "not_configured",
```

and to `sync_status_from_str`, before the `_ =>` arm:

```rust
        "not_configured" => SyncSourceStatus::NotConfigured,
```

- [ ] **Step 6: Add the GraphQL enum value**

In `backend/crates/api/src/graphql/types/enums.rs`, add `NotConfigured` to `SyncSourceStatusGql` after `Error`, and the matching arm to the `From` impl:

```rust
            types::SyncSourceStatus::NotConfigured => SyncSourceStatusGql::NotConfigured,
```

- [ ] **Step 7: Record the status instead of a fake error**

In `backend/crates/application/src/use_cases/sync.rs`, add a helper beside `update_sync_error` (~line 1000):

```rust
/// A connector with no credentials. Distinct from `update_sync_error`: nothing
/// failed, so `status` must not say `Error` — the UI reads that as a red alarm.
async fn update_sync_not_configured(
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    source: Source,
) -> Result<(), AppError> {
    sync_repo
        .upsert(&SyncStatus {
            source,
            user_id,
            last_sync_at: None,
            status: SyncSourceStatus::NotConfigured,
            error_message: None,
        })
        .await?;
    Ok(())
}
```

Then replace every call of the form `update_sync_error(sync_repo, user_id, Source::X, "Not configured").await?;` with `update_sync_not_configured(sync_repo, user_id, Source::X).await?;`. Find them all:

Run: `rg -n '"Not configured"' backend/crates/application/src/use_cases/sync.rs`
Expected: **12 hits** before the edit, zero after. Not four — `sync_source` and `sync_all` each carry their own copies, and Jira and Excel check configuration twice apiece. One site uses `ctx.sync_repo` rather than a bare `sync_repo`, so a blind find-and-replace on the exact string misses it.

A regex covers both receiver forms:

```bash
python3 - <<'PY'
import re, pathlib
p = pathlib.Path('backend/crates/application/src/use_cases/sync.rs')
src = p.read_text()
pat = re.compile(r'update_sync_error\((\w+(?:\.\w+)?), user_id, (Source::\w+), "Not configured"\)')
new, n = pat.subn(r'update_sync_not_configured(\1, user_id, \2)', src)
p.write_text(new)
print(f"replaced {n} call sites")
PY
```

Note `last_sync_at: None`: an unconfigured source has never synced, and reporting `Utc::now()` as a sync time (which `update_sync_error` does) would make the UI claim a sync just happened.

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cd backend && cargo test -p domain -p application -p infrastructure -p api 2>&1 | tail -6`
Expected: PASS. The compiler will have flagged any exhaustive `match` on `SyncSourceStatus` needing the new arm.

- [ ] **Step 9: Commit**

```bash
git add migrations/sqlite/016_add_project_status_and_not_configured.sql \
        backend/crates/domain/src/types/common.rs \
        backend/crates/infrastructure/src/database/conversions.rs \
        backend/crates/infrastructure/src/database/connection.rs \
        backend/crates/api/src/graphql/types/enums.rs \
        backend/crates/application/src/use_cases/sync.rs
git commit -m "Report an unconfigured connector as NotConfigured, not as an error"
```

---

### Task 2: `project_status` through the entity and repository

**Files:**
- Modify: `backend/crates/domain/src/types/gryzzly.rs` (`GryzzlyCatalogEntry`, ~line 9)
- Modify: `backend/crates/infrastructure/src/database/gryzzly_catalog_repo.rs` (`map_row` ~line 18, `upsert` ~line 40)

**Interfaces:**
- Consumes: the `project_status` column (Task 1)
- Produces: `GryzzlyCatalogEntry.project_status: Option<String>`, persisted and read back

- [ ] **Step 1: Write the failing test**

In `backend/crates/infrastructure/src/database/gryzzly_catalog_repo.rs`, inside `mod tests`, add:

```rust
    #[tokio::test]
    async fn upsert_persists_project_status() {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        let repo = SqliteGryzzlyCatalogRepository::new(pool);
        let user_id = seed_user(&repo).await;

        let mut entry = sample_entry(user_id, "t1", "Recette");
        entry.project_status = Some("done".to_string());
        repo.upsert(&entry).await.unwrap();

        let got = repo.find_by_gryzzly_task_id(user_id, "t1").await.unwrap().unwrap();
        assert_eq!(got.project_status.as_deref(), Some("done"));
    }

    /// NULL is the documented "unknown, treat as active" state for rows written
    /// before the column existed. It must survive a round-trip as NULL rather than
    /// being coerced to "active".
    #[tokio::test]
    async fn a_null_project_status_stays_null() {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        let repo = SqliteGryzzlyCatalogRepository::new(pool);
        let user_id = seed_user(&repo).await;

        let mut entry = sample_entry(user_id, "t2", "Pilotage");
        entry.project_status = None;
        repo.upsert(&entry).await.unwrap();

        let got = repo.find_by_gryzzly_task_id(user_id, "t2").await.unwrap().unwrap();
        assert_eq!(got.project_status, None);
    }

    /// A project closing between two syncs must update the stored status.
    #[tokio::test]
    async fn upsert_updates_project_status_on_conflict() {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        let repo = SqliteGryzzlyCatalogRepository::new(pool);
        let user_id = seed_user(&repo).await;

        let mut entry = sample_entry(user_id, "t3", "Cadrage");
        entry.project_status = Some("active".to_string());
        repo.upsert(&entry).await.unwrap();

        entry.project_status = Some("done".to_string());
        repo.upsert(&entry).await.unwrap();

        let got = repo.find_by_gryzzly_task_id(user_id, "t3").await.unwrap().unwrap();
        assert_eq!(got.project_status.as_deref(), Some("done"));
    }
```

The existing tests already build entries and seed a user. Read the top of `mod tests` and reuse whatever helpers are there; if there is no `sample_entry` / `seed_user`, add them from the shape the existing tests use:

```rust
    async fn seed_user(repo: &SqliteGryzzlyCatalogRepository) -> UserId {
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, email, name) VALUES (?, ?, ?)")
            .bind(user_id.to_string())
            .bind(format!("{user_id}@example.com"))
            .bind("U")
            .execute(&repo.pool)
            .await
            .unwrap();
        user_id
    }

    fn sample_entry(user_id: UserId, task_id: &str, name: &str) -> GryzzlyCatalogEntry {
        GryzzlyCatalogEntry {
            id: Uuid::new_v4(),
            user_id,
            gryzzly_task_id: task_id.to_string(),
            name: name.to_string(),
            gryzzly_project_id: "p1".to_string(),
            project_name: "Saft / CI-CD".to_string(),
            customer_name: Some("Saft".to_string()),
            is_active: true,
            project_status: None,
            last_synced_at: Utc::now(),
        }
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test -p infrastructure project_status 2>&1 | tail -15`
Expected: FAIL to compile — `struct 'GryzzlyCatalogEntry' has no field named 'project_status'`

- [ ] **Step 3: Add the field to the entity**

In `backend/crates/domain/src/types/gryzzly.rs`, in `pub struct GryzzlyCatalogEntry`, after `is_active`:

```rust
    /// Status of the owning Gryzzly project, verbatim from the API: `active` or
    /// `done`. `None` means unknown — a row written before the column existed —
    /// and is read as active, never as terminated.
    pub project_status: Option<String>,
```

Compiling now fails at every construction site. Fix each by adding `project_status`:
- `sync_gryzzly` in `application/src/use_cases/sync.rs` — set `None` for now; Task 4 fills it properly.
- `make_entry` in `sync.rs`'s `mod gryzzly_tests` (~line 1540) — `project_status: None`.
- `resolve_assigned`'s test fixtures and any other test the compiler points at.

- [ ] **Step 4: Read the column**

In `gryzzly_catalog_repo.rs`, in `map_row`, after the `is_active` line:

```rust
        project_status: row.try_get("project_status").ok().flatten(),
```

`try_get::<Option<String>, _>` returns `Result<Option<String>>`, so `.ok().flatten()` gives `None` both for a NULL value and for a missing column — which keeps `map_row` working against a pre-016 database rather than erroring.

`list_active` and `find_by_gryzzly_task_id` use `SELECT *`, so they need no change.

- [ ] **Step 5: Write the column**

In `upsert`, add `project_status` to the column list, one more `?` to `VALUES`, the conflict update, and the bind in the matching position:

```rust
            "INSERT INTO gryzzly_tasks
                (id, user_id, gryzzly_task_id, name, gryzzly_project_id, project_name, customer_name, is_active, project_status, last_synced_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_id, gryzzly_task_id) DO UPDATE SET
                name = excluded.name,
                gryzzly_project_id = excluded.gryzzly_project_id,
                project_name = excluded.project_name,
                customer_name = excluded.customer_name,
                is_active = excluded.is_active,
                project_status = excluded.project_status,
                last_synced_at = excluded.last_synced_at",
```

and between the `is_active` and `last_synced_at` binds:

```rust
        .bind(&entry.project_status)
```

The bind order must match the column order exactly — a positional mismatch here silently writes the timestamp into `project_status`.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure gryzzly_catalog 2>&1 | tail -8`
Expected: PASS, including the three new tests

- [ ] **Step 7: Commit**

```bash
git add backend/crates/domain/src/types/gryzzly.rs \
        backend/crates/infrastructure/src/database/gryzzly_catalog_repo.rs \
        backend/crates/application/src/use_cases/sync.rs
git commit -m "Persist the owning project's status on each catalog row"
```

---

### Task 3: Carry project status from the API and unfold `is_active`

**Files:**
- Modify: `backend/crates/application/src/services/gryzzly_client.rs` (`GryzzlyProject`)
- Modify: `backend/crates/infrastructure/src/connectors/gryzzly/mapper.rs`
- Modify: `backend/crates/infrastructure/src/connectors/gryzzly/client.rs`

**Interfaces:**
- Consumes: `RawGryzzlyProject.status`, `RawGryzzlyTask.completed_at/deleted_at`
- Produces:
  - `GryzzlyProject { id, name, customer_name, is_active, status: Option<String> }`
  - `map_task(raw: RawGryzzlyTask) -> GryzzlyTask` — **one argument now**, no `project_active`
  - `fetch_projects(active_only)` excludes soft-deleted projects in both modes

- [ ] **Step 1: Write the failing tests**

In `backend/crates/infrastructure/src/connectors/gryzzly/mapper.rs`, inside `mod tests`, replace the four `map_task` tests (`an_open_task_in_an_active_project_is_active`, `a_completed_task_is_inactive`, `a_deleted_task_is_inactive`, `an_open_task_in_an_inactive_project_is_inactive`) with:

```rust
    #[test]
    fn an_open_task_is_active() {
        let t = map_task(task("t1", None, None));
        assert_eq!(t.id, "t1");
        assert_eq!(t.name, "t1");
        assert_eq!(t.project_id, "p1");
        assert!(t.is_active);
    }

    #[test]
    fn a_completed_task_is_inactive() {
        assert!(!map_task(task("t1", Some("2026-01-01T00:00:00Z"), None)).is_active);
    }

    #[test]
    fn a_deleted_task_is_inactive() {
        assert!(!map_task(task("t1", None, Some("2026-01-01T00:00:00Z"))).is_active);
    }

    /// THE semantic change. `is_active` used to fold in the project's state, which is
    /// exactly what made a task on a CLOSED project look like a DELETED task. A live
    /// task stays active regardless of its project; the project's state now travels
    /// separately, in `project_status`.
    #[test]
    fn a_live_task_stays_active_even_when_its_project_is_done() {
        let t = map_task(task("t1", None, None));
        assert!(t.is_active, "project state must no longer suppress a live task");
    }

    #[test]
    fn map_project_carries_the_raw_status_string() {
        let p = map_project(project(Some("done"), None));
        assert_eq!(p.status.as_deref(), Some("done"));
        assert!(!p.is_active);

        let p = map_project(project(Some("active"), None));
        assert_eq!(p.status.as_deref(), Some("active"));
        assert!(p.is_active);
    }

    #[test]
    fn map_project_status_is_none_when_absent() {
        assert_eq!(map_project(project(None, None)).status, None);
    }
```

In `backend/crates/infrastructure/src/connectors/gryzzly/client.rs`, inside `mod tests`, add:

```rust
    /// Soft-deleted projects must never reach the catalog, whatever `active_only`
    /// says — `active_only = false` means "include done projects", not "include
    /// everything".
    #[tokio::test]
    async fn soft_deleted_projects_are_excluded_in_both_modes() {
        for active_only in [true, false] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/view/projects.list"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "ok": true, "cursor": null,
                    "payload": [
                        {"id": "live", "name": "Live", "status": "active", "deleted_at": null},
                        {"id": "done", "name": "Done", "status": "done", "deleted_at": null},
                        {"id": "gone", "name": "Gone", "status": "active",
                         "deleted_at": "2026-01-01T00:00:00Z"}
                    ]
                })))
                .mount(&server)
                .await;

            let got = client(&server).fetch_projects(active_only).await.unwrap();
            let ids: Vec<&str> = got.iter().map(|p| p.id.as_str()).collect();
            assert!(!ids.contains(&"gone"), "deleted project leaked (active_only={active_only})");
            if active_only {
                assert_eq!(ids, vec!["live"]);
            } else {
                assert_eq!(ids, vec!["live", "done"]);
            }
        }
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test -p infrastructure gryzzly 2>&1 | tail -20`
Expected: FAIL — `map_task` takes 2 arguments but 1 was supplied; no field `status` on `GryzzlyProject`; `soft_deleted_projects_are_excluded_in_both_modes` returns `["live","done","gone"]`

- [ ] **Step 3: Add `status` to the application DTO**

In `backend/crates/application/src/services/gryzzly_client.rs`, in `pub struct GryzzlyProject`, after `is_active`:

```rust
    /// Raw status string from the API: `active` or `done`. Carried alongside the
    /// derived `is_active` on purpose — inferring "done" from `!is_active` works
    /// only while soft-deleted projects are filtered out, and a rendered badge
    /// should not depend on a two-step inference across two layers.
    pub status: Option<String>,
```

- [ ] **Step 4: Update the mapper**

In `mapper.rs`, in `map_project`, add to the returned struct:

```rust
        status: raw.status.clone(),
```

`raw.status` is read twice now (once for `is_active`, once here), so clone it or reorder — compute `is_active` first into a local, then move `raw.status` in.

Change `map_task`'s signature and body:

```rust
/// A task is active when it is neither finished nor deleted **in its own right**.
///
/// This deliberately does NOT fold in the owning project's state. Folding it was
/// what made a task on a closed project indistinguishable from one deleted in
/// Gryzzly: both arrived as `is_active = false`. The project's state now travels
/// separately as `project_status` on the catalog row.
pub(crate) fn map_task(raw: RawGryzzlyTask) -> GryzzlyTask {
    GryzzlyTask {
        id: raw.id,
        name: raw.name.trim().to_string(),
        project_id: raw.project_id.unwrap_or_default(),
        is_active: raw.completed_at.is_none() && raw.deleted_at.is_none(),
    }
}
```

- [ ] **Step 5: Update the client**

In `client.rs`, in `fetch_projects`, filter soft-deleted projects before the `active_only` filter. The raw `deleted_at` is not on `GryzzlyProject`, so filter while still holding the raw value — inside the accumulation loop:

```rust
            for raw in envelope.payload.unwrap_or_default() {
                // A soft-deleted project is gone whatever the caller asked for.
                if raw.deleted_at.is_some() {
                    continue;
                }
                if seen.insert(raw.id.clone()) {
                    projects.push(map_project(raw));
                }
            }
```

And in `fetch_tasks`, drop the second argument:

```rust
                let task = map_task(raw);
```

removing the now-stale `// Callers pass only active project ids, so project_active is true.` comment.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure gryzzly 2>&1 | tail -8`
Expected: PASS. The compiler will also point at `GryzzlyProject` construction sites in tests needing the new `status` field.

- [ ] **Step 7: Commit**

```bash
git add backend/crates/application/src/services/gryzzly_client.rs \
        backend/crates/infrastructure/src/connectors/gryzzly/mapper.rs \
        backend/crates/infrastructure/src/connectors/gryzzly/client.rs
git commit -m "Carry the project status and stop folding it into task activeness"
```

---

### Task 4: Sync done projects and record their status

**Files:**
- Modify: `backend/crates/application/src/use_cases/sync.rs` (`sync_gryzzly`, ~lines 501-560)

**Interfaces:**
- Consumes: `fetch_projects(false)`, `GryzzlyProject.status` (Task 3), `GryzzlyCatalogEntry.project_status` (Task 2)
- Produces: catalog rows carrying their project's status; done projects' tasks no longer pruned

- [ ] **Step 1: Write the failing test**

`sync.rs` already has a `mod gryzzly_tests` (~line 1465) providing `MemCatalogRepo`, `NoopSyncRepo`, `make_entry`, and a `FakeGryzzly` whose `fetch_projects` **ignores** `active_only`. Reuse the repos; define a dedicated fake in the new test so it can assert on the flag, rather than changing `FakeGryzzly` and every existing construction of it.

Add inside `mod gryzzly_tests`:

```rust
    /// Done projects must reach the catalog carrying `project_status = "done"`, so a
    /// task on a closed project can be told apart from a deleted one. Before this,
    /// `fetch_projects(true)` dropped them and `soft_prune_missing` deactivated
    /// their tasks — which read identically to "deleted in Gryzzly".
    #[tokio::test]
    async fn sync_gryzzly_records_done_projects_with_their_status() {
        /// Local fake: unlike `FakeGryzzly` it asserts on `active_only`, which is
        /// the behaviour under test.
        struct TwoProjects;

        #[async_trait]
        impl GryzzlyClient for TwoProjects {
            async fn fetch_projects(
                &self,
                active_only: bool,
            ) -> Result<Vec<GryzzlyProject>, ConnectorError> {
                assert!(!active_only, "sync_gryzzly must ask for done projects too");
                Ok(vec![
                    GryzzlyProject {
                        id: "p-live".into(),
                        name: "Live".into(),
                        customer_name: Some("Acme".into()),
                        is_active: true,
                        status: Some("active".into()),
                    },
                    GryzzlyProject {
                        id: "p-done".into(),
                        name: "Closed".into(),
                        customer_name: Some("Saft".into()),
                        is_active: false,
                        status: Some("done".into()),
                    },
                ])
            }

            async fn fetch_tasks(
                &self,
                project_ids: &[String],
            ) -> Result<Vec<GryzzlyTask>, ConnectorError> {
                assert!(project_ids.contains(&"p-done".to_string()), "done project not queried");
                Ok(vec![
                    GryzzlyTask { id: "t-live".into(), name: "Pilotage".into(),
                                  project_id: "p-live".into(), is_active: true },
                    GryzzlyTask { id: "t-done".into(), name: "Recette".into(),
                                  project_id: "p-done".into(), is_active: true },
                ])
            }
        }

        let user_id: UserId = Uuid::parse_str("00000000-0000-0000-0000-000000000011").unwrap();
        let catalog = MemCatalogRepo::default();
        let sync_repo = NoopSyncRepo;

        sync_gryzzly(&TwoProjects, &catalog, &sync_repo, user_id, Utc::now())
            .await
            .unwrap();

        let done = catalog
            .find_by_gryzzly_task_id(user_id, "t-done")
            .await
            .unwrap()
            .expect("task on a done project must be in the catalog");
        assert_eq!(done.project_status.as_deref(), Some("done"));
        assert_eq!(done.project_name, "Closed");
        // The task itself is live; only its project closed.
        assert!(done.is_active, "a live task on a closed project stays active");

        let live = catalog
            .find_by_gryzzly_task_id(user_id, "t-live")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(live.project_status.as_deref(), Some("active"));
    }
```

`make_entry` in that module also needs `project_status: None` added — Task 2's field addition makes the compiler point at it.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd backend && cargo test -p application sync_gryzzly_records_done_projects 2>&1 | tail -15`
Expected: FAIL on the `assert!(!active_only)` — `sync_gryzzly` still calls `fetch_projects(true)`

- [ ] **Step 3: Fetch all live projects and record status**

In `sync_gryzzly`, change the fetch:

```rust
    // `false`: done projects belong in the catalog too, marked as terminated rather
    // than silently pruned. `fetch_projects` still excludes soft-deleted projects.
    let projects = match client.fetch_projects(false).await {
```

and in the entry construction, replace `project_status: None` (added in Task 2 step 3) with:

```rust
            project_status: proj.and_then(|p| p.status.clone()),
```

`by_project` already maps `project_id -> &GryzzlyProject`, so no new plumbing.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd backend && cargo test -p application 2>&1 | tail -6`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add backend/crates/application/src/use_cases/sync.rs
git commit -m "Sync terminated Gryzzly projects instead of pruning them away"
```

---

### Task 5: Expose `projectStatus` over GraphQL

**Files:**
- Modify: `backend/crates/api/src/graphql/types/gryzzly.rs`
- Modify: `backend/crates/cli/graphql/schema.graphql`

**Interfaces:**
- Consumes: `GryzzlyCatalogEntry.project_status` (Task 2)
- Produces: `GryzzlyTaskGql.projectStatus: String`, `AssignedGryzzlyTaskGql.projectStatus: String`, both nullable

- [ ] **Step 1: Write the failing test**

In `backend/crates/api/src/graphql/types/gryzzly.rs`, inside its `mod tests` (or create one), add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn entry(is_active: bool, project_status: Option<&str>) -> GryzzlyCatalogEntry {
        GryzzlyCatalogEntry {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            gryzzly_task_id: "t1".into(),
            name: "Recette".into(),
            gryzzly_project_id: "p1".into(),
            project_name: "Saft / CI-CD".into(),
            customer_name: Some("Saft".into()),
            is_active,
            project_status: project_status.map(str::to_string),
            last_synced_at: Utc::now(),
        }
    }

    #[test]
    fn an_active_row_on_a_done_project_is_not_stale_but_is_terminated() {
        let got = resolve_assigned("t1".into(), Some(entry(true, Some("done"))));
        assert!(!got.stale, "a closed project must not read as a missing row");
        assert_eq!(got.project_status.as_deref(), Some("done"));
    }

    /// The two markers are independent: a row can be both disabled in the catalog
    /// and owned by a closed project, and each means something different.
    #[test]
    fn a_disabled_row_keeps_its_project_status() {
        let got = resolve_assigned("t1".into(), Some(entry(false, Some("done"))));
        assert!(got.stale);
        assert_eq!(got.project_status.as_deref(), Some("done"));
    }

    #[test]
    fn an_orphaned_assignment_has_no_project_status() {
        let got = resolve_assigned("t1".into(), None);
        assert!(got.stale);
        assert_eq!(got.project_status, None);
    }

    #[test]
    fn an_unknown_project_status_is_carried_as_none() {
        let got = resolve_assigned("t1".into(), Some(entry(true, None)));
        assert!(!got.stale);
        assert_eq!(got.project_status, None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test -p api resolve_assigned 2>&1 | tail -12`
Expected: FAIL to compile — no field `project_status` on `AssignedGryzzlyTaskGql`

- [ ] **Step 3: Add the fields**

In `gryzzly.rs`, add to `GryzzlyTaskGql` (after `customer_name`) and to `AssignedGryzzlyTaskGql` (after `stale`):

```rust
    /// Status of the owning Gryzzly project (`active` | `done`), or null when
    /// unknown — a catalog row written before the column existed. Null renders as
    /// active: never as terminated.
    pub project_status: Option<String>,
```

In `From<GryzzlyCatalogEntry> for GryzzlyTaskGql`, add `project_status: e.project_status,`.

In `resolve_assigned`, add `project_status` to all three arms — `Some(e.project_status.clone())` will not compile since the field is already `Option<String>`; use `e.project_status` directly in the two `Some(e)` arms and `None` in the orphan arm. Note the first two arms both move out of `e`, so take `project_status` before `name`/`project_name` or destructure once.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd backend && cargo test -p api 2>&1 | tail -6`
Expected: PASS

- [ ] **Step 5: Update the CLI schema by hand**

`backend/crates/cli/graphql/schema.graphql` is consumed by `graphql-client` codegen at build time, so it must match. **Do not run `cargo run -p api -- export-schema`** unless you intend the side effect: it builds the DB pool first and therefore applies pending migrations to the real `aggregated_plan.db`.

Add `projectStatus: String` to both `GryzzlyTaskGql` and `AssignedGryzzlyTaskGql`, and `NOT_CONFIGURED` to `enum SyncSourceStatusGql`.

Run: `cd backend && cargo build -p cli 2>&1 | tail -5`
Expected: compiles — codegen accepted the schema

- [ ] **Step 6: Commit**

```bash
git add backend/crates/api/src/graphql/types/gryzzly.rs backend/crates/cli/graphql/schema.graphql
git commit -m "Expose the owning project's status on the Gryzzly GraphQL types"
```

---

### Task 6: The `[terminé]` badge in the picker

**Files:**
- Create: `frontend/src/components/gryzzly/TerminatedBadge.tsx`
- Modify: `frontend/src/lib/gryzzly-picker-options.ts`
- Modify: `frontend/src/lib/gryzzly-picker-options.test.ts`
- Modify: `frontend/src/hooks/use-gryzzly-tasks.ts`
- Modify: `frontend/src/components/gryzzly/GryzzlyTaskPicker.tsx`
- Create: `frontend/src/components/gryzzly/GryzzlyTaskPicker.test.tsx`

**Interfaces:**
- Consumes: `projectStatus` from GraphQL (Task 5)
- Produces: `<TerminatedBadge />`; `GryzzlyOption.projectStatus?: string | null`

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/lib/gryzzly-picker-options.test.ts`:

```ts
  it('carries projectStatus through to the built options', () => {
    const options = buildPickerOptions(
      [{ gryzzlyTaskId: 't1', name: 'Recette', projectName: 'Saft', projectStatus: 'done' }],
      null,
    );
    expect(options[0].projectStatus).toBe('done');
  });

  it('keeps projectStatus on a pinned assigned task absent from the active list', () => {
    const options = buildPickerOptions([], {
      gryzzlyTaskId: 't9',
      name: 'Cadrage',
      projectName: 'Saft',
      projectStatus: 'done',
      stale: true,
    });
    expect(options).toHaveLength(1);
    expect(options[0].projectStatus).toBe('done');
    expect(options[0].stale).toBe(true);
  });
```

Create `frontend/src/components/gryzzly/GryzzlyTaskPicker.test.tsx`. Mirror the mocking idiom of an existing component test (e.g. `TaskEditSheet.test.tsx`) for `urql`'s `useMutation` and the `useGryzzlyTasks` hook:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { GryzzlyTaskPicker } from './GryzzlyTaskPicker';

const executeAssign = vi.fn().mockResolvedValue({});
vi.mock('urql', () => ({ useMutation: () => [{}, executeAssign] }));

const mockOptions = vi.fn();
vi.mock('@/hooks/use-gryzzly-tasks', () => ({
  useGryzzlyTasks: () => ({ options: mockOptions(), fetching: false, error: null }),
}));

describe('GryzzlyTaskPicker', () => {
  beforeEach(() => {
    executeAssign.mockClear();
    mockOptions.mockReturnValue([
      { gryzzlyTaskId: 't1', name: 'Pilotage', projectName: 'Canal Plus', projectStatus: 'active' },
      { gryzzlyTaskId: 't2', name: 'Recette', projectName: 'Saft', projectStatus: 'done' },
    ]);
  });

  it('badges only the group header of a terminated project', async () => {
    render(<GryzzlyTaskPicker taskId="task-1" assigned={null} />);
    await userEvent.click(screen.getByRole('button', { name: /assign gryzzly task/i }));

    // One badge for the done project's group, none for the active one.
    expect(screen.getAllByText('terminé')).toHaveLength(1);
  });

  /// A project routinely closes with declarations still owed, so the row must
  /// remain clickable.
  it('still assigns a task whose project is terminated', async () => {
    render(<GryzzlyTaskPicker taskId="task-1" assigned={null} />);
    await userEvent.click(screen.getByRole('button', { name: /assign gryzzly task/i }));
    await userEvent.click(screen.getByRole('option', { name: /Recette/ }));

    expect(executeAssign).toHaveBeenCalledWith({ taskId: 'task-1', gryzzlyTaskId: 't2' });
  });

  it('badges the trigger when the assigned task’s project is terminated', () => {
    render(
      <GryzzlyTaskPicker
        taskId="task-1"
        assigned={{
          gryzzlyTaskId: 't2',
          name: 'Recette',
          projectName: 'Saft',
          projectStatus: 'done',
          stale: false,
        }}
      />,
    );
    expect(screen.getByText('terminé')).toBeInTheDocument();
  });

  it('shows both markers when a task is stale and its project terminated', () => {
    render(
      <GryzzlyTaskPicker
        taskId="task-1"
        assigned={{
          gryzzlyTaskId: 't2',
          name: 'Recette',
          projectName: 'Saft',
          projectStatus: 'done',
          stale: true,
        }}
      />,
    );
    expect(screen.getByText('stale')).toBeInTheDocument();
    expect(screen.getByText('terminé')).toBeInTheDocument();
  });

  it('shows no badge when nothing is terminated', async () => {
    mockOptions.mockReturnValue([
      { gryzzlyTaskId: 't1', name: 'Pilotage', projectName: 'Canal Plus', projectStatus: 'active' },
    ]);
    render(<GryzzlyTaskPicker taskId="task-1" assigned={null} />);
    await userEvent.click(screen.getByRole('button', { name: /assign gryzzly task/i }));

    expect(screen.queryByText('terminé')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && pnpm test -- gryzzly 2>&1 | tail -20`
Expected: FAIL — `TerminatedBadge` module not found / no `terminé` text rendered

- [ ] **Step 3: Create the badge**

`frontend/src/components/gryzzly/TerminatedBadge.tsx`:

```tsx
/** Marks a Gryzzly project that is closed (`status: "done"`).
 *
 * Muted grey on purpose: it is context, not a warning. The amber `stale` badge in
 * GryzzlyTaskPicker means something is wrong (the catalog row is gone or disabled);
 * a terminated project is merely finished, and its tasks stay selectable because a
 * project routinely closes with time declarations still owed on it.
 *
 * One component rather than an inline span per surface, so the picker and the task
 * edit sheet cannot drift apart. */
export function TerminatedBadge({ small = false }: { readonly small?: boolean }) {
  return (
    <span
      className={`inline-flex items-center rounded font-medium bg-gray-200 text-gray-600 flex-shrink-0 ${
        small ? 'px-1 py-0.5 text-[9px]' : 'px-1.5 py-0.5 text-[10px]'
      }`}
    >
      terminé
    </span>
  );
}
```

- [ ] **Step 4: Thread `projectStatus` through the option types and query**

In `frontend/src/lib/gryzzly-picker-options.ts`, add to both interfaces:

```ts
  projectStatus?: string | null;
```

`buildPickerOptions` needs no logic change — it spreads/copies options and pins the assigned task; confirm the pinned-task construction copies `projectStatus` rather than rebuilding the object field by field. If it rebuilds, add the field.

In `frontend/src/hooks/use-gryzzly-tasks.ts`, add `projectStatus` to the query selection set:

```
    gryzzlyTasks(search: $search, projectFilter: $projectFilter, limit: $limit) {
      gryzzlyTaskId
      name
      projectName
      projectStatus
    }
```

- [ ] **Step 5: Render the badge in the picker**

In `GryzzlyTaskPicker.tsx`:

Import it: `import { TerminatedBadge } from './TerminatedBadge';`

Add `projectStatus` to the mutation's selection set so a fresh assignment carries it back:

```
      gryzzlyTask {
        gryzzlyTaskId
        name
        projectName
        projectStatus
        stale
      }
```

On the **group header** — the picker groups by project, so one badge per group rather than one per row:

```tsx
                <div className="px-3 py-1 text-[10px] font-semibold text-gray-400 uppercase tracking-wider bg-gray-50 border-b border-gray-100 flex items-center gap-1.5">
                  <span className="truncate">{project}</span>
                  {items.some((o) => o.projectStatus === 'done') && <TerminatedBadge small />}
                </div>
```

On the **trigger**, after the existing `stale` badge block:

```tsx
          {assigned?.projectStatus === 'done' && <TerminatedBadge />}
```

Leave the row-level `stale` badge and the `onClick` untouched — terminated rows stay selectable.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd frontend && pnpm test -- gryzzly 2>&1 | tail -12`
Expected: PASS

- [ ] **Step 7: Type-check and commit**

Run: `cd frontend && pnpm build 2>&1 | tail -5`
Expected: no TypeScript errors

```bash
git add frontend/src/components/gryzzly/TerminatedBadge.tsx \
        frontend/src/components/gryzzly/GryzzlyTaskPicker.tsx \
        frontend/src/components/gryzzly/GryzzlyTaskPicker.test.tsx \
        frontend/src/lib/gryzzly-picker-options.ts \
        frontend/src/lib/gryzzly-picker-options.test.ts \
        frontend/src/hooks/use-gryzzly-tasks.ts
git commit -m "Badge tasks whose Gryzzly project is terminated, still selectable"
```

---

### Task 7: A truthful sync status bar

**Files:**
- Modify: `frontend/src/components/sync/SyncStatusBar.tsx`
- Create: `frontend/src/components/sync/SyncStatusBar.test.tsx`

**Interfaces:**
- Consumes: `status: 'NOT_CONFIGURED'` from GraphQL (Task 1); `errorMessage` already selected by `dashboard.graphql`
- Produces: nothing downstream

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/components/sync/SyncStatusBar.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { SyncStatusBar } from './SyncStatusBar';

const row = (over: Partial<Parameters<typeof SyncStatusBar>[0]['statuses'][number]> = {}) => ({
  source: 'GRYZZLY',
  status: 'SUCCESS',
  lastSyncAt: '2026-08-10T15:55:45Z',
  errorMessage: null,
  ...over,
});

describe('SyncStatusBar', () => {
  it('renders nothing when there are no statuses', () => {
    const { container } = render(<SyncStatusBar statuses={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  /// An unconfigured connector is not a failure. It used to arrive as
  /// status=error + errorMessage="Not configured" and paint a red Error dot.
  it('labels a not-configured source without calling it an error', () => {
    render(<SyncStatusBar statuses={[row({ status: 'NOT_CONFIGURED' })]} />);
    expect(screen.getByText('Non configuré')).toBeInTheDocument();
    expect(screen.queryByText('Error')).not.toBeInTheDocument();
  });

  /// The expiry date and the instruction are the whole value of the message, and a
  /// title tooltip hides them until you happen to hover.
  it('shows an error message inline, not only in a tooltip', () => {
    render(
      <SyncStatusBar
        statuses={[
          row({
            status: 'ERROR',
            errorMessage:
              'the Gryzzly session cookie expired on 2026-08-17 14:51:50 UTC — log in again on app.gryzzly.io (it lasts 7 days)',
          }),
        ]}
      />,
    );
    expect(screen.getByText(/expired on 2026-08-17/)).toBeInTheDocument();
  });

  it('offers a Gryzzly reconnect link when the session needs attention', () => {
    render(<SyncStatusBar statuses={[row({ status: 'NOT_CONFIGURED' })]} />);
    const link = screen.getByRole('link', { name: /reconnecter/i });
    expect(link).toHaveAttribute('href', 'https://app.gryzzly.io');
  });

  it('offers no reconnect link for a healthy Gryzzly', () => {
    render(<SyncStatusBar statuses={[row()]} />);
    expect(screen.queryByRole('link', { name: /reconnecter/i })).not.toBeInTheDocument();
  });

  /// The link is Gryzzly-specific: it exists because that cookie expires weekly.
  it('offers no reconnect link for another source in error', () => {
    render(<SyncStatusBar statuses={[row({ source: 'JIRA', status: 'ERROR', errorMessage: 'boom' })]} />);
    expect(screen.getByText('boom')).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: /reconnecter/i })).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && pnpm test -- SyncStatusBar 2>&1 | tail -20`
Expected: FAIL — no `Non configuré` text, no reconnect link, message not in the DOM

- [ ] **Step 3: Implement**

In `SyncStatusBar.tsx`:

Add to `getStatusLabel`, before the `IDLE`/default arm:

```tsx
    case 'NOT_CONFIGURED':
      return 'Non configuré';
```

`getStatusDotColor` already falls through to grey for unknown values, which is the right colour — add the case explicitly so the intent is readable rather than accidental:

```tsx
    case 'NOT_CONFIGURED':
      return '#9CA3AF'; // grey: nothing is wrong, nothing is configured either
```

Add a helper above the component:

```tsx
/** Gryzzly's credential is a browser cookie with a fixed 7-day life, so
 *  re-logging-in is a weekly chore rather than an incident. That is why this one
 *  source gets a direct link and the others do not. */
function needsGryzzlyReconnect(source: string, status: string): boolean {
  return source.toUpperCase() === 'GRYZZLY' && (status === 'ERROR' || status === 'NOT_CONFIGURED');
}
```

Wrap the existing flex row so reasons can sit beneath it, and render them. Replace the component's returned JSX outer structure with a column: the current dot row unchanged, then:

```tsx
      {statuses.some(s => s.errorMessage || needsGryzzlyReconnect(s.source, s.status)) && (
        <div className="flex flex-col gap-1 pt-1 border-t border-gray-100">
          {statuses
            .filter(s => s.errorMessage || needsGryzzlyReconnect(s.source, s.status))
            .map(s => (
              <div key={`${s.source}-reason`} className="flex items-start gap-1.5 text-xs">
                <span className="font-medium text-gray-500 flex-shrink-0">{s.source}</span>
                {s.errorMessage && <span className="text-gray-600">{s.errorMessage}</span>}
                {needsGryzzlyReconnect(s.source, s.status) && (
                  <a
                    href="https://app.gryzzly.io"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-blue-600 hover:underline flex-shrink-0"
                  >
                    Reconnecter
                  </a>
                )}
              </div>
            ))}
        </div>
      )}
```

Change the root element from `flex items-center gap-4` to `flex flex-col gap-2` and keep the existing dot row as an inner `div` with the old classes, so the healthy case looks unchanged.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && pnpm test -- SyncStatusBar 2>&1 | tail -10`
Expected: PASS, 6 tests

- [ ] **Step 5: Run the whole frontend suite and type-check**

Run: `cd frontend && pnpm test 2>&1 | tail -8 && pnpm build 2>&1 | tail -4`
Expected: all green, no TS errors

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/sync/SyncStatusBar.tsx frontend/src/components/sync/SyncStatusBar.test.tsx
git commit -m "Tell not-configured apart from failed, and surface the reason inline"
```

---

### Task 8: Documentation

**Files:**
- Modify: `SPEC_TECHNIQUE.md` (§10.6, the migration table, the config/status vocabulary)
- Modify: `SPEC_FONCTIONNELLE.md` (US-006)

- [ ] **Step 1: Update `SPEC_TECHNIQUE.md`**

Add a migration-table row after 015:

```markdown
| **016** | `016_add_project_status_and_not_configured.sql` | `gryzzly_tasks.project_status` (statut du projet propriétaire, `active` \| `done`, NULL = inconnu lu comme actif) et reconstruction de `sync_status` pour que le `CHECK` sur `status` admette `not_configured`. Voir § 10.6. |
```

In §10.6, amend the sync flow: step 2 now reads `fetch_projects(active_only = false)` — done projects are catalogued and marked, only soft-deleted ones excluded. Document the `is_active` semantic change explicitly: a catalog row's `is_active` is now the task's own liveness (`completed_at` and `deleted_at` null) and no longer folds the project's state, which is what previously made a task on a closed project indistinguishable from a deleted one. State that `project_status` carries the project's state instead, and that NULL means unknown and is read as active.

Add `not_configured` to the documented `sync_status.status` vocabulary wherever the four values are listed, noting it replaces the old `status = error` + `error_message = "Not configured"` encoding, and that `last_sync_at` stays NULL for it.

- [ ] **Step 2: Update `SPEC_FONCTIONNELLE.md`**

In US-006 acceptance criteria, add:

```markdown
- Les projets Gryzzly **terminés** sont désormais synchronisés eux aussi, et signalés par un badge `terminé` dans le sélecteur de tâche Gryzzly et sur la tâche assignée. Leurs tâches restent **sélectionnables** : un projet se clôt souvent alors qu'il reste des heures à déclarer dessus.
- Une source non configurée est affichée comme « Non configuré » (gris) et non comme une erreur (rouge). Pour Gryzzly, la raison exacte (session expirée, avec sa date) est affichée directement et accompagnée d'un lien **Reconnecter** vers `app.gryzzly.io`.
```

- [ ] **Step 3: Verify no contradictions remain**

Run: `cd /home/mbt/appfactory/aggregated_plan && rg -n 'clé d.API Gryzzly|projets actifs et tâches' SPEC_TECHNIQUE.md SPEC_FONCTIONNELLE.md | head`
Expected: no claim that only active projects are synced, and no surviving API-key claim

- [ ] **Step 4: Commit**

```bash
git add SPEC_TECHNIQUE.md SPEC_FONCTIONNELLE.md
git commit -m "Document terminated-project marking and the not-configured status"
```

---

### Task 9: Live verification

**Files:** none (verification only)

The cookie expires **2026-08-17 14:51:50 UTC**; log into `app.gryzzly.io` first if that has passed.

**Deployment coupling — read before starting.** Applying migration 016 makes the DB refuse any binary that lacks it, exactly as 015 did (`Migrate(VersionMissing(N))`). The `aplan-api` systemd user service runs `~/.local/bin/aplan-api`, which is a *separately installed* binary. Verify the same way as last time: stop the service, run `backend/target/debug/api` on :3001 with the service's `DATABASE_URL`, then afterwards either install the new binary or roll the migration back on the DB before restarting the service.

- [ ] **Step 1: Record the baseline**

```bash
cd /home/mbt/appfactory/aggregated_plan
sqlite3 backend/aggregated_plan.db \
  "select count(*) total, sum(is_active) active, count(distinct gryzzly_project_id) projects from gryzzly_tasks;"
sqlite3 backend/aggregated_plan.db "select count(*) from tasks where gryzzly_task_id is not null;"
```

Expected baseline: `71|37|32` and 2 assignments. Write the numbers down.

- [ ] **Step 2: Back up the DB and stop the service**

```bash
cd backend && STAMP=$(date +%Y%m%d-%H%M)
for f in aggregated_plan.db aggregated_plan.db-wal aggregated_plan.db-shm; do
  [ -f "$f" ] && cp "$f" "$f.bak-$STAMP-pre-mig016"
done
systemctl --user stop aplan-api.service
```

- [ ] **Step 3: Run the branch build and sync**

```bash
cd /home/mbt/appfactory/aggregated_plan/backend && cargo build -p api
DATABASE_URL='sqlite:/home/mbt/appfactory/aggregated_plan/backend/aggregated_plan.db?mode=rwc' \
  ./target/debug/api > /tmp/api-mig016.log 2>&1 &
sleep 5
sqlite3 aggregated_plan.db "select max(version) from _sqlx_migrations;"   # expect 16
cd /home/mbt/appfactory/aggregated_plan && aplan sync --source gryzzly
```

Expected: `GRYZZLY: SUCCESS`.

- [ ] **Step 4: Verify the catalog against the prediction**

```bash
sqlite3 backend/aggregated_plan.db \
  "select project_status, count(*), sum(is_active) from gryzzly_tasks group by project_status;"
sqlite3 backend/aggregated_plan.db \
  "select count(distinct gryzzly_project_id) from gryzzly_tasks where project_status = 'done';"
```

Expected shape: rows with `project_status = 'done'` now exist and **most of them are `is_active = 1`** — that is the `is_active` unfolding working. Total rows and total active both rise substantially (done projects' tasks were previously deactivated or absent). Rows still carrying NULL `project_status` are ones no longer returned by the API at all.

Confirm assignments survived:

```bash
sqlite3 backend/aggregated_plan.db "select count(*) from tasks where gryzzly_task_id is not null;"
```

Expected: still 2.

- [ ] **Step 5: Check the UI**

Start the frontend (`cd frontend && pnpm dev`), open a task's edit sheet, open the Gryzzly picker, and confirm: a closed project's group header carries `terminé`, its tasks are clickable, and assigning one works. Then check the Dashboard's sync bar shows Gryzzly green with no reason line.

To see the not-configured path: `aplan config set gryzzly.cookie_profile /nonexistent/Cookies`, sync, and confirm the bar shows grey `Non configuré` with a `Reconnecter` link — then `aplan config set gryzzly.cookie_profile ""`.

- [ ] **Step 6: Full test sweep**

```bash
cd backend && cargo test -p domain -p application -p infrastructure -p api 2>&1 | tail -4
cd ../frontend && pnpm test 2>&1 | tail -4
```

Expected: all green.

- [ ] **Step 7: Restore the service**

Either install the new binary (`cp backend/target/debug/api ~/.local/bin/aplan-api`) or roll migration 016 back on the DB, then `systemctl --user start aplan-api.service` and confirm `systemctl --user is-active aplan-api.service` reports `active`. **Ask which before choosing** — the same decision as last time.

- [ ] **Step 8: Report, no commit**

Report the before/after catalog numbers, how many rows now carry `project_status = 'done'`, and whether the active-count jump matches the `is_active` unfolding.

---

## Self-Review Notes

**Spec coverage:** Part A (`NotConfigured` + migration + frontend) → Tasks 1, 7. Part A's extended regression guard → Task 1. Part B (fetch all projects, unfold `is_active`, `project_status` column, expose, render) → Tasks 2, 3, 4, 5, 6. Part C (one migration) → Task 1. Testing section → tests inside Tasks 1-7 plus Task 9. Documentation → Task 8. Out-of-scope items are never implemented.

**One deviation from the spec, deliberate:** the spec's mockup showed `[terminé]` beside the project on every picker row. The picker actually groups rows under a project header, so the badge goes on the header once per group (plus the collapsed trigger). Same information, no repetition — recorded in Global Constraints and asserted by `badges only the group header of a terminated project`.

**Ordering constraint:** Task 1 must precede Task 4 (writing `not_configured` needs the widened CHECK) and Task 2 (the column must exist). Task 2 precedes Task 4 (`project_status` field), Task 3 precedes Task 4 (`GryzzlyProject.status`), Task 5 precedes Task 6 (`projectStatus` over the wire).
