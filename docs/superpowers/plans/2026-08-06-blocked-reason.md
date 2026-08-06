# Blocked Reason Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A task can only be `blocked` if it carries a non-empty reason, and that reason disappears the moment the task stops being blocked.

**Architecture:** A `BlockedReason` newtype makes "a reason exists" mean "a non-empty reason exists". A single domain function, `apply_status`, becomes the only sanctioned way to change a task's status and enforces the pairing in both directions. Two SQLite triggers make a violating write abort. The Jira/Excel sync generates a reason from the source status, but only when a task first becomes blocked.

**Tech Stack:** Rust (domain/application/infrastructure/api/cli crates), sqlx 0.8 + SQLite, async-graphql 7, clap 4, React 18 + Vitest.

**Spec:** `docs/superpowers/specs/2026-08-06-blocked-reason-design.md`

## Global Constraints

- **Never run workspace-wide cargo commands.** The `mcp` crate does not compile at HEAD. Every build, check and test command in this plan is scoped: `-p domain -p application -p infrastructure -p api -p cli`.
- **TDD is mandatory.** Write the test, run it, see it fail for the right reason, then implement. A step that says "run it to verify it fails" is not optional.
- **Specs are French, code and comments are English.** `SPEC_FONCTIONNELLE.md` and `SPEC_TECHNIQUE.md` must be updated in the same commit as the behaviour they describe (CLAUDE.md).
- **Commit messages:** plain imperative subject, no ticket prefix (no Jira ticket for this work), no `Co-Authored-By` and no `Signed-off-by` trailer.
- **Staging:** stage only the files listed in the task. Never `git add -A`.
- The invariant, referenced throughout: `blocked_reason.is_some()` ⇔ `status == TaskStatus::Blocked`.
- New business rule number is **R64** (`SPEC_FONCTIONNELLE.md` §7 currently ends at R63).
- New migration number is **015** (`migrations/sqlite/` currently ends at `014_create_sessions.sql`).

---

### Task 1: `BlockedReason` newtype and error variants

Self-contained in the `domain` crate. Nothing else compiles against it yet.

**Files:**
- Create: `backend/crates/domain/src/types/blocked_reason.rs`
- Modify: `backend/crates/domain/src/types/mod.rs:16` (add `pub mod blocked_reason;` and `pub use blocked_reason::*;`)
- Modify: `backend/crates/domain/src/errors.rs:22` (two new variants before the closing brace)

**Interfaces:**
- Consumes: `DomainError`, `DomainResult<T>` (existing, `domain/src/errors.rs`)
- Produces:
  - `BlockedReason` — `Debug + Clone + PartialEq + Eq + Serialize + Deserialize`, **not** `Copy`
  - `BlockedReason::new(raw: &str) -> DomainResult<BlockedReason>`
  - `BlockedReason::as_str(&self) -> &str`
  - `DomainError::BlockedReasonRequired`
  - `DomainError::BlockedReasonNotApplicable(TaskStatus)`

- [ ] **Step 1: Write the failing test**

Create `backend/crates/domain/src/types/blocked_reason.rs` containing *only* the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_accepts_a_normal_reason() {
        let r = BlockedReason::new("attente retour client").unwrap();
        assert_eq!(r.as_str(), "attente retour client");
    }

    #[test]
    fn new_trims_surrounding_whitespace() {
        let r = BlockedReason::new("  attente CI  ").unwrap();
        assert_eq!(r.as_str(), "attente CI");
    }

    #[test]
    fn new_rejects_an_empty_string() {
        assert!(BlockedReason::new("").is_err());
    }

    #[test]
    fn new_rejects_whitespace_only() {
        assert!(BlockedReason::new("   \t \n ").is_err());
    }
}
```

Register the module by adding `pub mod blocked_reason;` after line 16 of `types/mod.rs` and `pub use blocked_reason::*;` after line 33.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd backend && cargo test -p domain blocked_reason`
Expected: compilation error, `cannot find type BlockedReason in this scope`.

- [ ] **Step 3: Write the minimal implementation**

Put this above the test module in `blocked_reason.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::errors::{DomainError, DomainResult};

/// Why a task is blocked. Always non-empty and trimmed: constructing one is the
/// only way to get a value, so "a reason exists" never means "an empty string".
///
/// Deliberately not `Copy` — it owns a `String` — which is why `TaskStatus` does
/// not carry it as a payload. See the design doc for that trade-off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedReason(String);

impl BlockedReason {
    pub fn new(raw: &str) -> DomainResult<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(DomainError::ValidationError(
                "A blocked reason cannot be empty".to_string(),
            ));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd backend && cargo test -p domain blocked_reason`
Expected: 4 passed.

- [ ] **Step 5: Add the two error variants**

In `backend/crates/domain/src/errors.rs`, immediately before the closing `}` of `pub enum DomainError` (currently line 23):

```rust
    #[error("A reason is required to block a task")]
    BlockedReasonRequired,
    #[error("A blocked reason only applies to blocked tasks, got {0:?}")]
    BlockedReasonNotApplicable(TaskStatus),
```

`TaskStatus` must be in scope in `errors.rs`. Check the existing `use` line at the top — it already imports id types from `crate::types`; add `TaskStatus` to that import list.

- [ ] **Step 6: Verify the crate still compiles**

Run: `cd backend && cargo test -p domain`
Expected: everything passes, no warnings about unused variants (they are `pub`).

- [ ] **Step 7: Commit**

```bash
git add backend/crates/domain/src/types/blocked_reason.rs \
        backend/crates/domain/src/types/mod.rs \
        backend/crates/domain/src/errors.rs
git commit -m "Add a BlockedReason newtype that cannot hold an empty string"
```

---

### Task 2: `Task.blocked_reason` field and the `apply_status` rule

This is the task that ripples: adding a field to `Task` breaks every struct literal in the workspace (67 of them across 23 files, almost all test fixtures). Step 5 is the mechanical repair, and it is why this task is large — the code does not compile again until it is done.

**Files:**
- Create: `backend/crates/domain/src/rules/status.rs`
- Modify: `backend/crates/domain/src/rules/mod.rs:16` (add `pub mod status;`)
- Modify: `backend/crates/domain/src/types/task.rs:19` (new field after `status`)
- Modify: every file listed by the compiler in Step 5
- Modify: `SPEC_FONCTIONNELLE.md` §7 (rule R64)

**Interfaces:**
- Consumes: `BlockedReason`, `DomainError::BlockedReasonRequired`, `DomainError::BlockedReasonNotApplicable` (Task 1)
- Produces:
  - `Task.blocked_reason: Option<BlockedReason>` — public field, positioned directly after `status`
  - `domain::rules::status::apply_status(task: &mut Task, next: TaskStatus, reason: Option<BlockedReason>) -> DomainResult<()>`

- [ ] **Step 1: Write the failing test**

