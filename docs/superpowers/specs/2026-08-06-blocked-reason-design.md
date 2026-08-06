# Design — Mandatory reason on blocked tasks

**Date:** 2026-08-06
**Status:** Approved (design phase)
**Scope:** migration 015, domain (`TaskStatus` rules, `BlockedReason`), application
(`task_management`, `sync`, `recurrence`), API (GraphQL `Task` / `UpdateTaskInput`),
`aplan` CLI (`status` command), frontend (`StatusMenu.tsx`), both spec files.

## Problem

A task can be moved to `TaskStatus::Blocked` from four places — the GraphQL `updateTask`
mutation, the `aplan status` command, the React `StatusMenu`, and the Jira/Excel sync — and
none of them records *why*. A blocked task is indistinguishable from a stalled one: the
dashboard counts it as outstanding work (`dashboard.rs:270`) and the brief surfaces it
(`brief.rs:63`), but neither can say what is being waited on, so the information has to be
reconstructed from memory every time.

The request was initially framed as a new "en attente" status. Investigation showed
`TaskStatus::Blocked` already exists (`domain/src/types/common.rs:28`) and the sync already
maps both `"Blocked"` and `"bloqué"` onto it (`sync.rs:978`). A sixth status would have
duplicated it. What is missing is not a status but the reason attached to it.

## Decision

`Blocked` carries a mandatory, non-empty reason. The reason exists only while the task is
blocked, and is cleared when the task leaves that status.

**Invariant:** `blocked_reason.is_some()` ⇔ `status == Blocked`.

It is bidirectional on purpose. A one-way rule (`blocked ⇒ reason`) would leave stale reasons
on unblocked tasks and make "is this task blocked?" answerable two different ways.

## Model

A newtype in the domain, so that "a reason exists" always means "a non-empty reason exists":

```rust
pub struct BlockedReason(String);

impl BlockedReason {
    pub fn new(raw: &str) -> DomainResult<Self>;  // trims; rejects empty/whitespace
    pub fn as_str(&self) -> &str;
}
```

Rejection uses the existing `DomainError::ValidationError(String)` variant — no new variant is
needed for the empty case. The pairing failures do get their own variants, because callers
(GraphQL, CLI) must distinguish them to produce useful messages:

```rust
#[error("A reason is required to block a task")]
BlockedReasonRequired,
#[error("A blocked reason only applies to blocked tasks, got {0:?}")]
BlockedReasonNotApplicable(TaskStatus),
```

`Task` gains `pub blocked_reason: Option<BlockedReason>`.

### Rejected alternative: payload on the enum

`TaskStatus::Blocked(BlockedReason)` would make the invalid state unrepresentable, which is the
stronger design. It is not worth its cost here: `TaskStatus` is `Copy` and is matched, compared
and stored by value across 26 files. Removing `Copy` propagates through all of them for a
guarantee that the choke point below plus the database triggers already provide.

## Enforcement — one choke point

`Task` is a plain struct with public fields, mutated in place (`task.status = status`,
`task_management.rs:184`). There is no smart constructor to hang the invariant on, and adding
one is out of scope. The invariant is therefore enforced by making one function the only
sanctioned way to change a task's status:

```rust
// domain/src/rules/status.rs
pub fn apply_status(
    task: &mut Task,
    next: TaskStatus,
    reason: Option<BlockedReason>,
) -> DomainResult<()>;
```

| `next` | `reason` | Result |
|---|---|---|
| `Blocked` | `Some` | sets both; replaces an existing reason if already blocked |
| `Blocked` | `None` | `Err(BlockedReasonRequired)` |
| other | `None` | sets status, clears `blocked_reason` |
| other | `Some` | `Err(BlockedReasonNotApplicable(next))` |

Note the third row: leaving `Blocked` clears the reason as a side effect of the normal status
change, so no caller has to remember to do it.

Four call sites route through it: `update_task` (`task_management.rs:183`), `complete_task`
(`task_management.rs:331`), the sync upsert, and recurrence instantiation. Direct assignment to
`task.status` outside `apply_status` is the thing code review must catch; the database triggers
below are the backstop if one slips through.

## Migration 015

Two statements plus a backfill:

1. `ALTER TABLE tasks ADD COLUMN blocked_reason TEXT;`
2. Backfill every existing `blocked` row — one in the current database, verified by
   `SELECT status, COUNT(*) FROM tasks GROUP BY status` — with the literal
   `Raison non renseignée (migration 015)`. It runs before the triggers are created, so the
   table is never left in a state the guard would reject.
3. Two triggers, `BEFORE INSERT` and `BEFORE UPDATE OF status, blocked_reason`, each raising
   `ABORT` when the invariant is violated:

```sql
CREATE TRIGGER tasks_blocked_reason_insert BEFORE INSERT ON tasks
WHEN (NEW.status = 'blocked') <> (NEW.blocked_reason IS NOT NULL AND trim(NEW.blocked_reason) <> '')
BEGIN SELECT RAISE(ABORT, 'blocked tasks require a non-empty blocked_reason'); END;
```

**Why triggers rather than a `CHECK`.** SQLite cannot add a `CHECK` to an existing table, so a
`CHECK` means the full table-rebuild dance that `007_add_recurrence.sql` performed. The `tasks`
table has since grown to 31 columns with foreign keys and indexes; rebuilding it means
re-declaring every one of them, and a column omitted by accident is silent data loss. Triggers
give the identical guarantee — writes violating the invariant abort — with no rebuild. This is
an implementation choice within the approved decision of "enforce it at the database level",
not a weakening of it.

## Surfaces

**GraphQL.** `Task.blockedReason: String` in the read type.
`UpdateTaskInput.blockedReason: MaybeUndefined<String>`, following the pattern already used by
`delegated_to` and `planned_start` (`types/task.rs:227,240`): omitted leaves it unchanged, an
explicit value sets it, explicit null clears it. `updateTask` maps the domain errors onto
GraphQL errors rather than panicking.

**CLI.** `aplan status blocked --reason "attente retour client"`. If `--reason` is absent and
`stdin` is a TTY, prompt for it; if it is not a TTY — a script, a hook, an agent — fail with a
non-zero exit rather than hang. `--reason` on a non-blocked state is a usage error. `aplan ls`
and `aplan show` display the reason on blocked rows.

**React.** Selecting *Blocked* in `StatusMenu` reveals an inline text field with confirm and
cancel; the mutation fires only once a non-empty reason is entered. Choosing any other status
fires immediately as it does today. The reason shows on the blocked badge.

**Sync.** `map_jira_status` and `map_excel_status` return `(TaskStatus, Option<BlockedReason>)`,
generating a reason such as `Bloqué dans Jira (statut : Impediment)` from the raw source status.

The sync writes that generated reason **only on the transition into `blocked`**. If the task is
already blocked, the stored reason is left alone — otherwise every sync run would overwrite a
hand-written reason with a generated one.

## Tests

Written before the production code, per the project's TDD convention.

*Domain* — the six `apply_status` cases in the table above, plus `BlockedReason::new` rejecting
`""` and `"   "` and trimming `"  foo  "`.

*Infrastructure* — round-trip of a blocked task with its reason; both triggers reject their
invalid write (blocked with null reason, blocked with whitespace reason, non-blocked with a
reason); leaving `blocked` persists a null reason.

*Application* — sync sets the generated reason when a task transitions into blocked, and leaves
an existing reason untouched when the task was already blocked; `complete_task` on a blocked
task clears the reason.

*API* — `updateTask` to `BLOCKED` without a reason returns a GraphQL error; with one, it
succeeds and the reason is readable back.

*CLI* — `status blocked` without `--reason` and without a TTY exits non-zero; with `--reason` it
succeeds; `--reason` alongside `todo` is a usage error.

## Spec updates

`SPEC_FONCTIONNELLE.md` gains the business rule (blocking requires a reason, the reason
disappears on unblock, the sync generates one). `SPEC_TECHNIQUE.md` gains the `blocked_reason`
column, the triggers, and the GraphQL contract change. Both in the same commit as the code, per
CLAUDE.md.

## Risks

The choke point is a convention, not a compiler-enforced rule: nothing stops future code from
assigning `task.status` directly. The database triggers turn that mistake into a loud runtime
abort instead of silent corruption, which is the mitigation; making it a compile-time guarantee
is the rejected enum-payload design.

The frontend gains its first two-step status transition. If the inline field proves awkward in
use, the fallback is a small modal — a presentation change that touches neither the contract nor
the invariant.
