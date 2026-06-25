# Gryzzly Integration — Pull-only Catalog + aplan-Task Assignment

**Date:** 2026-06-23
**Status:** Design — awaiting review
**Author:** (brainstormed with Claude)

## Problem

[Gryzzly](https://app.gryzzly.io/) is a Slack/Teams-driven time-tracking & project-profitability
SaaS. The user already tracks time inside the Aggregated Plan cockpit (worklog entries +
half-day activity slots) and wants the cockpit to know **which Gryzzly work-item each aplan task
belongs to**. Concretely:

- Pull the Gryzzly **catalog** (active projects and their tasks) into the cockpit, read-only,
  as a reference list — like a lightweight version of the Jira sync.
- Let the user **assign an aplan task to a Gryzzly task** (a Gryzzly "task" is a *category of
  billable work within a project*, e.g. "Development" / "Specs" / "Meetings" — **not** a Jira-style
  deliverable). The Gryzzly **project is shown as context info only**.
- Keep the door open so a **future phase can upload tracked hours to the assigned Gryzzly task**
  as *declarations*, without a breaking refactor.

This phase is **pull-only**. Uploading hours is explicitly out of scope here but the data model is
designed to make it cheap to add later.

## Scope

**In scope**
- Read-only `GryzzlyClient` connector (active projects + tasks).
- A dedicated `gryzzly_tasks` cache table (a catalog), refreshed by sync — **never** routed
  through the `tasks` table.
- `Source::Gryzzly` wired through the sync engine, the `force_sync` GraphQL mutation, and the CLI.
- An optional Gryzzly-task assignment per aplan task (`tasks.gryzzly_task_id` +
  `tasks.gryzzly_project_id` snapshot), an `assignGryzzlyTask` mutation, a `gryzzlyTasks` query,
  and a searchable picker in the frontend.
- French spec updates (`SPEC_FONCTIONNELLE.md` / `SPEC_TECHNIQUE.md`) in the same commit.

**Out of scope (future — reserved, not built)**
- Pushing/uploading tracked hours to Gryzzly as declarations. See *Forward-compat reservations*.

## Fixed parameters (⚠️ VERIFY against live API before coding — public docs are login-gated)

| Item | Value (to confirm with a real key) |
|------|------------------------------------|
| Base URL | `https://api.gryzzly.io/v1` (overridable via config) |
| Auth | API key from Gryzzly *Administration → API Keys*, sent as `Authorization: Bearer <key>` |
| Entities used | `projects` (list, active/archived flag), `tasks` (list, belong to project), UUID ids |
| Pagination | **Unknown** — confirm cursor vs offset/page-number; replicate inside `fetch_*` |
| Rate limits | **Unknown** — add `429` / `Retry-After` handling in the connector |
| Task active flag | **Unknown** whether tasks carry their own active flag. If active is *project-level only*, derive catalog `is_active` from *project active AND present in last successful fetch*. |

These unknowns are a hard prerequisite for the connector task in the implementation plan.

## Architecture & data flow

Mirrors the existing Jira connector pattern: the **trait + cross-layer DTO live in `application`**,
the **reqwest impl + private `mapper.rs`/`types.rs` live in `infrastructure`**, and the API layer
only references the trait (`Arc<dyn GryzzlyClient>`) plus the concrete struct's `::new`.

```
Gryzzly REST API
   │  fetch_projects(active_only) / fetch_tasks(project_ids)
   ▼
GryzzlyClient (trait, application/services/gryzzly_client.rs)
HttpGryzzlyClient (infra/connectors/gryzzly/{client,types,mapper}.rs)
   │
sync_gryzzly()  ── upsert + SOFT prune ──▶  gryzzly_tasks  (cache / catalog table)
   │                                              ▲
sync_status row (Source::Gryzzly)                 │ LEFT JOIN for display (tolerates NULL)
                                                  │
   aplan task ── gryzzly_task_id + gryzzly_project_id (snapshot at assign) ──┘
```

The catalog refresh is the inverse of a future hours-push; the two never share a code path
(`sync_*` is strictly inbound).

## Data model — migration `009_create_gryzzly_catalog.sql`

Conventions per the repo: TEXT UUID ids, `INTEGER` booleans, ISO-8601 TEXT timestamps,
`user_id TEXT NOT NULL REFERENCES users(id)`, app-supplied timestamps (no `DEFAULT datetime('now')`).

**`gryzzly_tasks`** (denormalized catalog — "project is just for info", so no separate project table):

```sql
CREATE TABLE gryzzly_tasks (
    id                 TEXT PRIMARY KEY,
    user_id            TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    gryzzly_task_id    TEXT NOT NULL,
    name               TEXT NOT NULL,
    gryzzly_project_id TEXT NOT NULL,         -- kept now: future declarations need it
    project_name       TEXT NOT NULL,
    customer_name      TEXT,
    is_active          INTEGER NOT NULL DEFAULT 1,
    last_synced_at     TEXT NOT NULL,
    UNIQUE(user_id, gryzzly_task_id)
);
CREATE INDEX idx_gryzzly_tasks_user_active_project
    ON gryzzly_tasks(user_id, is_active, project_name);
```

**Assignment columns on `tasks`** (plain nullable `ALTER`, like `delegated_to` in migration 008 —
SQLite `ADD COLUMN` cannot carry FK/UNIQUE/non-constant default, which is fine):

```sql
ALTER TABLE tasks ADD COLUMN gryzzly_task_id TEXT;     -- the assignment + future push target
ALTER TABLE tasks ADD COLUMN gryzzly_project_id TEXT;  -- snapshot at assign time (forward-compat)
```

> **Why snapshot `gryzzly_project_id` onto the task:** a future hours-push needs `(gryzzly_user_id,
> gryzzly_task_id, gryzzly_project_id, date, duration, description)`. Resolving the project from the
> live catalog row is fragile (the row can be soft-disabled, the catalog rebuilt, or synced on a
> different machine). Snapshotting the project id at assignment time guarantees the push can always
> be constructed. Cost today: one nullable column.

## Connector + sync wiring

- **Trait** `GryzzlyClient: Send + Sync` (application/services), `#[async_trait]`, returning
  `Result<_, ConnectorError>`. **Generically named** so a future `push_declaration(...)` method can
  be added without renaming. Read methods now:
  - `fetch_projects(active_only: bool) -> Result<Vec<GryzzlyProject>, ConnectorError>`
  - `fetch_tasks(project_ids: &[String]) -> Result<Vec<GryzzlyCatalogTask>, ConnectorError>`
- **`HttpGryzzlyClient`** (infra): `new(base_url, api_key)`, reqwest client built once with a 30s
  timeout, `Authorization: Bearer` per request, status → `ConnectorError`
  (401/403 → `AuthFailed`, other non-2xx → `Http`, transport → `NetworkError`, json → `ParseError`).
  Map `429` to `Http{429,..}` and apply a bounded `Retry-After`-aware retry inside the connector.
- **`GryzzlyCatalogRepository`** (new trait in application + sqlx impl in infrastructure): upsert by
  `(user_id, gryzzly_task_id)`, soft-disable stale, fetch-for-display. Mirrors the *shape* of
  `task_repo`'s `find_by_source` / prune but against `gryzzly_tasks`.
- **`sync_gryzzly`** (use case) modeled on `sync_jira`: mark `Syncing` → fetch active projects →
  fetch tasks → upsert catalog rows → soft-prune → mark `Success`.

### Two hardening changes (from design validation)

1. **Soft-prune, never empty-wipe (high severity).** The Jira `delete_stale_by_source` deletes
   *all* rows for a source when the keep-list is empty — catastrophic for a lookup catalog on a
   transient empty fetch. `sync_gryzzly` instead:
   - sets `is_active = 0` on rows no longer returned (never hard-deletes a row referenced by any
     `task.gryzzly_task_id`),
   - **skips pruning entirely if the fetch returns empty** while the catalog was previously
     non-empty, recording a sync error rather than wiping,
   - has a unit test asserting an empty fetch leaves existing rows intact.

2. **`Source` enum fan-out is all-or-nothing (high severity).** `source_from_str` silently coerces
   unknown strings to `Source::Personal`, so a partially-wired source corrupts data without an
   error. The same commit MUST update every exhaustive site (see *File-by-file changes*) and add a
   **round-trip unit test over every `Source` variant** (`to_str` → `from_str`) so a missing arm
   fails CI.

## GraphQL + assignment (with explicit stale-state handling)

- **Query** `gryzzlyTasks(search: String, projectFilter: String, limit: Int)` — active rows only,
  grouped by project, **server-side search/limit** to avoid shipping a large catalog to the client.
- **Mutation** `assignGryzzlyTask(taskId: ID!, gryzzlyTaskId: ID)` — `null` clears. Dedicated
  mutation (clean boundary; avoids the `updateTask` recurrence template-field guard,
  `has_template_only_fields`). Resolves and **snapshots `gryzzly_project_id`** from the catalog at
  assign time. `gryzzly_task_id` must **not** be added to `has_template_only_fields` (it is an
  instance-level user field, settable on recurring instances).
- **Task GraphQL field** exposes the assigned Gryzzly task via a **LEFT JOIN** that tolerates NULL,
  with three defined states:
  1. assigned + active catalog row → normal (`name` + `project_name`).
  2. assigned + `is_active = 0` row → return cached `name`/`project_name` + `stale = true`; the
     picker still includes this entry so the user can see/clear it.
  3. assigned + no catalog row → return `gryzzly_task_id` with `name = null`, `stale = true`
     (never crash the join).
- N+1 guard for task lists: batch-resolve the catalog lookup across the result set (DataLoader /
  collect ids), or accept per-task lookups for the MVP's modest list sizes (decided at impl time;
  the `(user_id, is_active, project_name)` index supports either).

