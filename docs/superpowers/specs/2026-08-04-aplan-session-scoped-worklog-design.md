# Design — Session-scoped worklog, concurrent sessions on different tasks

**Date:** 2026-08-04
**Status:** Approved (design phase)
**Scope:** migration 014, backend (domain/application/api), `aplan` CLI, session hooks
(`~/.claude/hooks/aplan-session-start.sh`, `aplan-session-end.sh`), `aplan` skill (`SKILL.md`).

## Problem

The link between a Claude Code session and an Aggregated Plan task is a **single global
pointer** — two `configuration` keys, `aplan.active_task_id` and `aplan.active_since`
(`crates/cli/src/commands.rs:63`). Three failures follow from that.

1. **A session's decision is not persistable.** Answering "Ne pas tracker" at SessionStart
   writes nothing anywhere, so a hook re-fire mid-session re-injects "Currently tracking
   aplan task: …" read off the global pointer, and the model reports tracking a task the
   user explicitly opted out of. Observed 2026-08-04.
2. **Two sessions cannot work on two tasks.** The second `aplan start` overwrites the
   pointer, so the first session's subsequent `aplan log` calls land on the second
   session's task.
3. **The watermark is shared across all tasks.** `flushWorklogTime` reads `from` off the
   single `aplan.active_since` key and advances it (`crates/api/src/graphql/mutation.rs:199-222`).
   Flushing task B therefore advances the watermark for task A too, and A's entries logged
   before that flush are never materialized into activity slots. This one loses time even
   without concurrency, whenever two tasks interleave.

## Goals

- One Claude Code session logs to **its own** task; N sessions run concurrently on N tasks
  without interfering.
- A session's tracking decision — including "do not track" — **persists** for the life of
  that session and is what the hook reports back on any re-fire.
- Materializing worklog time is **order-independent and idempotent**: no shared watermark,
  concurrent flushes converge, a backdated entry is still picked up.
- Worklog entries and activity slots carry **who produced them**, so the day can be read
  per actor.
- Overlapping time across tasks is **visible**, never silently corrected.

## Non-goals

- **No automatic proration of overlapping time.** Explicit product decision: each task
  gets the time its own entries document, double counting is accepted and flagged, and the
  arbitration stays with the human at the `aplan timesheet` review that already exists.
- No alert row for overlaps. Display only, in `journal` / `timesheet` / `dash`.
- No frontend work. The new columns and the `sessions` query are exposed on the GraphQL
  schema, but no UI consumes them in this iteration.
- No multi-machine or multi-user concern. Single user, single loopback backend.

## Chosen model — two natures of actor

The user's mental model becomes the code's model:

- **The global pointer is the human, working by hand.** `aplan.active_task_id` /
  `aplan.active_since` keep their current meaning and their current single-task-at-a-time
  behaviour. Unchanged.
- **A session is a Claude.** One row per Claude Code session, each with its own task, its
  own mode, its own flush window.

Nothing merges the two. The human's pointer never moves because a Claude switched task,
and no Claude's task moves because the human did.

A worklog entry carries its author: `session_id` NULL means the human.

## Schema — migration 014

```sql
CREATE TABLE sessions (
  id            TEXT PRIMARY KEY,               -- CLAUDE_CODE_SESSION_ID
  user_id       TEXT NOT NULL REFERENCES users(id),
  task_id       TEXT REFERENCES tasks(id) ON DELETE SET NULL,
  mode          TEXT NOT NULL CHECK (mode IN ('tracking','off')),
  label         TEXT,                           -- hook payload `cwd`, for display
  started_at    TEXT NOT NULL,
  last_seen_at  TEXT NOT NULL,
  last_flush_at TEXT,
  ended_at      TEXT
);
CREATE INDEX idx_sessions_user_open ON sessions(user_id, ended_at);

ALTER TABLE worklog_entries ADD COLUMN session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL;
ALTER TABLE activity_slots  ADD COLUMN session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL;
ALTER TABLE activity_slots  ADD COLUMN source     TEXT;   -- 'worklog' | 'manual'
```

SQLite constraints on `ALTER TABLE ADD COLUMN` shape this: an added column carrying
`REFERENCES` must default to NULL, and the enum on `source` is **enforced in Rust**
(`SlotSource`), not by a SQL `CHECK` — same choice as `worklog_entries.consolidated_at` in
migration 012, and it avoids the table rebuild that fixing a `CHECK` costs in SQLite
(cf. migration 013). The `CHECK` on `sessions.mode` is fine: fresh `CREATE TABLE`.

`source` is nullable in SQL and read as "unclassified"; the classification pass below fills
every existing row, and every write path sets it explicitly afterwards. A NULL that survives
anyway — a row written by an older binary against a migrated DB — **is read as `'manual'`**,
so the failure mode of the unexpected is a slot that does not get rebuilt, never a slot that
gets deleted.

### Classifying pre-014 slots — one-shot, precise, not heuristic

