# aplan Sessions — Plan 1: socle

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every Claude Code session its own row, its own task and its own tracking
decision, so N sessions can work on N tasks while the global pointer keeps meaning "the
human, working by hand".

**Architecture:** A new `sessions` table holds one row per Claude Code session, keyed by the
`CLAUDE_CODE_SESSION_ID` the harness already exports into every Bash call. Worklog entries
and activity slots gain a nullable `session_id` (NULL = the human) and slots gain a `source`
telling the worklog projection apart from anything hand-made. The CLI resolves an implicit
target as `--task` → session's task → global pointer, and refuses (exit 4) instead of falling
back when the session exists but is not tracking. Time materialization is **unchanged** in
this plan: plan 2 owns the watermark.

**Tech Stack:** Rust (stable), sqlx 0.8 + SQLite (runtime queries), async-graphql 7, Axum 0.7,
clap 4 (`env` feature), graphql_client 0.14, wiremock + assert_cmd for CLI tests.

**Spec:** `docs/superpowers/specs/2026-08-04-aplan-session-scoped-worklog-design.md`

## Global Constraints

- **Branch first.** The repo is on `main`; never commit there directly:
  `cd ~/appfactory/aggregated_plan && git switch -c feat/aplan-sessions-socle`
- **Commit messages:** plain imperative subject (no Jira ticket exists for this work). **No
  `Co-Authored-By` footer, no `Signed-off-by` trailer.** Stage only the files the task names —
  never `git add -A` / `git add .`.
- **DDD layer rules are strict.** `domain/` = pure business logic, zero I/O, deps limited to
  chrono/serde/uuid/thiserror. `application/` = repository traits + use cases, depends on
  domain only. `infrastructure/` = sqlx implementations. `api/` = Axum + async-graphql.
- **Rust conventions:** no `.unwrap()` in production code, `thiserror` for error enums,
  `#[async_trait]` on async traits, map `sqlx::Error` → `RepositoryError::Database(e.to_string())`,
  runtime `sqlx::query` (never the compile-time `sqlx::query!`).
- **Tests are inline** in `#[cfg(test)] mod tests`; database tests use `sqlite::memory:` via
  `create_sqlite_pool`.
- **`CLAUDE_CODE_SESSION_ID` is set in your own shell whenever you run the suite inside a
  Claude Code session.** Every CLI integration test must call
  `.env_remove("CLAUDE_CODE_SESSION_ID")` unless it is specifically testing env pickup.
  Skipping this makes the suite pass locally, exercise a different branch than the one under
  test, and behave differently in a plain terminal.
- **Do not touch the meaning of `aplan.active_task_id` / `aplan.active_since`.** They are the
  human's pointer. Plan 2 owns the watermark rewrite.
- **Local-day reasoning always goes through** `application::use_cases::worklog::user_timezone`.
  A second reading of `aplan.timezone` can disagree with the first and put one entry on two
  different days.
- **Spec maintenance:** behaviour changes update `SPEC_TECHNIQUE.md` (French) in the same
  commit. Task 10 does this once for the whole plan.
- Migration `014` is the next free number.

**Commands you will use:**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p domain
cargo test -p application
cargo test -p infrastructure
cargo test -p api
cargo test -p cli
cargo check          # enumerates every struct-literal site a new field breaks
cargo clippy
```

---

## File Structure

**Created:**

| File | Responsibility |
|---|---|
| `migrations/sqlite/014_create_sessions.sql` | `sessions` table, `session_id` on entries and slots, `source` on slots |
| `backend/crates/domain/src/types/session.rs` | `Session`, `SessionMode`, `SessionTargetRefusal` — the rule that decides what a session logs against |
| `backend/crates/application/src/repositories/session_repository.rs` | `SessionRepository` trait |
| `backend/crates/application/src/use_cases/session_tracking.rs` | bind / set mode / end / list / resolve-target use cases |
| `backend/crates/application/src/use_cases/slot_classification.rs` | the one-shot pre-014 provenance pass |
| `backend/crates/infrastructure/src/database/session_repo.rs` | `SqliteSessionRepository` |
| `backend/crates/api/src/graphql/types/session.rs` | `SessionGql` |
| `backend/crates/cli/graphql/session.graphql` | `Session` query operation |
| `backend/crates/cli/graphql/bind_session.graphql` | `BindSession` mutation |
| `backend/crates/cli/graphql/set_session_mode.graphql` | `SetSessionMode` mutation |
| `backend/crates/cli/graphql/end_session.graphql` | `EndSession` mutation |
| `backend/crates/cli/graphql/open_sessions.graphql` | `OpenSessions` query |
| `backend/crates/cli/src/session_cmd.rs` | `aplan session …` and `aplan sessions` |

**Modified:**

| File | Change |
|---|---|
| `backend/crates/domain/src/types/mod.rs` | export `session` |
| `backend/crates/domain/src/types/activity.rs` | `SlotSource`, two `ActivitySlot` constructors, two new fields |
| `backend/crates/domain/src/types/worklog.rs` | `session_id` field + `by_session` |
| `backend/crates/application/src/repositories/mod.rs` | export `session_repository`; `set_source` on `ActivitySlotRepository` |
| `backend/crates/application/src/use_cases/mod.rs` | export the two new modules |
| `backend/crates/application/src/use_cases/{activity_tracking,worklog,reattribution,activity_reporting,brief}.rs` | slot literals → constructors |
| `backend/crates/infrastructure/src/database/{mod,conversions,activity_repo,worklog_repo}.rs` | wire the repo, the enums, and the new columns |
| `backend/crates/infrastructure/src/database/connection.rs` | migration tests |
| `backend/crates/api/src/main.rs` | build `session_repo`, run the classification pass |
| `backend/crates/api/src/graphql/{schema,query,mutation}.rs`, `types/{mod,enums,activity}.rs` | session surface + `sessionId` on `addWorklogEntry` |
| `backend/crates/cli/src/{cli,main,commands,lookup,queries}.rs` | `--session`, resolution order, wiring |
| `backend/crates/cli/graphql/{add_worklog_entry,schema}.graphql` | `sessionId` argument, regenerated schema |
| `backend/crates/cli/tests/integration.rs` | resolution-order and session-command tests |
| `SPEC_TECHNIQUE.md` | document the session model (French) |

---

## Task 1: Migration 014

**Files:**
- Create: `migrations/sqlite/014_create_sessions.sql`
- Modify: `backend/crates/infrastructure/src/database/connection.rs` (add tests to the
  existing `#[cfg(test)] mod migration_tests`)

**Interfaces:**
- Consumes: nothing.
- Produces: table `sessions(id, user_id, task_id, mode, label, started_at, last_seen_at,
  last_flush_at, ended_at)`; columns `worklog_entries.session_id`,
  `activity_slots.session_id`, `activity_slots.source`.

- [ ] **Step 1: Write the failing migration tests**

Append to `mod migration_tests` in `backend/crates/infrastructure/src/database/connection.rs`:

```rust
    #[tokio::test]
    async fn migrations_create_the_sessions_table() {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sessions'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, 1, "014 must create the sessions table");
    }

    #[tokio::test]
    async fn migrations_add_the_authorship_and_provenance_columns() {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        for (table, column) in [
            ("worklog_entries", "session_id"),
            ("activity_slots", "session_id"),
            ("activity_slots", "source"),
        ] {
            let names: Vec<(String,)> =
                sqlx::query_as("SELECT name FROM pragma_table_info(?)")
                    .bind(table)
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            assert!(
                names.iter().any(|(n,)| n == column),
                "{table}.{column} should exist after 014"
            );
        }
    }

    /// `mode` is the one column a wrong write would make meaningless: a session
    /// neither tracking nor off has no defined behaviour, so the store refuses it.
    #[tokio::test]
    async fn the_sessions_table_rejects_an_unknown_mode() {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        let result = sqlx::query(
            "INSERT INTO sessions (id, user_id, mode, started_at, last_seen_at)
             VALUES ('s1', '00000000-0000-0000-0000-000000000001', 'maybe',
                     '2026-08-04T09:00:00+00:00', '2026-08-04T09:00:00+00:00')",
        )
        .execute(&pool)
        .await;
        assert!(result.is_err(), "the CHECK on mode must reject `maybe`");
    }

    /// Adding a column to a populated table is the failure mode of an `ALTER`
    /// that SQLite would rather reject than migrate: the existing rows must all
    /// still be there, with the new columns null.
    #[tokio::test]
    async fn the_new_columns_leave_existing_rows_untouched() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let mut migrator = sqlx::migrate!("../../../migrations/sqlite");
        let all = migrator.migrations.to_vec();
        assert!(
            all.iter().any(|m| m.version == 14),
            "014 must be part of the embedded set"
        );

        migrator.migrations = Cow::Owned(all.iter().filter(|m| m.version < 14).cloned().collect());
        migrator.run(&pool).await.expect("001..013 apply");

        sqlx::query(
            "INSERT INTO users (id, name, email, created_at)
             VALUES ('00000000-0000-0000-0000-000000000001', 'T', 't@example.test', '2026-08-04T09:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tasks (id, user_id, title, source, status, urgency, impact, created_at, updated_at)
             VALUES ('t1', '00000000-0000-0000-0000-000000000001', 'Tâche', 'manual', 'todo', 2, 2,
                     '2026-08-04T09:00:00+00:00', '2026-08-04T09:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO activity_slots (id, user_id, task_id, start_time, end_time, half_day, date, created_at)
             VALUES ('sl1', '00000000-0000-0000-0000-000000000001', 't1',
                     '2026-08-04T09:00:00+00:00', '2026-08-04T11:00:00+00:00', 'morning',
                     '2026-08-04', '2026-08-04T11:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        migrator.ignore_missing = true;
        migrator.migrations = Cow::Owned(all.iter().filter(|m| m.version == 14).cloned().collect());
        migrator.run(&pool).await.expect("014 applies");

        let rows: Vec<(String, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT id, session_id, source FROM activity_slots ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            rows,
            vec![("sl1".to_string(), None, None)],
            "the row survives and its new columns are null"
        );
    }
```

If `tasks` or `activity_slots` reject those inserts, read the real column list with
`sqlx::query_as("SELECT name FROM pragma_table_info('tasks')")` and fix the literal — do not
change the assertions.

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p infrastructure migration
```

Expected: FAIL — `sessions` table missing, `014 must be part of the embedded set`.

- [ ] **Step 3: Write the migration**

Create `migrations/sqlite/014_create_sessions.sql`:

```sql
-- 014_create_sessions.sql
-- One row per Claude Code session.
--
-- The global `aplan.active_task_id` / `aplan.active_since` pair keeps its meaning
-- untouched: it is the human, working by hand, one task at a time. These rows are
-- the other actors — one per Claude session, each with its own task — so two
-- sessions can work on two tasks without overwriting one another's pointer.
CREATE TABLE sessions (
    id            TEXT PRIMARY KEY,                              -- CLAUDE_CODE_SESSION_ID
    user_id       TEXT NOT NULL REFERENCES users(id),
    task_id       TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    mode          TEXT NOT NULL CHECK (mode IN ('tracking','off')),
    label         TEXT,                                          -- the hook's `cwd`, for display
    started_at    TEXT NOT NULL,
    last_seen_at  TEXT NOT NULL,
    last_flush_at TEXT,
    ended_at      TEXT
);

-- `aplan sessions` reads the open ones; the idle-session reaper (plan 3) reads the
-- same index from the other end.
CREATE INDEX idx_sessions_user_open ON sessions(user_id, ended_at);

-- Authorship. NULL means the human: the global pointer has no session row, and it
-- never will.
ALTER TABLE worklog_entries ADD COLUMN session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL;
ALTER TABLE activity_slots  ADD COLUMN session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL;

-- Provenance: 'worklog' is a slot the flush projection owns and a rebuild may
-- replace, 'manual' is anything else (a live timer, a hand-made slot).
--
-- Deliberately left NULL for the rows already in the table, and deliberately without
-- a CHECK: the enum is enforced in Rust (`SlotSource`), because fixing a CHECK on an
-- existing SQLite table costs the full table rebuild that migration 013 had to do.
-- The API's one-shot classification pass fills these rows from the data itself, and
-- until it has run — or if it ever misses one — a NULL reads as 'manual', so the
-- unknown is protected rather than rebuilt away.
ALTER TABLE activity_slots ADD COLUMN source TEXT;
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p infrastructure migration
```

Expected: PASS, including the pre-existing `the_migrated_schema_has_no_broken_foreign_key`
(the two added `REFERENCES` columns default to NULL, which is what lets SQLite accept them
with `foreign_keys = ON`).

- [ ] **Step 5: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add migrations/sqlite/014_create_sessions.sql \
        backend/crates/infrastructure/src/database/connection.rs
git commit -m "Add sessions table, slot provenance and entry authorship columns"
```

---

## Task 2: The `Session` domain type

**Files:**
- Create: `backend/crates/domain/src/types/session.rs`
- Modify: `backend/crates/domain/src/types/mod.rs`

**Interfaces:**
- Consumes: `domain::errors::DomainError`, `domain::types::common::{UserId, TaskId}`.
- Produces:
  - `pub type SessionId = String`
  - `pub enum SessionMode { Tracking, Off }`
  - `pub enum SessionTargetRefusal { Ended, NotTracked, NoTask }`
  - `pub struct Session { id, user_id, task_id: Option<TaskId>, mode, label: Option<String>, started_at, last_seen_at, last_flush_at: Option<DateTime<Utc>>, ended_at: Option<DateTime<Utc>> }`
  - `Session::tracking(id, user_id, task_id, label, now) -> Result<Session, DomainError>`
  - `Session::off(id, user_id, label, now) -> Result<Session, DomainError>`
  - `Session::is_open(&self) -> bool`
  - `Session::flush_window_start(&self) -> DateTime<Utc>`
  - `Session::target(&self) -> Result<TaskId, SessionTargetRefusal>`
  - `pub const SESSION_LABEL_MAX_LEN: usize = 200`

- [ ] **Step 1: Write the failing tests**

