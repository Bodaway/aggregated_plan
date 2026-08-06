# aplan Sessions — Plan 3: hooks, lifecycle, and overlap

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the session machinery actually drive itself — the hooks register a session and persist its "do not track" decision, `start`/`stop` act on the session that is asking, idle sessions are reaped, and time two tasks claim in the same hour becomes visible instead of silently doubling.

**Architecture:** Everything the CLI and backend need already exists and is tested; this plan wires it to the two lifecycle events that were left alone in plans 1 and 2. The SessionStart hook stops re-deriving state from the human's pointer and reads the session's own row instead; the SessionEnd hook stops flushing whatever the human was tracking and flushes the ending session's task. Overlap is a pure read-time computation over closed slots — nothing stored, nothing corrected.

**Tech Stack:** Rust (stable), sqlx 0.8 + SQLite, async-graphql 7, Axum 0.7, clap 4, graphql_client 0.14, wiremock + assert_cmd, bash + jq for the hooks.

**Spec:** `docs/superpowers/specs/2026-08-04-aplan-session-scoped-worklog-design.md`, sections "Lifecycle and hooks" and "Overlap — visible, never corrected". Approved by the user; this plan implements it and adds no design of its own.

**Predecessors:** plan 1 (`891220f..9c8d8c6`) built the socle; plan 2 (`36e9a23..78d860d`) made the flush a rebuild and split the two windows. Both merged on `feat/aplan-sessions-socle`.

## Two halves, and where you may stop

**Tasks 1–6 are the session lifecycle.** They are what make the feature work without the user thinking about it, and task 5 closes the last place the original defect is still live.

**Tasks 7–9 are overlap display.** They share nothing with the lifecycle but the `sessions` table, and they ship independently. If the user wants to stop after task 6, the branch is coherent and complete without them.

Task 10 documents whatever landed.

## The risk that shapes this whole plan

**The two hooks live in `~/.claude/hooks/`, outside the repository and outside git's safety net.** `aplan-session-start.sh` runs at the start of **every** Claude Code session on this machine. A syntax error, a hang, or a `set -e` on a failing `aplan` call there degrades or breaks every new session the user opens, with no `git checkout` to undo it.

Therefore, for tasks 4 and 5, without exception:

1. **Copy the current hook to `<name>.bak-YYYYMMDD-pre-plan3` before the first edit.** Both hooks already carry a `.bak-20260803` from an earlier change, so the convention exists.
2. **Never edit the installed hook in place.** Develop in the session scratchpad, test there, and install only a version that has passed the tests below.
3. **The hook must stay a silent no-op when anything is missing** — no `aplan` on PATH, no `jq`, backend unreachable, malformed stdin. The current hooks already do this (`command -v … || exit 0`, `|| exit 0` on the `aplan current` call); preserve that property exactly. A hook that fails loudly at session start is worse than one that does nothing.
4. **Test by piping payloads to the script directly**, never by opening a new Claude session to see what happens. The payload shapes are given in each task.

## Global Constraints

- **Branch:** continue on `feat/aplan-sessions-socle`. Never commit to `main`.
- Commit messages: plain imperative subject, short body for the *why*. **No `Co-Authored-By` footer, no `Signed-off-by` trailer.** Stage only the files a task names — never `git add -A`. **Never push.**
- **DDD layers are strict.** `domain/` = pure, zero I/O, deps limited to chrono/serde/uuid/thiserror. `application/` = repository traits + use cases, depends on domain only. `infrastructure/` = sqlx. `api/` = Axum + async-graphql. `cli/` depends on **no** workspace crate.
- No `.unwrap()` in production code. Runtime `sqlx::query`, never `sqlx::query!`. `sqlx::Error` → `RepositoryError::Database(e.to_string())`.
- **All local-day reasoning goes through `crate::time`** — plan 2 consolidated `to_local`, `local_day_start` and `local_window` there precisely so a second conversion cannot drift.
- **Green suite is `cargo test --workspace` — no `--exclude` flag.** Baseline at plan 3's start: **1080 passed / 0 failed**. Reattribution must stay **27/27** (`cargo test -p application reattribut`).
- **The env A/B must stay identical:** `env -u CLAUDE_CODE_SESSION_ID cargo test -p cli` and `CLAUDE_CODE_SESSION_ID=deadbeef cargo test -p cli`, both **105 passed / 0 failed**. The harness exports that variable into every Bash call, so a suite sensitive to it passes on your machine and fails in a plain terminal. Capture output to a file before counting.
- **The user's database is LIVE.** `backend/aggregated_plan.db` is at migration 14 with real history (94 `manual` / 56 `worklog` slots), and a systemd user service `aplan-api.service` holds it on port 3001. **Do not write to it, do not restart the service, do not run a server against it.** Tests use `sqlite::memory:`. If a schema regeneration is needed, point `DATABASE_URL` at a scratch file — `create_sqlite_pool` runs migrations *before* the `ExportSchema` early return, so aiming it at the live file would migrate the user's database.
- **Two standing warnings from plan 2, which cost real time there:** eight invented or wrong details came out of that plan's own text, so read the real code and report every substitution rather than trusting a snippet; and five test-harness lies were found, so if a stub or fake cannot distinguish the two outcomes your test must separate, fix it or escalate — never write an assertion it cannot fail. Two are still open in `use_cases/worklog.rs`'s test module: `FakeActivityRepo::update` returns `Ok(())` without mutating, `find_active` always returns `None`.
- Spec maintenance: `SPEC_TECHNIQUE.md` (French) is updated once, by task 10.

---

## File Structure

**Created:**

| File | Responsibility |
|---|---|
| `backend/crates/domain/src/rules/overlap.rs` | the pure overlap rule over closed slots |
| `backend/crates/application/src/use_cases/session_reaper.rs` | find idle open sessions, flush and end each |

**Modified:**

| File | Change |
|---|---|
| `backend/crates/application/src/repositories/session_repository.rs` | `list_idle_open` — the method plan 1 deliberately deferred |
| `backend/crates/infrastructure/src/database/session_repo.rs` | its SQL |
| `backend/crates/application/src/use_cases/session_tracking.rs` | one of the four `SessionRepository` test doubles |
| `backend/crates/api/src/graphql/tests.rs` | the other two test doubles |
| `backend/crates/application/Cargo.toml` | adds the `tracing` dependency the reaper needs |
| `backend/crates/api/src/jobs.rs` | the reaper joins the existing background scheduler |
| `backend/crates/api/src/main.rs` | pass `session_repo` to the job's deps |
| `backend/crates/cli/src/commands.rs` | `start` / `stop` bind the session when one is asking; `journal` prints overlaps |
| `backend/crates/cli/graphql/*.graphql` | overlap fields on the journal query |
| `backend/crates/api/src/graphql/{query,types/activity}.rs` | expose overlap on the journal |
| `~/.claude/hooks/aplan-session-start.sh` | read the session's own row; persist "do not track" |
| `~/.claude/hooks/aplan-session-end.sh` | flush **the session's** task, then end the session |
| `.claude/skills/aplan/SKILL.md` | the session vocabulary and the exit-4 row |
| `SPEC_TECHNIQUE.md` | § 7.3.5 (lifecycle) and § 7.3.6 (overlap) |