Create `backend/crates/domain/src/rules/status.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BlockedReason, TaskStatus};

    fn blocked_task() -> Task {
        let mut t = tests_support::sample_task();
        t.status = TaskStatus::Blocked;
        t.blocked_reason = Some(BlockedReason::new("attente retour client").unwrap());
        t
    }

    #[test]
    fn blocking_with_a_reason_sets_both() {
        let mut task = tests_support::sample_task();
        let reason = BlockedReason::new("attente retour client").unwrap();

        apply_status(&mut task, TaskStatus::Blocked, Some(reason.clone())).unwrap();

        assert_eq!(task.status, TaskStatus::Blocked);
        assert_eq!(task.blocked_reason, Some(reason));
    }

    #[test]
    fn blocking_without_a_reason_is_refused() {
        let mut task = tests_support::sample_task();

        let err = apply_status(&mut task, TaskStatus::Blocked, None).unwrap_err();

        assert!(matches!(err, DomainError::BlockedReasonRequired));
        assert_eq!(task.status, TaskStatus::Todo, "the task must be left untouched");
    }

    #[test]
    fn leaving_blocked_clears_the_reason() {
        let mut task = blocked_task();

        apply_status(&mut task, TaskStatus::InProgress, None).unwrap();

        assert_eq!(task.status, TaskStatus::InProgress);
        assert_eq!(task.blocked_reason, None);
    }

    #[test]
    fn reblocking_replaces_the_previous_reason() {
        let mut task = blocked_task();
        let newer = BlockedReason::new("attente CI").unwrap();

        apply_status(&mut task, TaskStatus::Blocked, Some(newer.clone())).unwrap();

        assert_eq!(task.blocked_reason, Some(newer));
    }

    #[test]
    fn a_reason_on_a_non_blocked_status_is_refused() {
        let mut task = tests_support::sample_task();
        let reason = BlockedReason::new("attente retour client").unwrap();

        let err = apply_status(&mut task, TaskStatus::Done, Some(reason)).unwrap_err();

        assert!(matches!(
            err,
            DomainError::BlockedReasonNotApplicable(TaskStatus::Done)
        ));
        assert_eq!(task.status, TaskStatus::Todo, "the task must be left untouched");
    }

    #[test]
    fn moving_between_two_unblocked_statuses_leaves_the_reason_none() {
        let mut task = tests_support::sample_task();

        apply_status(&mut task, TaskStatus::Done, None).unwrap();

        assert_eq!(task.status, TaskStatus::Done);
        assert_eq!(task.blocked_reason, None);
    }
}
```

The tests need a `Task` fixture. Rather than inventing another one, reuse the pattern already used by `domain/src/rules/priority.rs` — open it, copy its inline fixture constructor into a `#[cfg(test)] pub(crate) mod tests_support` block inside `status.rs`, and rename it `sample_task()`. It must return a task with `status: TaskStatus::Todo` and `blocked_reason: None`.

Register the module: add `pub mod status;` to `backend/crates/domain/src/rules/mod.rs`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd backend && cargo test -p domain rules::status`
Expected: compilation error — `apply_status` not found, and `no field blocked_reason on type Task`.

- [ ] **Step 3: Add the field to `Task`**

In `backend/crates/domain/src/types/task.rs`, directly after line 19 (`pub status: TaskStatus,`):

```rust
    /// Why this task is blocked. Non-`None` if and only if `status == TaskStatus::Blocked` —
    /// go through `rules::status::apply_status` rather than assigning `status` directly, and
    /// the pairing takes care of itself. The database enforces it too (migration 015).
    pub blocked_reason: Option<BlockedReason>,
```

- [ ] **Step 4: Write the `apply_status` implementation**

Above the test module in `backend/crates/domain/src/rules/status.rs`:

```rust
use crate::errors::{DomainError, DomainResult};
use crate::types::{BlockedReason, Task, TaskStatus};

/// The only sanctioned way to change a task's status.
///
/// `Task` exposes its fields publicly and is mutated in place all over the codebase, so the
/// blocked/reason invariant cannot be enforced by construction. It is enforced here instead:
/// route every status change through this function and the pairing holds. Assigning
/// `task.status` directly bypasses it — the migration-015 triggers are the backstop.
///
/// Leaving `Blocked` clears the reason as a side effect, so callers never have to remember to.
pub fn apply_status(
    task: &mut Task,
    next: TaskStatus,
    reason: Option<BlockedReason>,
) -> DomainResult<()> {
    match (next, reason) {
        (TaskStatus::Blocked, None) => Err(DomainError::BlockedReasonRequired),
        (TaskStatus::Blocked, Some(reason)) => {
            task.status = next;
            task.blocked_reason = Some(reason);
            Ok(())
        }
        (other, Some(_)) => Err(DomainError::BlockedReasonNotApplicable(other)),
        (other, None) => {
            task.status = other;
            task.blocked_reason = None;
            Ok(())
        }
    }
}
```

- [ ] **Step 5: Repair every broken struct literal**

Adding a field breaks all `Task { .. }` literals. Get the exact list:

```bash
cd backend && cargo check -p domain -p application -p infrastructure -p api -p cli 2>&1 \
  | rg -n 'missing field `blocked_reason`' -A 2
```

Add `blocked_reason: None,` directly after the `status:` line in each one. Two of them are **not** `None` and must be checked by hand:

- any fixture that sets `status: TaskStatus::Blocked` — give it `Some(BlockedReason::new("...").unwrap())`
- `domain/src/rules/alerts.rs` has 8 literals; confirm none of them is blocked before defaulting them all to `None`

Repeat the `cargo check` until it is clean. Do not move on with a red compiler.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd backend && cargo test -p domain`
Expected: all pass, including the 6 new `rules::status` tests.

Then confirm nothing else regressed:
Run: `cd backend && cargo test -p domain -p application -p infrastructure -p api -p cli`
Expected: all pass. Nothing calls `apply_status` yet, so behaviour is unchanged.

- [ ] **Step 7: Document rule R64**

In `SPEC_FONCTIONNELLE.md` §7 "Règles métier", after R63:

```markdown
**R64 — Raison de blocage obligatoire :** une tâche ne peut passer au statut `blocked`
qu'accompagnée d'une raison non vide, saisie par l'utilisateur ou générée par la
synchronisation. La raison est effacée dès que la tâche quitte ce statut : une raison
présente signifie donc toujours, et uniquement, que la tâche est bloquée.
```

- [ ] **Step 8: Commit**

```bash
git add backend/crates/domain/src/rules/status.rs \
        backend/crates/domain/src/rules/mod.rs \
        backend/crates/domain/src/types/task.rs \
        SPEC_FONCTIONNELLE.md
git add -u backend/crates
git commit -m "Enforce the blocked/reason pairing in one domain function

Task has public fields and is mutated in place everywhere, so the invariant
cannot hold by construction. apply_status becomes the single sanctioned way
to change status, and clearing the reason on the way out of Blocked is a side
effect callers no longer have to remember."
```

`git add -u backend/crates` picks up the mechanical `blocked_reason: None,` repairs from Step 5. Review `git diff --cached --stat` before committing and confirm it contains nothing but those.

---

### Task 3: Migration 015 and persistence

**Files:**
- Create: `migrations/sqlite/015_add_blocked_reason.sql`
- Modify: `backend/crates/infrastructure/src/database/task_repo.rs:55` (`map_task_row`), `:406` (`save`)
- Modify: `SPEC_TECHNIQUE.md` §7 (new subsection after §7.3)