The rebuild in §5 deletes a task's `source='worklog'` slots before rewriting them.
Applied to historical rows this is a fork with no free branch: blanket-marking them
`'worklog'` risks destroying a slot the user created by hand in the UI, and blanket-marking
them untouchable risks double counting on the cutover day (a half-day flushed before the
migration keeps its old slot *and* gains a rebuilt one).

Neither is necessary, because the provenance is recoverable from the data. A pre-014 closed
slot is flush-derived **iff its span coincides with a block `derive_time_blocks` yields from
its task's worklog entries** — that function is the only thing that ever wrote those spans.
So migration 014 is followed by a one-shot classification pass, in Rust because it needs the
domain rule:

- for each existing closed slot with a `task_id`, recompute its task's blocks over its local
  day and compare span (with the `MIN_BLOCK_MINUTES` single-timestamp case handled as
  `materialize_worklog_time` handles it) → `'worklog'` on a match, `'manual'` otherwise;
- open slots and slots with no `task_id` → `'manual'` (a running timer is never a projection);
- guarded and made idempotent by a config key, `aplan.slot_source_classified`, so a restart
  does not redo it.

Result: a single rule holds everywhere afterwards — **a rebuild replaces `source='worklog'`
and protects `source='manual'`** — with no cutover special case, no double count, and no
hand-made slot at risk.

### Reattribution alignment

`reattribute_worklog_entries` currently replaces any *closed* slot in the affected
half-days (`is_rebuildable`, `crates/domain/src/rules/reattribution.rs:171`) and documents
the resulting canonicalisation as accepted. It now restricts itself the same way — closed
**and** `source='worklog'` — which is a strict tightening: hand-made slots stop being
collateral. The "What it deliberately does not promise" paragraph of that module's header
(`crates/application/src/use_cases/reattribution.rs:33-47`) is updated in the same commit.

## Target resolution

For `log`, `note`, `status`, `done`, `remember`, and any future verb with an implicit target:

1. `--task <target>` explicit — always wins.
2. Otherwise `--session <id>` (the hooks pass it explicitly) or, absent that,
   `CLAUDE_CODE_SESSION_ID` from the environment → **that session's task**.
   - Session in `mode='off'` → **refuse, exit 4**, with a message naming the reason
     ("session not tracked"). No silent fallback to the global pointer: that fallback is
     exactly how failure 1 produced a wrong claim.
   - Session in `mode='tracking'` with no task → refuse, exit 4.
3. Otherwise → the global pointer (the human).

`CLAUDE_CODE_SESSION_ID` is present in the environment of every Bash tool call, including
subagents' — verified 2026-08-04. Subagents of a session therefore log to that session's
task, which is the intent.

### Semantic change to `start` / `stop`

**`aplan start X` run from inside a Claude session binds the session and no longer touches
the human's pointer** — it flushes the task that session was previously on, then repoints
the session. `aplan stop` symmetrically flushes and unbinds the session only. Run outside a
session, both keep today's behaviour on the global pointer. Approved explicitly; it is the
one behaviour a user could be surprised by.

### New surface

- `aplan sessions` — open sessions: task, label, since, last write, mode. The human's
  pointer is shown as one extra display-only line, so the command answers "who is on what
  right now" in one view.
- `aplan session show|bind|off|end [--session <id>]` — what the hooks drive.
- `aplan current` gains an `actor` field in `--json` (the session id, or `manual`).

GraphQL: `sessions` query; `bindSession`, `setSessionMode`, `endSession` mutations;
`flushWorklogTime` gains an optional `sessionId`. No `touchSession`: `last_seen_at` is
refreshed server-side by any session-scoped write, so a session that is working is a session
that is seen, with nothing extra to call.

## Flush becomes an idempotent rebuild

`flushWorklogTime` stops being "from the watermark to now, append slots". It becomes:

1. The window — the session's `last_flush_at ?? started_at`, or `aplan.active_since` for the
   human — is used for **one purpose only: determining which local half-days the task's
   entries touched.**
2. For each of those half-days: delete that task's `source='worklog'` slots in it, then
   rewrite them from **all** of that task's entries in that half-day (not only the ones in
   the window), via `derive_time_blocks`.
3. Advance `last_flush_at` (or `aplan.active_since`).

The window narrows work; it never decides truth. Truth is always the full set of entries in
the half-day. That yields the properties the concurrency needs:

- **Order-independent.** Flushing task B reads and writes nothing of task A's.
- **Idempotent.** Two flushes of the same task over the same half-day produce the same slots;
  concurrent flushes converge instead of duplicating.
- **Backdate-safe.** An entry logged with a past `logged_at` is picked up, because
  membership is by half-day, not by watermark comparison.
- **Widening the window is free**, which removes the whole class of "the watermark moved past
  unflushed entries" bug rather than relocating it.

The shared primitive is extracted rather than duplicated:
`rebuild_task_projection(user_id, task_id, half_days, tz)` in
`crates/application/src/use_cases/worklog.rs`, called by both the flush and
`reattribute_worklog_entries` — which already performs exactly this operation inline
(`crates/application/src/use_cases/reattribution.rs:233`, `481-499`). This refactor is the
substance of the approach: the correctness argument written in that module's header
(lines 19-31) starts covering the flush too.