---

## Task 1: `list_idle_open` and the reaper use case

**Files:**
- Modify: `backend/crates/application/src/repositories/session_repository.rs`
- Modify: `backend/crates/infrastructure/src/database/session_repo.rs`
- Create: `backend/crates/application/src/use_cases/session_reaper.rs`
- Modify: `backend/crates/application/src/use_cases/mod.rs`
- Modify: `backend/crates/application/src/use_cases/session_tracking.rs` — its test-module `InMemorySessionRepository` is one of the four implementors
- Modify: `backend/crates/api/src/graphql/tests.rs` — the other two implementors live here
- Modify: `backend/crates/application/Cargo.toml` — `application` has **no `tracing` dependency**; the `reap_idle_sessions` body below calls `tracing::warn!`, so add `tracing = { workspace = true }`

> The three entries after `mod.rs` were missing from this list in the plan's first draft, and the commit recipe at the end of this task named only two directories. Following either literally produces a tree that does not compile (`E0046: missing list_idle_open`). Stage what the change actually needs, including `backend/Cargo.lock`.

**Interfaces:**
- Consumes: `SessionRepository::{end, set_last_flush}`, `Session::flush_window_start()`, `materialize_worklog_time(worklog_repo, activity_repo, config_repo, user_id, task_id, from, now)`.
- Produces:
  - `SessionRepository::list_idle_open(&self, user_id: UserId, idle_before: DateTime<Utc>) -> Result<Vec<Session>, RepositoryError>` — open sessions whose `last_seen_at` is older than `idle_before`. **A required method, not a defaulted one.**
  - `pub struct ReapOutcome { pub reaped: u32, pub slots_written: u32 }`
  - `pub async fn reap_idle_sessions(session_repo, worklog_repo, activity_repo, config_repo, user_id, idle_before: DateTime<Utc>, now: DateTime<Utc>) -> Result<ReapOutcome, AppError>`

**Required, not defaulted — and this is deliberate.** `WorklogRepository`, `ActivitySlotRepository` and `MemoryRepository` all give newly-added methods a loud `Err("… is not implemented by this repository")` default, and it would be easy to copy that here. Don't. Those traits are old and have doubles scattered across crates, so a default is the only way to add a method without touching them all; `SessionRepository` is new as of plan 1 and **every one of its methods is required**. Keeping that means the compiler, not a runtime error, tells you about a missing implementation — and a double that returned an empty list would make the reaper look like it had found nothing to reap.

There are exactly **four** implementors; expect to write all four:

| Implementor | File |
|---|---|
| `SqliteSessionRepository` | `infrastructure/src/database/session_repo.rs:59` |
| `InMemorySessionRepository` | `application/src/use_cases/session_tracking.rs:156` (test module) |
| `InMemorySessionRepository` | `api/src/graphql/tests.rs:681` |
| `FailingTouchSessionRepository` | `api/src/graphql/tests.rs:793` |

The two test doubles need a real filter, not a stub — the reaper's use-case tests read through one of them. `FailingTouchSessionRepository` exists to fail `touch`; give it a working `list_idle_open` unless a test needs otherwise.

- [ ] **Step 1: Write the failing repository tests**

Append to `session_repo.rs`'s test module, reusing its `setup()`, `user_id()`, `task_id()` and `t(h)` helpers (read them — plan 1's Task 6 added them and plan 2 found four invented helper names across its own snippets):

```rust
    #[tokio::test]
    async fn list_idle_open_returns_only_stale_open_sessions() {
        let repo = SqliteSessionRepository::new(setup().await);
        // Seen recently — alive.
        repo.upsert(&Session::tracking("fresh".into(), user_id(), task_id(), None, t(16)).unwrap())
            .await
            .unwrap();
        // Seen long ago — idle.
        repo.upsert(&Session::tracking("stale".into(), user_id(), task_id(), None, t(2)).unwrap())
            .await
            .unwrap();
        // Idle but already closed — not ours to reap twice.
        repo.upsert(&Session::tracking("closed".into(), user_id(), task_id(), None, t(2)).unwrap())
            .await
            .unwrap();
        repo.end("closed", user_id(), t(3)).await.unwrap();

        let idle = repo.list_idle_open(user_id(), t(10)).await.unwrap();

        let ids: Vec<&str> = idle.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["stale"]);
    }

    #[tokio::test]
    async fn list_idle_open_is_scoped_to_the_user() {
        let repo = SqliteSessionRepository::new(setup().await);
        repo.upsert(&Session::tracking("stale".into(), user_id(), task_id(), None, t(2)).unwrap())
            .await
            .unwrap();

        let other = Uuid::parse_str("00000000-0000-0000-0000-0000000000ff").unwrap();
        assert!(repo.list_idle_open(other, t(10)).await.unwrap().is_empty());
    }
```

- [ ] **Step 2: Run to verify they fail**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p infrastructure list_idle_open
```

Expected: FAIL to compile — `no method named list_idle_open`.

- [ ] **Step 3: Add the trait method and its SQL**

Trait, in `session_repository.rs`, beside `list_open`:

```rust
    /// Open sessions whose `last_seen_at` is older than `idle_before`, oldest first.
    /// What the reaper reads.
    async fn list_idle_open(
        &self,
        user_id: UserId,
        idle_before: DateTime<Utc>,
    ) -> Result<Vec<Session>, RepositoryError>;
```

SQL, in `session_repo.rs`, mirroring `list_open`'s shape and its `row_to_session` mapper:

```rust
    async fn list_idle_open(
        &self,
        user_id: UserId,
        idle_before: DateTime<Utc>,
    ) -> Result<Vec<Session>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, user_id, task_id, mode, label, started_at, last_seen_at,
                    last_flush_at, ended_at
             FROM sessions
             WHERE user_id = ? AND ended_at IS NULL AND last_seen_at < ?
             ORDER BY last_seen_at ASC",
        )
        .bind(user_id.to_string())
        .bind(idle_before.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(row_to_session).collect()
    }