**Interfaces:**
- Consumes: `Task.blocked_reason`, `BlockedReason::new`, `BlockedReason::as_str` (Tasks 1–2)
- Produces: `tasks.blocked_reason` column; triggers `tasks_blocked_reason_insert` and `tasks_blocked_reason_update`

- [ ] **Step 1: Write the failing test**

In `backend/crates/infrastructure/src/database/task_repo.rs`, inside the existing `#[cfg(test)] mod tests`. Follow the in-memory pool setup the neighbouring tests already use (`sqlite::memory:` + migrations):

```rust
#[tokio::test]
async fn round_trips_a_blocked_task_with_its_reason() {
    let pool = setup_test_db().await;
    let repo = SqliteTaskRepository::new(pool.clone());
    let mut task = sample_task();
    task.status = TaskStatus::Blocked;
    task.blocked_reason = Some(BlockedReason::new("attente retour client").unwrap());

    repo.save(&task).await.unwrap();
    let loaded = repo.find_by_id(task.id).await.unwrap().unwrap();

    assert_eq!(loaded.status, TaskStatus::Blocked);
    assert_eq!(
        loaded.blocked_reason.as_ref().map(BlockedReason::as_str),
        Some("attente retour client")
    );
}

#[tokio::test]
async fn unblocking_persists_a_null_reason() {
    let pool = setup_test_db().await;
    let repo = SqliteTaskRepository::new(pool.clone());
    let mut task = sample_task();
    task.status = TaskStatus::Blocked;
    task.blocked_reason = Some(BlockedReason::new("attente CI").unwrap());
    repo.save(&task).await.unwrap();

    task.status = TaskStatus::InProgress;
    task.blocked_reason = None;
    repo.save(&task).await.unwrap();

    let loaded = repo.find_by_id(task.id).await.unwrap().unwrap();
    assert_eq!(loaded.blocked_reason, None);
}

#[tokio::test]
async fn the_trigger_rejects_a_blocked_row_without_a_reason() {
    let pool = setup_test_db().await;
    let err = sqlx::query(
        "INSERT INTO tasks (id, user_id, title, source, status, urgency, impact, tracking_state, created_at, updated_at)
         VALUES ('11111111-1111-1111-1111-111111111111', ?, 'x', 'personal', 'blocked', 1, 2, 'followed', '2026-08-06', '2026-08-06')",
    )
    .bind(test_user_id().to_string())
    .execute(&pool)
    .await
    .unwrap_err();

    assert!(
        err.to_string().contains("blocked tasks require a non-empty blocked_reason"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn the_trigger_rejects_a_reason_on_an_unblocked_row() {
    let pool = setup_test_db().await;
    let err = sqlx::query(
        "INSERT INTO tasks (id, user_id, title, source, status, blocked_reason, urgency, impact, tracking_state, created_at, updated_at)
         VALUES ('22222222-2222-2222-2222-222222222222', ?, 'x', 'personal', 'todo', 'nope', 1, 2, 'followed', '2026-08-06', '2026-08-06')",
    )
    .bind(test_user_id().to_string())
    .execute(&pool)
    .await
    .unwrap_err();

    assert!(
        err.to_string().contains("blocked tasks require a non-empty blocked_reason"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn the_trigger_rejects_a_whitespace_only_reason() {
    let pool = setup_test_db().await;
    let err = sqlx::query(
        "INSERT INTO tasks (id, user_id, title, source, status, blocked_reason, urgency, impact, tracking_state, created_at, updated_at)
         VALUES ('33333333-3333-3333-3333-333333333333', ?, 'x', 'personal', 'blocked', '   ', 1, 2, 'followed', '2026-08-06', '2026-08-06')",
    )
    .bind(test_user_id().to_string())
    .execute(&pool)
    .await
    .unwrap_err();

    assert!(
        err.to_string().contains("blocked tasks require a non-empty blocked_reason"),
        "unexpected error: {err}"
    );
}
```

Reuse the existing helpers in that test module for `setup_test_db()`, `sample_task()` and the test user id — read the top of the module and match the names actually there rather than introducing new ones.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test -p infrastructure task_repo::tests::`
Expected: the round-trip tests fail (`no such column: blocked_reason`), and the trigger tests fail because the insert *succeeds* — `unwrap_err` panics.

- [ ] **Step 3: Write the migration**

Create `migrations/sqlite/015_add_blocked_reason.sql`:

```sql
-- Adds tasks.blocked_reason and the triggers enforcing the blocked/reason pairing.
--
-- Invariant: blocked_reason is non-null and non-blank if and only if status = 'blocked'.
--
-- Triggers rather than a CHECK: SQLite cannot add a CHECK to an existing table, which would
-- mean the full table-rebuild that 007 performed. `tasks` now has 31 columns plus foreign keys
-- and indexes, and a column dropped by accident during a rebuild is silent data loss. The
-- triggers give the same guarantee — a violating write aborts — without touching the table.

ALTER TABLE tasks ADD COLUMN blocked_reason TEXT;

-- Backfill before the guard goes up, so the table is never in a state the triggers reject.
UPDATE tasks
   SET blocked_reason = 'Raison non renseignée (migration 015)'
 WHERE status = 'blocked'
   AND (blocked_reason IS NULL OR trim(blocked_reason) = '');

CREATE TRIGGER tasks_blocked_reason_insert
BEFORE INSERT ON tasks
WHEN (NEW.status = 'blocked')
  <> (NEW.blocked_reason IS NOT NULL AND trim(NEW.blocked_reason) <> '')
BEGIN
    SELECT RAISE(ABORT, 'blocked tasks require a non-empty blocked_reason');
END;

CREATE TRIGGER tasks_blocked_reason_update
BEFORE UPDATE OF status, blocked_reason ON tasks
WHEN (NEW.status = 'blocked')
  <> (NEW.blocked_reason IS NOT NULL AND trim(NEW.blocked_reason) <> '')
BEGIN
    SELECT RAISE(ABORT, 'blocked tasks require a non-empty blocked_reason');
END;
```

Note for the reviewer: `SqliteTaskRepository::save` uses `INSERT OR REPLACE`, which SQLite implements as delete-then-insert, so it is the *insert* trigger that guards the repository's writes. The update trigger covers direct `UPDATE` statements from migrations and any future code path.

- [ ] **Step 4: Wire the column into the repository**

In `map_task_row` (`task_repo.rs:55`), alongside the other `Option<String>` column reads:

```rust
    let blocked_reason_str: Option<String> = Row::get(row, "blocked_reason");
```

and where the `Task` is constructed, after `status`:

```rust
        blocked_reason: match blocked_reason_str {
            Some(ref s) => Some(
                BlockedReason::new(s).map_err(|e| RepositoryError::Database(e.to_string()))?,
            ),
            None => None,
        },
```

In `save` (`task_repo.rs:406`): add `blocked_reason` to the column list right after `status`, add one more `?` to the `VALUES` tuple, and add the bind directly after the `task_status_to_str(task.status)` bind:

```rust
        .bind(task.blocked_reason.as_ref().map(BlockedReason::as_str))