## Frontend — assignment picker

A searchable combobox (shadcn/ui, matching existing task controls) on the task detail/edit surface:
options grouped by Gryzzly project, showing customer → project → task context, a "clear assignment"
affordance, and a **stale badge** for states 2/3. The currently-assigned task is always shown even
when inactive, so a stale assignment can be cleared.

## Config & credentials

Per-user key-value in the `configuration` table (matching Jira — free-form keys, no schema/trait
change):

- `gryzzly.api_key`, `gryzzly.base_url` (default `https://api.gryzzly.io/v1`).
- Reserved for the future push: `gryzzly.user_id` (maps the local user → Gryzzly identity).
- `.env.example` gets a commented hint pointing to the Settings page (no real env var, like Jira).

The client is built lazily in `force_sync` from config (Some only when `gryzzly.api_key` is present
and non-empty; otherwise the source records `update_sync_error(.., "Not configured")`).

## Forward-compat reservations (uploading hours — documented now, built later)

Zero code cost today; these are decisions captured so the push phase is additive:

- **Separate code path.** A future push lives in a new `use_cases/gryzzly_export.rs` + a
  `pushGryzzlyHours` mutation — **never** inside `sync_source`/`sync_all` (inbound-only).
- **Canonical time source = `activity_slots`.** Time exists in both `worklog_entries` (raw
  `logged_at`) and `activity_slots` (already materialized from worklog by
  `materialize_worklog_time`). The push reads **only `activity_slots`** to avoid double-counting;
  per-task/per-date rollup = `SUM(end_time − start_time) WHERE task_id = ? AND task_id IS NOT NULL
  AND end_time IS NOT NULL GROUP BY date` (half-day breakdown available via `slot.half_day`). A
  worklog flush must precede a push. This rollup helper does **not** exist yet.