Create `backend/crates/domain/src/types/session.rs` with only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }
    fn tid() -> TaskId {
        Uuid::new_v4()
    }
    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-04T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn tracking_starts_open_on_its_task() {
        let s = Session::tracking("abc".into(), uid(), tid(), None, t0()).unwrap();
        assert_eq!(s.mode, SessionMode::Tracking);
        assert!(s.is_open());
        assert_eq!(s.started_at, t0());
        assert_eq!(s.last_seen_at, t0());
        assert!(s.last_flush_at.is_none());
    }

    #[test]
    fn an_empty_id_is_refused() {
        let err = Session::tracking("   ".into(), uid(), tid(), None, t0()).unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)));
    }

    #[test]
    fn the_id_is_trimmed_not_reformatted() {
        // The value is minted by another program: we normalise whitespace and keep
        // the rest verbatim, whatever shape it has.
        let s = Session::tracking("  not-a-uuid  ".into(), uid(), tid(), None, t0()).unwrap();
        assert_eq!(s.id, "not-a-uuid");
    }

    #[test]
    fn an_oversize_label_is_truncated_rather_than_refused() {
        // The label is a working directory, not user input. Failing a bind over it
        // would cost a session its worklog for a display string.
        let long = "x".repeat(SESSION_LABEL_MAX_LEN + 50);
        let s = Session::tracking("abc".into(), uid(), tid(), Some(long), t0()).unwrap();
        assert_eq!(s.label.unwrap().chars().count(), SESSION_LABEL_MAX_LEN);
    }

    #[test]
    fn a_tracking_session_targets_its_task() {
        let task = tid();
        let s = Session::tracking("abc".into(), uid(), task, None, t0()).unwrap();
        assert_eq!(s.target(), Ok(task));
    }

    #[test]
    fn an_off_session_refuses_a_target_instead_of_falling_back() {
        // The whole point of the feature: "ne pas tracker" must be a refusal the
        // caller has to handle, never a silent fallback onto the human's pointer.
        let s = Session::off("abc".into(), uid(), None, t0()).unwrap();
        assert_eq!(s.target(), Err(SessionTargetRefusal::NotTracked));
    }

    #[test]
    fn a_tracking_session_without_a_task_refuses_too() {
        let mut s = Session::tracking("abc".into(), uid(), tid(), None, t0()).unwrap();
        s.task_id = None;
        assert_eq!(s.target(), Err(SessionTargetRefusal::NoTask));
    }

    #[test]
    fn an_ended_session_refuses_before_anything_else() {
        let mut s = Session::tracking("abc".into(), uid(), tid(), None, t0()).unwrap();
        s.ended_at = Some(t0() + chrono::Duration::hours(2));
        assert!(!s.is_open());
        assert_eq!(s.target(), Err(SessionTargetRefusal::Ended));
    }

    #[test]
    fn the_flush_window_starts_at_the_last_flush_when_there_was_one() {
        let mut s = Session::tracking("abc".into(), uid(), tid(), None, t0()).unwrap();
        assert_eq!(s.flush_window_start(), t0(), "no flush yet → session start");

        let later = t0() + chrono::Duration::hours(3);
        s.last_flush_at = Some(later);
        assert_eq!(s.flush_window_start(), later);
    }

    #[test]
    fn mode_round_trips_through_its_wire_form() {
        for mode in [SessionMode::Tracking, SessionMode::Off] {
            assert_eq!(SessionMode::parse(mode.as_str()).unwrap(), mode);
        }
        assert!(SessionMode::parse("maybe").is_err());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p domain session
```

Expected: FAIL to compile — `cannot find type Session in this scope`.

- [ ] **Step 3: Write the implementation**

Prepend to `backend/crates/domain/src/types/session.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::DomainError;

use super::common::*;

/// A Claude Code session id, exactly as the harness exports it
/// (`CLAUDE_CODE_SESSION_ID`).
///
/// A `String`, not a `Uuid`, on purpose: the value is minted by another program. If
/// the harness ever changes its format, parsing it here would turn every log call
/// into "this session does not exist" — a silent loss of worklog, which is the one
/// failure this whole feature exists to prevent. We store what we are given.
pub type SessionId = String;

/// How long a label may be before it is cut. It is a working directory shown in
/// `aplan sessions`, so length is a display concern, never a reason to fail a bind.
pub const SESSION_LABEL_MAX_LEN: usize = 200;

/// What a session was told to do with its worklog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionMode {
    /// Logging is on, against `Session::task_id`.
    Tracking,
    /// The user answered "ne pas tracker" for this session. Persisted precisely so a
    /// re-fired SessionStart hook reports the decision instead of re-deriving one
    /// from the human's pointer.
    Off,
}

impl SessionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionMode::Tracking => "tracking",
            SessionMode::Off => "off",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "tracking" => Ok(SessionMode::Tracking),
            "off" => Ok(SessionMode::Off),
            other => Err(DomainError::ValidationError(format!(
                "unknown session mode `{other}`"
            ))),
        }
    }
}

/// Why a session cannot be the implicit target of a logging verb.
///
/// Three distinct reasons rather than one boolean, because each one deserves its own
/// sentence at the terminal: "this session is not tracked" is a decision the user
/// made, "no task bound" is a setup step they still owe, and "session ended" means
/// they are looking at a stale id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTargetRefusal {
    Ended,
    NotTracked,
    NoTask,
}

/// One Claude Code session, and what it logs against.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub user_id: UserId,
    pub task_id: Option<TaskId>,
    pub mode: SessionMode,
    pub label: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    /// Up to when this session's time has already been materialized. `None` means
    /// "nothing yet", and the window then starts at `started_at`.
    pub last_flush_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl Session {
    /// A session that logs against `task_id`.
    pub fn tracking(
        id: SessionId,
        user_id: UserId,
        task_id: TaskId,
        label: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        Self::new(id, user_id, Some(task_id), SessionMode::Tracking, label, now)
    }

    /// A session the user opted out of tracking.
    pub fn off(
        id: SessionId,
        user_id: UserId,
        label: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        Self::new(id, user_id, None, SessionMode::Off, label, now)
    }

    fn new(
        id: SessionId,
        user_id: UserId,
        task_id: Option<TaskId>,
        mode: SessionMode,
        label: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        let id = id.trim().to_string();
        if id.is_empty() {
            return Err(DomainError::ValidationError(
                "session id cannot be empty".into(),
            ));
        }
        Ok(Self {
            id,
            user_id,
            task_id,
            mode,
            label: label.map(|l| l.chars().take(SESSION_LABEL_MAX_LEN).collect()),
            started_at: now,
            last_seen_at: now,
            last_flush_at: None,
            ended_at: None,
        })
    }

    pub fn is_open(&self) -> bool {
        self.ended_at.is_none()
    }

    /// The instant a flush should start looking from. Plan 2 uses this to pick the
    /// half-days it rebuilds; it never decides which entries count.
    pub fn flush_window_start(&self) -> DateTime<Utc> {
        self.last_flush_at.unwrap_or(self.started_at)
    }

    /// The task an implicit-target verb should write to, or why it must not write.
    ///
    /// Ended is checked first because it is the most specific state: a stale id is a
    /// different mistake from a deliberate opt-out, and telling the two apart is what
    /// keeps the message useful.
    pub fn target(&self) -> Result<TaskId, SessionTargetRefusal> {
        if self.ended_at.is_some() {
            return Err(SessionTargetRefusal::Ended);
        }
        if self.mode == SessionMode::Off {
            return Err(SessionTargetRefusal::NotTracked);
        }
        self.task_id.ok_or(SessionTargetRefusal::NoTask)
    }
}
```

Add to `backend/crates/domain/src/types/mod.rs`, following the existing lines:

```rust
pub mod session;
pub use session::*;
```

Match whatever re-export form the neighbouring modules already use in that file — if they are
declared `mod worklog; pub use worklog::*;`, mirror that exactly.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p domain session
```

Expected: PASS (11 tests). `Uuid` is imported for the test helpers only — if the compiler warns
that it is unused in the non-test build, move the import into `mod tests`.

- [ ] **Step 5: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/domain/src/types/session.rs \
        backend/crates/domain/src/types/mod.rs
git commit -m "Add Session domain type with an explicit target refusal"
```

---

## Task 3: Slot provenance and entry authorship in the domain

**Files:**
- Modify: `backend/crates/domain/src/types/activity.rs`
- Modify: `backend/crates/domain/src/types/worklog.rs`
- Modify (mechanical, compiler-driven): `backend/crates/application/src/use_cases/activity_tracking.rs`,
  `worklog.rs`, `reattribution.rs`, `activity_reporting.rs`, `brief.rs`,
  `backend/crates/domain/src/rules/reattribution.rs`,
  `backend/crates/infrastructure/src/database/activity_repo.rs`

**Interfaces:**
- Consumes: `SessionId` from Task 2.
- Produces:
  - `pub enum SlotSource { Worklog, Manual }` with `SlotSource::is_projection(&self) -> bool`
    — **not** `is_rebuildable`: that name already belongs to the free function
    `is_rebuildable(&ActivitySlot)` in `domain/src/rules/reattribution.rs:171`, which answers
    "may this slot be rebuilt". This method is one *input* to that answer, and two same-named
    predicates with different truth conditions beside a deletion site is how billable hours
    get deleted. Ruled by the human during task 3's review.
  - `ActivitySlot.source: SlotSource`, `ActivitySlot.session_id: Option<SessionId>`
  - `ActivitySlot::from_worklog(user_id, task_id, session_id, start_time, end_time, half_day, date, now) -> ActivitySlot`
  - `ActivitySlot::manual(user_id, task_id: Option<TaskId>, start_time, end_time: Option<DateTime<Utc>>, half_day, date, now) -> ActivitySlot`
  - `WorklogEntry.session_id: Option<SessionId>` and `WorklogEntry::by_session(self, Option<SessionId>) -> WorklogEntry`

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `backend/crates/domain/src/types/activity.rs` (create the module if
the file has none):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn uid() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }
    fn t(h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, h, 0, 0).unwrap()
    }

    #[test]
    fn a_worklog_slot_is_rebuildable_and_carries_its_author() {
        let task = Uuid::new_v4();
        let slot = ActivitySlot::from_worklog(
            uid(),
            task,
            Some("sess-1".into()),
            t(9),
            t(11),
            HalfDay::Morning,
            t(9).date_naive(),
            t(11),
        );
        assert_eq!(slot.task_id, Some(task));
        assert_eq!(slot.end_time, Some(t(11)));
        assert_eq!(slot.source, SlotSource::Worklog);
        assert_eq!(slot.session_id.as_deref(), Some("sess-1"));
        assert!(slot.source.is_projection());
    }

    #[test]
    fn a_manual_slot_is_never_rebuildable() {
        // The regression this field exists to prevent: today's flush only ever
        // appends, so nothing protects a hand-made slot from a rebuild that
        // canonicalises the half-day it sits in.
        let slot = ActivitySlot::manual(
            uid(),
            None,
            t(14),
            None,
            HalfDay::Afternoon,
            t(14).date_naive(),
            t(14),
        );
        assert_eq!(slot.source, SlotSource::Manual);
        assert!(slot.session_id.is_none());
        assert!(!slot.source.is_projection());
    }
}
```

Append to `mod tests` in `backend/crates/domain/src/types/worklog.rs`:

```rust
    #[test]
    fn an_entry_is_the_humans_until_a_session_claims_it() {
        let entry = WorklogEntry::new(uid(), tid(), "fait".into(), t0(), t0()).unwrap();
        assert!(
            entry.session_id.is_none(),
            "NULL is the human working by hand"
        );

        let claimed = entry.by_session(Some("sess-1".into()));
        assert_eq!(claimed.session_id.as_deref(), Some("sess-1"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p domain
```

Expected: FAIL to compile — `no function or associated item named from_worklog`,
`no method named by_session`.

- [ ] **Step 3: Write the implementation**

Rewrite the non-test part of `backend/crates/domain/src/types/activity.rs`:

```rust
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::*;
use super::session::SessionId;

/// Where a slot came from, and therefore whether anything may replace it.
///
/// `activity_slots` are a projection of worklog timestamps, and the flush rebuilds
/// that projection by dropping what it wrote before. Without this distinction the
/// rebuild has no way to tell its own output from a slot the user created by hand,
/// and the automatic path would silently delete the manual one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotSource {
    /// Written by the worklog projection. A rebuild owns it.
    Worklog,
    /// Anything else: a live timer, a hand-made slot, a row whose provenance is
    /// unknown. Never rebuilt.
    Manual,
}

impl SlotSource {
    /// Is this slot one the worklog projection owns?
    ///
    /// Deliberately not called `is_rebuildable`: that question is answered by
    /// `domain::rules::reattribution::is_rebuildable`, which also requires the slot to
    /// be closed. This is one input to it, and a rebuild that consulted only this
    /// input would delete a running timer's slot.
    pub fn is_projection(&self) -> bool {
        matches!(self, SlotSource::Worklog)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySlot {
    pub id: ActivitySlotId,
    pub user_id: UserId,
    pub task_id: Option<TaskId>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub half_day: HalfDay,
    pub date: NaiveDate,
    pub created_at: DateTime<Utc>,
    /// Who produced the time. `None` is the human, working by hand.
    pub session_id: Option<SessionId>,
    pub source: SlotSource,
}

impl ActivitySlot {
    /// A closed slot the worklog projection owns.
    #[allow(clippy::too_many_arguments)]
    pub fn from_worklog(
        user_id: UserId,
        task_id: TaskId,
        session_id: Option<SessionId>,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        half_day: HalfDay,
        date: NaiveDate,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            task_id: Some(task_id),
            start_time,
            end_time: Some(end_time),
            half_day,
            date,
            created_at: now,
            session_id,
            source: SlotSource::Worklog,
        }
    }

    /// A slot no rebuild may touch — including an open one, which is a running timer.
    #[allow(clippy::too_many_arguments)]
    pub fn manual(
        user_id: UserId,
        task_id: Option<TaskId>,
        start_time: DateTime<Utc>,
        end_time: Option<DateTime<Utc>>,
        half_day: HalfDay,
        date: NaiveDate,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            task_id,
            start_time,
            end_time,
            half_day,
            date,
            created_at: now,
            session_id: None,
            source: SlotSource::Manual,
        }
    }
}
```

In `backend/crates/domain/src/types/worklog.rs`, add the field to the struct and the builder
to the `impl`, leaving `new`'s signature alone:

```rust
pub struct WorklogEntry {
    // ... existing fields ...
    /// The session that wrote this entry. `None` is the human, working by hand.
    pub session_id: Option<SessionId>,
}
```