```

The bind order must match the column order exactly — count them before running.

Add `BlockedReason` to the `domain::types` import at the top of the file.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure`
Expected: all pass, including the five new tests.

- [ ] **Step 6: Document the schema change**

In `SPEC_TECHNIQUE.md`, add a subsection after §7.3 (`014_create_sessions.sql`):

```markdown
### 7.4 Migration `015_add_blocked_reason.sql` — raison de blocage

Ajoute `tasks.blocked_reason TEXT` et deux déclencheurs, `tasks_blocked_reason_insert`
(`BEFORE INSERT`) et `tasks_blocked_reason_update` (`BEFORE UPDATE OF status, blocked_reason`),
qui avortent toute écriture violant l'invariant : `blocked_reason` est non nulle et non blanche
si et seulement si `status = 'blocked'`.

Des déclencheurs plutôt qu'une contrainte `CHECK` : SQLite ne sait pas ajouter un `CHECK` à une
table existante, ce qui imposerait la reconstruction complète pratiquée par la migration `007`.
`tasks` compte désormais 31 colonnes, des clés étrangères et des index ; une colonne oubliée
pendant une reconstruction serait une perte de données silencieuse. La garantie est identique.

Les lignes déjà `blocked` sont rétro-remplies avec `Raison non renseignée (migration 015)`
avant la création des déclencheurs.

`SqliteTaskRepository::save` utilisant `INSERT OR REPLACE` — soit un delete suivi d'un insert —
c'est le déclencheur d'insertion qui garde les écritures du dépôt.
```

- [ ] **Step 7: Commit**

```bash
git add migrations/sqlite/015_add_blocked_reason.sql \
        backend/crates/infrastructure/src/database/task_repo.rs \
        SPEC_TECHNIQUE.md
git commit -m "Persist the blocked reason and guard the invariant with triggers

SQLite cannot add a CHECK to an existing table, and rebuilding a 31-column
tasks table to get one risks dropping a column silently. Two BEFORE triggers
give the same guarantee without touching the table."
```

---

### Task 4: Route `update_task` and `complete_task` through `apply_status`

**Files:**
- Modify: `backend/crates/application/src/use_cases/task_management.rs:34` (input field), `:183-185` (status branch), `:331` (complete), `:60-90` (create literal)

**Interfaces:**
- Consumes: `apply_status`, `BlockedReason` (Tasks 1–2)
- Produces: `task_management::UpdateTaskInput.blocked_reason: Option<Option<String>>` — `Some(Some(s))` sets, `Some(None)` clears, `None` leaves unchanged. Task 6 (GraphQL) and Task 7 (CLI) both build this.

- [ ] **Step 1: Write the failing test**

In the existing `#[cfg(test)] mod tests` of `task_management.rs`, matching the in-memory repo fixture already used there:

```rust
#[tokio::test]
async fn update_task_blocks_with_a_reason() {
    let repo = InMemoryTaskRepository::new();
    let task = seed_task(&repo).await;

    let updated = update_task(
        &repo,
        task.id,
        UpdateTaskInput {
            status: Some(TaskStatus::Blocked),
            blocked_reason: Some(Some("attente retour client".to_string())),
            ..empty_update()
        },
        today(),
    )
    .await
    .unwrap();

    assert_eq!(updated.status, TaskStatus::Blocked);
    assert_eq!(
        updated.blocked_reason.as_ref().map(BlockedReason::as_str),
        Some("attente retour client")
    );
}

#[tokio::test]
async fn update_task_refuses_to_block_without_a_reason() {
    let repo = InMemoryTaskRepository::new();
    let task = seed_task(&repo).await;

    let err = update_task(
        &repo,
        task.id,
        UpdateTaskInput { status: Some(TaskStatus::Blocked), ..empty_update() },
        today(),
    )
    .await
    .unwrap_err();

    assert!(matches!(err, AppError::Validation(_)), "got {err:?}");
}

#[tokio::test]
async fn update_task_clears_the_reason_when_leaving_blocked() {
    let repo = InMemoryTaskRepository::new();
    let task = seed_blocked_task(&repo, "attente CI").await;

    let updated = update_task(
        &repo,
        task.id,
        UpdateTaskInput { status: Some(TaskStatus::InProgress), ..empty_update() },
        today(),
    )
    .await
    .unwrap();

    assert_eq!(updated.blocked_reason, None);
}

#[tokio::test]
async fn update_task_can_reword_the_reason_of_a_still_blocked_task() {
    let repo = InMemoryTaskRepository::new();
    let task = seed_blocked_task(&repo, "attente CI").await;

    let updated = update_task(
        &repo,
        task.id,
        UpdateTaskInput {
            blocked_reason: Some(Some("attente retour client".to_string())),
            ..empty_update()
        },
        today(),
    )
    .await
    .unwrap();

    assert_eq!(updated.status, TaskStatus::Blocked);
    assert_eq!(
        updated.blocked_reason.as_ref().map(BlockedReason::as_str),
        Some("attente retour client")
    );
}

#[tokio::test]
async fn completing_a_blocked_task_clears_its_reason() {
    let repo = InMemoryTaskRepository::new();
    let task = seed_blocked_task(&repo, "attente CI").await;

    let completed = complete_task(&repo, task.id).await.unwrap();

    assert_eq!(completed.status, TaskStatus::Done);
    assert_eq!(completed.blocked_reason, None);
}
```

Two helpers are needed. `empty_update()` returns an `UpdateTaskInput` with every field `None` — if the module already has such a helper under another name, use that instead of adding one. `seed_blocked_task(&repo, reason)` saves a task already in `Blocked` with that reason.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test -p application task_management`
Expected: compilation error, `UpdateTaskInput` has no field `blocked_reason`.

- [ ] **Step 3: Add the input field**

In `task_management.rs`, after line 34 (`pub status: Option<TaskStatus>,`):

```rust
    /// Set to Some(Some(text)) to set the reason, Some(None) to clear it, None to leave
    /// unchanged. Paired with `status` by `apply_status`: blocking needs a reason, and
    /// unblocking discards whatever was there.
    pub blocked_reason: Option<Option<String>>,
```

Fix the resulting compilation errors in every `UpdateTaskInput { .. }` literal — `cargo check -p application -p api -p cli` lists them.

- [ ] **Step 4: Replace the status branch**

Replace lines 183-185 of `task_management.rs`:

```rust
    if let Some(status) = input.status {
        task.status = status;
    }
```

with:

```rust
    // Status and reason move together — see domain::rules::status::apply_status.
    let reason = match input.blocked_reason {
        Some(Some(ref text)) => Some(
            BlockedReason::new(text).map_err(|e| AppError::Validation(e.to_string()))?,
        ),
        // Explicit clear, or nothing said: for a status change to Blocked the absence of a
        // reason is what apply_status rejects; for any other status it is what it expects.
        Some(None) | None => None,
    };

    match (input.status, reason) {
        // Rewording the reason of a task that is already blocked, with no status change.
        (None, Some(reason)) => {
            apply_status(&mut task, TaskStatus::Blocked, Some(reason))
                .map_err(|e| AppError::Validation(e.to_string()))?;
        }
        (Some(status), reason) => {
            apply_status(&mut task, status, reason)
                .map_err(|e| AppError::Validation(e.to_string()))?;
        }
        (None, None) => {}
    }