`aplan.active_since` keeps a single meaning afterwards: the start of the **human's** window.

## Overlap — visible, never corrected

Nothing stored, nothing repaired. A read-time computation over closed slots of one user:
two slots on *different* tasks whose `[start_time, end_time]` intervals intersect overlap by
the length of the intersection. Pure, so it lives in
`crates/domain/src/rules/` alongside the other projection rules.

Surfaced as:

- `aplan journal` — a line per overlapping pair, both tasks named and the actor identified:
  `⚠ recouvrement 47 min — Saft cadrage ↔ Cartier (session a1b2 ↔ manuel)`.
- `aplan timesheet` — the day's raw total plus its gap against elapsed wall-clock time, so
  the arbitration happens where a human already reviews the day.
- `aplan dash` — one summary line when the day carries any overlap.

The double count remains in the slots, by design. It simply becomes impossible to miss.

## Lifecycle and hooks

**SessionStart** (`aplan-session-start.sh`) reads the `session_id` that Claude Code already
provides on stdin and that the hook currently ignores, then asks `aplan session show
--session <id> --json`:

| State | Injected context |
|---|---|
| Unknown session | Today's mandatory `AskUserQuestion`, then `aplan session bind --session <id> <task>` or `aplan session off --session <id>` |
| Known, `mode='off'` | One line: logging is disabled for this session, do not ask again, never call `aplan log/start/stop/flush` |
| Known, `mode='tracking'` | One line confirming the session's task |
| `source=clear` | Force the re-choice even when known — the user wants that choice explicit at `/clear` |

`source=resume|compact` follows the table (confirm, never re-ask), which is what fixes
failure 1: the injected line now comes from the session's own record instead of the global
pointer.

`last_seen_at` is refreshed on every session-scoped write.

**SessionEnd** (`aplan-session-end.sh`) flushes **that session's** task and sets `ended_at`.
It no longer reads `aplan current`, so it cannot flush another session's task or the human's.

**Zombie sessions** — `kill -9`, a crash, a closed laptop — are closed by the existing
background job (`crates/api/src/jobs.rs`): a session whose `last_seen_at` is older than 12 h
is flushed and gets `ended_at`. The threshold is a config key, `aplan.session_idle_timeout_hours`.

## Testing

**Domain** — overlap detection (disjoint, touching, nested, same-task ignored, multi-pair);
the pre-014 classification comparison (span matches a derived block / does not / single-timestamp
`MIN_BLOCK_MINUTES` case).

**Application** — two flushes of one task over one half-day produce one set of slots
(idempotence); two sessions on two tasks flush without reading or writing each other's slots;
two sessions on the same task converge; an entry backdated under the old watermark is
materialized; a `source='manual'` slot in a rebuilt half-day survives; the classification pass
is idempotent under its guard key.

**API** — `bindSession` / `setSessionMode` / `endSession` round-trip; `flushWorklogTime` with
and without `sessionId`; `sessions` query excludes ended sessions.

**CLI** (`crates/cli/tests/integration.rs`) — resolution order across the three levels;
exit 4 with a named reason in `mode='off'`; `aplan start` inside a session leaves
`aplan.active_task_id` untouched; `aplan sessions` output.

**Hooks** — bash-level test of the four branches of the table above, plus the no-backend and
no-`jq` no-op paths that already exist.

## Risks

- **The classification pass is the only irreversible step.** It writes `source` on every
  historical slot from a comparison against today's entries. Entries edited since a slot was
  flushed will make that slot read as `'manual'` — conservative in the safe direction (it
  will be protected, not destroyed), and the existing `aplan reattribute --from … --to …`
  preview reports any such half-day whose hours look wrong. The DB is 1.7 MB and already has a
  `.bak` convention
  (`aggregated_plan.db.bak-20260803-pre012`); take one before running it.
- **`aplan start`'s new semantics inside a session** could confuse a future reader of a
  transcript ("Claude started task X, why is my pointer still on Y?"). Mitigated by
  `aplan current` naming the actor and `aplan sessions` showing both.
- **Overlap flagging is display-only**, so a day can still be *committed* to Gryzzly with a
  raw total above elapsed time. Accepted: the timesheet review is a human gate, and this
  design's job is to make the overlap legible there.

## Implementation staging

Three plans, each independently shippable and testable:

1. **Sessions socle** — migration 014, `Session` domain type + repository, classification
   pass, GraphQL surface, CLI resolution order + `sessions`/`session` commands. Flush still
   behaves as today.
2. **Idempotent flush** — extract `rebuild_task_projection`, rewire `flushWorklogTime` and
   `reattribute_worklog_entries` onto it, retire the global-watermark read.
3. **Hooks and overlap** — rewrite both hooks against `session show`, overlap rule + its
   display in `journal` / `timesheet` / `dash`, update the `aplan` SKILL.md.