```rust
    /// Attribute the entry to the session that wrote it.
    ///
    /// A builder rather than a `new` parameter: `new` has 40-odd call sites, almost
    /// all of them tests that have nothing to say about authorship, and widening its
    /// signature would churn every one of them to pass `None`.
    pub fn by_session(mut self, session_id: Option<SessionId>) -> Self {
        self.session_id = session_id;
        self
    }
```

Add `session_id: None` to the struct literal inside `WorklogEntry::new`, and
`use super::session::SessionId;` to the imports.

- [ ] **Step 4: Fix every site the compiler names**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo check 2>&1 | grep -E '^(error|  -->)' | head -60
```

There are ~16 `ActivitySlot { … }` literals. Replace each by the constructor that matches its
intent:

- `application/src/use_cases/worklog.rs` (`materialize_worklog_time`) → `ActivitySlot::from_worklog(user_id, task_id, None, start_utc, end_utc, block.half_day, block.date, Utc::now())`. Session attribution on
  materialized slots arrives with plan 2, which is where the flush learns which session asked;
  passing `None` here keeps this plan's behaviour identical to today's.
- `application/src/use_cases/reattribution.rs` (the rebuild) → `ActivitySlot::from_worklog(...)`, same shape.
- `application/src/use_cases/activity_tracking.rs` (`start_activity`, `create_manual_activity_slot`) → `ActivitySlot::manual(...)`.
- `infrastructure/src/database/activity_repo.rs` (row → domain) → build the literal directly and
  read the two new columns; Task 6 rewrites this properly, so for now pass
  `session_id: None, source: SlotSource::Manual` and leave a `// Task 6` marker.
- `domain/src/rules/reattribution.rs`, `application/src/use_cases/activity_reporting.rs`,
  `brief.rs` → test fixtures; add `session_id: None, source: SlotSource::Manual` to each literal,
  except any fixture that exists to represent a flush-derived slot, which takes
  `SlotSource::Worklog`.

`is_rebuildable` in `domain/src/rules/reattribution.rs` keeps its current body in this task —
Task 6 of plan 2 is where it starts consulting `source`. Do not change it here, and do not
change its tests.

- [ ] **Step 5: Run the whole backend suite**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test
```

Expected: PASS. Any red test here means a fixture was given the wrong `source` — fix the
fixture, never the assertion.

- [ ] **Step 6: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/domain backend/crates/application backend/crates/infrastructure
git commit -m "Carry slot provenance and entry authorship through the domain"
```

---

## Task 4: `SessionRepository` trait

**Files:**
- Create: `backend/crates/application/src/repositories/session_repository.rs`
- Modify: `backend/crates/application/src/repositories/mod.rs`

**Interfaces:**
- Consumes: `domain::types::{Session, SessionId, UserId}`, `crate::errors::RepositoryError`.
- Produces: `pub trait SessionRepository` with `find_by_id`, `upsert`, `list_open`, `touch`,
  `set_last_flush`, `end`.

- [ ] **Step 1: Write the trait**

There is no test for a trait definition; the tests live with the double (Task 5) and the SQLite
implementation (Task 6). Create
`backend/crates/application/src/repositories/session_repository.rs`:

```rust
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::types::*;

use crate::errors::RepositoryError;

/// Persistence for the session actors.
///
/// Note what is absent: no `delete`. A session is history — which entries it wrote,
/// which half-days its flush owns — and history that can vanish is history the
/// reattribution repair cannot reason about. Sessions end; they do not disappear.
#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn find_by_id(
        &self,
        id: &str,
        user_id: UserId,
    ) -> Result<Option<Session>, RepositoryError>;

    /// Insert, or overwrite the mutable columns of an existing row: `task_id`,
    /// `mode`, `label`, `last_seen_at`. `started_at` is never rewritten — a session
    /// that rebinds is the same session, and plan 2's flush window is anchored on it.
    async fn upsert(&self, session: &Session) -> Result<(), RepositoryError>;

    /// Open sessions, most recently seen first. What `aplan sessions` prints.
    async fn list_open(&self, user_id: UserId) -> Result<Vec<Session>, RepositoryError>;

    /// Bump `last_seen_at`. Returns false when no open session has that id.
    async fn touch(
        &self,
        id: &str,
        user_id: UserId,
        at: DateTime<Utc>,
    ) -> Result<bool, RepositoryError>;

    /// Advance the flush watermark of one session. Plan 2's flush calls this.
    async fn set_last_flush(
        &self,
        id: &str,
        user_id: UserId,
        at: DateTime<Utc>,
    ) -> Result<bool, RepositoryError>;

    /// Close the session. Idempotent: an already-ended session keeps its first
    /// `ended_at`, because that is when the work actually stopped.
    async fn end(
        &self,
        id: &str,
        user_id: UserId,
        at: DateTime<Utc>,
    ) -> Result<bool, RepositoryError>;
}
```

Add to `backend/crates/application/src/repositories/mod.rs`, matching the existing style:

```rust
pub mod session_repository;
pub use session_repository::*;
```

- [ ] **Step 2: Verify it compiles**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo check -p application
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/application/src/repositories/
git commit -m "Define the SessionRepository trait"
```

---

## Task 5: Session use cases

**Files:**
- Create: `backend/crates/application/src/use_cases/session_tracking.rs`
- Modify: `backend/crates/application/src/use_cases/mod.rs`

**Interfaces:**
- Consumes: `SessionRepository` (Task 4), `Session` / `SessionMode` / `SessionTargetRefusal` (Task 2).
- Produces:
  - `pub struct BindOutcome { pub session: Session, pub previous_task: Option<TaskId> }`
  - `bind_session(repo, user_id, id, task_id, label, now) -> Result<BindOutcome, AppError>`
  - `set_session_mode(repo, user_id, id, mode, label, now) -> Result<Session, AppError>`
  - `end_session(repo, user_id, id, now) -> Result<Option<Session>, AppError>`
  - `list_open_sessions(repo, user_id) -> Result<Vec<Session>, AppError>`
  - `resolve_session_target(repo, user_id, id, now) -> Result<TaskId, AppError>`
  - `pub struct InMemorySessionRepository` in the test module (referenced by later tasks' reading, not by their code)

- [ ] **Step 1: Write the failing tests**

Create `backend/crates/application/src/use_cases/session_tracking.rs` with the test module
first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::TimeZone;
    use std::sync::Mutex;
    use uuid::Uuid;

    use crate::errors::RepositoryError;

    #[derive(Default)]
    struct InMemorySessionRepository {
        rows: Mutex<Vec<Session>>,
    }

    #[async_trait]
    impl SessionRepository for InMemorySessionRepository {
        async fn find_by_id(
            &self,
            id: &str,
            user_id: UserId,
        ) -> Result<Option<Session>, RepositoryError> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.id == id && s.user_id == user_id)
                .cloned())
        }

        async fn upsert(&self, session: &Session) -> Result<(), RepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            match rows.iter_mut().find(|s| s.id == session.id) {
                Some(existing) => {
                    existing.task_id = session.task_id;
                    existing.mode = session.mode;
                    existing.label = session.label.clone();
                    existing.last_seen_at = session.last_seen_at;
                }
                None => rows.push(session.clone()),
            }
            Ok(())
        }

        async fn list_open(&self, user_id: UserId) -> Result<Vec<Session>, RepositoryError> {
            let mut open: Vec<Session> = self
                .rows
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.user_id == user_id && s.is_open())
                .cloned()
                .collect();
            open.sort_by(|a, b| b.last_seen_at.cmp(&a.last_seen_at));
            Ok(open)
        }

        async fn touch(
            &self,
            id: &str,
            user_id: UserId,
            at: DateTime<Utc>,
        ) -> Result<bool, RepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            match rows
                .iter_mut()
                .find(|s| s.id == id && s.user_id == user_id && s.is_open())
            {
                Some(s) => {
                    s.last_seen_at = at;
                    Ok(true)
                }
                None => Ok(false),
            }
        }

        async fn set_last_flush(
            &self,
            id: &str,
            user_id: UserId,
            at: DateTime<Utc>,
        ) -> Result<bool, RepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            match rows.iter_mut().find(|s| s.id == id && s.user_id == user_id) {
                Some(s) => {
                    s.last_flush_at = Some(at);
                    Ok(true)
                }
                None => Ok(false),
            }
        }

        async fn end(
            &self,
            id: &str,
            user_id: UserId,
            at: DateTime<Utc>,
        ) -> Result<bool, RepositoryError> {
            let mut rows = self.rows.lock().unwrap();
            match rows.iter_mut().find(|s| s.id == id && s.user_id == user_id) {
                Some(s) if s.is_open() => {
                    s.ended_at = Some(at);
                    Ok(true)
                }
                _ => Ok(false),
            }
        }
    }

    fn uid() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }
    fn t(h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, h, 0, 0).unwrap()
    }

    #[tokio::test]
    async fn binding_an_unknown_session_creates_it_tracking() {
        let repo = InMemorySessionRepository::default();
        let task = Uuid::new_v4();

        let out = bind_session(&repo, uid(), "s1", task, Some("/home/mbt/x".into()), t(9))
            .await
            .unwrap();

        assert_eq!(out.session.task_id, Some(task));
        assert_eq!(out.session.mode, SessionMode::Tracking);
        assert!(out.previous_task.is_none(), "nothing to flush on a first bind");
    }

    #[tokio::test]
    async fn rebinding_reports_the_task_to_flush_and_keeps_started_at() {
        let repo = InMemorySessionRepository::default();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        bind_session(&repo, uid(), "s1", first, None, t(9)).await.unwrap();

        let out = bind_session(&repo, uid(), "s1", second, None, t(11)).await.unwrap();

        assert_eq!(out.session.task_id, Some(second));
        assert_eq!(
            out.previous_task,
            Some(first),
            "the caller has to flush what the session was on"
        );
        assert_eq!(
            out.session.started_at,
            t(9),
            "a rebind is the same session; plan 2 anchors its window here"
        );
        assert_eq!(out.session.last_seen_at, t(11));
    }

    #[tokio::test]
    async fn rebinding_to_the_same_task_reports_nothing_to_flush() {
        let repo = InMemorySessionRepository::default();
        let task = Uuid::new_v4();
        bind_session(&repo, uid(), "s1", task, None, t(9)).await.unwrap();

        let out = bind_session(&repo, uid(), "s1", task, None, t(11)).await.unwrap();

        assert!(out.previous_task.is_none());
    }

    #[tokio::test]
    async fn binding_revives_a_session_that_was_off() {
        // The user answered "ne pas tracker", then changed their mind mid-session.
        let repo = InMemorySessionRepository::default();
        set_session_mode(&repo, uid(), "s1", SessionMode::Off, None, t(9))
            .await
            .unwrap();

        let task = Uuid::new_v4();
        let out = bind_session(&repo, uid(), "s1", task, None, t(10)).await.unwrap();

        assert_eq!(out.session.mode, SessionMode::Tracking);
        assert_eq!(out.session.target(), Ok(task));
    }

    #[tokio::test]
    async fn setting_a_session_off_clears_its_task() {
        // Leaving a stale task_id behind would let any later code path resolve a
        // target for a session the user opted out of.
        let repo = InMemorySessionRepository::default();
        bind_session(&repo, uid(), "s1", Uuid::new_v4(), None, t(9))
            .await
            .unwrap();

        let s = set_session_mode(&repo, uid(), "s1", SessionMode::Off, None, t(10))
            .await
            .unwrap();

        assert_eq!(s.mode, SessionMode::Off);
        assert!(s.task_id.is_none());
        assert_eq!(s.target(), Err(SessionTargetRefusal::NotTracked));
    }

    #[tokio::test]
    async fn resolving_a_target_refuses_an_off_session_instead_of_falling_back() {
        let repo = InMemorySessionRepository::default();
        set_session_mode(&repo, uid(), "s1", SessionMode::Off, None, t(9))
            .await
            .unwrap();

        let err = resolve_session_target(&repo, uid(), "s1", t(10)).await.unwrap_err();

        assert!(
            matches!(err, AppError::Validation(ref m) if m.contains("not tracked")),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn resolving_a_target_reports_an_unknown_session_as_not_found() {
        let repo = InMemorySessionRepository::default();
        let err = resolve_session_target(&repo, uid(), "ghost", t(10)).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn resolving_a_target_bumps_last_seen() {
        // last_seen_at is what the idle reaper reads. A session that logs is a
        // session that is alive, so resolution is the natural heartbeat.
        let repo = InMemorySessionRepository::default();
        let task = Uuid::new_v4();
        bind_session(&repo, uid(), "s1", task, None, t(9)).await.unwrap();

        let resolved = resolve_session_target(&repo, uid(), "s1", t(15)).await.unwrap();

        assert_eq!(resolved, task);
        let after = repo.find_by_id("s1", uid()).await.unwrap().unwrap();
        assert_eq!(after.last_seen_at, t(15));
    }

    #[tokio::test]
    async fn ending_is_idempotent_and_keeps_the_first_instant() {
        let repo = InMemorySessionRepository::default();
        bind_session(&repo, uid(), "s1", Uuid::new_v4(), None, t(9))
            .await
            .unwrap();

        let first = end_session(&repo, uid(), "s1", t(17)).await.unwrap();
        assert_eq!(first.unwrap().ended_at, Some(t(17)));

        let second = end_session(&repo, uid(), "s1", t(19)).await.unwrap();
        assert!(second.is_none(), "a second end is a no-op");
        let row = repo.find_by_id("s1", uid()).await.unwrap().unwrap();
        assert_eq!(row.ended_at, Some(t(17)));
    }

    #[tokio::test]
    async fn listing_shows_only_open_sessions_most_recent_first() {
        let repo = InMemorySessionRepository::default();
        bind_session(&repo, uid(), "s1", Uuid::new_v4(), None, t(9)).await.unwrap();
        bind_session(&repo, uid(), "s2", Uuid::new_v4(), None, t(11)).await.unwrap();
        bind_session(&repo, uid(), "s3", Uuid::new_v4(), None, t(10)).await.unwrap();
        end_session(&repo, uid(), "s3", t(12)).await.unwrap();

        let open = list_open_sessions(&repo, uid()).await.unwrap();

        let ids: Vec<&str> = open.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["s2", "s1"]);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p application session_tracking
```

Expected: FAIL to compile — `cannot find function bind_session`.

- [ ] **Step 3: Write the implementation**

Prepend to `backend/crates/application/src/use_cases/session_tracking.rs`:

```rust
use chrono::{DateTime, Utc};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::SessionRepository;

/// A bind, and the task the session was on before it.
///
/// The previous task travels back to the caller instead of being flushed here: this
/// crate must not decide when billing-relevant time is materialized, and plan 2 is
/// what teaches the flush to be idempotent. Until then the CLI flushes it exactly as
/// `aplan start` does today, so behaviour is unchanged.
pub struct BindOutcome {
    pub session: Session,
    pub previous_task: Option<TaskId>,
}

/// Point a session at `task_id`, creating it if this is its first bind.
///
/// A bind is also a tracking decision: a session that was `off` and is now given a
/// task is tracking again, because the only way to get here is the user asking for it.
pub async fn bind_session(
    repo: &dyn SessionRepository,
    user_id: UserId,
    id: &str,
    task_id: TaskId,
    label: Option<String>,
    now: DateTime<Utc>,
) -> Result<BindOutcome, AppError> {
    match repo.find_by_id(id, user_id).await? {
        Some(mut existing) => {
            let previous_task = existing.task_id.filter(|prev| *prev != task_id);
            existing.task_id = Some(task_id);
            existing.mode = SessionMode::Tracking;
            existing.last_seen_at = now;
            if label.is_some() {
                existing.label = label;
            }
            repo.upsert(&existing).await?;
            Ok(BindOutcome {
                session: existing,
                previous_task,
            })
        }
        None => {
            let session = Session::tracking(id.to_string(), user_id, task_id, label, now)?;
            repo.upsert(&session).await?;
            Ok(BindOutcome {
                session,
                previous_task: None,
            })
        }
    }
}

/// Record what a session was told to do. `Off` also clears the task: a stale
/// `task_id` on an opted-out session is exactly the state that let a re-fired hook
/// claim to be tracking something the user had declined.
pub async fn set_session_mode(
    repo: &dyn SessionRepository,
    user_id: UserId,
    id: &str,
    mode: SessionMode,
    label: Option<String>,
    now: DateTime<Utc>,
) -> Result<Session, AppError> {
    let mut session = match repo.find_by_id(id, user_id).await? {
        Some(existing) => existing,
        None => Session::off(id.to_string(), user_id, label.clone(), now)?,
    };
    session.mode = mode;
    if mode == SessionMode::Off {
        session.task_id = None;
    }
    session.last_seen_at = now;
    if label.is_some() {
        session.label = label;
    }
    repo.upsert(&session).await?;
    Ok(session)
}

/// Close a session. `Ok(None)` means there was nothing open to close.
pub async fn end_session(
    repo: &dyn SessionRepository,
    user_id: UserId,
    id: &str,
    now: DateTime<Utc>,
) -> Result<Option<Session>, AppError> {
    if !repo.end(id, user_id, now).await? {
        return Ok(None);
    }
    Ok(repo.find_by_id(id, user_id).await?)
}

pub async fn list_open_sessions(
    repo: &dyn SessionRepository,
    user_id: UserId,
) -> Result<Vec<Session>, AppError> {
    Ok(repo.list_open(user_id).await?)
}

/// What this session logs against — or a refusal.
///
/// The refusal is the feature. Falling back to the human's pointer when a session
/// declines tracking is how a Claude ends up reporting work on a task the user
/// explicitly opted out of.
pub async fn resolve_session_target(
    repo: &dyn SessionRepository,
    user_id: UserId,
    id: &str,
    now: DateTime<Utc>,
) -> Result<TaskId, AppError> {
    let session = repo
        .find_by_id(id, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("session {id}")))?;

    let target = session.target().map_err(|refusal| match refusal {
        SessionTargetRefusal::NotTracked => AppError::Validation(format!(
            "session {id} is not tracked (aplan logging is off for it)"
        )),
        SessionTargetRefusal::NoTask => {
            AppError::Validation(format!("session {id} has no task bound"))
        }
        SessionTargetRefusal::Ended => {
            AppError::Validation(format!("session {id} has ended"))
        }
    })?;

    repo.touch(id, user_id, now).await?;
    Ok(target)
}
```

Add to `backend/crates/application/src/use_cases/mod.rs`:

```rust
pub mod session_tracking;
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p application session_tracking
```

Expected: PASS (10 tests).

- [ ] **Step 5: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/application/src/use_cases/
git commit -m "Add session bind, mode, end and target-resolution use cases"
```

---

## Task 6: `SqliteSessionRepository` and the new columns

**Files:**
- Create: `backend/crates/infrastructure/src/database/session_repo.rs`
- Modify: `backend/crates/infrastructure/src/database/mod.rs`
- Modify: `backend/crates/infrastructure/src/database/conversions.rs`
- Modify: `backend/crates/infrastructure/src/database/activity_repo.rs`
- Modify: `backend/crates/infrastructure/src/database/worklog_repo.rs`

**Interfaces:**
- Consumes: `SessionRepository` (Task 4), `Session` / `SlotSource` (Tasks 2-3).
- Produces:
  - `pub struct SqliteSessionRepository` (+ `SqliteSessionRepository::new(pool)`), re-exported from `database::mod`
  - `conversions::{session_mode_to_str, session_mode_from_str, slot_source_to_str, slot_source_from_str}`
  - `activity_slots` and `worklog_entries` round-trip their new columns

- [ ] **Step 1: Write the failing conversion tests**

Append to `mod tests` in `backend/crates/infrastructure/src/database/conversions.rs`:

```rust
    #[test]
    fn session_mode_round_trips() {
        for mode in [SessionMode::Tracking, SessionMode::Off] {
            assert_eq!(session_mode_from_str(session_mode_to_str(mode)), mode);
        }
    }

    #[test]
    fn an_unreadable_session_mode_falls_back_to_off() {
        // A row we cannot interpret must not be able to log. Reading it as
        // `tracking` would make a corrupt row write to a task nobody chose.
        assert_eq!(session_mode_from_str("garbage"), SessionMode::Off);
    }

    #[test]
    fn slot_source_round_trips() {
        for source in [SlotSource::Worklog, SlotSource::Manual] {
            assert_eq!(
                slot_source_from_str(Some(slot_source_to_str(source))),
                source
            );
        }
    }

    #[test]
    fn an_unclassified_slot_reads_as_manual() {
        // Migration 014 leaves historical rows NULL until the classification pass
        // runs. A NULL read as `worklog` would let a rebuild delete a slot whose
        // provenance nobody has established yet.
        assert_eq!(slot_source_from_str(None), SlotSource::Manual);
        assert_eq!(slot_source_from_str(Some("nonsense")), SlotSource::Manual);
    }
```

- [ ] **Step 2: Run to verify they fail**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p infrastructure conversions
```

Expected: FAIL — `cannot find function session_mode_to_str`.

- [ ] **Step 3: Write the conversions**

Append to `backend/crates/infrastructure/src/database/conversions.rs`, next to the existing
`*_to_str` / `*_from_str` pairs:

```rust
pub fn session_mode_to_str(m: SessionMode) -> &'static str {
    m.as_str()
}

/// Anything unreadable is `Off`: a row we cannot interpret must not be able to log.
pub fn session_mode_from_str(s: &str) -> SessionMode {
    SessionMode::parse(s).unwrap_or(SessionMode::Off)
}

pub fn slot_source_to_str(s: SlotSource) -> &'static str {
    match s {
        SlotSource::Worklog => "worklog",
        SlotSource::Manual => "manual",
    }
}

/// NULL and anything unrecognised are `Manual` — the value nothing rebuilds.
/// Migration 014 leaves pre-existing rows NULL on purpose, and the safe reading of
/// "provenance unknown" is "do not touch it".
pub fn slot_source_from_str(s: Option<&str>) -> SlotSource {
    match s {
        Some("worklog") => SlotSource::Worklog,
        _ => SlotSource::Manual,
    }
}
```

- [ ] **Step 4: Run to verify they pass**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p infrastructure conversions
```

Expected: PASS.

- [ ] **Step 5: Write the failing repository tests**

Create `backend/crates/infrastructure/src/database/session_repo.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::connection::create_sqlite_pool;
    use chrono::TimeZone;

    async fn setup() -> SqlitePool {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        sqlx::query(
            "INSERT INTO tasks (id, user_id, title, source, status, urgency, impact, created_at, updated_at)
             VALUES (?, ?, 'Tâche', 'manual', 'todo', 2, 2, ?, ?)",
        )
        .bind(task_id().to_string())
        .bind(user_id().to_string())
        .bind(t(8).to_rfc3339())
        .bind(t(8).to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }
    fn task_id() -> TaskId {
        Uuid::parse_str("00000000-0000-0000-0000-0000000000aa").unwrap()
    }
    fn t(h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, h, 0, 0).unwrap()
    }

    #[tokio::test]
    async fn a_session_round_trips_every_column() {
        let repo = SqliteSessionRepository::new(setup().await);
        let session =
            Session::tracking("s1".into(), user_id(), task_id(), Some("/tmp/x".into()), t(9))
                .unwrap();

        repo.upsert(&session).await.unwrap();
        let found = repo.find_by_id("s1", user_id()).await.unwrap().unwrap();

        assert_eq!(found.id, "s1");
        assert_eq!(found.task_id, Some(task_id()));
        assert_eq!(found.mode, SessionMode::Tracking);
        assert_eq!(found.label.as_deref(), Some("/tmp/x"));
        assert_eq!(found.started_at, t(9));
        assert_eq!(found.last_seen_at, t(9));
        assert!(found.last_flush_at.is_none());
        assert!(found.ended_at.is_none());
    }

    #[tokio::test]
    async fn upsert_keeps_started_at_and_overwrites_the_rest() {
        let repo = SqliteSessionRepository::new(setup().await);
        let mut session =
            Session::tracking("s1".into(), user_id(), task_id(), None, t(9)).unwrap();
        repo.upsert(&session).await.unwrap();

        session.started_at = t(15); // a caller that got this wrong must not win
        session.last_seen_at = t(15);
        session.mode = SessionMode::Off;
        session.task_id = None;
        repo.upsert(&session).await.unwrap();

        let found = repo.find_by_id("s1", user_id()).await.unwrap().unwrap();
        assert_eq!(found.started_at, t(9), "started_at is written once");
        assert_eq!(found.last_seen_at, t(15));
        assert_eq!(found.mode, SessionMode::Off);
        assert!(found.task_id.is_none());
    }

    #[tokio::test]
    async fn find_by_id_is_scoped_to_the_user() {
        let repo = SqliteSessionRepository::new(setup().await);
        let session =
            Session::tracking("s1".into(), user_id(), task_id(), None, t(9)).unwrap();
        repo.upsert(&session).await.unwrap();

        let other = Uuid::parse_str("00000000-0000-0000-0000-0000000000ff").unwrap();
        assert!(repo.find_by_id("s1", other).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn touch_and_set_last_flush_move_only_their_own_column() {
        let repo = SqliteSessionRepository::new(setup().await);
        repo.upsert(&Session::tracking("s1".into(), user_id(), task_id(), None, t(9)).unwrap())
            .await
            .unwrap();

        assert!(repo.touch("s1", user_id(), t(11)).await.unwrap());
        assert!(repo.set_last_flush("s1", user_id(), t(12)).await.unwrap());

        let found = repo.find_by_id("s1", user_id()).await.unwrap().unwrap();
        assert_eq!(found.last_seen_at, t(11));
        assert_eq!(found.last_flush_at, Some(t(12)));
        assert_eq!(found.started_at, t(9));
    }

    #[tokio::test]
    async fn touch_reports_false_for_an_unknown_or_ended_session() {
        let repo = SqliteSessionRepository::new(setup().await);
        assert!(!repo.touch("ghost", user_id(), t(11)).await.unwrap());

        repo.upsert(&Session::tracking("s1".into(), user_id(), task_id(), None, t(9)).unwrap())
            .await
            .unwrap();
        repo.end("s1", user_id(), t(17)).await.unwrap();
        assert!(
            !repo.touch("s1", user_id(), t(18)).await.unwrap(),
            "an ended session is not alive"
        );
    }

    #[tokio::test]
    async fn end_is_idempotent() {
        let repo = SqliteSessionRepository::new(setup().await);
        repo.upsert(&Session::tracking("s1".into(), user_id(), task_id(), None, t(9)).unwrap())
            .await
            .unwrap();

        assert!(repo.end("s1", user_id(), t(17)).await.unwrap());
        assert!(!repo.end("s1", user_id(), t(19)).await.unwrap());
        let found = repo.find_by_id("s1", user_id()).await.unwrap().unwrap();
        assert_eq!(found.ended_at, Some(t(17)));
    }

    #[tokio::test]
    async fn list_open_excludes_ended_and_orders_by_last_seen_desc() {
        let repo = SqliteSessionRepository::new(setup().await);
        for (id, hour) in [("s1", 9), ("s2", 11), ("s3", 10)] {
            repo.upsert(
                &Session::tracking(id.into(), user_id(), task_id(), None, t(hour)).unwrap(),
            )
            .await
            .unwrap();
        }
        repo.end("s3", user_id(), t(12)).await.unwrap();

        let open = repo.list_open(user_id()).await.unwrap();

        let ids: Vec<&str> = open.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["s2", "s1"]);
    }
}
```

- [ ] **Step 6: Run to verify they fail**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p infrastructure session_repo
```

Expected: FAIL to compile.

- [ ] **Step 7: Write the repository**

Prepend to `backend/crates/infrastructure/src/database/session_repo.rs`:

```rust
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::SessionRepository;
use domain::types::*;

use super::conversions::{session_mode_from_str, session_mode_to_str};

pub struct SqliteSessionRepository {
    pool: SqlitePool,
}

impl SqliteSessionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RepositoryError::Serialization(format!("bad timestamp `{s}`: {e}")))
}

fn row_to_session(row: &sqlx::sqlite::SqliteRow) -> Result<Session, RepositoryError> {
    let task_id: Option<String> = Row::get(row, "task_id");
    let task_id = match task_id {
        Some(raw) => Some(
            Uuid::parse_str(&raw)
                .map_err(|e| RepositoryError::Serialization(format!("bad task id: {e}")))?,
        ),
        None => None,
    };
    let user_id: String = Row::get(row, "user_id");
    let mode: String = Row::get(row, "mode");
    let started_at: String = Row::get(row, "started_at");
    let last_seen_at: String = Row::get(row, "last_seen_at");
    let last_flush_at: Option<String> = Row::get(row, "last_flush_at");
    let ended_at: Option<String> = Row::get(row, "ended_at");

    Ok(Session {
        id: Row::get(row, "id"),
        user_id: Uuid::parse_str(&user_id)
            .map_err(|e| RepositoryError::Serialization(format!("bad user id: {e}")))?,
        task_id,
        mode: session_mode_from_str(&mode),
        label: Row::get(row, "label"),
        started_at: parse_datetime(&started_at)?,
        last_seen_at: parse_datetime(&last_seen_at)?,
        last_flush_at: last_flush_at.as_deref().map(parse_datetime).transpose()?,
        ended_at: ended_at.as_deref().map(parse_datetime).transpose()?,
    })
}

#[async_trait]
impl SessionRepository for SqliteSessionRepository {
    async fn find_by_id(
        &self,
        id: &str,
        user_id: UserId,
    ) -> Result<Option<Session>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, user_id, task_id, mode, label, started_at, last_seen_at,
                    last_flush_at, ended_at
             FROM sessions WHERE id = ? AND user_id = ?",
        )
        .bind(id)
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => Ok(Some(row_to_session(row)?)),
            None => Ok(None),
        }
    }

    async fn upsert(&self, session: &Session) -> Result<(), RepositoryError> {
        // `started_at` is absent from the UPDATE clause on purpose: a rebind is the
        // same session, and plan 2's flush window is anchored on it. Letting a
        // caller rewrite it would move a window that has already been used.
        sqlx::query(
            "INSERT INTO sessions
                (id, user_id, task_id, mode, label, started_at, last_seen_at, last_flush_at, ended_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                task_id      = excluded.task_id,
                mode         = excluded.mode,
                label        = excluded.label,
                last_seen_at = excluded.last_seen_at",
        )
        .bind(&session.id)
        .bind(session.user_id.to_string())
        .bind(session.task_id.map(|t| t.to_string()))
        .bind(session_mode_to_str(session.mode))
        .bind(session.label.as_deref())
        .bind(session.started_at.to_rfc3339())
        .bind(session.last_seen_at.to_rfc3339())
        .bind(session.last_flush_at.map(|d| d.to_rfc3339()))
        .bind(session.ended_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn list_open(&self, user_id: UserId) -> Result<Vec<Session>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, user_id, task_id, mode, label, started_at, last_seen_at,
                    last_flush_at, ended_at
             FROM sessions
             WHERE user_id = ? AND ended_at IS NULL
             ORDER BY last_seen_at DESC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(row_to_session).collect()
    }

    async fn touch(
        &self,
        id: &str,
        user_id: UserId,
        at: DateTime<Utc>,
    ) -> Result<bool, RepositoryError> {
        let result = sqlx::query(
            "UPDATE sessions SET last_seen_at = ?
             WHERE id = ? AND user_id = ? AND ended_at IS NULL",
        )
        .bind(at.to_rfc3339())
        .bind(id)
        .bind(user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn set_last_flush(
        &self,
        id: &str,
        user_id: UserId,
        at: DateTime<Utc>,
    ) -> Result<bool, RepositoryError> {
        let result = sqlx::query(
            "UPDATE sessions SET last_flush_at = ? WHERE id = ? AND user_id = ?",
        )
        .bind(at.to_rfc3339())
        .bind(id)
        .bind(user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn end(
        &self,
        id: &str,
        user_id: UserId,
        at: DateTime<Utc>,
    ) -> Result<bool, RepositoryError> {
        // `ended_at IS NULL` in the WHERE is what makes this idempotent: the first
        // close wins, because that is when the work actually stopped.
        let result = sqlx::query(
            "UPDATE sessions SET ended_at = ?
             WHERE id = ? AND user_id = ? AND ended_at IS NULL",
        )
        .bind(at.to_rfc3339())
        .bind(id)
        .bind(user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }
}
```

Register it in `backend/crates/infrastructure/src/database/mod.rs`:

```rust
pub mod session_repo;
pub use session_repo::SqliteSessionRepository;
```

- [ ] **Step 8: Run to verify they pass**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p infrastructure session_repo
```

Expected: PASS (7 tests).

- [ ] **Step 9: Persist the new slot and entry columns**

`activity_repo.rs` and `worklog_repo.rs` currently ignore the three new columns. Write a test
for each round-trip first, appended to their existing `mod tests`:

```rust
    // activity_repo.rs
    #[tokio::test]
    async fn a_slot_round_trips_its_provenance_and_author() {
        let pool = setup().await;
        let repo = SqliteActivitySlotRepository::new(pool);
        let date = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        let start = DateTime::parse_from_rfc3339("2026-08-04T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let slot = ActivitySlot::from_worklog(
            test_user_id(),
            existing_task_id(),
            Some("sess-1".into()),
            start,
            start + chrono::Duration::hours(2),
            HalfDay::Morning,
            date,
            start,
        );

        repo.save(&slot).await.unwrap();
        let found = repo.find_by_id(slot.id).await.unwrap().unwrap();

        assert_eq!(found.source, SlotSource::Worklog);
        assert_eq!(found.session_id.as_deref(), Some("sess-1"));
    }

    #[tokio::test]
    async fn a_slot_written_before_014_reads_as_manual() {
        // The NULL the migration leaves behind must be the protected value.
        let pool = setup().await;
        sqlx::query(
            "INSERT INTO activity_slots (id, user_id, task_id, start_time, end_time, half_day, date, created_at)
             VALUES ('legacy-1', ?, ?, '2026-08-04T09:00:00+00:00', '2026-08-04T11:00:00+00:00',
                     'morning', '2026-08-04', '2026-08-04T11:00:00+00:00')",
        )
        .bind(test_user_id().to_string())
        .bind(existing_task_id().to_string())
        .execute(&pool)
        .await
        .unwrap();
        let repo = SqliteActivitySlotRepository::new(pool);

        // Read it back through the date query: the row id is not a UUID, which is
        // exactly what a pre-014 row can look like.
        let slots = repo
            .find_by_user_and_date(test_user_id(), NaiveDate::from_ymd_opt(2026, 8, 4).unwrap())
            .await
            .unwrap();

        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].source, SlotSource::Manual);
        assert!(slots[0].session_id.is_none());
    }
```

If `activity_repo.rs`'s existing test module has no `existing_task_id` helper, or its `setup()`
does not seed a task, add both — a slot's `task_id` carries a foreign key.

Then update the SQL:

- every `INSERT INTO activity_slots` gains `session_id, source` with
  `.bind(slot.session_id.as_deref())` and `.bind(slot_source_to_str(slot.source))`;
- every `UPDATE activity_slots` that rewrites the row (not the targeted `end_time` updates)
  carries the same two columns;
- every `SELECT` adds `session_id, source`, and the row mapper fills
  `session_id: Row::get(row, "session_id")` and
  `source: slot_source_from_str(Row::get::<Option<String>, _>(row, "source").as_deref())`;
- `worklog_repo.rs` does the same for `session_id` only:
  `.bind(entry.session_id.as_deref())` on insert, `Row::get(row, "session_id")` on read. Its
  `UPDATE` must not clear it — an edit of the body does not change who wrote the entry.

- [ ] **Step 10: Run the infrastructure suite**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p infrastructure
```

Expected: PASS, including the ~40 pre-existing `worklog_repo` tests.

- [ ] **Step 11: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/infrastructure/src/database/
git commit -m "Persist sessions, slot provenance and entry authorship in SQLite"
```

---

## Task 7: The one-shot provenance classification

**Files:**
- Create: `backend/crates/application/src/use_cases/slot_classification.rs`
- Modify: `backend/crates/application/src/repositories/activity_slot_repository.rs` (wherever
  `ActivitySlotRepository` is declared — find it with `cargo doc` or by grepping the
  `repositories` module)
- Modify: `backend/crates/infrastructure/src/database/activity_repo.rs`
- Modify: `backend/crates/application/src/use_cases/mod.rs`
- Modify: `backend/crates/api/src/main.rs`

**Interfaces:**
- Consumes: `ActivitySlotRepository`, `WorklogRepository`, `ConfigRepository`,
  `worklog::user_timezone`, `domain::rules::worklog_time::{derive_time_blocks, MIN_BLOCK_MINUTES}`.
- Produces:
  - `ActivitySlotRepository::set_source(&self, ids: &[ActivitySlotId], source: SlotSource) -> Result<u64, RepositoryError>` (loud default)
  - `pub struct ClassificationOutcome { pub worklog: u32, pub manual: u32, pub skipped: bool }`
  - `classify_slot_sources(activity_repo, worklog_repo, config_repo, user_id, from, to, now) -> Result<ClassificationOutcome, AppError>`
  - config key `aplan.slot_source_classified`

**Why this task exists:** migration 014 leaves every pre-existing slot's `source` NULL, and a
NULL reads as `Manual`. Left that way, the very first rebuild after the migration would find a
half-day already carrying an unclassified flush-derived slot, refuse to replace it, and write a
second one beside it — the same morning billed twice. Blanket-marking those rows `Worklog`
instead would let a rebuild delete a slot the user made by hand.

> **CORRECTED 2026-08-04, mid-execution.** The rule below and the code in Steps 2-4 test the
> wrong invariant. Comparing a slot's span against today's `derive_time_blocks` output tests how
> entries are *grouped* into blocks, and that grouping changed: the 45-minute gap rule landed in
> `abda52a` today, and the flush previously wrote incrementally against a watermark. Measured on
> the real database, the span rule classified 12 of 52 candidates.
>
> The invariant that held across the change is that a slot's boundaries **are** entry timestamps.
> A closed slot with a task is `Worklog` iff some entry of that task has
> `logged_at == start_time`, **and** some entry has `logged_at == end_time` **or**
> `end_time == start_time + MIN_BLOCK_MINUTES` (which is **1** minute, not 5). Everything else is
> `Manual`. Measured: 52/52 match the first condition, 42 the second's first branch, 10 the
> second's other branch, 0 unexplained, 0 with the round-minute start a hand-made slot would
> carry. The rule compares exact UTC instants, so it needs **no timezone** — drop the
> `user_timezone` plumbing from this path entirely rather than leaving an unused read.
>
> Expected verification result on a copy of the real database: `worklog = 52, manual = 94`.

- [ ] **Step 1: Add `set_source` to the repository trait**

In the file declaring `ActivitySlotRepository`:

```rust
    /// Stamp `source` on the given slots. Returns how many rows moved.
    ///
    /// Loud default, like the rest of the added trait methods in this crate: a double
    /// that silently reported `0` would make the classification pass look finished
    /// while every row stayed NULL — and a NULL row is one a rebuild will not touch,
    /// so the failure would surface weeks later as a double-counted morning.
    async fn set_source(
        &self,
        _ids: &[ActivitySlotId],
        _source: SlotSource,
    ) -> Result<u64, RepositoryError> {
        Err(RepositoryError::Database(
            "set_source is not implemented by this repository".into(),
        ))
    }
```

Implement it in `SqliteActivitySlotRepository`:

```rust
    async fn set_source(
        &self,
        ids: &[ActivitySlotId],
        source: SlotSource,
    ) -> Result<u64, RepositoryError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders = std::iter::repeat("?")
            .take(ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("UPDATE activity_slots SET source = ? WHERE id IN ({placeholders})");
        let mut query = sqlx::query(&sql).bind(slot_source_to_str(source));
        for id in ids {
            query = query.bind(id.to_string());
        }
        let result = query
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(result.rows_affected())
    }
```

- [ ] **Step 2: Write the failing use-case tests**

Create `backend/crates/application/src/use_cases/slot_classification.rs` with the test module
first. It reuses the fake repositories that `use_cases/worklog.rs` already defines for its own
tests — copy `FakeActivityRepo`, the fake worklog repo and the fake config repo from
`worklog.rs`'s `mod tests` into this module, adding a `sources: Mutex<HashMap<ActivitySlotId,
SlotSource>>` recorder and a `set_source` implementation to the activity fake:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // ... the three fakes, copied from use_cases/worklog.rs's test module and
    // extended with `set_source` ...

    fn t(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, h, m, 0).unwrap()
    }

    #[tokio::test]
    async fn a_slot_whose_span_matches_a_derived_block_is_classified_worklog() {
        // Two entries at 09:00 and 11:00 Paris time produce one morning block
        // spanning exactly those two instants. A slot with that span is the flush's.
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(9, 0)]).await;
        let slot = closed_slot(t(7, 0), t(9, 0));
        activity.save(&slot).await.unwrap();

        let outcome = classify_slot_sources(
            &activity, &worklog, &config, user_id(),
            date(2026, 8, 1), date(2026, 8, 31), t(12, 0),
        )
        .await
        .unwrap();

        assert_eq!(outcome.worklog, 1);
        assert_eq!(outcome.manual, 0);
        assert_eq!(activity.recorded_source(slot.id), Some(SlotSource::Worklog));
    }

    #[tokio::test]
    async fn a_hand_made_slot_is_classified_manual() {
        // Same day, same task, but a span no pair of entries accounts for.
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(9, 0)]).await;
        let slot = closed_slot(t(13, 0), t(15, 30));
        activity.save(&slot).await.unwrap();

        let outcome = classify_slot_sources(
            &activity, &worklog, &config, user_id(),
            date(2026, 8, 1), date(2026, 8, 31), t(16, 0),
        )
        .await
        .unwrap();

        assert_eq!(outcome.worklog, 0);
        assert_eq!(outcome.manual, 1);
        assert_eq!(activity.recorded_source(slot.id), Some(SlotSource::Manual));
    }

    #[tokio::test]
    async fn a_single_entry_block_matches_its_minimum_duration() {
        // One entry yields a zero-length block, which the flush persists as
        // MIN_BLOCK_MINUTES. The comparison has to apply the same rule or every
        // single-entry slot in history reads as hand-made.
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0)]).await;
        let slot = closed_slot(t(7, 0), t(7, 0) + Duration::minutes(MIN_BLOCK_MINUTES));
        activity.save(&slot).await.unwrap();

        let outcome = classify_slot_sources(
            &activity, &worklog, &config, user_id(),
            date(2026, 8, 1), date(2026, 8, 31), t(12, 0),
        )
        .await
        .unwrap();

        assert_eq!(outcome.worklog, 1);
    }

    #[tokio::test]
    async fn a_slot_with_no_task_is_manual_without_consulting_any_entry() {
        let (activity, worklog, config) = fakes_with_entries(&[]).await;
        let mut slot = closed_slot(t(7, 0), t(9, 0));
        slot.task_id = None;
        activity.save(&slot).await.unwrap();

        let outcome = classify_slot_sources(
            &activity, &worklog, &config, user_id(),
            date(2026, 8, 1), date(2026, 8, 31), t(12, 0),
        )
        .await
        .unwrap();

        assert_eq!(outcome.manual, 1);
    }

    #[tokio::test]
    async fn the_pass_is_skipped_once_the_guard_key_is_set() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(9, 0)]).await;
        activity.save(&closed_slot(t(7, 0), t(9, 0))).await.unwrap();

        let first = classify_slot_sources(
            &activity, &worklog, &config, user_id(),
            date(2026, 8, 1), date(2026, 8, 31), t(12, 0),
        )
        .await
        .unwrap();
        assert!(!first.skipped);

        let second = classify_slot_sources(
            &activity, &worklog, &config, user_id(),
            date(2026, 8, 1), date(2026, 8, 31), t(13, 0),
        )
        .await
        .unwrap();

        assert!(second.skipped, "a restart must not re-classify");
        assert_eq!(second.worklog, 0);
        assert_eq!(second.manual, 0);
    }
}
```