```

Note the `(None, Some(reason))` arm: sending only a reason means "reword the current blockage". It forces the status to `Blocked`, which is a no-op for an already-blocked task and a deliberate block for any other — consistent with the invariant either way.

Import `apply_status` and `BlockedReason` at the top of the file.

- [ ] **Step 5: Route `complete_task`**

Replace line 331 (`task.status = TaskStatus::Done;`) with:

```rust
    apply_status(&mut task, TaskStatus::Done, None)
        .map_err(|e| AppError::Validation(e.to_string()))?;
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd backend && cargo test -p application`
Expected: all pass, including the five new ones.

- [ ] **Step 7: Commit**

```bash
git add backend/crates/application/src/use_cases/task_management.rs
git add -u backend/crates
git commit -m "Route task status changes through apply_status

update_task and complete_task were assigning task.status directly, which is
exactly the bypass the invariant cannot survive. Sending only a reason now
means rewording the current blockage."
```

---

### Task 5: The sync generates a reason, and only on the transition

**Files:**
- Modify: `backend/crates/application/src/use_cases/sync.rs:122-123`, `:148-149`, `:390-407`, `:424`, `:943` (`map_jira_status`), `:974` (`map_excel_status`)

**Interfaces:**
- Consumes: `apply_status`, `BlockedReason` (Tasks 1–2)
- Produces: `map_jira_status(&str) -> (TaskStatus, Option<BlockedReason>)` and `map_excel_status(&str) -> (TaskStatus, Option<BlockedReason>)` — both private to the module

- [ ] **Step 1: Write the failing test**

In the existing `#[cfg(test)] mod tests` of `sync.rs`:

```rust
#[test]
fn map_jira_status_generates_a_reason_for_blocked() {
    let (status, reason) = map_jira_status("Impediment");

    assert_eq!(status, TaskStatus::Blocked);
    assert_eq!(
        reason.as_ref().map(BlockedReason::as_str),
        Some("Bloqué dans Jira (statut : Impediment)")
    );
}

#[test]
fn map_jira_status_generates_no_reason_for_other_statuses() {
    let (status, reason) = map_jira_status("In Progress");

    assert_eq!(status, TaskStatus::InProgress);
    assert_eq!(reason, None);
}

#[test]
fn map_excel_status_generates_a_reason_for_blocked() {
    let (status, reason) = map_excel_status("bloqué");

    assert_eq!(status, TaskStatus::Blocked);
    assert_eq!(
        reason.as_ref().map(BlockedReason::as_str),
        Some("Bloqué dans Excel (statut : bloqué)")
    );
}
```

Then the behaviour that matters — the sync must not trample a hand-written reason. Add this alongside the existing Jira sync tests, following whatever fixture they use to drive `sync_jira_tasks` with a stub client:

```rust
#[tokio::test]
async fn jira_sync_keeps_an_existing_reason_on_an_already_blocked_task() {
    // A task already blocked with a reason the user typed.
    let repo = InMemoryTaskRepository::new();
    let mut existing = sample_jira_task();
    existing.status = TaskStatus::Blocked;
    existing.blocked_reason = Some(BlockedReason::new("attente retour client").unwrap());
    repo.save(&existing).await.unwrap();

    // Jira still reports it blocked.
    let client = StubJiraClient::returning(vec![jira_issue(&existing, "Blocked")]);
    sync_jira_tasks(&client, &repo, /* ...as the neighbouring tests do... */).await.unwrap();

    let reloaded = repo.find_by_id(existing.id).await.unwrap().unwrap();
    assert_eq!(
        reloaded.blocked_reason.as_ref().map(BlockedReason::as_str),
        Some("attente retour client"),
        "the sync must not overwrite a hand-written reason"
    );
}

#[tokio::test]
async fn jira_sync_generates_a_reason_when_a_task_becomes_blocked() {
    let repo = InMemoryTaskRepository::new();
    let existing = sample_jira_task(); // status Todo, no reason
    repo.save(&existing).await.unwrap();

    let client = StubJiraClient::returning(vec![jira_issue(&existing, "Impediment")]);
    sync_jira_tasks(&client, &repo, /* ... */).await.unwrap();

    let reloaded = repo.find_by_id(existing.id).await.unwrap().unwrap();
    assert_eq!(reloaded.status, TaskStatus::Blocked);
    assert_eq!(
        reloaded.blocked_reason.as_ref().map(BlockedReason::as_str),
        Some("Bloqué dans Jira (statut : Impediment)")
    );
}
```

Read the existing sync tests first and match their stub-client and argument conventions exactly — the placeholders above stand in for whatever those are.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test -p application sync`
Expected: compilation error — `map_jira_status` returns `TaskStatus`, not a tuple.

- [ ] **Step 3: Change the two mappers**

`map_jira_status` (line 943) keeps its matching logic and changes only its return. Replace the blocked branch and the fallthrough:

```rust
fn map_jira_status(jira_status: &str) -> (TaskStatus, Option<BlockedReason>) {
    let lower = jira_status.to_lowercase();
    // ... existing Done / InProgress branches, each returning (status, None) ...

    // Blocked states
    if lower.contains("blocked") || lower.contains("impediment") {
        let reason = BlockedReason::new(&format!("Bloqué dans Jira (statut : {jira_status})"))
            .expect("the format string is never empty");
        return (TaskStatus::Blocked, Some(reason));
    }
    (TaskStatus::Todo, None)
}
```

`map_excel_status` (line 974) likewise:

```rust
fn map_excel_status(status: &str) -> (TaskStatus, Option<BlockedReason>) {
    match status.to_lowercase().as_str() {
        "done" | "closed" | "resolved" | "complete" | "completed" | "terminé" => {
            (TaskStatus::Done, None)
        }
        "in progress" | "en cours" | "active" => (TaskStatus::InProgress, None),
        "blocked" | "bloqué" => {
            let reason = BlockedReason::new(&format!("Bloqué dans Excel (statut : {status})"))
                .expect("the format string is never empty");
            (TaskStatus::Blocked, Some(reason))
        }
        _ => (TaskStatus::Todo, None),
    }
}
```

Both `expect` calls are safe: the formatted string always contains literal text, so `BlockedReason::new` cannot see an empty input.

- [ ] **Step 4: Update the four call sites**

For an **existing** task (`sync.rs:122-123` for Jira, `:407` for Excel), the reason is written only on the transition:

```rust
                task.jira_status = Some(jira_task.status.clone());
                let (next_status, generated) = map_jira_status(&jira_task.status);
                // Only generate a reason when the task becomes blocked. If it was already
                // blocked, keep what is stored — otherwise every sync run would overwrite a
                // reason the user typed with a generated one.
                let reason = if task.status == TaskStatus::Blocked {
                    task.blocked_reason.clone()
                } else {
                    generated
                };
                apply_status(&mut task, next_status, reason)
                    .map_err(|e| AppError::Validation(e.to_string()))?;