```

**Note the comparison is on RFC 3339 text.** `list_open`'s `ORDER BY last_seen_at DESC` has the same property and plan 2's review found it harmless there because real clocks carry fractional seconds uniformly. Here it is a `<` filter rather than an ordering, so state in a comment that the same assumption applies, and pick fixture instants in your tests that are hours apart rather than sub-second, so the test does not depend on it.

- [ ] **Step 4: Run to verify they pass**

```bash
cargo test -p infrastructure list_idle_open
```

- [ ] **Step 5: Write the failing use-case tests**

Create `session_reaper.rs` with its test module first. Reuse the `InMemorySessionRepository` from `use_cases/session_tracking.rs`'s test module and the fake worklog/activity/config repos from `use_cases/worklog.rs`'s — **and before relying on them, check that each method you touch actually mutates state.** Plan 2 found a `delete` that was a no-op and a config stub that discarded every write; both made a test pass for the wrong reason.

```rust
    #[tokio::test]
    async fn reaping_flushes_the_sessions_own_task_and_closes_it() {
        // An idle session with an entry in its window: the reaper must materialize
        // that time before closing, or it is lost the moment the row is closed.
        let (session_repo, worklog, activity, config) = fakes_with_idle_session().await;

        let outcome = reap_idle_sessions(
            &session_repo, &worklog, &activity, &config, user_id(), t(10), t(12),
        )
        .await
        .unwrap();

        assert_eq!(outcome.reaped, 1);
        assert!(outcome.slots_written >= 1, "the idle session's time was materialized");
        let row = session_repo.find_by_id("stale", user_id()).await.unwrap().unwrap();
        assert!(row.ended_at.is_some(), "the session is closed");
    }

    #[tokio::test]
    async fn reaping_leaves_a_fresh_session_alone() {
        let (session_repo, worklog, activity, config) = fakes_with_fresh_session().await;

        let outcome = reap_idle_sessions(
            &session_repo, &worklog, &activity, &config, user_id(), t(10), t(12),
        )
        .await
        .unwrap();

        assert_eq!(outcome.reaped, 0);
        let row = session_repo.find_by_id("fresh", user_id()).await.unwrap().unwrap();
        assert!(row.ended_at.is_none());
    }

    #[tokio::test]
    async fn a_session_with_no_task_is_closed_without_flushing() {
        // `mode = off`, or a bind that never happened: there is nothing to materialize,
        // and asking the flush for a `None` task would be a bug, not a no-op.
        let (session_repo, worklog, activity, config) = fakes_with_idle_untracked_session().await;

        let outcome = reap_idle_sessions(
            &session_repo, &worklog, &activity, &config, user_id(), t(10), t(12),
        )
        .await
        .unwrap();

        assert_eq!(outcome.reaped, 1);
        assert_eq!(outcome.slots_written, 0);
    }

    #[tokio::test]
    async fn one_sessions_flush_failure_does_not_block_the_others() {
        // The reaper runs unattended. If it aborted on the first failure, one wedged
        // session would keep every later one from ever being flushed.
        let (session_repo, worklog, activity, config) = fakes_with_two_idle_one_failing().await;

        let outcome = reap_idle_sessions(
            &session_repo, &worklog, &activity, &config, user_id(), t(10), t(12),
        )
        .await
        .unwrap();

        assert_eq!(outcome.reaped, 1, "the healthy session was still closed");
    }
```

Write the four `fakes_with_*` helpers to match. If a fake you need cannot express one of these situations, say so rather than weakening the assertion.

- [ ] **Step 6: Run to verify they fail, then implement**

```bash
cargo test -p application session_reaper
```

Expected: FAIL to compile.

```rust
use chrono::{DateTime, Utc};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::{
    ActivitySlotRepository, ConfigRepository, SessionRepository, WorklogRepository,
};
use crate::use_cases::worklog::materialize_worklog_time;

/// What one reaping pass did.
pub struct ReapOutcome {
    pub reaped: u32,
    pub slots_written: u32,
}

