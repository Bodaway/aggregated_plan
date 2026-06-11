# Design — Claude logs to the worklog, with self-closing time blocks

**Date:** 2026-06-11
**Status:** Approved (design phase)
**Scope:** `aplan` CLI, backend (domain/application/api), session hooks, `aplan` skill (`SKILL.md`).

## Problem

Today, when a Claude Code session is linked to an Aggregated Plan task:

1. Claude logs progress with `aplan note`, which calls `appendTaskNotes` → appends free
   text to the task's **`notes`** field. The user wants Claude's progress to go to the
   **worklog** (the timestamped `WorklogEntry` system) instead.
2. The session "link" is an **open activity slot** created by `aplan start`
   (`startActivity`, `end_time = NULL`). There is **no SessionEnd/Stop hook**, so the slot
   is never closed automatically — it stays open until the user manually runs `aplan stop`
   or `aplan done`. This is the "task track that stays open until we stop work" the user
   wants to eliminate.

There is already a full `WorklogEntry` system (timestamped entries with `logged_at`,
`add_worklog_entry` / `list_worklog_entries` use cases, GraphQL mutations + query, frontend
usage) — but **the `aplan` CLI exposes no command to write to it**.

## Goals

- Claude's incremental logging writes **worklog entries**, not the `notes` field.
- Time spent on the task is tracked as **closed activity slots only** — nothing is ever
  left in a "running" (`end_time = NULL`) state in the database.
- Multi-day correctness: a session left linked across days produces **one block per day,
  per half-day, bounded by the actual worklog-entry timestamps** — idle nights/weekends are
  never counted.
- The `aplan` skill (`SKILL.md`) and the SessionStart hook are updated to match.

## Non-goals

- Re-deriving `date` / `half_day` for the *existing* paths (`start_activity`, the
  `createActivitySlot` UI mutation) — those keep their current UTC-based behavior. Only the
  new worklog-derived materialization path is timezone-correct.
- Changing how the frontend reads/writes worklog entries or activity slots.
- Migrating historical `notes`-field content into worklog entries.

## Chosen model

**Worklog entries are the source of truth. Activity slots are a derived projection,
materialized at lifecycle boundaries.**

- **Worklog entry** (`WorklogEntry`, exists) — durable record of *what* Claude did and
  *when* (`logged_at`). Replaces the `notes` field for Claude's logging.
- **Activity slot** — always written **closed** (start + end), derived from entries. Never
  opened live, so nothing can dangle.
- **Active-task pointer** — two `configuration` keys replace the open slot as the session
  link:
  - `aplan.active_task_id` — the task this session is logging against.
  - `aplan.active_since` — watermark: entries with `logged_at >= active_since` have not yet
    been materialized into slots.
- **Local timezone** — a `configuration` key `aplan.timezone` (IANA name, e.g.
  `Europe/Paris`, default `Europe/Paris`). Day and half-day boundaries are computed in this
  zone, DST-aware, so multi-day splits land on the correct local calendar day.

Rejected alternative: a pure derived (never-materialized) view. Simpler and fully
idempotent, but the existing workload (R16), journal, and dashboard features read
`activity_slots`, so time would disappear from those views.

## CLI surface (`backend/crates/cli`)

| Command | Before | After |
|---|---|---|
| `aplan log "<text>"` | — (new) | `addWorklogEntry` on active task (or `--task TARGET`). Claude's logging verb. |
| `aplan note "<text>"` | `appendTaskNotes` → `notes` field | **Unchanged.** Kept for the user's manual free-text notes. |
| `aplan start <task>` | opens an activity slot | sets `aplan.active_task_id` + `aplan.active_since`; if a *different* task was active, flushes it first. No open slot. |
| `aplan stop` | closes the open slot | flushes the active window into closed slots, then clears the pointer keys. |
| `aplan done [task]` | completes + stops open slot | completes + flush + clear pointer. |
| `aplan current` | reads `currentActivity` (open slot) | reads `aplan.active_task_id` and resolves the task. |

`log` resolves its target like `note`: explicit `--task`, else the active pointer; exit
code `4` (precondition failed) when neither is available.

## Backend changes

Layered per DDD (`domain` → `application` → `api`).

### domain (`crates/domain`)
- Pure function `derive_time_blocks(local_times: &[NaiveDateTime]) -> Vec<LocalBlock>` where
  `LocalBlock { start: NaiveDateTime, end: NaiveDateTime, date: NaiveDate, half_day }`. It
  operates on **local** naive datetimes (the application layer does the UTC→local
  conversion), so the domain crate stays pure (no `chrono-tz` dependency):
  - group by calendar day (`date()` of the local naive datetime);
  - within a day, split at the half-day boundary (morning ≤ 12:00, afternoon ≥ 13:00) using
    the existing `rules::workload::half_day_of`;
  - each (day, half-day) group with ≥1 entry → one block from its min to its max time;
  - a group whose min == max (single entry) gets a minimal non-zero end (e.g. +1 minute) so
    the existing `end > start` validation passes.
- Unit-tested in isolation (single day, multi-day, AM-only, PM-only, crossing noon, single
  entry, empty input).