```

Apply the same shape at the Excel call site (`:390-407`), substituting `map_excel_status`.

For a **new** task (`sync.rs:148-149` and the Excel equivalent near `:424`), destructure into the literal:

```rust
                let (status, blocked_reason) = map_jira_status(&jira_task.status);
                let task = Task {
                    // ...
                    jira_status: Some(jira_task.status.clone()),
                    status,
                    blocked_reason,
                    // ...
                };
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd backend && cargo test -p application`
Expected: all pass. Existing `map_jira_status`/`map_excel_status` assertions (`sync.rs:1320`, `:1331`) now compare tuples — update them to `assert_eq!(map_jira_status("Blocked").0, TaskStatus::Blocked)`.

- [ ] **Step 6: Commit**

```bash
git add backend/crates/application/src/use_cases/sync.rs
git commit -m "Generate a blocked reason from the source status on transition only

Jira and Excel can move a task to blocked with nobody around to type a reason,
so the mappers now produce one. Writing it on every run would overwrite what
the user typed, so it lands only when the task was not already blocked."
```

---

### Task 6: GraphQL contract

**Files:**
- Modify: `backend/crates/api/src/graphql/types/task.rs:75` (resolver), `:240` (input field)
- Modify: `backend/crates/api/src/graphql/mutation.rs:1789` area (`convert_update_input`)
- Modify: `backend/crates/api/src/graphql/tests.rs`
- Modify: `SPEC_TECHNIQUE.md` §8.1
- Regenerate: `backend/crates/cli/graphql/schema.graphql`

**Interfaces:**
- Consumes: `task_management::UpdateTaskInput.blocked_reason` (Task 4)
- Produces: GraphQL `Task.blockedReason: String` (nullable) and `UpdateTaskInput.blockedReason: String` (`MaybeUndefined`)

- [ ] **Step 1: Write the failing test**

In `backend/crates/api/src/graphql/tests.rs`, following the schema-execution helper the neighbouring mutation tests use:

```rust
#[tokio::test]
async fn update_task_to_blocked_requires_a_reason() {
    let schema = test_schema().await;
    let id = seed_task(&schema).await;

    let resp = schema
        .execute(format!(
            r#"mutation {{ updateTask(id: "{id}", input: {{ status: BLOCKED }}) {{ id }} }}"#
        ))
        .await;

    assert!(!resp.errors.is_empty(), "blocking without a reason must fail");
    assert!(resp.errors[0].message.contains("reason is required"), "got {:?}", resp.errors);
}

#[tokio::test]
async fn update_task_to_blocked_with_a_reason_succeeds_and_reads_back() {
    let schema = test_schema().await;
    let id = seed_task(&schema).await;

    let resp = schema
        .execute(format!(
            r#"mutation {{ updateTask(id: "{id}", input: {{ status: BLOCKED, blockedReason: "attente retour client" }}) {{ status blockedReason }} }}"#
        ))
        .await;

    assert!(resp.errors.is_empty(), "got {:?}", resp.errors);
    let data = resp.data.into_json().unwrap();
    assert_eq!(data["updateTask"]["status"], "BLOCKED");
    assert_eq!(data["updateTask"]["blockedReason"], "attente retour client");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test -p api update_task_to_blocked`
Expected: both fail — `Unknown field "blockedReason"`.

- [ ] **Step 3: Add the resolver and the input field**

In `types/task.rs`, next to the `delegated_to` resolver at line 75:

```rust
    /// Why this task is blocked. Non-null if and only if `status` is `BLOCKED`.
    async fn blocked_reason(&self) -> Option<&str> {
        self.0.blocked_reason.as_ref().map(BlockedReason::as_str)
    }
```

In `UpdateTaskInput` (line 240 area), next to `delegated_to`:

```rust
    /// Set to a reason to block, explicit null to clear, omit to leave unchanged.
    /// Blocking without a reason is refused.
    #[graphql(default)]
    pub blocked_reason: MaybeUndefined<String>,
```

- [ ] **Step 4: Map it in `convert_update_input`**

In `mutation.rs`, inside the returned `task_management::UpdateTaskInput { .. }` literal (line 1772 onwards), following the `planned_start` pattern:

```rust
        blocked_reason: match input.blocked_reason {
            MaybeUndefined::Value(text) => Some(Some(text)),
            MaybeUndefined::Null      => Some(None),
            MaybeUndefined::Undefined => None,
        },
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd backend && cargo test -p api`
Expected: all pass.

- [ ] **Step 6: Regenerate the SDL the CLI compiles against**

Run: `cd backend && cargo run -p api -- export-schema`

**Warning:** this command builds the pool before exporting, so it applies pending migrations to the real `backend/aggregated_plan.db`. That is expected here — migration 015 is meant to land — but be aware it is a write to the live database, not a pure codegen step. Confirm `backend/crates/cli/graphql/schema.graphql` now contains `blockedReason` in both `Task` and `UpdateTaskInput`.

- [ ] **Step 7: Document the contract**

In `SPEC_TECHNIQUE.md` §8.1, add `blockedReason: String` to the `Task` type and to `input UpdateTaskInput`, with a one-line note in French that blocking without a reason returns a GraphQL error.

- [ ] **Step 8: Commit**

```bash
git add backend/crates/api/src/graphql/types/task.rs \
        backend/crates/api/src/graphql/mutation.rs \
        backend/crates/api/src/graphql/tests.rs \
        backend/crates/cli/graphql/schema.graphql \
        SPEC_TECHNIQUE.md
git commit -m "Expose blockedReason on the Task type and the update input"
```

---

### Task 7: CLI — `--reason`, TTY prompt, display

**Files:**
- Modify: `backend/crates/cli/src/cli.rs:168-172` (the `Status` variant)
- Modify: `backend/crates/cli/src/main.rs:72-78` (dispatch)
- Modify: `backend/crates/cli/src/commands.rs:430-448` (`status`)
- Modify: `backend/crates/cli/graphql/update_task_status.graphql`
- Modify: `backend/crates/cli/tests/integration.rs`
- Modify: `SPEC_TECHNIQUE.md` §2.5

**Interfaces:**
- Consumes: GraphQL `UpdateTaskInput.blockedReason` (Task 6)
- Produces: `aplan status <state> [--reason TEXT] [--task TARGET]`

No new dependency: `std::io::IsTerminal` has been stable since Rust 1.70.

- [ ] **Step 1: Write the failing test**

In `backend/crates/cli/tests/integration.rs`, following the `assert_cmd` + `wiremock` pattern the file already uses:

```rust
#[tokio::test]
async fn status_blocked_without_a_reason_fails_when_not_a_tty() {
    let server = mock_server_with_task().await;

    // Tests never have a TTY on stdin, so this exercises the non-interactive path.
    let mut cmd = aplan_cmd(&server);
    cmd.args(["status", "blocked", "--task", "TEST-1"]);

    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("--reason is required"));
}