/// Close every session that has gone quiet, materializing its time first.
///
/// The order matters and is the whole point: flush, then close. Closing first would
/// leave the session's entries with no window that will ever select them again — the
/// row is closed, so no later `aplan log` can revive it — and the time would be lost.
///
/// A failure on one session is logged by the caller and skipped, never propagated: the
/// reaper runs unattended, and one wedged session must not stop every later one from
/// being flushed for the rest of the day.
pub async fn reap_idle_sessions(
    session_repo: &dyn SessionRepository,
    worklog_repo: &dyn WorklogRepository,
    activity_repo: &dyn ActivitySlotRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    idle_before: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<ReapOutcome, AppError> {
    let idle = session_repo.list_idle_open(user_id, idle_before).await?;
    let mut reaped = 0u32;
    let mut slots_written = 0u32;

    for session in idle {
        // A session with no task has nothing to materialize — `mode = off`, or a bind
        // that never happened. Closing it is still right.
        if let Some(task_id) = session.task_id {
            match materialize_worklog_time(
                worklog_repo,
                activity_repo,
                config_repo,
                user_id,
                task_id,
                session.flush_window_start(),
                now,
            )
            .await
            {
                Ok(outcome) => slots_written += outcome.slots_written,
                Err(e) => {
                    tracing::warn!(
                        session = %session.id,
                        "reaper could not flush an idle session, leaving it open: {e}"
                    );
                    // Leave it open on purpose: an unflushed session that stays open can
                    // be flushed by the next pass, whereas one closed without its flush
                    // has lost its time for good.
                    continue;
                }
            }
        }
        if session_repo.end(&session.id, user_id, now).await? {
            reaped += 1;
        }
    }

    Ok(ReapOutcome { reaped, slots_written })
}
```

Add `pub mod session_reaper;` to `use_cases/mod.rs`.

- [ ] **Step 7: Run everything and commit**

```bash
cargo test -p application session_reaper
cargo test --workspace
```

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/application backend/crates/infrastructure
git commit -F - <<'EOF'
Reap sessions that have gone quiet, flushing before closing

`list_idle_open` shipped with no caller in plan 1; this is it. Flush then
close, never the reverse: a session closed without its flush has no window
that will ever select its entries again, so the time is lost.
EOF
```

---

## Task 2: Wire the reaper into the background scheduler

**Files:**
- Modify: `backend/crates/api/src/jobs.rs`
- Modify: `backend/crates/api/src/main.rs`

**Interfaces:**
- Consumes: `reap_idle_sessions` and `ReapOutcome` (Task 1).
- Produces: the reaper running on the existing background loop, with the idle threshold read from the config key `aplan.session_idle_timeout_hours` (default **12**).

**The existing shape, verified — build on it rather than beside it.**

`api/src/jobs.rs:38` is `pub async fn run_eod_scheduler(deps: EodDeps, user_id: UserId)`: a `loop` that runs one `run_eod_pass`, converts the result into an `AttemptOutcome::{Succeeded, Failed}`, feeds it to `health.observe(observed, Utc::now(), &policy)`, logs via a local `report()`, and sleeps `decision.retry_in`. The policy is `RetryPolicy::end_of_day()` (`jobs.rs:39`) — a `const fn` in `application/src/jobs.rs:36` with `base: 5 min` and a ceiling it backs off to. `main.rs:203` spawns it once.

**Reuse `RetryPolicy` + `JobHealth`; do not hand-roll a sleep.** Two shapes are defensible and you must pick one and justify it in your report:

- **A second scheduler** — `run_session_reaper_scheduler(deps, user_id)` with its own `RetryPolicy::session_reaper()` constructor beside `end_of_day()`, its own `JobHealth`, spawned from `main.rs` next to line 203.
- **The reaper inside the existing loop**, before or after the end-of-day pass.

**A reaper failure must not feed the end-of-day health signal, whichever you choose.** That back-off exists to stop hammering a broken git/Gryzzly integration; folding an unrelated failure into it would slow timesheet reconstruction for a reason that has nothing to do with timesheets. If you put the reaper in the existing loop, its outcome stays out of `health.observe`.

**Two concrete details that are easy to miss:**

- `EodDeps` (`jobs.rs:18`) carries `worklog_repo` and `config_repo` but **not** `activity_repo` and **not** `session_repo`. The reaper needs all four. Prefer a separate deps struct over widening `EodDeps` with fields the end-of-day pass never reads.
- `main.rs:135` moves `session_repo` into the GraphQL schema **without** `.clone()` (unlike `activity_repo` at `:119`). Passing it to a job as well means changing that line to `session_repo.clone()`.

- [ ] **Step 1: Write the failing test**

The threshold parsing is the part worth pinning, because a misread key silently changes how aggressively the reaper closes sessions:

```rust
    #[tokio::test]
    async fn the_idle_threshold_defaults_to_twelve_hours() {
        let config = StubConfigRepository::default();
        assert_eq!(idle_timeout_hours(&config, user_id()).await.unwrap(), 12);
    }

    #[tokio::test]
    async fn the_idle_threshold_is_read_from_configuration() {
        let config = StubConfigRepository::default();
        config.set(user_id(), "aplan.session_idle_timeout_hours", "3").await.unwrap();
        assert_eq!(idle_timeout_hours(&config, user_id()).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn an_unparseable_threshold_falls_back_to_the_default() {
        // A corrupt value must not make the reaper close every session immediately.
        let config = StubConfigRepository::default();
        config.set(user_id(), "aplan.session_idle_timeout_hours", "soon").await.unwrap();
        assert_eq!(idle_timeout_hours(&config, user_id()).await.unwrap(), 12);
    }
```

**Check the config stub you use actually stores what it is given.** Plan 2 found `StubConfigRepository` discarding every write and always reading empty, which made two assertions unobservable; it was fixed in `api/graphql/tests.rs`, but verify whichever one you reach here.

- [ ] **Step 2: Run to verify they fail, then implement**

```bash
cargo test -p api idle_threshold
```

Add a small helper beside the job, and call the reaper from the loop. The failure direction that matters: a reaper error must log and continue, never abort the scheduler — the end-of-day pass shares that loop and must keep running.

- [ ] **Step 3: Pass `session_repo` into the job's deps**

`main.rs` already builds `session_repo` for the schema. Add it to the job's deps struct and pass a clone, exactly as `config_repo` is passed today.

- [ ] **Step 4: Run the suite and commit**

```bash
cargo test --workspace
```

```bash
git add backend/crates/api
git commit -F - <<'EOF'
Run the session reaper on the existing background loop

Threshold from `aplan.session_idle_timeout_hours`, default 12 h, with an
unparseable value falling back rather than closing everything at once. A
reaper failure logs and continues: the end-of-day pass shares this loop.
EOF
```

---

## Task 3: `start`, `stop` and `flush` act on the session that is asking

**Files:**
- Modify: `backend/crates/cli/src/main.rs` (the `Start`, `Stop` and `Flush` match arms at `:32`, `:33`, `:34`)
- Modify: `backend/crates/cli/src/commands.rs` (`start` at ~`:57`, `stop` at ~`:380`, `flush`)
- Modify: `backend/crates/cli/tests/integration.rs`

**Interfaces:**
- Consumes: `bindSession` / `endSession` mutations, `flush_task(client, task_id, session: Option<&str>)`.
- Produces: `start`, `stop` and `flush` acting on **the session** when one is present, and keeping their current global-pointer behaviour when none is.

**This is the semantic change plan 1 deliberately deferred**, and the reason it waited is that the hooks depend on it: plan 1's note said it "lands with the hooks in plan 3, because that is the change the hooks depend on".

**`flush` is in scope, and this is the part that is easy to miss.** `main.rs:32-34` reads:

```rust
        cli::Commands::Start { task } => commands::start(&args.api_url, args.json, &task),
        cli::Commands::Stop => commands::stop(&args.api_url, args.json),
        cli::Commands::Flush { task } => commands::flush(&args.api_url, args.json, &task),
```

None of the three receives `args.session`, while `Note` on the very next line does. `--session` is declared `global = true` with `env = "CLAUDE_CODE_SESSION_ID"`, so `aplan flush --session s1 <task>` **parses fine today and silently ignores the session**, flushing against the human's `aplan.active_since` window instead. That is the same shared-watermark defect, in a third place, and Task 5's hook cannot be correct until it is fixed.

Behaviour:
- `aplan start <task>` **with** a session id → `bindSession`, which flushes the session's previous task against that session's own window (plan 2 wired this). **The human's pointer must not move.**
- `aplan start <task>` **without** a session id → today's behaviour exactly: flush the human's previous task, set `aplan.active_task_id` and re-arm `aplan.active_since`.
- `aplan stop` **with** a session id → flush the session's task against its own window, then `endSession`. **The human's pointer must not be cleared.**
- `aplan stop` **without** → today's behaviour.
- `aplan flush <task>` **with** a session id → flush against **that session's** window and advance **that session's** `last_flush_at`. Touches no configuration key.
- `aplan flush <task>` **without** → today's behaviour: the human's window, the human's `aplan.active_since`.

- [ ] **Step 1: Write the failing tests**

Six cases. Every CLI integration test goes through the shared `aplan()` builder, which strips `CLAUDE_CODE_SESSION_ID` by construction; pass `--session` explicitly where a session is the point. Mount each matcher so it accepts only the body shape you intend, with `.expect(1)` — `reqwest`'s `.json()` serializes compactly, so `"sessionId":"s1"` and `"sessionId":null` are distinguishable as literal substrings, and plan 2 caught a vacuous test exactly here. Reuse the existing `NoSessionIdOnTheWire` matcher for the no-session cases.

1. `aplan --session s1 start <task>` → a `BindSession` request is made, and **no** `UpdateConfiguration` touches `aplan.active_task_id` (`.expect(0)`).
2. `aplan start <task>` with no session → `UpdateConfiguration` sets `aplan.active_task_id`, and no `BindSession` is made.
3. `aplan --session s1 stop` → a `FlushWorklogTime` carrying `"sessionId":"s1"` and an `EndSession`, with **no** `UpdateConfiguration` clearing the pointer.
4. `aplan stop` with no session → today's flush with no session id, and the pointer cleared.
5. `aplan --session s1 flush <task>` → `FlushWorklogTime` carrying `"sessionId":"s1"`.
6. `aplan flush <task>` with no session → `FlushWorklogTime` with no string-valued `sessionId` on the wire (the `NoSessionIdOnTheWire` matcher).

> **Do not assert `.expect(0)` on `UpdateConfiguration` for cases 5 and 6.** `flush` never calls that mutation — `commands.rs:1008-1031` issues only `FlushWorklogTime`, and the watermark advance happens **server-side** inside that resolver, which chooses between the session's `last_flush_at` and the human's `aplan.active_since`. An `.expect(0)` there would pass identically before and after your change: a vacuous assertion of exactly the kind this plan's reviews have caught three times. The wire-level `sessionId` is the only thing a CLI test can honestly observe here; the server's choice of watermark is already covered by plan 2's API tests.
>
> Cases 1 and 3 are different — `start` and `stop` *do* write `aplan.active_task_id` themselves, so `.expect(0)` on `UpdateConfiguration` is meaningful for those two and must stay.

The exact defect in `flush` is not merely a missing parameter: `commands.rs:1019` passes `session_id: None` as a literal, so `aplan flush --session s1 <task>` sends `sessionId: null` and the session is dropped on the wire.

- [ ] **Step 2: Run to verify cases 1, 3 and 5 fail**

```bash
cargo test -p cli -- start stop flush
```

Expected: 1, 3 and 5 FAIL on unmet expectations — today all three commands write the human's keys and drop the session.

- [ ] **Step 3: Implement**

Thread `args.session.as_deref()` into all three arms in `main.rs`, exactly as the `Note` arm on line 40 already does, then use it inside each command. Keep the two paths visibly separate — a reader must be able to see which keys each branch touches — and comment *why* the session branch must not touch the human's pointer: the two windows and the two pointers are never crossed, and a session moving the human's tracking is the mirror image of the defect this whole feature exists to fix.

- [ ] **Step 4: Run everything, including the env A/B**

```bash
cargo test --workspace
S=/tmp/claude-1000/-home-mbt/f17e71e0-7cc2-4f9a-a087-7658e467f8af/scratchpad
env -u CLAUDE_CODE_SESSION_ID cargo test -p cli > $S/ab-a.txt 2>&1; echo "A exit=$?"
CLAUDE_CODE_SESSION_ID=deadbeef cargo test -p cli > $S/ab-b.txt 2>&1; echo "B exit=$?"
rg -o '[0-9]+ passed; [0-9]+ failed' $S/ab-a.txt
rg -o '[0-9]+ passed; [0-9]+ failed' $S/ab-b.txt
```

Both must be identical. **This task is the likeliest in the plan to break that**, because it changes behaviour on a path fed by that very variable.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/cli
git commit -F - <<'EOF'
Bind and close the session when `start`/`stop` are asked by one

A session's start must not move the human's pointer, and its stop must not
clear it. Plan 1 deferred this until the hooks needed it; they do now.
EOF
```

---

## Task 4: Rewrite the SessionStart hook

**Files:**
- Modify: `~/.claude/hooks/aplan-session-start.sh` (outside the repository — read "The risk that shapes this whole plan" again before starting)

**Interfaces:**
- Consumes: `aplan session show --session <id> --json`, `aplan session bind --session <id> <task>`, `aplan session off --session <id>`, `aplan ls --json`.
- Produces: a hook that reads **the session's own row** instead of re-deriving state from the human's pointer.

**The defect this closes, located exactly.** The installed hook is 133 lines. Its `resume|compact` branch (`:60-86`) reads `current_id` from `aplan current --json` — **the human's pointer** — and injects `Currently tracking aplan task: "<title>"`. Its `*)` branch (`:88-129`, covering `startup`, `clear` and any unknown source) already emits the mandatory four-option question ending in `Ne pas tracker`, and `:121` already instructs the model never to log for the rest of the session.

**The choice is never persisted anywhere.** Nothing writes it, so the next SessionStart re-fire re-reads the human's pointer and announces tracking of a task the user declined. That is the bug that started this design, and it reproduced in the session that wrote this plan. Plan 1 gave the decision a home; this task makes the hook write and read it.

Two properties of the current file must survive verbatim:
- `[ -n "${APLAN_UNATTENDED:-}" ] && exit 0` (`:23`) — cron sessions have no user to answer a question.
- The defensive stdin read (`:27-29`): `payload=$(cat)`, then `jq -r '.source // "startup"' 2>/dev/null || echo startup`, then `[ -z "$source" ] && source=startup`. Add `session_id` extraction in exactly this style.

The output shape is `jq -nc --arg ctx "$context" '{hookSpecificOutput:{hookEventName:"SessionStart", additionalContext:$ctx}}'` (`:132`). Keep it.

The four branches, from the design spec, mapped onto that structure:

| State | Injected context |
|---|---|
| Unknown session | The existing mandatory `AskUserQuestion`, but the actions become `aplan session bind --session <id> <task>` and `aplan session off --session <id>` |
| Known, `mode = off` | One line: logging is disabled for this session, **do not ask again**, never call `aplan log/start/stop/flush` |
| Known, `mode = tracking` | One line confirming **the session's** task |
| `source = clear` | Force the re-choice even when known — the user wants that choice explicit at `/clear` |

`source = resume` and `source = compact` follow the table: confirm, never re-ask. **That is the fix.** The mandatory question's `Ne pas tracker` branch must now run `aplan session off --session <id>` so the decision survives the next re-fire — today it runs no command at all, which is the whole bug.

**`aplan current` keeps exactly one legitimate use: the unknown-session branch.** When no session row exists and the human's pointer is set, offering `Continuer : <the human's task>` as Option 1 is genuinely good — the human is working on something by hand and has just opened a Claude on it. But the action behind that option becomes `aplan session bind --session <id> <task>`, **not** "no aplan command needed" as `:118` says today. The human's pointer must not move. For a *known* session the pointer is never consulted.

