# Gryzzly Sync State Indicator + Terminated-Project Symbol

**Date:** 2026-08-10
**Status:** Design — awaiting review
**Author:** (brainstormed with Claude)
**Builds on:** `2026-08-10-gryzzly-internal-api-auth-design.md` (branch `gryzzly-internal-api-auth`)

## Problem

Two gaps, both surfaced by making the Gryzzly sync actually run for the first time.

### 1. The sync-state indicator cannot tell the truth

`frontend/src/components/sync/SyncStatusBar.tsx` already renders one coloured dot per source
on the Dashboard, so Gryzzly appears there automatically. But three distinct situations collapse
into one red **Error** dot with the reason readable only by hovering for a `title` tooltip:

- **never configured** — `sync.rs:764` records this as `status = error` with
  `error_message = "Not configured"`. There is no `NotConfigured` status; an unconfigured
  connector is reported as a failure. This misleads for Jira, Outlook and Excel too, which share
  the same `update_sync_error(…, "Not configured")` call.
- **session expired** — the Gryzzly cookie has a fixed 7-day life, so this is the state the user
  will meet most often. Its message ("the Gryzzly session cookie expired on … — log in again on
  app.gryzzly.io") is precise and actionable, and currently invisible without a hover.
- **a real failure** — network, keyring, API error.

Consequence: an expired session is discovered by noticing a stale catalog, not by looking at the app.

### 2. A terminated project is indistinguishable from a deleted task

`sync_gryzzly` calls `fetch_projects(active_only = true)`, so projects with `status = "done"` never
enter the catalog. Their tasks fall out of the synced batch, `soft_prune_missing` sets
`is_active = 0`, and `buildPickerOptions` (`frontend/src/lib/gryzzly-picker-options.ts`) renders any
aplan task still assigned to one as `stale: true` — the exact same rendering as a task that was
deleted in Gryzzly. Observed scale: the live sync produced 15 projects with active tasks out of 20
active projects, against 37 projects total, so **17 done projects are invisible**.

`gryzzly_tasks` has no project-status column, so nothing can currently tell the two apart.

## Decisions taken

| Question | Answer |
|---|---|
| Scope of "sync project terminate" | Sync done projects into the catalog and mark them — not a sync-run marker |
| Tasks on a done project in the picker | Visible, marked, **still selectable** |
| Symbol | A muted `[terminé]` badge beside the project name |
| Not-configured state | Model it as a real status (A2), not a frontend string match (A1) |

Selectable-not-blocked is deliberate: a project routinely closes while time declarations are still
owed on it, and refusing the assignment would push the user back into the Gryzzly UI.

## Part A — `SyncSourceStatus::NotConfigured`

`domain::types::SyncSourceStatus` gains a fifth variant:

```rust
pub enum SyncSourceStatus { Idle, Syncing, Success, Error, NotConfigured }
```

- `conversions.rs` maps it to/from `"not_configured"`.
- `sync.rs` — the four `else` branches that today call
  `update_sync_error(sync_repo, user_id, Source::X, "Not configured")` instead write status
  `NotConfigured`. The message stops carrying the state as prose.
- `SyncSourceStatusGql` gains `NotConfigured`; `schema.graphql` is regenerated.
- **Migration**: `sync_status.status` has its own CHECK, `('idle','syncing','success','error')`,
  so the new value needs the same table rebuild 015 performed for `source`. Both live in one
  migration with Part B's column (§C).

### Closing the bug class, not just the bug

015 added `sync_status_accepts_every_source_variant`, which enumerates `Source`. That test would
**not** have caught this one: it guards the `source` column only, and the same enumerated-CHECK trap
exists on `status`. So this design extends the guard to both columns —
`sync_status_accepts_every_status_variant`, enumerating `SyncSourceStatus`. Three instances of this
bug have now been found in this codebase (`alerts.alert_type` in 013, `sync_status.source` in 015,
`sync_status.status` here); the pair of tests is what stops a fourth.

### Frontend

`SyncStatusBar` changes, all driven by typed data:

- `NOT_CONFIGURED` → grey hollow dot, label `Non configuré`. Not a red error.
- Any source in `ERROR` renders its `errorMessage` inline beneath the dot row instead of tooltip-only,
  so the expiry date and the instruction are readable at a glance.
- When a Gryzzly row is `ERROR` or `NOT_CONFIGURED`, a `Reconnecter` link opens
  `https://app.gryzzly.io` in a new tab. This is the one Gryzzly-specific affordance, justified by
  the 7-day cookie making re-login a routine chore rather than an incident.

The existing `getStatusDotColor` / `getStatusLabel` switches already have `default` arms, so unknown
values keep degrading to grey/Idle rather than crashing.

## Part B — Terminated projects in the catalog

### Fetch all live projects

`sync_gryzzly` switches to `fetch_projects(false)` so done projects are included, and
`HttpGryzzlyClient::fetch_projects` gains an explicit exclusion of soft-deleted projects
(`deleted_at` non-null) — those must stay out of the catalog whatever `active_only` says. Cost:
`expandedProjectMetrics.get` runs once per non-deleted project, ~37 calls instead of 20, and the
catalog roughly doubles.

### Unfold `is_active`

Today `map_task(raw, project_active)` ANDs the project's activeness into the task's own. That fold
is precisely what makes a closed project look like a deleted task. New rule:

```rust
is_active = completed_at.is_none() && deleted_at.is_none()
```

— the task's own liveness, nothing else. `map_task`'s `project_active` parameter goes away, and
`fetch_tasks` no longer passes `true` with an explanatory comment.

**This is a deliberate semantic change to an existing column, with a visible one-off effect:** on the
next sync, rows that were deactivated *only* because their project was done flip back to
`is_active = 1`. The active count jumps. That is the correction, not a regression — but it should not
surprise anyone reading the numbers.

### Carry project status separately

New column `gryzzly_tasks.project_status TEXT`, values `active` | `done`, **NULL for pre-existing
rows**. NULL reads as "unknown, treat as active": a row imported by the old
`scripts/gryzzly/import_catalog.py` predates the column and must not suddenly render as terminated.

- `application::services::GryzzlyProject` gains `status: Option<String>` — the raw API value,
  alongside the existing derived `is_active`. The alternative was to infer doneness from
  `!is_active`, which happens to work once soft-deleted projects are excluded, but makes a rendered
  badge depend on a two-step inference across two layers. Carrying the string is one redundant field
  against a silent mis-render.
- `domain::GryzzlyCatalogEntry` gains `project_status: Option<String>`.
- `SqliteGryzzlyCatalogRepository::upsert` writes it; `list_active` and
  `find_by_gryzzly_task_id` select it.
- `sync_gryzzly` already builds a project map to denormalise `project_name` / `customer_name`; it
  fills `project_status` from that map's `status` — so no new plumbing through `GryzzlyTask`.

`soft_prune_missing` is untouched: done projects are now *in* the batch, so their tasks are no longer
pruned, which is the whole point.

### Expose and render

- `GryzzlyTaskGql` gains `projectStatus: String` (nullable), mirroring the column.
  `backend/crates/cli/graphql/schema.graphql` must be regenerated for both this field and the new
  `SyncSourceStatusGql` value, or the CLI's `graphql-client` codegen fails at build time.
  **Regenerate deliberately**: `cargo run -p api -- export-schema` builds the pool first, so it
  applies pending migrations to the real `aggregated_plan.db` as a side effect. Run it knowing that,
  or hand-edit the two additions.
- `GryzzlyOption` and `AssignedGryzzlyTask` in `gryzzly-picker-options.ts` gain
  `projectStatus?: string | null`. `buildPickerOptions` carries it through unchanged — sorting and
  the assigned-task pinning stay as they are.
- A shared `frontend/src/components/gryzzly/TerminatedBadge.tsx` renders the muted `[terminé]` pill,
  used by `GryzzlyTaskPicker` (dropdown rows) and `TaskEditSheet` (the assigned line). One component
  rather than two inline spans, so the two surfaces cannot drift apart.
- The badge shows when `projectStatus === 'done'`. It is independent of `stale`: a task can be both
  terminated (project closed) and stale (gone from the catalog), and the two mean different things.

```
Gryzzly task picker

  Pilotage          — Canal Plus / Refonte
  Développement     — Canal Plus / Refonte
  Recette           — Saft / CI-CD  [terminé]
  Cadrage           — Saft / CI-CD  [terminé]

Task edit sheet

  Gryzzly:  Recette — Saft / CI-CD  [terminé]
```

`ProjectSummarySidebar` and `TimesheetTimeline` also display Gryzzly project names. They are **out of
scope**: they aggregate by project for a chosen day, where a closed project is not a decision point.
Revisit only if the badge proves useful in the picker first.

## Part C — One migration

`016_add_project_status_and_not_configured.sql` does both, in the order below:

1. `ALTER TABLE gryzzly_tasks ADD COLUMN project_status TEXT;` — plain add, no CHECK, NULL default.
   No constraint on the value: the API is the authority on its own vocabulary, and 013/015 are the
   record of what enumerating an external vocabulary in a CHECK costs.
2. Rebuild `sync_status` so the `status` CHECK admits `not_configured`, following 013's documented
   procedure exactly as 015 did — same inventory (no explicit index, nothing references the table),
   same reliance on sqlx's per-migration transaction, same deferral of `PRAGMA foreign_key_check` to
   the test suite.

**Deployment note carried over from the previous design:** applying a migration makes the DB refuse
an older binary (`Migrate(VersionMissing(N))`). Install the new binary and let it migrate; do not
point a stale binary at a migrated database.

## Testing

Domain / application:

- `map_task` no longer folds project activeness: a task with null timestamps is active even when its
  project is done; a completed or deleted task is inactive regardless.
- `sync_gryzzly` writes `project_status` from the project map, and `done` projects' tasks survive
  `soft_prune_missing`.
- `conversions.rs`: `NotConfigured` round-trips through `"not_configured"`.

Infrastructure:

- `sync_status_accepts_every_status_variant` — enumerates `SyncSourceStatus`, the sibling of 015's
  source test. Both must exist.
- Repository: `upsert` persists `project_status`; a pre-existing row with NULL survives a round-trip
  as NULL rather than becoming `'active'`.
- `fetch_projects` excludes soft-deleted projects even with `active_only = false`.

Frontend (Vitest):

- `buildPickerOptions` carries `projectStatus` through, and still pins an assigned task that is
  absent from the active list.
- `GryzzlyTaskPicker` renders `[terminé]` exactly for `projectStatus === 'done'`, and the row stays
  selectable — a click still fires the assign mutation.
- A task that is both terminated and stale shows both markers.
- `SyncStatusBar`: `NOT_CONFIGURED` renders grey and is not labelled an error; an `ERROR` row shows
  its `errorMessage` inline; the Gryzzly `Reconnecter` link appears for both states and not for
  a healthy source.

## Documentation

`SPEC_TECHNIQUE.md` — §10.6 (project fetch now includes `done`, the `is_active` semantic change, the
new column), the config/status vocabulary, and the migration table row for 016.
`SPEC_FONCTIONNELLE.md` — US-006 gains the terminated-project marking and the sync-state states.

## Out of scope

- Blocking assignment to a terminated project.
- The badge in `ProjectSummarySidebar` / `TimesheetTimeline`.
- Any write to Gryzzly — unchanged from the previous design, and still absolute.
- Filtering container tasks out of the picker, still a possible follow-up.