#[tokio::test]
async fn status_blocked_with_a_reason_succeeds() {
    let server = mock_server_with_task().await;

    let mut cmd = aplan_cmd(&server);
    cmd.args(["status", "blocked", "--task", "TEST-1", "--reason", "attente retour client"]);

    cmd.assert().success();
}

#[tokio::test]
async fn reason_on_a_non_blocked_state_is_a_usage_error() {
    let server = mock_server_with_task().await;

    let mut cmd = aplan_cmd(&server);
    cmd.args(["status", "todo", "--task", "TEST-1", "--reason", "nope"]);

    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("--reason only applies to blocked"));
}
```

Match the actual helper names in that file (`aplan_cmd`, the wiremock setup) rather than these stand-ins.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test -p cli status_blocked`
Expected: failure — clap rejects the unknown `--reason` argument.

- [ ] **Step 3: Add the flag**

In `cli.rs`, replace the `Status` variant (lines 168-172):

```rust
    /// Set the status of the currently-tracked task (or --task TARGET).
    /// Blocking requires a reason: pass --reason, or be prompted for it on a terminal.
    Status {
        state: StatusArg,
        #[arg(long)]
        task: Option<String>,
        /// Why the task is blocked. Required with `blocked`, rejected with any other state.
        #[arg(long)]
        reason: Option<String>,
    },
```

Thread it through `main.rs:72-78` as a fourth argument to `commands::status`.

- [ ] **Step 4: Change the GraphQL document**

`backend/crates/cli/graphql/update_task_status.graphql`:

```graphql
mutation UpdateTaskStatus($id: ID!, $status: TaskStatusGql!, $blockedReason: String) {
  updateTask(id: $id, input: { status: $status, blockedReason: $blockedReason }) {
    id
    title
    sourceId
    status
    blockedReason
  }
}
```

`graphql_client` regenerates `update_task_status::Variables` with a `blocked_reason: Option<String>` field at build time.

- [ ] **Step 5: Resolve the reason in `commands::status`**

Add the signature parameter and, before building the request:

```rust
pub fn status(
    api_url: &str,
    json: bool,
    state: &StatusArg,
    task: Option<&str>,
    session: Option<&str>,
    reason: Option<String>,
) -> ExitCode {
    let blocked_reason = match resolve_blocked_reason(state, reason) {
        Ok(r) => r,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::Generic;
        }
    };
    // ... existing client + resolve_target ...
    let result = client.run::<UpdateTaskStatus>(update_task_status::Variables {
        id: target.id.clone(),
        status: state.as_graphql(),
        blocked_reason,
    });
```

and the helper, in the same file:

```rust
/// Blocking needs a reason. Take `--reason` if given; otherwise prompt, but only when stdin is
/// a terminal — scripts, hooks and agents must fail fast rather than hang on a read that will
/// never be answered.
fn resolve_blocked_reason(
    state: &StatusArg,
    reason: Option<String>,
) -> Result<Option<String>, String> {
    use std::io::{BufRead, IsTerminal, Write};

    if !matches!(state, StatusArg::Blocked) {
        return match reason {
            Some(_) => Err("--reason only applies to blocked".to_string()),
            None => Ok(None),
        };
    }

    if let Some(text) = reason {
        return match text.trim() {
            "" => Err("--reason cannot be empty".to_string()),
            trimmed => Ok(Some(trimmed.to_string())),
        };
    }

    if !std::io::stdin().is_terminal() {
        return Err("--reason is required to block a task".to_string());
    }

    print!("Raison du blocage : ");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;

    match line.trim() {
        "" => Err("a reason is required to block a task".to_string()),
        trimmed => Ok(Some(trimmed.to_string())),
    }
}
```

- [ ] **Step 6: Show the reason on output**

In the non-JSON success branch (line 464), when the new status is blocked, append the reason:

```rust
            match r.data.update_task.blocked_reason.as_deref() {
                Some(reason) => println!(
                    "↻ {}: status → {:?} ({})",
                    label, r.data.update_task.status, reason
                ),
                None => println!("↻ {}: status → {:?}", label, r.data.update_task.status),
            }
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cd backend && cargo test -p cli`
Expected: all pass.

- [ ] **Step 8: Document the CLI surface**

In `SPEC_TECHNIQUE.md` §2.5, update the `aplan status` entry: `--reason` is required with `blocked`, prompted for on a terminal, refused with any other state, and a non-interactive invocation without it exits non-zero.

- [ ] **Step 9: Commit**

```bash
git add backend/crates/cli/src/cli.rs \
        backend/crates/cli/src/main.rs \
        backend/crates/cli/src/commands.rs \
        backend/crates/cli/graphql/update_task_status.graphql \
        backend/crates/cli/tests/integration.rs \
        SPEC_TECHNIQUE.md
git commit -m "Require a reason on aplan status blocked

Prompt for it on a terminal, fail fast without one anywhere else: a hook or an
agent must not hang on a read nobody will answer."
```

---

### Task 8: React — inline reason field in `StatusMenu`

**Files:**
- Modify: `frontend/src/components/task/StatusMenu.tsx`
- Create: `frontend/src/components/task/StatusMenu.test.tsx`

**Interfaces:**
- Consumes: GraphQL `UpdateTaskInput.blockedReason` (Task 6)
- Produces: nothing other components depend on

- [ ] **Step 1: Write the failing test**

Create `frontend/src/components/task/StatusMenu.test.tsx`, following the urql mocking already used by `TaskCard.test.tsx` — read that file first and reuse its provider setup:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { StatusMenu } from './StatusMenu';