Write the `fakes_with_entries`, `closed_slot`, `date` and `user_id` helpers to match — the
first returns the three fakes with the given `logged_at` instants stored as entries on one
task, `closed_slot` builds an `ActivitySlot` on that task with `source: SlotSource::Manual`
(the NULL reading) via the struct literal.

- [ ] **Step 3: Run to verify they fail**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p application slot_classification
```

Expected: FAIL to compile — `cannot find function classify_slot_sources`.

- [ ] **Step 4: Write the implementation**

Prepend to `backend/crates/application/src/use_cases/slot_classification.rs`:

```rust
use std::collections::HashMap;

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use domain::rules::worklog_time::{derive_time_blocks, MIN_BLOCK_MINUTES};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::{
    ActivitySlotRepository, ConfigRepository, WorklogFilter, WorklogRepository,
    WORKLOG_FILTER_MAX_LIMIT,
};
use crate::use_cases::configuration;
use crate::use_cases::worklog::user_timezone;

/// Set once the pass has run, so a restart does not redo it.
pub const CLASSIFIED_KEY: &str = "aplan.slot_source_classified";

pub struct ClassificationOutcome {
    pub worklog: u32,
    pub manual: u32,
    /// True when the guard key was already set and nothing was read or written.
    pub skipped: bool,
}

/// Give every pre-014 slot the provenance the data says it has.
///
/// A closed slot came from a flush **iff its span is one of the spans
/// [`derive_time_blocks`] yields from its own task's entries on its own local day** —
/// that function is the only thing that ever wrote those spans, and the flush copies
/// an entry's `logged_at` verbatim into the slot, so the comparison is exact equality
/// rather than a tolerance.
///
/// Everything else is `Manual`: an open slot (a running timer), a slot with no task,
/// a span no entry accounts for. Erring toward `Manual` errs toward not rebuilding,
/// which loses no time — the opposite error deletes hours.
pub async fn classify_slot_sources(
    activity_repo: &dyn ActivitySlotRepository,
    worklog_repo: &dyn WorklogRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    from: NaiveDate,
    to: NaiveDate,
    now: DateTime<Utc>,
) -> Result<ClassificationOutcome, AppError> {
    if config_repo.get(user_id, CLASSIFIED_KEY).await?.is_some() {
        return Ok(ClassificationOutcome {
            worklog: 0,
            manual: 0,
            skipped: true,
        });
    }

    let tz = user_timezone(config_repo, user_id).await?;
    let slots = activity_repo
        .find_by_user_and_date_range(user_id, from, to)
        .await?;

    // One entry read per (task, local day) rather than per slot: a day with six
    // slots on one task would otherwise re-derive the same blocks six times.
    let mut spans_cache: HashMap<(TaskId, NaiveDate), Vec<(DateTime<Utc>, DateTime<Utc>)>> =
        HashMap::new();
    let mut worklog_ids: Vec<ActivitySlotId> = Vec::new();
    let mut manual_ids: Vec<ActivitySlotId> = Vec::new();

    for slot in &slots {
        let (task_id, end_time) = match (slot.task_id, slot.end_time) {
            (Some(task_id), Some(end_time)) => (task_id, end_time),
            _ => {
                manual_ids.push(slot.id);
                continue;
            }
        };

        let key = (task_id, slot.date);
        if !spans_cache.contains_key(&key) {
            let spans = flush_spans(worklog_repo, user_id, task_id, slot.date, tz).await?;
            spans_cache.insert(key, spans);
        }
        let spans = &spans_cache[&key];

        if spans.contains(&(slot.start_time, end_time)) {
            worklog_ids.push(slot.id);
        } else {
            manual_ids.push(slot.id);
        }
    }

    activity_repo
        .set_source(&worklog_ids, SlotSource::Worklog)
        .await?;
    activity_repo
        .set_source(&manual_ids, SlotSource::Manual)
        .await?;

    configuration::set_config(config_repo, user_id, CLASSIFIED_KEY, &now.to_rfc3339()).await?;

    Ok(ClassificationOutcome {
        worklog: worklog_ids.len() as u32,
        manual: manual_ids.len() as u32,
        skipped: false,
    })
}

/// The spans a flush would have written for `task_id` on the local day `date`.
async fn flush_spans(
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    task_id: TaskId,
    date: NaiveDate,
    tz: chrono_tz::Tz,
) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>, AppError> {
    let filter = WorklogFilter {
        task_ids: Some(vec![task_id]),
        from: None,
        to: None,
        limit: WORKLOG_FILTER_MAX_LIMIT,
        offset: 0,
    };
    let entries = worklog_repo.list(user_id, &filter).await?;

    let mut local_to_utc: HashMap<chrono::NaiveDateTime, DateTime<Utc>> = HashMap::new();
    let mut local_times = Vec::new();
    for entry in &entries {
        let local = tz
            .from_utc_datetime(&entry.logged_at.naive_utc())
            .naive_local();
        if local.date() != date {
            continue;
        }
        local_to_utc.insert(local, entry.logged_at);
        local_times.push(local);
    }

    Ok(derive_time_blocks(&local_times)
        .into_iter()
        .filter_map(|block| {
            let start = *local_to_utc.get(&block.start)?;
            let mut end = *local_to_utc.get(&block.end)?;
            if end <= start {
                // The same rule `materialize_worklog_time` applies to a
                // single-timestamp block. Omitting it here would read every
                // one-entry slot in history as hand-made.
                end = start + Duration::minutes(MIN_BLOCK_MINUTES);
            }
            Some((start, end))
        })
        .collect())
}
```

`configuration::set_config` is `application::use_cases::configuration::set_config`, already
declared in `use_cases/mod.rs:9` and used the same way at `api/src/graphql/mutation.rs:220`.

Add to `backend/crates/application/src/use_cases/mod.rs`:

```rust
pub mod slot_classification;
```

- [ ] **Step 5: Run to verify they pass**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p application slot_classification
```

Expected: PASS (5 tests).

- [ ] **Step 6: Run it at startup**

In `backend/crates/api/src/main.rs`, **below the `ExportSchema` early return** — corrected
2026-08-04 by human ruling during task 7's review. The original instruction ("before the schema
is built") put a one-shot irreversible write ahead of that return, so
`cargo run -p api -- export-schema`, the documented way to regenerate
`crates/cli/graphql/schema.graphql`, would classify the real database with its log line swallowed
by the shell redirect. A codegen command must not write history. Placing it after the return costs
two `Arc::clone`s where `SchemaDeps` is built, and nothing rebuilds a half-day until a request
arrives:

```rust
    // Migration 014 leaves `activity_slots.source` NULL. Classify those rows once,
    // from the data itself, before anything can rebuild a half-day: a NULL is read
    // as `Manual`, so an unclassified flush-derived slot would survive a rebuild and
    // the same morning would be counted twice.
    match application::use_cases::slot_classification::classify_slot_sources(
        activity_repo.as_ref(),
        worklog_repo.as_ref(),
        config_repo.as_ref(),
        default_user_id,
        chrono::NaiveDate::from_ymd_opt(2020, 1, 1).expect("static date"),
        chrono::Utc::now().date_naive(),
        chrono::Utc::now(),
    )
    .await
    {
        Ok(outcome) if outcome.skipped => {
            tracing::debug!("slot provenance already classified");
        }
        Ok(outcome) => tracing::info!(
            worklog = outcome.worklog,
            manual = outcome.manual,
            "classified pre-014 activity slot provenance"
        ),
        // A failure here must not stop the server: every unclassified row reads as
        // `Manual`, which is the conservative value, and the pass retries on the
        // next boot because the guard key was never written.
        Err(e) => tracing::error!("slot provenance classification failed: {e}"),
    }
```

`default_user_id` is derived a few lines further down in `main.rs` — move its `let` above this
block if needed, or reuse `crate::state::DEFAULT_USER_ID_STR` the same way `build_schema` does.

- [ ] **Step 7: Verify against the real database, with a backup first**

```bash
cd ~/appfactory/aggregated_plan/backend
cp aggregated_plan.db aggregated_plan.db.bak-$(date +%Y%m%d)-pre014
cargo run -p api 2>&1 | grep -i 'classified' | head -3
```

Expected: one `classified pre-014 activity slot provenance` line with non-zero counts. Then
check the split looks sane — the great majority of task-attributed closed slots should be
`worklog`:

```bash
sqlite3 aggregated_plan.db \
  "SELECT source, COUNT(*) FROM activity_slots GROUP BY source;"
```

If `manual` dominates, stop and report it before continuing: it means the comparison is off
(most likely a timezone reading), not that the history is hand-made. Restore the `.bak`, clear
`aplan.slot_source_classified` from `configuration`, and re-run after fixing.

- [ ] **Step 8: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/application backend/crates/infrastructure backend/crates/api/src/main.rs
git commit -m "Classify pre-014 activity slot provenance from the entries themselves"
```

---

## Task 8: GraphQL surface

**Files:**
- Create: `backend/crates/api/src/graphql/types/session.rs`
- Modify: `backend/crates/api/src/graphql/types/mod.rs`, `types/enums.rs`, `types/activity.rs`
- Modify: `backend/crates/api/src/graphql/{schema,query,mutation}.rs`
- Modify: `backend/crates/api/src/main.rs`
- Modify: `backend/crates/api/src/graphql/tests.rs`
- Modify: `backend/crates/cli/graphql/schema.graphql` (regenerated)

**Interfaces:**
- Consumes: everything from Tasks 2-6.
- Produces:
  - `ClaudeSessionGql(pub Session)` with fields `id`, `taskId`, `task`, `mode`, `label`, `startedAt`, `lastSeenAt`, `lastFlushAt`, `endedAt`
    — **not** `SessionGql`: `query.rs` already exposes `session: SessionGql!` for Microsoft OAuth
    status, consumed by `frontend/src/hooks/use-session.ts` and gating the whole UI through
    `AuthGate.tsx`. Renaming that field would lock the user out of their own cockpit, and frontend
    work is a non-goal of this plan, so the new surface yields instead. Corrected 2026-08-04 during
    task 8, after a subagent's filtered `grep` wrongly reported the old field unused.
  - `SessionModeGql`, `SlotSourceGql` in `types/enums.rs`
  - query `claudeSession(id: String!): ClaudeSessionGql`, query `openClaudeSessions: [ClaudeSessionGql!]!`
  - mutations `bindSession(sessionId: String!, taskId: ID!, label: String): BindSessionResult`,
    `setSessionMode(sessionId: String!, mode: SessionModeGql!, label: String): Session!`,
    `endSession(sessionId: String!): Session`
  - `BindSessionResult { session: Session!, previousTaskId: ID }`
  - `addWorklogEntry` gains `sessionId: String` (optional, additive)
  - `ActivitySlot` gains `sessionId: String` and `source: SlotSourceGql`

- [ ] **Step 1: Write the failing resolver tests**

Append to `backend/crates/api/src/graphql/tests.rs`, following the shape of the existing tests
in that file (they build a schema over in-memory SQLite repositories — reuse the same helper):

```rust
#[tokio::test]
async fn bind_session_then_read_it_back() {
    let (schema, task_id) = schema_with_one_task().await;

    let bind = schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_id}", label: "/tmp/x") {{
                 session {{ id mode taskId label }} previousTaskId }} }}"#
        ))
        .await;
    assert!(bind.errors.is_empty(), "{:?}", bind.errors);
    let data = bind.data.into_json().unwrap();
    assert_eq!(data["bindSession"]["session"]["mode"], "TRACKING");
    assert_eq!(data["bindSession"]["session"]["taskId"], task_id.to_string());
    assert!(data["bindSession"]["previousTaskId"].is_null());

    let read = schema
        .execute(r#"{ session(id: "s1") { id mode label } }"#)
        .await;
    assert!(read.errors.is_empty(), "{:?}", read.errors);
    assert_eq!(read.data.into_json().unwrap()["session"]["label"], "/tmp/x");
}

#[tokio::test]
async fn set_session_mode_off_clears_the_task() {
    let (schema, task_id) = schema_with_one_task().await;
    schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_id}") {{ session {{ id }} }} }}"#
        ))
        .await;

    let off = schema
        .execute(r#"mutation { setSessionMode(sessionId: "s1", mode: OFF) { mode taskId } }"#)
        .await;

    assert!(off.errors.is_empty(), "{:?}", off.errors);
    let data = off.data.into_json().unwrap();
    assert_eq!(data["setSessionMode"]["mode"], "OFF");
    assert!(data["setSessionMode"]["taskId"].is_null());
}

#[tokio::test]
async fn open_sessions_excludes_an_ended_one() {
    let (schema, task_id) = schema_with_one_task().await;
    for id in ["s1", "s2"] {
        schema
            .execute(format!(
                r#"mutation {{ bindSession(sessionId: "{id}", taskId: "{task_id}") {{ session {{ id }} }} }}"#
            ))
            .await;
    }
    schema
        .execute(r#"mutation { endSession(sessionId: "s2") { id endedAt } }"#)
        .await;

    let open = schema.execute(r#"{ openSessions { id } }"#).await;

    assert!(open.errors.is_empty(), "{:?}", open.errors);
    let list = open.data.into_json().unwrap()["openSessions"].clone();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], "s1");
}