**`base_rules` (`:38-48`) contains the defect in prose** and must change: `"This Claude Code session is linked to your currently-tracked aplan task — the active-task pointer IS the link."` That sentence is now false. The link is the session row; the pointer is the human. Line `:46` ("run `aplan start <task>` first") also needs the Task 3 semantics — inside a session that binds the session and leaves the pointer alone.

### The wire format the hook must parse — verified, and three of these bite silently

Both hooks read `aplan session show --session <id> --json`. With `--json`, `session_cmd.rs:103` prints `print_json(&r.raw)` — the **raw GraphQL response**, envelope included.

1. **The jq path is `.data.claudeSession`, not a top-level field.** A hook written against `.mode` gets `null` and takes the wrong branch, silently.
2. **`mode` is `"TRACKING"` or `"OFF"` — uppercase.** `SessionModeGql` is an async-graphql enum (`schema.graphql:1398-1401`) and serializes SCREAMING_SNAKE, while the database column and this plan's prose both use lowercase `tracking`/`off`. **A jq test against `"off"` never matches**, so every opted-out session would take the tracking branch — reintroducing the exact bug this task exists to fix, in a form no compiler or test would catch. Compare against `"OFF"`.
3. **`claude_session.graphql` does not select the task's title, so add it: `task { id title }`.** The server-side resolver already exists — `ClaudeSessionGql::task` at `api/src/graphql/types/session.rs:30-38` returns `SessionTaskSummaryGql { id, title }` — only the CLI's selection set is missing it. No schema regeneration is needed. Without this the hook can only say `Currently tracking task 3f2a1b8c-…`, and `aplan session show`'s human output has the same problem (`session_cmd.rs:118` prints `task: <uuid>`); fix that line too while you are there.
4. **An unknown session id is an error, not an empty success.** `session_cmd.rs:111` reports `LookupError::SessionUnknown`. Make sure your `|| exit 0` guard treats that as "no session row" and falls through to the unknown-session branch rather than aborting the hook before it emits anything.