describe('StatusMenu', () => {
  it('does not fire the mutation when Blocked is picked without a reason', async () => {
    const executeUpdate = vi.fn();
    renderWithUrql(<StatusMenu taskId="t1" status="TODO" />, { executeUpdate });

    await userEvent.click(screen.getByRole('button', { name: /to do/i }));
    await userEvent.click(screen.getByRole('menuitem', { name: /blocked/i }));

    expect(screen.getByLabelText(/raison/i)).toBeInTheDocument();
    expect(executeUpdate).not.toHaveBeenCalled();
  });

  it('sends status and reason together once confirmed', async () => {
    const executeUpdate = vi.fn().mockResolvedValue({ data: {} });
    renderWithUrql(<StatusMenu taskId="t1" status="TODO" />, { executeUpdate });

    await userEvent.click(screen.getByRole('button', { name: /to do/i }));
    await userEvent.click(screen.getByRole('menuitem', { name: /blocked/i }));
    await userEvent.type(screen.getByLabelText(/raison/i), 'attente retour client');
    await userEvent.click(screen.getByRole('button', { name: /valider/i }));

    await waitFor(() =>
      expect(executeUpdate).toHaveBeenCalledWith({
        id: 't1',
        input: { status: 'BLOCKED', blockedReason: 'attente retour client' },
      })
    );
  });

  it('keeps the confirm button disabled while the field is blank', async () => {
    renderWithUrql(<StatusMenu taskId="t1" status="TODO" />, { executeUpdate: vi.fn() });

    await userEvent.click(screen.getByRole('button', { name: /to do/i }));
    await userEvent.click(screen.getByRole('menuitem', { name: /blocked/i }));

    expect(screen.getByRole('button', { name: /valider/i })).toBeDisabled();
  });

  it('still fires immediately for a non-blocked status', async () => {
    const executeUpdate = vi.fn().mockResolvedValue({ data: {} });
    renderWithUrql(<StatusMenu taskId="t1" status="TODO" />, { executeUpdate });

    await userEvent.click(screen.getByRole('button', { name: /to do/i }));
    await userEvent.click(screen.getByRole('menuitem', { name: /done/i }));

    await waitFor(() =>
      expect(executeUpdate).toHaveBeenCalledWith({ id: 't1', input: { status: 'DONE' } })
    );
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && pnpm test StatusMenu`
Expected: failures — no field with an accessible name matching `/raison/i`, and the mutation fires on the Blocked click.

- [ ] **Step 3: Add the reason step**

In `StatusMenu.tsx`:

- Add the field to the mutation document:
  ```
  mutation UpdateTaskStatus($id: ID!, $input: UpdateTaskInput!) { updateTask(id: $id, input: $input) { id status blockedReason } }
  ```
- Add `const [pendingBlock, setPendingBlock] = useState(false);` and `const [reason, setReason] = useState('');`
- In `handleSelect`, intercept `BLOCKED`:
  ```tsx
  if (value === 'BLOCKED') {
    setPendingBlock(true);
    return;
  }
  await executeUpdate({ id: taskId, input: { status: value } });
  ```
  Note this branch must run *before* the existing `if (value === status) return;` early exit, so that re-picking Blocked on an already-blocked task opens the field to reword the reason.
- Render the inline form when `pendingBlock` is true, in place of the menu list:
  ```tsx
  <form
    className="p-2 flex flex-col gap-2 min-w-[220px]"
    onSubmit={(e) => {
      e.preventDefault();
      void confirmBlock();
    }}
  >
    <label htmlFor={`blocked-reason-${taskId}`} className="text-xs text-gray-600">
      Raison du blocage
    </label>
    <input
      id={`blocked-reason-${taskId}`}
      value={reason}
      onChange={(e) => setReason(e.target.value)}
      autoFocus
      className="border border-gray-200 rounded px-2 py-1 text-xs"
    />
    <div className="flex gap-2 justify-end">
      <button type="button" onClick={cancelBlock} className="text-xs px-2 py-1 text-gray-600">
        Annuler
      </button>
      <button
        type="submit"
        disabled={reason.trim() === ''}
        className="text-xs px-2 py-1 rounded bg-red-100 text-red-700 disabled:opacity-50"
      >
        Valider
      </button>
    </div>
  </form>
  ```
- `confirmBlock` sends `{ id: taskId, input: { status: 'BLOCKED', blockedReason: reason.trim() } }`, then closes the menu and resets both pieces of state. `cancelBlock` resets them without sending.
- Reset `pendingBlock` and `reason` whenever the menu closes, so reopening never shows a stale draft.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && pnpm test StatusMenu`
Expected: 4 passed.

- [ ] **Step 5: Check the whole frontend suite and the types**

Run: `cd frontend && pnpm test && pnpm build`
Expected: both clean. `pnpm build` runs `tsc`, which catches the new field against the generated GraphQL types.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/task/StatusMenu.tsx \
        frontend/src/components/task/StatusMenu.test.tsx
git commit -m "Ask for a reason inline when blocking a task from the status menu"
```

---

### Task 9: End-to-end check against the running stack

No new code. This is the gate that catches what unit tests cannot: the real database, the real migration, the real CLI against the real API.

**Files:** none

- [ ] **Step 1: Confirm the full scoped suite is green**

Run: `cd backend && cargo test -p domain -p application -p infrastructure -p api -p cli`
Expected: all pass. Do not run workspace-wide — `mcp` does not compile.

- [ ] **Step 2: Lint**

Run: `cd backend && cargo clippy -p domain -p application -p infrastructure -p api -p cli`
Expected: no new warnings attributable to this work.

- [ ] **Step 3: Confirm the migration applied to the real database**

Run: `sqlite3 backend/aggregated_plan.db "SELECT status, blocked_reason FROM tasks WHERE status='blocked';"`
Expected: the one pre-existing blocked task now shows `Raison non renseignée (migration 015)`.

Run: `sqlite3 backend/aggregated_plan.db "SELECT name FROM sqlite_master WHERE type='trigger' AND name LIKE 'tasks_blocked%';"`
Expected: both trigger names.

- [ ] **Step 4: Exercise the CLI against a running API**

Start the API (`cd backend && cargo run -p api`), then in another shell:

```bash
aplan status blocked --task <some-task> --reason "vérification plan"   # succeeds, prints the reason
aplan show <some-task>                                                  # reason visible
echo | aplan status blocked --task <some-task>                          # exits non-zero, no hang
aplan status todo --task <some-task>                                    # succeeds
sqlite3 backend/aggregated_plan.db "SELECT blocked_reason FROM tasks WHERE id='<id>';"  # NULL
```

The third command is the one that matters: piping `echo` makes stdin a non-TTY, which must produce a fast failure rather than a blocked read.

- [ ] **Step 5: Restore the task you used**

Put the task back to whatever status it had before Step 4.

- [ ] **Step 6: Final commit if anything was touched**

If Steps 1-2 required fixes, commit them:

```bash
git add -u backend
git commit -m "Fix clippy warnings from the blocked-reason work"
```

Otherwise there is nothing to commit — the feature is complete.

---

## Self-Review

**Spec coverage.** Model and invariant → Tasks 1-2. Newtype → Task 1. Rejected enum-payload alternative → recorded in Task 1's doc comment. Choke point and its four call sites → Tasks 2, 4 (update/complete), 5 (sync); recurrence writes `Todo` at `recurrence.rs:312` inside a struct literal and `Cancelled` at `:385`, both covered by the mechanical repair in Task 2 Step 5 — neither can produce a blocked task, so neither needs `apply_status`. Migration, backfill, triggers, the `INSERT OR REPLACE` note → Task 3. GraphQL → Task 6. CLI → Task 7. React → Task 8. Sync transition-only rule → Task 5. All five test families from the spec → Tasks 2, 3, 4, 5, 6, 7, 8. Both spec files → Tasks 2, 3, 6, 7.

**Type consistency.** `apply_status(&mut Task, TaskStatus, Option<BlockedReason>) -> DomainResult<()>` is used with that exact signature in Tasks 2, 4 and 5. `BlockedReason::new(&str) -> DomainResult<BlockedReason>` and `as_str(&self) -> &str` are used consistently throughout. The application-layer field is `Option<Option<String>>` (Task 4) and the GraphQL field is `MaybeUndefined<String>` (Task 6), bridged in `convert_update_input` — deliberately different types, matching the `planned_start` precedent.

**Known softness.** Tasks 5, 7 and 8 reference existing test fixtures by placeholder name (`StubJiraClient`, `aplan_cmd`, `renderWithUrql`). Each step says to read the neighbouring tests and match what is actually there. That is unavoidable without transcribing three test harnesses into the plan, but it is the one place an implementer must look around before typing.