### application (`crates/application/use_cases`)
- `materialize_worklog_time(worklog_repo, activity_repo, config_repo, user_id, task_id, from, now)`:
  1. resolve the timezone from `aplan.timezone` (default `Europe/Paris`) via `chrono-tz`;
  2. `list_worklog_entries` filtered by `task_id` and `logged_at ∈ [from, now]`;
  3. convert each entry's `logged_at` (UTC) to a local `NaiveDateTime` in that zone;
  4. `derive_time_blocks` over the local times;
  5. for each block, convert the local `start`/`end` back to UTC instants (DST-aware) and
     write a closed `ActivitySlot` with the block's local `date` / `half_day` (not the
     UTC-derived ones — so this path bypasses `create_manual_activity_slot`'s UTC derivation
     and constructs the slot directly, or calls a new `create_slot_with_classification`);
  6. return the new watermark (`now`).
- `chrono-tz` is added to the **application** crate only; the `domain` crate stays pure.
- Idempotency: the caller advances `aplan.active_since` to the returned watermark, so the
  same entries are never materialized twice. Append-only — no slot is rewritten or deleted.

### api (`crates/api/graphql`)
- One mutation: `flushWorklogTime(taskId: ID!): FlushResult` wrapping the use case, where
  `FlushResult { activeSince: DateTime!, slotsWritten: Int! }`. The CLI uses `activeSince`
  to advance the watermark and `slotsWritten` for user feedback.
- New CLI GraphQL ops: `addWorklogEntry` (for `log`) and `flushWorklogTime` (for the
  lifecycle commands). The `configuration` query/mutation already exist for the pointer.

## Hooks (`~/.claude/hooks`, `~/.claude/settings.json`)

- **New `aplan-session-end.sh`** (SessionEnd event): if `aplan.active_task_id` is set, call
  `flushWorklogTime` for it. Keeps the pointer (the task stays linked for the next session);
  only the watermark advances. Silent no-op when the CLI/backend is unavailable, matching
  the session-start hook.
- **`aplan-session-start.sh`**: read `aplan.active_task_id` (via `aplan current`) instead of
  `currentActivity`. Swap the logging instruction `aplan note` → `aplan log` in `base_rules`.
  Remove the "running worklog IS the link" phrasing (the pointer is now the link).

## `aplan` skill (`SKILL.md`) — explicitly in scope

- Hot-path recipe table: `"log a note about X"` → `aplan log --json "X"` (was `aplan note`).
  Add a row distinguishing **`aplan log`** (worklog entry — Claude's progress) from
  **`aplan note`** (manual note on the `notes` field — the user's).
- Document the new lifecycle: `start` sets the link, time is materialized as closed blocks
  on `stop`/`done`/session-end, and there is never an open timer.
- Update the "currently-tracked task" / `current` description to reflect the pointer.

## Lifecycle flows

```
aplan start SCB-457      -> active_task_id=SCB-457, active_since=now   (no slot)
aplan log "root cause"   -> WorklogEntry(logged_at=now)
aplan log "fix pushed"   -> WorklogEntry(logged_at=now)
SessionEnd hook          -> flushWorklogTime(SCB-457):
                              derive blocks from entries in [active_since, now],
                              write closed slots, active_since := now
                            (pointer kept — task still linked)
... next day, new session ...
aplan log "tests green"  -> WorklogEntry
aplan done               -> flush (covers the new day's entries) + clear pointer
```

Crash (SessionEnd didn't fire): `active_since` did not advance, so the orphaned entries are
still in `[active_since, now]` and the next flush (next session's start-switch or end)
materializes them. Self-healing.

## Edge cases

- **Multi-day**: per-day, per-half-day blocks from real entry times (Mon 14:02→15:30; Tue no
  entries → no block; Wed 09:10→11:45).
- **Switching tasks** mid-session: `aplan start <other>` flushes the previous task before
  repointing.
- **No entries since last flush**: flush writes nothing.
- **Single entry in a half-day**: block gets a minimal non-zero duration.
- **Double flush**: watermark prevents recounting.
- **Local timezone / DST**: `date` and `half_day` for materialized blocks are computed in
  `aplan.timezone` (default `Europe/Paris`), DST-aware via per-timestamp conversion — so
  late-evening or early-morning work lands on the correct local day, and a span crossing a
  DST change is converted correctly. The existing `start_activity` / `createActivitySlot`
  paths keep their current UTC behavior (out of scope).

## Testing strategy

- **domain**: unit tests for `derive_time_blocks` (all grouping/splitting cases above).
- **application**: `materialize_worklog_time` against in-memory repos — entries → expected
  closed slots; watermark advance; empty window; multi-day.
- **cli**: `tests/integration.rs` — `log` writes an entry; `start`/`stop` set/clear the
  pointer and flush; `current` reads the pointer.
- **hooks**: manual verification (session-end flush; session-start reads pointer).

## Files touched (anticipated)

- `backend/crates/domain/src/rules/` — `derive_time_blocks` over local naive datetimes (+ tests).
- `backend/crates/application/` — `materialize_worklog_time` use case (+ tests); add
  `chrono-tz` dependency and a slot constructor that accepts explicit `date`/`half_day`.
- `backend/crates/api/src/graphql/` — `flushWorklogTime` mutation; schema regen.
- `backend/crates/cli/src/` — `cli.rs` (`Log` subcommand), `commands.rs` (`log`, updated
  `start`/`stop`/`done`/`current`), `queries.rs` (new GraphQL ops).
- `~/.claude/hooks/aplan-session-end.sh` (new), `aplan-session-start.sh` (edit),
  `~/.claude/settings.json` (register SessionEnd hook).
- `~/.claude/skills/aplan/SKILL.md`.
- `SPEC_FONCTIONNELLE.md` / `SPEC_TECHNIQUE.md` per the repo's spec-maintenance rule.