#[tokio::test]
async fn an_entry_carries_the_session_that_wrote_it() {
    let (schema, task_id) = schema_with_one_task().await;
    schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_id}") {{ session {{ id }} }} }}"#
        ))
        .await;

    let added = schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "fait", sessionId: "s1") {{ id sessionId }} }}"#
        ))
        .await;

    assert!(added.errors.is_empty(), "{:?}", added.errors);
    assert_eq!(
        added.data.into_json().unwrap()["addWorklogEntry"]["sessionId"],
        "s1"
    );
}

#[tokio::test]
async fn an_entry_without_a_session_is_the_humans() {
    let (schema, task_id) = schema_with_one_task().await;

    let added = schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "fait") {{ sessionId }} }}"#
        ))
        .await;

    assert!(added.errors.is_empty(), "{:?}", added.errors);
    assert!(added.data.into_json().unwrap()["addWorklogEntry"]["sessionId"].is_null());
}
```

If `tests.rs` has no `schema_with_one_task` helper, write one modelled on the file's existing
setup function, returning `(AppSchema, Uuid)` and registering `session_repo` in the deps.

- [ ] **Step 2: Run to verify they fail**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p api session
```

Expected: FAIL — `Unknown field "bindSession"`.

- [ ] **Step 3: Write the GraphQL types**

Create `backend/crates/api/src/graphql/types/session.rs`:

```rust
use std::sync::Arc;

use async_graphql::{Context, Object, SimpleObject, ID};
use chrono::{DateTime, Utc};

use application::repositories::TaskRepository;
use domain::types::Session;

use super::enums::SessionModeGql;

/// GraphQL wrapper for the domain Session entity.
pub struct SessionGql(pub Session);

#[Object]
impl SessionGql {
    /// The Claude Code session id. A `String`, not an `ID`: it is minted by another
    /// program and is never a row id of ours to resolve.
    async fn id(&self) -> String {
        self.0.id.clone()
    }

    async fn task_id(&self) -> Option<ID> {
        self.0.task_id.map(|t| ID(t.to_string()))
    }

    async fn task(&self, ctx: &Context<'_>) -> Option<SessionTaskSummaryGql> {
        let task_id = self.0.task_id?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>().ok()?;
        let task = task_repo.find_by_id(task_id).await.ok()??;
        Some(SessionTaskSummaryGql {
            id: ID(task.id.to_string()),
            title: task.title,
        })
    }

    async fn mode(&self) -> SessionModeGql {
        self.0.mode.into()
    }

    async fn label(&self) -> Option<String> {
        self.0.label.clone()
    }

    async fn started_at(&self) -> DateTime<Utc> {
        self.0.started_at
    }

    async fn last_seen_at(&self) -> DateTime<Utc> {
        self.0.last_seen_at
    }

    async fn last_flush_at(&self) -> Option<DateTime<Utc>> {
        self.0.last_flush_at
    }

    async fn ended_at(&self) -> Option<DateTime<Utc>> {
        self.0.ended_at
    }
}

#[derive(SimpleObject)]
pub struct SessionTaskSummaryGql {
    pub id: ID,
    pub title: String,
}

/// A bind, and the task the session was on before it — which the caller flushes.
#[derive(SimpleObject)]
pub struct BindSessionResultGql {
    pub session: SessionGql,
    pub previous_task_id: Option<ID>,
}
```

`SimpleObject` cannot wrap a manual `#[Object]` field — if the compiler refuses
`BindSessionResultGql`, give it an `#[Object]` impl with two async fields instead.

Add to `types/enums.rs`, mirroring `HalfDayGql`:

```rust
#[derive(async_graphql::Enum, Copy, Clone, Eq, PartialEq)]
pub enum SessionModeGql {
    Tracking,
    Off,
}

impl From<domain::types::SessionMode> for SessionModeGql {
    fn from(m: domain::types::SessionMode) -> Self {
        match m {
            domain::types::SessionMode::Tracking => SessionModeGql::Tracking,
            domain::types::SessionMode::Off => SessionModeGql::Off,
        }
    }
}

impl From<SessionModeGql> for domain::types::SessionMode {
    fn from(m: SessionModeGql) -> Self {
        match m {
            SessionModeGql::Tracking => domain::types::SessionMode::Tracking,
            SessionModeGql::Off => domain::types::SessionMode::Off,
        }
    }
}

#[derive(async_graphql::Enum, Copy, Clone, Eq, PartialEq)]
pub enum SlotSourceGql {
    Worklog,
    Manual,
}

impl From<domain::types::SlotSource> for SlotSourceGql {
    fn from(s: domain::types::SlotSource) -> Self {
        match s {
            domain::types::SlotSource::Worklog => SlotSourceGql::Worklog,
            domain::types::SlotSource::Manual => SlotSourceGql::Manual,
        }
    }
}
```

Match the exact derive attributes the neighbouring enums in that file use — if they write
`#[derive(Enum, Copy, Clone, Eq, PartialEq)]` with `Enum` imported, do the same.

Add two fields to `ActivitySlotGql` in `types/activity.rs`:

```rust
    /// Which session's work this slot projects. Null is the human.
    async fn session_id(&self) -> Option<String> {
        self.0.session_id.clone()
    }

    /// Whether the worklog projection owns this slot.
    async fn source(&self) -> super::enums::SlotSourceGql {
        self.0.source.into()
    }
```

Export the new module from `types/mod.rs`.

- [ ] **Step 4: Write the resolvers**

In `query.rs`:

```rust
    /// One session by its Claude Code id.
    async fn session(&self, ctx: &Context<'_>, id: String) -> Result<Option<SessionGql>> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn SessionRepository>>()?;
        Ok(repo
            .find_by_id(&id, user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .map(SessionGql))
    }

    /// Every session still open, most recently seen first.
    async fn open_sessions(&self, ctx: &Context<'_>) -> Result<Vec<SessionGql>> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn SessionRepository>>()?;
        let sessions = session_tracking::list_open_sessions(repo.as_ref(), user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(sessions.into_iter().map(SessionGql).collect())
    }
```

In `mutation.rs`:

```rust
    /// Point a session at a task. Returns the task it was on before, if any, so the
    /// caller can flush it.
    async fn bind_session(
        &self,
        ctx: &Context<'_>,
        session_id: String,
        task_id: ID,
        label: Option<String>,
    ) -> Result<BindSessionResultGql> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn SessionRepository>>()?;
        let tid = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {e}")))?;

        let outcome = session_tracking::bind_session(
            repo.as_ref(),
            user_id,
            &session_id,
            tid,
            label,
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(BindSessionResultGql {
            previous_task_id: outcome.previous_task.map(|t| ID(t.to_string())),
            session: SessionGql(outcome.session),
        })
    }

    /// Record what a session was told to do. `OFF` also clears its task.
    async fn set_session_mode(
        &self,
        ctx: &Context<'_>,
        session_id: String,
        mode: SessionModeGql,
        label: Option<String>,
    ) -> Result<SessionGql> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn SessionRepository>>()?;
        let session = session_tracking::set_session_mode(
            repo.as_ref(),
            user_id,
            &session_id,
            mode.into(),
            label,
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(SessionGql(session))
    }

    /// Close a session. Null when there was nothing open to close.
    async fn end_session(
        &self,
        ctx: &Context<'_>,
        session_id: String,
    ) -> Result<Option<SessionGql>> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn SessionRepository>>()?;
        Ok(session_tracking::end_session(
            repo.as_ref(),
            user_id,
            &session_id,
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?
        .map(SessionGql))
    }
```

Extend `add_worklog_entry` (`mutation.rs:124`) with a trailing `session_id: Option<String>`
parameter and attribute the entry with it. The use case keeps its signature; the mutation
applies the builder:

```rust
        let entry = worklog_uc::add_worklog_entry(repo.as_ref(), user_id, tid, body, logged_at, now)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
```

becomes a two-step: build the entry through the use case, then persist the authorship. Since
`add_worklog_entry` already persists, add the `session_id` parameter to that use case instead
— it is a two-line change (`WorklogEntry::new(...).by_session(session_id)`) with three call
sites, and it keeps a single write. Update the use case's own tests to pass `None`.

Register the repository: add `session_repo: Arc<dyn SessionRepository>` to `SchemaDeps`,
destructure it, `.data(session_repo)` it in `build_schema`, and construct it in `main.rs`:

```rust
    let session_repo: Arc<dyn application::repositories::SessionRepository> =
        Arc::new(SqliteSessionRepository::new(db_pool.clone()));
```

- [ ] **Step 5: Run to verify they pass**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p api
```

Expected: PASS, including every pre-existing api test.

- [ ] **Step 6: Regenerate the CLI's schema copy**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo run -p api -- export-schema > crates/cli/graphql/schema.graphql
git diff --stat crates/cli/graphql/schema.graphql
```

Expected: the diff adds `Session`, `SessionModeGql`, `SlotSourceGql`, `BindSessionResultGql`,
the two queries, the three mutations, and the new arguments. If `export-schema` is not a
subcommand of the api binary, check `crates/cli/README.md:189` for the current incantation.

- [ ] **Step 7: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/api backend/crates/application backend/crates/cli/graphql/schema.graphql
git commit -m "Expose sessions over GraphQL and attribute worklog entries"
```

---

## Task 9: CLI — `--session`, resolution order, `session` commands

**Files:**
- Create: `backend/crates/cli/src/session_cmd.rs`
- Create: `backend/crates/cli/graphql/{session,open_sessions,bind_session,set_session_mode,end_session}.graphql`
- Modify: `backend/crates/cli/graphql/add_worklog_entry.graphql`
- Modify: `backend/crates/cli/src/{cli,main,queries,lookup,commands}.rs`
- Modify: `backend/crates/cli/tests/integration.rs`

**Interfaces:**
- Consumes: the GraphQL surface from Task 8.
- Produces:
  - global flag `--session <ID>` with `env = "CLAUDE_CODE_SESSION_ID"`
  - `aplan sessions`, `aplan session show|bind|off|end`
  - `lookup::resolve_target(client, session: Option<&str>, task: Option<&str>) -> Result<TaskRef, LookupError>`
  - `LookupError::{SessionNotTracked, SessionNoTask, SessionEnded, SessionUnknown}`, all exit 4
    except `SessionUnknown` (exit 2)

- [ ] **Step 1: Write the failing integration tests**

Append to `backend/crates/cli/tests/integration.rs`:

```rust
/// Session-aware flows must never inherit the developer's own session id: the suite
/// runs inside a Claude Code session, where `CLAUDE_CODE_SESSION_ID` is exported into
/// every command. Without this the test exercises a different branch than it claims.
fn aplan_no_session() -> Command {
    let mut cmd = Command::cargo_bin("aplan").unwrap();
    cmd.env_remove("CLAUDE_CODE_SESSION_ID");
    cmd
}

fn session_body(mode: &str, task_id: Option<&str>) -> serde_json::Value {
    json!({
        "data": {
            "session": {
                "id": "s1",
                "taskId": task_id,
                "mode": mode,
                "label": "/home/mbt/appfactory/aggregated_plan",
                "startedAt": "2026-08-04T09:00:00+00:00",
                "lastSeenAt": "2026-08-04T09:00:00+00:00",
                "lastFlushAt": null,
                "endedAt": null
            }
        }
    })
}

#[tokio::test]
async fn log_targets_the_sessions_task_not_the_global_pointer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("Session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_body(
            "TRACKING",
            Some("00000000-0000-0000-0000-000000000001"),
        )))
        .mount(&server)
        .await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("AddWorklogEntry"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "addWorklogEntry": {
                "id": "e1", "taskId": "00000000-0000-0000-0000-000000000001",
                "loggedAt": "2026-08-04T10:00:00+00:00", "sessionId": "s1" } }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan_no_session()
        .args(["--api-url", &url, "--session", "s1", "log", "fait"])
        .assert()
        .success()
        .stdout(predicate::str::contains("worklog entry added"));
}

#[tokio::test]
async fn log_refuses_exit_4_when_the_session_is_not_tracked() {
    // The bug this feature exists to kill: an opted-out session must refuse, not
    // fall back onto the human's pointer.
    let server = mock_graphql(session_body("OFF", None)).await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "--session", "s1", "log", "fait"])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("not tracked"));
}

#[tokio::test]
async fn log_refuses_exit_4_when_the_session_has_no_task() {
    let server = mock_graphql(session_body("TRACKING", None)).await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "--session", "s1", "log", "fait"])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("no task"));
}

#[tokio::test]
async fn an_explicit_task_wins_over_the_session() {
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("AddWorklogEntry"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "addWorklogEntry": {
                "id": "e1", "taskId": "00000000-0000-0000-0000-000000000001",
                "loggedAt": "2026-08-04T10:00:00+00:00", "sessionId": null } }
        })))
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    // No `Session` mock is mounted: resolving one would be a bug, and the missing
    // stub makes that visible instead of silent.
    aplan_no_session()
        .args([
            "--api-url", &url, "--session", "s1", "log", "fait",
            "--task", "00000000-0000-0000-0000-000000000001",
        ])
        .assert()
        .success();
}

#[tokio::test]
async fn without_a_session_the_global_pointer_still_answers() {
    // The human, working by hand. Unchanged behaviour.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "configuration": {
                "aplan.active_task_id": "00000000-0000-0000-0000-000000000001" } }
        })))
        .mount(&server)
        .await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("AddWorklogEntry"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "addWorklogEntry": {
                "id": "e1", "taskId": "00000000-0000-0000-0000-000000000001",
                "loggedAt": "2026-08-04T10:00:00+00:00", "sessionId": null } }
        })))
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "log", "fait"])
        .assert()
        .success();
}

#[tokio::test]
async fn the_session_id_is_picked_up_from_the_environment() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("Session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_body("OFF", None)))
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    // Refusing on an OFF session proves the env var reached the resolver: a CLI
    // that ignored it would have fallen through to the global pointer and asked
    // for `GetConfiguration`, which is not mocked here.
    aplan()
        .env("CLAUDE_CODE_SESSION_ID", "s1")
        .args(["--api-url", &url, "log", "fait"])
        .assert()
        .code(4);
}