- **Idempotency ledger.** A future declarations ledger keyed `(user_id, gryzzly_task_id, date)` →
  `{ remote_declaration_id, source_hash }` makes re-pushes update-not-duplicate. This requires a
  **second migration** in the push phase — the 009 migration deliberately does not pre-bake it.
- **Push signature reserved:** `push_declaration(gryzzly_user_id, gryzzly_task_id,
  gryzzly_project_id, date, duration_secs, description)`. `gryzzly.user_id` must be validated
  against the token's identity before any POST (fail loudly; never log hours to the wrong user).
- **`SyncResult` counters.** `SyncResult` is task-centric (`tasks_created/updated/removed`). For
  `Source::Gryzzly` either add catalog counters (`catalog_upserted/catalog_pruned`) or document
  that the `tasks_*` fields count catalog rows for this source, so reporting isn't misleading.

## Risk register (from design validation — full detail in the brainstorm)

| # | Risk | Sev | Resolution in this design |
|---|------|-----|----------------------------|
| 1 | Empty/transient fetch wipes catalog (copied `delete_stale` footgun) | High | Soft-prune + skip-prune-on-empty + never hard-delete referenced rows (+test) |
| 2 | `source_from_str` silently coerces unknown → `Personal` | High | Update all exhaustive sites same commit + round-trip test |
| 3 | Dangling assignment undefined across picker/Task/push | High | Three explicit states; LEFT JOIN tolerant of NULL; picker shows inactive-assigned |
| 4 | Future push painted into a corner (no project mapping / idempotency) | High | Snapshot `gryzzly_project_id` on task; reserve ledger + signature + canonical source |
| 5 | Unverified Gryzzly API (pagination, rate limit, active flag) | Med | Hard prerequisite to verify; 429 handling; derive `is_active` if project-level only |
| 6 | CLI codegen drift (vendored `schema.graphql` + generated `SourceGql`) | Med | Same-commit checklist incl. regen + `cargo build -p cli` |
| 7 | Picker scale / N+1 on Task render | Med | Server-side search/limit + index; batch the Task→catalog resolver |
| 8 | `SyncResult` counters semantically wrong for a catalog source | Low | Add catalog counters or document the reuse |
| 9 | Multi-user / credential scoping | Low | MVP assumes one local user == one Gryzzly identity, per-user API key |