- [ ] **Step 1: Back up the installed hook**

```bash
cp ~/.claude/hooks/aplan-session-start.sh \
   ~/.claude/hooks/aplan-session-start.sh.bak-$(date +%Y%m%d)-pre-plan3
ls -la ~/.claude/hooks/
```

- [ ] **Step 2: Write the payload fixtures and a test script, in the scratchpad**

Claude Code passes a JSON payload on stdin carrying at least `session_id`, `source` and `cwd`. Create five fixtures under the session scratchpad — unknown/`startup`, known-tracking/`resume`, known-off/`resume`, known-tracking/`clear`, and a malformed one — plus a small runner that pipes each to the candidate script and prints the injected `additionalContext`.

The current hook already extracts `.source` defensively (`jq -r '.source // "startup"' … || echo startup`) and defaults an empty value; keep that, and add `session_id` the same way. **Read the installed script in full before rewriting it** — it carries several deliberate properties, including the `APLAN_UNATTENDED` early exit for cron sessions, that must survive.

- [ ] **Step 3: Develop the new hook in the scratchpad and run the fixtures**

Expected, per fixture: the unknown/`startup` case emits the mandatory question; known-tracking/`resume` emits one confirmation line naming the task; known-off/`resume` emits the do-not-track line **and no question**; known-tracking/`clear` emits the question again; the malformed payload emits valid JSON or nothing, and exits 0.

The last one is not optional. A hook that emits malformed JSON or a non-zero exit at session start degrades every session the user opens.

- [ ] **Step 4: Prove the failure it fixes, against the old hook**

Pipe the known-off/`resume` fixture to the **backup** copy and show that it injects tracking context anyway — that is the original defect, reproduced, and it is the evidence this task is worth its risk. Then pipe the same fixture to the new script and show the do-not-track line. Record both outputs verbatim in your report.

- [ ] **Step 5: Install and re-verify**

Copy the tested script over the installed one, re-run all five fixtures against the installed path, and confirm identical output. Do not open a new Claude session to test.

- [ ] **Step 6: Commit a copy into the repository**

The hooks are not tracked by git, which is why they have no safety net. Commit a reference copy under `docs/hooks/` so the repository records what was installed, and say in the body that the live file lives in `~/.claude/hooks/`.

```bash
cd ~/appfactory/aggregated_plan
mkdir -p docs/hooks
cp ~/.claude/hooks/aplan-session-start.sh docs/hooks/aplan-session-start.sh
git add docs/hooks/aplan-session-start.sh
git commit -F - <<'EOF'
Read the session's own row at SessionStart, not the human's pointer

The hook announced tracking derived from `aplan.active_task_id`, so a
re-fire mid-session claimed a task the user had declined — the defect that
started this whole design. The decision has had a home since plan 1; this
reads it. Reference copy only: the live hook is in ~/.claude/hooks/.
EOF
```

---

## Task 5: Rewrite the SessionEnd hook

**Files:**
- Modify: `~/.claude/hooks/aplan-session-end.sh` (outside the repository)

**Interfaces:**
- Consumes: `aplan session show --session <id> --json`, `aplan --session <id> flush --json <task_id>` (which only honours the session after Task 3).
- Produces: a hook that flushes **the ending session's** task against that session's own window. **It does not close the session** — see below.

**This is the last place the original defect is still live.** The installed hook is 16 lines and its body is:

```bash
current_json=$(aplan current --json 2>/dev/null) || exit 0
task_id=$(printf '%s' "$current_json" | jq -r '.currentActivity.task.id // empty')
[ -z "$task_id" ] && exit 0
aplan flush --json "$task_id" >/dev/null 2>&1 || exit 0
```

`aplan current` is the **human's** pointer, and `aplan flush` with no session advances the human's `aplan.active_since`. So every Claude SessionEnd today flushes whatever the human is tracking. Plan 2's rebuild limits the damage — any later flush naming a half-day repairs it — but a half-day that never gets another flush keeps its time unmaterialized.

**The hook must NOT end the session, and this is a correction to the plan's earlier draft.** Ending the row here looks tidy and breaks resume:

- A Claude Code session id survives `claude --resume`. If SessionEnd closed the row, resuming that transcript would fire SessionStart with the same id against an **ended** session — a fifth state absent from the design spec's four-branch table, and one `Session::target()` already refuses by name.
- `upsert` overwrites only `task_id`, `mode`, `label` and `last_seen_at`; it cannot clear `ended_at`. Re-opening would need a new repository method — machinery bought to undo something we chose to do.
- **The reaper is the sole closer**, and Task 1 built it for exactly this. Plan 2's idempotent rebuild is what makes the reaper's later second flush harmless: it rebuilds the same half-days to the same slots.
- The cost is that `aplan sessions` lists sessions seen in the last 12 hours as open. That is honest rather than wrong — `list_open` orders by `last_seen_at` so live sessions stay on top — and the reaper trims the rest.