#[tokio::test]
async fn sessions_lists_the_open_ones() {
    let server = mock_graphql(json!({
        "data": { "openSessions": [
            { "id": "s1", "taskId": "00000000-0000-0000-0000-000000000001",
              "task": { "id": "00000000-0000-0000-0000-000000000001", "title": "Saft cadrage" },
              "mode": "TRACKING", "label": "/home/mbt/x",
              "startedAt": "2026-08-04T09:00:00+00:00",
              "lastSeenAt": "2026-08-04T10:30:00+00:00",
              "lastFlushAt": null, "endedAt": null }
        ] }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "sessions"])
        .assert()
        .success()
        .stdout(predicate::str::contains("s1"))
        .stdout(predicate::str::contains("Saft cadrage"));
}

#[tokio::test]
async fn session_off_persists_the_decision() {
    let server = mock_graphql(json!({
        "data": { "setSessionMode": {
            "id": "s1", "taskId": null, "mode": "OFF", "label": null,
            "startedAt": "2026-08-04T09:00:00+00:00",
            "lastSeenAt": "2026-08-04T09:00:00+00:00",
            "lastFlushAt": null, "endedAt": null } }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "session", "off", "--session", "s1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not tracking"));
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p cli
```

Expected: FAIL — unrecognized argument `--session`.

- [ ] **Step 3: Add the flag, the operations and the commands**

`cli.rs`, in the `Cli` struct next to `api_url`:

```rust
    /// The Claude Code session this invocation belongs to. Defaults to
    /// `CLAUDE_CODE_SESSION_ID`, which the harness exports into every Bash call, so
    /// a Claude never has to pass it. Absent (a plain terminal), the global pointer
    /// answers instead: that pointer is the human, working by hand.
    #[arg(long, env = "CLAUDE_CODE_SESSION_ID", global = true)]
    pub session: Option<String>,
```

`cli.rs`, two new subcommands:

```rust
    /// List the open Claude sessions and what each one is working on.
    Sessions,
    /// Manage this session's aplan link.
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
```

```rust
#[derive(Subcommand, Debug)]
pub enum SessionAction {
    /// Show the session's link — what the SessionStart hook reads.
    Show,
    /// Link this session to TASK. Does not move the global pointer.
    Bind {
        task: String,
        /// Displayed in `aplan sessions`. Defaults to the working directory.
        #[arg(long)]
        label: Option<String>,
    },
    /// Disable aplan logging for this session, persistently.
    Off,
    /// Close this session.
    End,
}
```

The five `.graphql` operations, mirroring the existing files:

> **CORRECTED 2026-08-04 after task 8.** The field is `claudeSession`, not `session` — that name
> belongs to the Microsoft OAuth status query the frontend's AuthGate depends on. The operation
> names below are also renamed so graphql_client generates `ClaudeSession` / `OpenClaudeSessions`
> rather than a `Session` struct that reads as the wrong thing. Verified against the regenerated
> `graphql/schema.graphql`.

```graphql
# graphql/claude_session.graphql
query ClaudeSession($id: String!) {
  claudeSession(id: $id) {
    id
    taskId
    mode
    label
    startedAt
    lastSeenAt
    lastFlushAt
    endedAt
  }
}
```

```graphql
# graphql/open_claude_sessions.graphql
query OpenClaudeSessions {
  openClaudeSessions {
    id
    taskId
    task { id title }
    mode
    label
    startedAt
    lastSeenAt
    lastFlushAt
    endedAt
  }
}
```

```graphql
# graphql/bind_session.graphql
mutation BindSession($sessionId: String!, $taskId: ID!, $label: String) {
  bindSession(sessionId: $sessionId, taskId: $taskId, label: $label) {
    session { id taskId mode label }
    previousTaskId
  }
}
```

```graphql
# graphql/set_session_mode.graphql
mutation SetSessionMode($sessionId: String!, $mode: SessionModeGql!, $label: String) {
  setSessionMode(sessionId: $sessionId, mode: $mode, label: $label) {
    id
    taskId
    mode
    label
    startedAt
    lastSeenAt
    lastFlushAt
    endedAt
  }
}
```

```graphql
# graphql/end_session.graphql
mutation EndSession($sessionId: String!) {
  endSession(sessionId: $sessionId) { id endedAt }
}
```

Add `sessionId` to `graphql/add_worklog_entry.graphql`:

```graphql
mutation AddWorklogEntry($taskId: ID!, $body: String!, $sessionId: String) {
  addWorklogEntry(taskId: $taskId, body: $body, sessionId: $sessionId) {
    id
    taskId
    loggedAt
    sessionId
  }
}
```

Declare all six in `queries.rs`, following the existing derive blocks verbatim (only
`query_path` and the struct name change).

- [ ] **Step 4: Write the resolution order**

In `lookup.rs`, add the four refusals and the new entry point, keeping `resolve_task` for the
callers that have no session concept:

```rust
    /// The session exists but the user turned logging off for it. A refusal, never a
    /// fallback: falling back onto the global pointer is exactly how a Claude ends up
    /// reporting work on a task the user declined.
    #[error("session {0} is not tracked — aplan logging is off for this session\nhint: `aplan session bind <task>` to start tracking it")]
    SessionNotTracked(String),
    #[error("session {0} has no task bound\nhint: `aplan session bind <task>`")]
    SessionNoTask(String),
    #[error("session {0} has ended")]
    SessionEnded(String),
    #[error("no session {0} is known to aplan")]
    SessionUnknown(String),
```

```rust
impl LookupError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            LookupError::NoCurrentActivity => ExitCode::PreconditionFailed,
            LookupError::NotFound(_) => ExitCode::NotFound,
            LookupError::Ambiguous { .. } => ExitCode::Ambiguous,
            LookupError::Client(_) => ExitCode::Generic,
            // A session that refuses is a precondition the store will not leave,
            // which is what exit 4 means everywhere else in this CLI.
            LookupError::SessionNotTracked(_)
            | LookupError::SessionNoTask(_)
            | LookupError::SessionEnded(_) => ExitCode::PreconditionFailed,
            LookupError::SessionUnknown(_) => ExitCode::NotFound,
        }
    }
}
```

```rust
/// Resolve the task a verb with an implicit target should write to.
///
/// Three levels, in this order:
///   1. `--task` — always wins, and never touches the session.
///   2. the session (`--session`, or `CLAUDE_CODE_SESSION_ID`) — a Claude.
///   3. the global pointer — the human, working by hand.
///
/// Level 2 refuses rather than falling through to level 3. That refusal is the
/// feature: it is what makes "ne pas tracker" hold for a whole session.
///
/// The three-way refusal below mirrors `domain::types::SessionTargetRefusal`. It is
/// restated here rather than shared because this crate deliberately depends on no
/// workspace crate — it talks to the backend over GraphQL like any other client.
pub fn resolve_target(
    client: &Client,
    session: Option<&str>,
    task: Option<&str>,
) -> Result<TaskRef, LookupError> {
    if let Some(token) = task.filter(|t| !t.trim().is_empty()) {
        return resolve_task(client, Some(token));
    }
    match session.filter(|s| !s.trim().is_empty()) {
        Some(id) => resolve_from_session(client, id),
        None => resolve_task(client, None),
    }
}

fn resolve_from_session(client: &Client, id: &str) -> Result<TaskRef, LookupError> {
    use crate::queries::{session, Session as SessionQuery};

    let result = client.run::<SessionQuery>(session::Variables { id: id.to_string() })?;
    let found = result
        .data
        .session
        .ok_or_else(|| LookupError::SessionUnknown(id.to_string()))?;

    if found.ended_at.is_some() {
        return Err(LookupError::SessionEnded(id.to_string()));
    }
    if !matches!(found.mode, session::SessionModeGql::TRACKING) {
        return Err(LookupError::SessionNotTracked(id.to_string()));
    }
    let task_id = found
        .task_id
        .filter(|t| !t.is_empty())
        .ok_or_else(|| LookupError::SessionNoTask(id.to_string()))?;

    hydrate_by_id(client, &task_id)
}
```

The generated enum variant for `TRACKING` may be spelled `Tracking` or carry an `Other(String)`
arm depending on graphql_client's codegen — read the generated name from the compiler error and
match it, keeping the `!matches!(…)` shape so any unknown value refuses rather than logs.

Thread `session` through the commands that take an implicit target — `log`, `note`, `status`,
`done`, `remember` — by adding a `session: Option<&str>` parameter and swapping
`resolve_task(&client, task)` for `resolve_target(&client, session, task)`. `main.rs` passes
`args.session.as_deref()`. For `log`, also pass the id to the mutation:

```rust
    let result = client.run::<AddWorklogEntry>(add_worklog_entry::Variables {
        task_id: target.id.clone(),
        body: joined,
        session_id: session.map(|s| s.to_string()),
    });
```

`aplan start` / `aplan stop` keep operating on the global pointer **in this plan**; plan 3
switches them to bind the session, because that is the change the hooks depend on and it wants
to land with them.

- [ ] **Step 5: Write `session_cmd.rs`**

Create `backend/crates/cli/src/session_cmd.rs` with `sessions(api_url, json)` and
`session(api_url, json, session_id, action)`. Behaviour:

- `Sessions` — one line per open session:
  `● s1  Saft cadrage : Évolution Base de données  (depuis 09:00, vu 10:30)  /home/mbt/x`,
  followed by one display-only line for the global pointer read from `GetConfiguration` +
  `GetTask`: `○ manuel (toi)  <title>`. With `--json`, emit the raw `openSessions` payload plus
  a `manual` key — a session-shaped object is not invented for the human, because the human has
  no session row and never will.
- `Session { action: Show }` — print the session's mode and task, or
  `no session id (pass --session or run inside Claude Code)` and exit 4 when `session_id` is
  `None`.
- `Session { action: Bind { task, label } }` — resolve the task with `resolve_task`, run
  `BindSession` with `label` defaulting to `std::env::current_dir()`, then, if
  `previousTaskId` came back, `flush_task(&client, &previous)` — the same call `aplan start`
  already makes, so time behaviour is unchanged in this plan. Print
  `▶ session s1 → <title>`.
- `Session { action: Off }` — run `SetSessionMode` with `OFF`, print
  `○ session s1: not tracking (aplan logging off for this session)`.
- `Session { action: End }` — run `EndSession`, print `■ session s1 closed`.

Every branch that needs a session id and has none exits 4 with that same message — a command
that silently did nothing is how the original bug stayed invisible.

Declare `mod session_cmd;` in `main.rs` and dispatch:

```rust
        cli::Commands::Sessions => session_cmd::sessions(&args.api_url, args.json),
        cli::Commands::Session { action } => {
            session_cmd::session(&args.api_url, args.json, args.session.as_deref(), &action)
        }
```

- [ ] **Step 6: Add `actor` to `current --json`**

In `commands::current`, add an `actor` field to the `--json` payload: the session id when one
resolved, `"manual"` otherwise. **Additive only** — `aplan-session-end.sh` and
`aplan-session-start.sh` read `.currentActivity.task.id` today, and plan 3 owns rewriting them.
Restructuring the payload here breaks the hooks between two plans.

- [ ] **Step 7: Run the CLI suite**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p cli
```

Expected: PASS, including the pre-existing tests. Then prove the env-var trap is really handled:

```bash
cd ~/appfactory/aggregated_plan/backend
env -u CLAUDE_CODE_SESSION_ID cargo test -p cli
CLAUDE_CODE_SESSION_ID=deadbeef cargo test -p cli
```

Both must give identical results. If they differ, a test is missing its `.env_remove`.

- [ ] **Step 8: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/cli
git commit -m "Resolve the log target from the session, refusing when it is not tracked"
```

---

## Task 10: Documentation and manual verification

**Files:**
- Modify: `SPEC_TECHNIQUE.md`
- Modify: `backend/crates/cli/README.md`

- [ ] **Step 1: Document the model in `SPEC_TECHNIQUE.md` (French)**

Add a section describing: the two natures of actor (global pointer = the human, `sessions` =
the Claudes), the `sessions` table and its columns, the three-level target resolution with the
refusal at level 2, `activity_slots.source` and why a NULL reads as `manual`, the one-shot
classification pass and its guard key `aplan.slot_source_classified`. State explicitly that
the flush is still watermark-based in this plan and that plan 2 replaces it.

Update the table count in the "Database" paragraph: 21 tables becomes 22.

- [ ] **Step 2: Document the new commands in `crates/cli/README.md`**

`aplan sessions`, `aplan session show|bind|off|end`, the global `--session` flag and its env
default, and the exit-4 refusals with their meanings.

- [ ] **Step 3: Verify the whole thing by hand**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test && cargo clippy 2>&1 | tail -5
cargo run -p api &     # leave it running
```

In two separate terminals, with two different fake ids:

```bash
aplan --session sess-A session bind "Saft cadrage"
aplan --session sess-B session bind "Cartier"
aplan --session sess-A log "premier point sur A"
aplan --session sess-B log "premier point sur B"
aplan sessions
aplan --session sess-B session off
aplan --session sess-B log "ne doit pas passer"; echo "exit=$?"
aplan journal
```

Expected: `sessions` shows both, then one; the last `log` prints a "not tracked" message and
`exit=4`; `journal` shows one entry on each task and **the global pointer is wherever it was
before you started** — check with `aplan current`.

- [ ] **Step 4: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add SPEC_TECHNIQUE.md backend/crates/cli/README.md
git commit -m "Document the session model and the new session commands"
```

---

## Self-review notes

**Spec coverage.** §Modèle → Tasks 2, 5. §Schéma → Task 1. §Classification → Task 7.
§Résolution de cible → Tasks 5, 9. §Alignement réattribution → deferred to plan 2, where
`is_rebuildable` starts reading `source`; this plan only makes the column exist and be correct.
§Flush idempotent, §Chevauchement, §Cycle de vie et hooks → plans 2 and 3 by design.
§Nouvelle surface → Tasks 8, 9, minus `aplan session bind` moving the global pointer, which
Task 9 Step 4 deliberately leaves to plan 3.

**Known deviation from the spec.** The spec put the span-comparison test in the domain layer.
It turned out to need no new domain rule — the comparison is `Vec::contains` over spans that
the existing `derive_time_blocks` already produces — so its tests live with the use case that
builds those spans (Task 7). No behaviour differs.

**Deferred on purpose.** `SessionRepository::list_idle_open` (the 12-hour reaper needs it, and
that is plan 3); session attribution on materialized slots (`from_worklog(…, None, …)` in Task
3, filled in by plan 2 when the flush learns which session asked).