## File-by-file changes (grounded against current code)

**New files**
- `backend/crates/application/src/services/gryzzly_client.rs` — `GryzzlyClient` trait + `GryzzlyProject` / `GryzzlyCatalogTask` DTOs.
- `backend/crates/infrastructure/src/connectors/gryzzly/{mod,client,types,mapper}.rs` — reqwest impl, raw API DTOs (private), pure mapper.
- `backend/crates/application/src/repositories/gryzzly_catalog_repository.rs` — catalog repo trait.
- `backend/crates/infrastructure/src/database/gryzzly_catalog_repo.rs` — sqlx impl.
- `migrations/sqlite/009_create_gryzzly_catalog.sql` — `gryzzly_tasks` table + `tasks` ALTERs.

**Edited (the `Source` fan-out + wiring)**
- `backend/crates/domain/src/types/common.rs` — add `Gryzzly` to `enum Source`.
- `backend/crates/infrastructure/src/database/conversions.rs` — both `source_to_str` / `source_from_str` arms.
- `backend/crates/api/src/graphql/types/enums.rs` — `SourceGql::Gryzzly` + both `From` impls.
- `backend/crates/application/src/use_cases/sync.rs` — `SyncContext.gryzzly_client`, `Source::Gryzzly` arm in `sync_source`, dispatch in `sync_all`, new `sync_gryzzly`.
- `backend/crates/api/src/graphql/mutation.rs` — build `Arc<dyn GryzzlyClient>` from config in `force_sync`; add the `assignGryzzlyTask` resolver.
- `backend/crates/api/src/graphql/query.rs` (or the query resolver module) — add the `gryzzlyTasks(search, projectFilter, limit)` resolver.
- `backend/crates/application/src/services/mod.rs`, `application/src/repositories/mod.rs` & `infrastructure/src/connectors/mod.rs` — module re-exports.
- `backend/crates/domain/src/types/task.rs` — `gryzzly_task_id` + `gryzzly_project_id: Option<String>` (+ fixtures).
- `backend/crates/infrastructure/src/database/task_repo.rs` — `map_task_row` (`try_get().ok().flatten()`) + `save()` INSERT list/placeholders/binds aligned (+ fixtures).
- `backend/crates/api/src/graphql/types/task.rs` — assigned-Gryzzly-task field with stale state.
- CLI: `cli/src/cli.rs` (`SourceArg::Gryzzly`), `cli/src/commands.rs` (map arm), `cli/graphql/schema.graphql` (`GRYZZLY` enum value) + **regenerate graphql-client**.
- `backend/crates/mcp/src/server.rs` — add `"gryzzly"` arm to `parse_source` for completeness (mcp is broken/out-of-scope; compiles via wildcard regardless).
- `SPEC_FONCTIONNELLE.md` / `SPEC_TECHNIQUE.md` — French updates for the new source, catalog, and assignment field.

## Testing (TDD — red → green → refactor)

- **Mapper:** raw Gryzzly DTO → catalog DTO (incl. tolerant parsing of bad/missing fields).
- **Connector:** status → `ConnectorError` mapping; `429` retry behavior.
- **`sync_gryzzly`:** upsert creates/updates; **empty fetch skips prune** (rows intact);
  soft-delete preserves a row referenced by a `task.gryzzly_task_id`.
- **`Source`:** round-trip every variant through `source_to_str`/`source_from_str`.
- **Assignment:** set / clear; snapshot of `gryzzly_project_id`; the three stale states surface
  correctly on the Task object.

## Open prerequisite

Before the connector task in the implementation plan, verify against the live Gryzzly API (with a
real key): auth scheme, base path, pagination mechanism, rate limits, and whether tasks carry an
active/archived flag. Adjust `fetch_*` and the `is_active` derivation accordingly.