`aplan --session <id> stop` still ends the session. That is a deliberate act by whoever is driving, not a lifecycle event, and the distinction is the point.

**Read Task 4's "The wire format the hook must parse" section before starting.** All four items apply here identically: the jq path is `.data.claudeSession`, `mode` is uppercase `"OFF"`/`"TRACKING"`, an unknown session is an error rather than an empty success, and Task 4 adds `task { id title }` to the query. Two of the four fail silently if you get them wrong.

- [ ] **Step 1: Back up the installed hook**

```bash
cp ~/.claude/hooks/aplan-session-end.sh \
   ~/.claude/hooks/aplan-session-end.sh.bak-$(date +%Y%m%d)-pre-plan3
```

- [ ] **Step 2: Write fixtures and prove the current failure**

Three payloads: a session with a task, a session in `mode = off`, and an unknown session id. Pipe the first to the **backup** copy while the human's pointer is on a *different* task, and show that it flushes the human's task rather than the session's. Record it — that is the defect.

**Read this before running anything against a live backend:** the systemd service `aplan-api.service` is serving the user's real database on port 3001, and a flush **writes**. Prove the defect without mutating that data — read the backup hook's `aplan flush` invocation and show which task id it computes (e.g. by piping the payload to a copy whose last line is `echo` instead of `aplan flush`), rather than letting a real flush land. A stub-and-echo probe is sufficient evidence here and costs the user nothing.

- [ ] **Step 3: Develop the replacement in the scratchpad**

It must: read the session's row by the payload's `session_id`; if the session has a task, run `aplan --session "$sid" flush --json "$task_id"`; if `mode = off`, there is no task, or the session is unknown, do nothing and exit 0. It never ends the session and never touches `aplan current`. Preserve the silent-no-op guards (`command -v aplan`, `command -v jq`, `|| exit 0` on every `aplan` call) exactly as the current 16-line hook has them.

Take `session_id` from the stdin payload, not from `$CLAUDE_CODE_SESSION_ID`. The variable may well be present in the hook's environment, and `--session` would then default to it — but the payload is the contract, and relying on an inherited variable is the kind of implicit coupling that makes a failure invisible when it changes.

**Do not fall back to the human's pointer when the session is unknown.** That fallback is the shape the design spec forbids by name, and plan 2 already removed the server-side version of it.

- [ ] **Step 4: Run the fixtures, install, re-verify**

Same discipline as Task 4: all three fixtures pass in the scratchpad, install, re-run against the installed path, identical output. Never test by ending a real session.

- [ ] **Step 5: Commit the reference copy**

```bash
cd ~/appfactory/aggregated_plan
cp ~/.claude/hooks/aplan-session-end.sh docs/hooks/aplan-session-end.sh
git add docs/hooks/aplan-session-end.sh
git commit -F - <<'EOF'
Flush the ending session's task at SessionEnd, not the human's

The hook read `aplan current --json` and flushed with no session, so every
Claude SessionEnd flushed whatever the human was tracking and advanced the
global key. This was the last live instance of the shared-watermark defect.
EOF
```

---

## Task 6: Update the aplan skill

**Files:**
- Modify: `.claude/skills/aplan/SKILL.md`

The skill is what a Claude reads to drive the cockpit, so a stale instruction here is a stale instruction in every session. Three things changed under it:

- **The session vocabulary.** `aplan sessions`, `aplan session show|bind|off|end`, the global `--session` flag defaulting from `CLAUDE_CODE_SESSION_ID`, and the fact that a session's task is separate from the human's pointer.
- **The exit-4 row** (`SKILL.md:132`) must gain the three session refusals: the session is not tracked, it has no task bound, it has ended. An unknown session id is exit **2**, not 4 — a lookup failure rather than a state.
- **`remember` is the deliberate exception**: it never refuses. `--task` wins, else a tracking session attaches, else the memory is unattached and the command succeeds — including for an `off` session. The reason belongs in the text, because someone will otherwise "fix" the inconsistency: memories sit outside the worklog rules, and an unattached memory misattributes nothing, where a misattributed worklog entry is billable time on the wrong task.

Also correct the retargeting instruction: `SKILL.md` and the SessionStart hook both currently say to run `aplan start <task>` to retarget mid-session. After Task 3 that is right for a session too — but say plainly that it binds the session and does **not** move the human's pointer.

- [ ] **Step 1: Read the whole file, then edit**

There is no test for a skill document. Read it end to end first; it carries a long narrative about a real incident (roughly 4h35 of one session's time landing on another task) whose lesson must survive the edit.

- [ ] **Step 2: Commit**

```bash
git add .claude/skills/aplan/SKILL.md
git commit -m "Teach the aplan skill the session vocabulary and its refusals"
```

---

## Task 7: The overlap rule

**Files:**
- Create: `backend/crates/domain/src/rules/overlap.rs`
- Modify: `backend/crates/domain/src/rules/mod.rs`

**Interfaces:**
- Produces:
  - `pub struct Overlap { pub a: ActivitySlotId, pub b: ActivitySlotId, pub minutes: i64 }`
  - `pub fn find_overlaps(slots: &[ActivitySlot]) -> Vec<Overlap>`

Pure, zero I/O, in `domain` beside the other projection rules. Two closed slots on **different tasks** whose `[start_time, end_time]` intervals intersect overlap by the length of the intersection. Same-task pairs are not overlaps — a task legitimately has several stretches in a half-day. Open slots hold no hours and are excluded.

**`ActivitySlot::task_id` is `Option<TaskId>`, and that is not a formality** — `startActivity(taskId: null)` is reachable from the UI, so untagged slots exist in the real data. A slot with `task_id: None` is time attributed to nobody, so it cannot constitute "two tasks claim the same hour" and must be excluded before pairing. Excluding it also keeps Task 9 honest: the display names both tasks, and a `None` slot has no name to print. The relevant fields are `id`, `task_id: Option<TaskId>`, `start_time: DateTime<Utc>`, `end_time: Option<DateTime<Utc>>` and `session_id: Option<SessionId>` (`None` = the human) — that last one is what Task 9 needs to say `session a1b2 ↔ manuel`.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn two_disjoint_slots_do_not_overlap() { /* 09:00-10:00 and 10:00-11:00 → none */ }

    #[test]
    fn touching_slots_do_not_overlap() { /* end == start is not an intersection */ }

    #[test]
    fn a_partial_intersection_is_measured_in_minutes() { /* 09:00-10:00 vs 09:30-11:00 → 30 */ }

    #[test]
    fn a_nested_slot_overlaps_by_its_own_length() { /* 09:00-12:00 vs 10:00-10:30 → 30 */ }

    #[test]
    fn two_slots_on_the_same_task_are_not_an_overlap() { /* same task_id → none */ }

    #[test]
    fn an_open_slot_is_never_an_overlap() { /* end_time None → none */ }

    #[test]
    fn an_untagged_slot_is_never_an_overlap() { /* task_id None, intersecting a tagged slot → none */ }

    #[test]
    fn three_mutually_overlapping_slots_yield_three_pairs() { /* pairs, not a merged span */ }
```

Write each body against the file's own fixture style — read a neighbouring rules module first rather than inventing a helper.

- [ ] **Step 2: Run to verify they fail, then implement, then run again**

```bash
cargo test -p domain overlap
```

- [ ] **Step 3: Commit**

```bash
git add backend/crates/domain
git commit -F - <<'EOF'
Measure the overlap between two tasks' slots

Pure rule, pairs rather than merged spans: the user arbitrates at the
timesheet review, and a merged span hides which two tasks collided.
EOF
```

---

## Task 8: Expose overlap on the journal

**Files:**
- Modify: `backend/crates/application/src/use_cases/activity_reporting.rs`
- Modify: `backend/crates/api/src/graphql/{query.rs,types/activity.rs}`
- Modify: `backend/crates/cli/graphql/activity_journal.graphql`
- Modify: `backend/crates/cli/graphql/schema.graphql` (regenerated)

**Interfaces:**
- Consumes: `find_overlaps` (Task 7).
- Produces: the journal query returning the day's overlaps alongside its slots, so `journal`, `dash` and `timesheet` can all read one computation.

Compute at read time. Nothing is stored and nothing is corrected — that is the spec's explicit non-goal, and the user's decision: each task keeps the time its own entries document, double counting is accepted and **flagged**.

The current query is:

```graphql
query ActivityJournal($date: NaiveDate!) {
  activityJournal(date: $date) {
    id taskId startTime endTime halfDay durationMinutes
    task { id title }
  }
}
```

Decide where the overlap list attaches — a sibling field on a wrapper type, or a resolver-level query taking the same date — and say why. A wrapper changes the existing field's shape and every consumer; a sibling query is additive. **The frontend consumes `activityJournal`** (`frontend/src/hooks/use-activity.ts`), and frontend work is a non-goal, so **additive is almost certainly right**: verify what the frontend selects before choosing.

- [ ] **Step 1: Write the failing resolver test, run it, implement, run again**

The test must assert both tasks are named and the minutes are right, not merely that a list is non-empty.

- [ ] **Step 2: Regenerate the CLI schema against a scratch database**

```bash
cd ~/appfactory/aggregated_plan/backend
DATABASE_URL=sqlite:/tmp/claude-1000/-home-mbt/f17e71e0-7cc2-4f9a-a087-7658e467f8af/scratchpad/schema-p3.db?mode=rwc \
  cargo run -p api -- export-schema > crates/cli/graphql/schema.graphql
sqlite3 aggregated_plan.db "SELECT MAX(version) FROM _sqlx_migrations;"   # must print 14
```

- [ ] **Step 3: Run the suite and commit**

---

## Task 9: Show overlaps in `journal`, `dash` and `timesheet`

**Files:**
- Modify: `backend/crates/cli/src/commands.rs` — the three commands are `Dash` (`cli.rs:189`), `Journal` (`cli.rs:197`) and `Timesheet` (`cli.rs:252`); `journal`'s handler is at ~`commands.rs:434`
- Modify: `backend/crates/cli/tests/integration.rs`

The spec's wording, which is what the user approved:

- `aplan journal` — a line per overlapping pair, both tasks named and the actors identified: `⚠ recouvrement 47 min — Saft cadrage ↔ Cartier (session a1b2 ↔ manuel)`.
- `aplan timesheet` — the day's raw total and its gap against elapsed wall-clock time, so the arbitration happens where a human already reviews the day.
- `aplan dash` — one summary line when the day carries any overlap.

Match the language of each command's existing output — the CLI's user-facing strings are French in some places and English in others; read each before writing.

- [ ] **Step 1: Write the failing integration tests, run, implement, run**

Each command gets one test asserting the overlap line appears with both task titles and the minutes, and one asserting a clean day prints no warning at all. A warning on a day with no overlap would train the user to ignore it.

- [ ] **Step 2: Run the suite and the env A/B, then commit**

---

## Task 10: Documentation

**Files:**
- Modify: `SPEC_TECHNIQUE.md`
- Modify: `docs/superpowers/specs/2026-08-04-aplan-session-scoped-worklog-design.md`

- [ ] **Step 1: Add § 7.3.5 (lifecycle) and § 7.3.6 (overlap) in French**

§ 7.3.5: the four SessionStart branches and why `resume`/`compact` never re-ask; SessionEnd flushing the session's own task; `start`/`stop` acting on the session that asks; the reaper, its threshold key and the flush-then-close order. § 7.3.6: overlap computed at read time, never stored and never corrected, displayed in the three commands, with the user's decision — double counting accepted and flagged — stated as such.

- [ ] **Step 2: Mark the design spec's staging item 3 complete** with the commit range, noting that its § "Lifecycle and hooks" and § "Overlap" now describe shipped behaviour.

- [ ] **Step 3: Commit**

---

## Self-review notes

**Spec coverage.** § "Lifecycle and hooks" → tasks 1–6: the four-branch table (4), SessionEnd (5), `last_seen_at` refreshed on every session-scoped write (already shipped in plan 1's task via `addWorklogEntry`), the 12-hour reaper (1, 2), `start`/`stop` (3). § "Overlap — visible, never corrected" → tasks 7–9, including the spec's exact display wording. § "New surface" → task 6 for the skill, task 9 for the commands.

**Carried in from plan 2's final review, not invented here.** Task 5 exists because that review established the SessionEnd hook is the last live instance of the original defect. Task 3 exists because plan 1 deferred it explicitly "until the hooks need it". The `aplan stop`-has-no-session-awareness minor from that review is closed by task 3.

**Deliberately out of scope.** No frontend work — task 8 is additive precisely to avoid it. The two looser `FakeActivityRepo` methods stay as they are unless a task reaches them. `resolve_session_target` stays dead; the CLI owns that refusal.

**The known risk, restated because it is the only irreversible thing here.** Tasks 4 and 5 modify files outside the repository that run on every session start and end. Back them up, develop in the scratchpad, test by piping payloads, install only what passed, and never verify by opening or ending a real session. If a hook cannot be made to satisfy the malformed-payload fixture, stop and escalate rather than installing it.
