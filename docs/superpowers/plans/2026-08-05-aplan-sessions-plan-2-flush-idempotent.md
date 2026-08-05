# aplan Sessions — Plan 2: idempotent flush

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make materializing worklog time a rebuild of the projection per (task, half-day) instead of an append driven by a single global watermark, so concurrent sessions on different tasks stop losing each other's hours.

**Architecture:** The reattribution repair already does exactly this — it drops a task's stale projection over the affected half-days and rewrites it from what the entries now say. This plan extracts that into one shared primitive, teaches `is_rebuildable` to consult `activity_slots.source` so the rebuild owns only what the projection wrote, and rewires the flush onto it. The flush window stops deciding truth and only selects which half-days to rebuild; truth always comes from every entry in them.

**Tech Stack:** Rust (stable), sqlx 0.8 + SQLite (runtime queries), async-graphql 7, Axum 0.7, clap 4, graphql_client 0.14, wiremock + assert_cmd.

**Spec:** `docs/superpowers/specs/2026-08-04-aplan-session-scoped-worklog-design.md`, sections "Flush becomes an idempotent rebuild" and "Reattribution alignment". Approved by the user; this plan implements it and adds no design of its own.

**Predecessor:** plan 1 (`2026-08-04-aplan-sessions-plan-1-socle.md`), merged as commits `891220f..9c8d8c6` on `feat/aplan-sessions-socle`.

## Why this plan must land before plan 3

Plan 3 rewrites the SessionStart hook so every Claude session calls `aplan session bind`. Today `session bind` flushes the session's previous task using the **global** window and then lets the server advance that global key — unlike `aplan start`, which also re-arms it. That is a latent inherited flaw while binds are hand-typed; it becomes automatic and daily the moment a hook calls it on every session start. Tasks 5 and 6 below close it. **Do not start plan 3 until this plan is merged.**

## Global Constraints

- **Branch:** continue on `feat/aplan-sessions-socle` (already pushed to `origin`). Never commit to `main`.
- Commit messages: plain imperative subject, short body for the *why*. **No `Co-Authored-By` footer, no `Signed-off-by` trailer.** Stage only the files a task names — never `git add -A` / `git add .`. **Never push.**
- **DDD layers are strict.** `domain/` = pure, zero I/O, deps limited to chrono/serde/uuid/thiserror. `application/` = repository traits + use cases, depends on domain only. `infrastructure/` = sqlx implementations. `api/` = Axum + async-graphql. `cli/` depends on **no** workspace crate — it talks GraphQL like any other client.
- No `.unwrap()` in production code (test code may use it freely). Runtime `sqlx::query`, never `sqlx::query!`. `sqlx::Error` maps to `RepositoryError::Database(e.to_string())`.
- **Any local-day reasoning goes through `application::use_cases::worklog::user_timezone`.** Two readings of `aplan.timezone` that disagree put one entry on two different local days.
- **Green suite is `cargo test --workspace` — no `--exclude` flag.** `crates/mcp` was excluded from the workspace in `09e6670` (it has never compiled). Baseline at plan 2's start: **1049 passed / 0 failed**, plus whatever the pending api-double test adds.
- **The env A/B must stay identical:** `env -u CLAUDE_CODE_SESSION_ID cargo test -p cli` and `CLAUDE_CODE_SESSION_ID=deadbeef cargo test -p cli`, both at 100 passed / 0 failed. The harness exports that variable into every Bash call, so a suite that is sensitive to it passes on your machine and fails in a plain terminal. Capture output to a file before counting — a bare pipeline has silently dropped lines for several agents on this branch.
- **The user's real database is LIVE and already migrated.** `backend/aggregated_plan.db` is at migration 14 with the provenance classification applied (94 `manual` / 56 `worklog`), and a systemd user service `aplan-api.service` holds it on port 3001. **Do not write to it, do not restart the service, do not run a server against it.** Tests use `sqlite::memory:`; if you must exercise a write path against real data, copy the database to the session scratchpad and point `DATABASE_URL` at the copy.
- **`activity_slots.source` is the deletion gate.** A slot marked `worklog` is one a rebuild may delete; `manual` and any unreadable value are protected. Getting that backwards deletes hand-made time, so every task that touches the gate proves it with a test.
- Spec maintenance: `SPEC_TECHNIQUE.md` (French) is updated by Task 7 for the whole plan, not per task.

**Commands:**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test --workspace          # the green gate, no flag
cargo test -p domain            # fast inner loop
cargo test -p application
cargo clippy --workspace
```

---

## File Structure

**Modified:**

| File | Change |
|---|---|
| `backend/crates/domain/src/rules/reattribution.rs` | `is_rebuildable` starts consulting `source` |
| `backend/crates/infrastructure/src/database/activity_repo.rs` | round-trip test for the `UPDATE` path that writes `session_id`/`source` |
| `backend/crates/application/src/use_cases/worklog.rs` | new `RebuildPlan` + `plan_task_projection` + `apply_task_projection`; `materialize_worklog_time` rewired onto them |
| `backend/crates/application/src/use_cases/reattribution.rs` | its inline rebuild replaced by calls to the shared primitive; behaviour unchanged |
| `backend/crates/api/src/graphql/mutation.rs` | `flushWorklogTime` gains `sessionId`, reads/advances the per-session window |
| `backend/crates/cli/graphql/flush_worklog_time.graphql` | `sessionId` argument |
| `backend/crates/cli/src/{commands,session_cmd}.rs` | `flush_task` carries the session; the false comment corrected |
| `SPEC_TECHNIQUE.md` | § 7.3.4 — the rebuild and the per-session window |

**No new files.** The primitive belongs beside `materialize_worklog_time` in `use_cases/worklog.rs`, which is the module that owns the projection.

---

## Task 1: Teach `is_rebuildable` to consult provenance

**Files:**
- Modify: `backend/crates/domain/src/rules/reattribution.rs:167-173` and its test module

**Interfaces:**
- Consumes: `SlotSource::is_projection(&self) -> bool` (plan 1, `domain/src/types/activity.rs`).
- Produces: `is_rebuildable(&ActivitySlot) -> bool` now requiring **both** closed and `source == Worklog`.

**Why this is Task 1:** plan 1 installed `activity_slots.source` and classified 148 real slots against it, but **nothing in production reads it yet** — `is_rebuildable` still checks only `end_time.is_some()`. Until this task lands, the whole classification is inert and a rebuild would delete hand-made time. Everything else in this plan makes rebuilds happen more often, so the gate closes first.

- [ ] **Step 1: Write the failing tests**

Append to the test module in `backend/crates/domain/src/rules/reattribution.rs`:

```rust
    /// The gate this whole feature exists for: a slot the user made by hand is not
    /// the projection's to delete, however closed it is.
    #[test]
    fn a_manual_slot_is_never_rebuildable_even_when_closed() {
        let mut s = slot(HalfDay::Morning, "2026-08-04", false);
        s.source = SlotSource::Manual;
        assert!(!is_rebuildable(&s));
    }

    /// And an unclassified row reads as manual, so it is protected too — that is
    /// what makes the one-shot classification safe to have missed a row.
    #[test]
    fn a_closed_worklog_slot_is_the_only_rebuildable_shape() {
        let mut s = slot(HalfDay::Morning, "2026-08-04", false);
        s.source = SlotSource::Worklog;
        assert!(is_rebuildable(&s), "closed + worklog is the projection's own");

        s.end_time = None;
        assert!(!is_rebuildable(&s), "a running timer holds no hours");
    }
```

The file's existing `fn slot(half_day, date, active)` helper already derives `source` as closed⇒`Worklog` / open⇒`Manual`, so the two pre-existing tests (`an_open_slot_counts_for_nothing_and_is_never_rebuilt`, `a_closed_slot_is_rebuildable`) stay green without edits. If they go red, the helper's derivation is wrong — fix the helper, not the assertions.

- [ ] **Step 2: Run to verify they fail**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p domain is_rebuildable
```

Expected: `a_manual_slot_is_never_rebuildable_even_when_closed` FAILS (`is_rebuildable` currently returns true for it).

- [ ] **Step 3: Write the implementation**

```rust
/// Is this slot one the reattribution repair — or the flush — may replace?
///
/// Two conditions, and each rules out a different disaster.
///
/// **Closed.** An open slot is a *running* activity: deleting it would stop a timer
/// nobody asked to stop, and it accounts for no hours anyway.
///
/// **Owned by the projection.** `source == Worklog` means the flush wrote this slot
/// from worklog entries, so rewriting it from those same entries is a no-op or a
/// correction. A `Manual` slot came from somewhere else — the live timer, the UI, a
/// row whose provenance the one-shot classification could not establish — and
/// deleting it destroys time no entry can reproduce. `SlotSource::from_db` reads an
/// unreadable or NULL value as `Manual` precisely so that the unknown lands on the
/// protected side of this line.
pub fn is_rebuildable(slot: &ActivitySlot) -> bool {
    slot.source.is_projection() && slot.end_time.is_some()
}
```

Add `use crate::types::activity::SlotSource;` to the test module if it is not already imported (plan 1 added it at `:179`).

- [ ] **Step 4: Run to verify they pass, then the whole suite**

```bash
cargo test -p domain is_rebuildable
cargo test --workspace
```

Expected: the two new tests PASS. **The workspace suite may now show failures in `application`'s reattribution tests** — that is real information, not collateral: any test that relied on a `Manual` slot being rebuilt was asserting the old, wrong behaviour. Read each failure and decide: if the fixture represents flush-derived time it should be `Worklog` (fix the fixture); if it represents hand-made time then its old expectation was the defect (fix the expectation, and say so in your report). Do not blanket-flip fixtures to `Worklog` to get green.

- [ ] **Step 5: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/domain/src/rules/reattribution.rs
git commit -F - <<'EOF'
Gate the rebuild on slot provenance, not only on closedness

Plan 1 classified every historical slot as worklog- or hand-derived and
nothing read the column. Until it does, a rebuild deletes hand-made time,
so the classification was inert and the guard it exists for was open.
EOF
```

If Step 4 required application-side fixture or expectation changes, stage those files in this same commit — they are the same change.

---

## Task 2: Close the untested slot `UPDATE` path

**Files:**
- Modify: `backend/crates/infrastructure/src/database/activity_repo.rs` (test module only)

**Interfaces:**
- Consumes: `ActivitySlot::from_worklog(...)`, `ActivitySlotRepository::{save, update, find_by_id}`.
- Produces: nothing new — this task only pins existing behaviour.

**Why now:** plan 1's whole-branch review flagged this as "the one with teeth" and said it must block plan 2's rebuild wiring rather than be deferred again. `activity_repo.rs`'s `UPDATE` writes `session_id` and `source`, and no test covers that path. All current callers read-then-write, so nothing is broken today — but from Task 4 onward the rebuild path exists, and a future caller reconstructing an `ActivitySlot` from parts would silently flip `Worklog` → `Manual`, leaving a slot the rebuild will no longer replace: a duplicate half-day, which is the exact failure this plan exists to prevent.

- [ ] **Step 1: Write the failing test**

Append to the test module in `backend/crates/infrastructure/src/database/activity_repo.rs`:

```rust
    /// The `UPDATE` path writes `source` and `session_id` like the `INSERT` does.
    /// Nothing exercises it today because every caller reads the slot first, mutates
    /// it and writes it back — so a regression here would be invisible until a
    /// rebuild refused to replace a slot it had itself written.
    #[tokio::test]
    async fn update_preserves_a_slots_provenance_and_author() {
        let pool = setup().await;
        let repo = SqliteActivitySlotRepository::new(pool);
        let date = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        let start = DateTime::parse_from_rfc3339("2026-08-04T09:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);

        let slot = ActivitySlot::from_worklog(
            test_user_id(),
            existing_task_id(),
            Some("sess-update".into()),
            start,
            start + chrono::Duration::hours(2),
            HalfDay::Morning,
            date,
            start,
        );
        repo.save(&slot).await.unwrap();

        // Read-mutate-write, the shape every real caller uses.
        let mut round_tripped = repo.find_by_id(slot.id).await.unwrap().unwrap();
        round_tripped.end_time = Some(start + chrono::Duration::hours(3));
        repo.update(&round_tripped).await.unwrap();

        let after = repo.find_by_id(slot.id).await.unwrap().unwrap();
        assert_eq!(after.end_time, Some(start + chrono::Duration::hours(3)));
        assert_eq!(
            after.source,
            SlotSource::Worklog,
            "an update must not downgrade the projection's own slot to Manual"
        );
        assert_eq!(after.session_id.as_deref(), Some("sess-update"));
    }
```

Use the file's existing `setup()`, `test_user_id()` and `existing_task_id()` helpers — plan 1's Task 6 added them. If `existing_task_id` does not exist, read the file's other tests and reuse whatever they seed; a slot's `task_id` carries a foreign key, and `tasks.source` has a `CHECK` allowing only `jira`, `excel`, `obsidian`, `personal`, `outlook`.

- [ ] **Step 2: Run to verify it fails against a broken UPDATE**

The test should PASS immediately — the `UPDATE` is already correct. That makes it a regression guard, not a bug fix, so **prove it can fail**: temporarily remove `source = ?` from the `UPDATE` statement's SET list, run the test, watch it go red on the `SlotSource::Worklog` assertion, then restore the statement.

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p infrastructure update_preserves_a_slots_provenance
```

Record both the red (with the clause removed) and the green (restored) in your report. A guard nobody has seen fail is a guard nobody should trust.

- [ ] **Step 3: Run the whole suite**

```bash
cargo test --workspace
```

- [ ] **Step 4: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/infrastructure/src/database/activity_repo.rs
git commit -F - <<'EOF'
Pin provenance across the slot UPDATE path

Every current caller reads the slot before writing it back, so a dropped
source column here would stay invisible until a rebuild declined to replace
a slot it had written itself — a duplicated half-day. Verified red by
removing the clause.
EOF
```

---

## Task 3: Extract the shared projection rebuild

**Files:**
- Modify: `backend/crates/application/src/use_cases/worklog.rs`
- Modify: `backend/crates/application/src/use_cases/reattribution.rs:481-545` (its `rebuildable_slots` and `project_slots` become calls into the new primitive)

**Interfaces:**
- Consumes: `domain::rules::reattribution::{is_rebuildable, AffectedHalfDay}`, `domain::rules::worklog_time::{derive_time_blocks, MIN_BLOCK_MINUTES}`, `ActivitySlotRepository`, `WorklogRepository`, `worklog::user_timezone`.
- Produces, in `application::use_cases::worklog`:
  - `pub struct RebuildPlan { pub task_id: TaskId, pub delete: Vec<ActivitySlot>, pub write: Vec<ActivitySlot> }`
  - `pub async fn plan_task_projection(activity_repo: &dyn ActivitySlotRepository, worklog_repo: &dyn WorklogRepository, user_id: UserId, task_id: TaskId, half_days: &[AffectedHalfDay], tz: chrono_tz::Tz, now: DateTime<Utc>) -> Result<RebuildPlan, AppError>`
  - `pub async fn apply_task_projection(activity_repo: &dyn ActivitySlotRepository, plan: &RebuildPlan) -> Result<(), AppError>`

**Why two functions and not one with a flag:** reattribution previews before it writes (`confirm`), and the flush always writes. A `plan` / `apply` split gives the preview an honest read-only path and gives both callers the same arithmetic, without a boolean parameter that readers have to decode at every call site.

- [ ] **Step 1: Write the failing tests**

Append to the test module in `backend/crates/application/src/use_cases/worklog.rs`. Reuse the file's existing `FakeActivityRepo`, fake worklog repo and fake config repo.

```rust
    fn half_day(date: &str, hd: HalfDay) -> AffectedHalfDay {
        AffectedHalfDay { date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(), half_day: hd }
    }

    /// The plan deletes only what the projection owns and rewrites from the entries.
    #[tokio::test]
    async fn the_plan_replaces_a_worklog_slot_and_spares_a_manual_one() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(7, 30)]).await;

        let mine = ActivitySlot::from_worklog(
            user_id(), task_id(), None, t(7, 0), t(7, 30),
            HalfDay::Morning, date(2026, 8, 4), t(9, 0),
        );
        let hand_made = ActivitySlot::manual(
            user_id(), Some(task_id()), t(10, 0), Some(t(11, 0)),
            HalfDay::Morning, date(2026, 8, 4), t(11, 0),
        );
        activity.save(&mine).await.unwrap();
        activity.save(&hand_made).await.unwrap();

        let tz = user_timezone(&config, user_id()).await.unwrap();
        let plan = plan_task_projection(
            &activity, &worklog, user_id(), task_id(),
            &[half_day("2026-08-04", HalfDay::Morning)], tz, t(12, 0),
        ).await.unwrap();

        let deleted: Vec<_> = plan.delete.iter().map(|s| s.id).collect();
        assert!(deleted.contains(&mine.id), "the projection's own slot is replaced");
        assert!(!deleted.contains(&hand_made.id), "a hand-made slot is never deleted");
        assert_eq!(plan.write.len(), 1, "the two entries are one stretch of work");
        assert_eq!(plan.write[0].source, SlotSource::Worklog);
    }

    /// Applying twice leaves the same slots — the property the flush needs.
    #[tokio::test]
    async fn applying_the_plan_twice_is_idempotent() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(7, 30)]).await;
        let tz = user_timezone(&config, user_id()).await.unwrap();
        let units = [half_day("2026-08-04", HalfDay::Morning)];

        for _ in 0..2 {
            let plan = plan_task_projection(
                &activity, &worklog, user_id(), task_id(), &units, tz, t(12, 0),
            ).await.unwrap();
            apply_task_projection(&activity, &plan).await.unwrap();
        }

        let slots = activity
            .find_by_user_and_date(user_id(), date(2026, 8, 4))
            .await
            .unwrap();
        assert_eq!(slots.len(), 1, "the second apply replaced rather than appended");
    }

    /// One task's rebuild never reads or writes another task's slots.
    #[tokio::test]
    async fn the_plan_leaves_another_tasks_slots_alone() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(7, 30)]).await;
        let other = Uuid::new_v4();
        let theirs = ActivitySlot::from_worklog(
            user_id(), other, None, t(7, 0), t(7, 30),
            HalfDay::Morning, date(2026, 8, 4), t(9, 0),
        );
        activity.save(&theirs).await.unwrap();

        let tz = user_timezone(&config, user_id()).await.unwrap();
        let plan = plan_task_projection(
            &activity, &worklog, user_id(), task_id(),
            &[half_day("2026-08-04", HalfDay::Morning)], tz, t(12, 0),
        ).await.unwrap();

        assert!(plan.delete.iter().all(|s| s.task_id == Some(task_id())));
        assert!(plan.write.iter().all(|s| s.task_id == Some(task_id())));
    }

    /// A half-day the caller did not name is not touched, even for the same task.
    #[tokio::test]
    async fn the_plan_is_scoped_to_the_named_half_days() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(14, 0)]).await;
        let morning_slot = ActivitySlot::from_worklog(
            user_id(), task_id(), None, t(14, 0), t(14, 1),
            HalfDay::Afternoon, date(2026, 8, 4), t(15, 0),
        );
        activity.save(&morning_slot).await.unwrap();

        let tz = user_timezone(&config, user_id()).await.unwrap();
        let plan = plan_task_projection(
            &activity, &worklog, user_id(), task_id(),
            &[half_day("2026-08-04", HalfDay::Morning)], tz, t(16, 0),
        ).await.unwrap();

        assert!(
            plan.delete.is_empty(),
            "the afternoon slot is outside the named half-day"
        );
        assert!(plan.write.iter().all(|s| s.half_day == HalfDay::Morning));
    }
```

`fakes_with_entries`, `user_id`, `task_id`, `t` and `date` are the helpers plan 1's Task 7 added to `use_cases/slot_classification.rs`. If `worklog.rs`'s test module does not have equivalents, copy them in — the two modules already keep parallel fakes, which is this file's established convention.

- [ ] **Step 2: Run to verify they fail**

```bash
cd ~/appfactory/aggregated_plan/backend
cargo test -p application plan_task_projection
```

Expected: FAIL to compile — `cannot find function plan_task_projection`.

- [ ] **Step 3: Write the primitive**

Add to `backend/crates/application/src/use_cases/worklog.rs`:

```rust
/// What a rebuild of one task's projection over some half-days would do.
///
/// Separated from the applying so the reattribution preview and the flush share one
/// piece of arithmetic: a preview that computed its figures differently from the
/// write would report numbers nobody could reproduce.
pub struct RebuildPlan {
    pub task_id: TaskId,
    /// Slots the projection owns in these half-days, to be dropped first. Dropping
    /// them is what makes the rewrite exact: without it, a half-day that already
    /// carried a slot would keep it *and* gain the rebuilt one, and the same morning
    /// would be billed twice.
    pub delete: Vec<ActivitySlot>,
    /// What this task's entries in these half-days say the time was.
    pub write: Vec<ActivitySlot>,
}

/// Compute the rebuild of `task_id`'s projection over `half_days`. Reads only.
///
/// `half_days` bounds the blast radius; it never decides truth. Truth is every entry
/// of this task falling in those half-days — which is why widening the caller's
/// window is harmless, and why an entry logged with a backdated `logged_at` is
/// picked up rather than skipped by a watermark comparison.
pub async fn plan_task_projection(
    activity_repo: &dyn ActivitySlotRepository,
    worklog_repo: &dyn WorklogRepository,
    user_id: UserId,
    task_id: TaskId,
    half_days: &[AffectedHalfDay],
    tz: chrono_tz::Tz,
    now: DateTime<Utc>,
) -> Result<RebuildPlan, AppError> {
    let mut delete = Vec::new();
    let mut seen_dates: Vec<NaiveDate> = Vec::new();
    for unit in half_days {
        if seen_dates.contains(&unit.date) {
            continue;
        }
        seen_dates.push(unit.date);
        for slot in activity_repo.find_by_user_and_date(user_id, unit.date).await? {
            let mine = slot.task_id == Some(task_id);
            let named = half_days
                .iter()
                .any(|u| u.date == slot.date && u.half_day == slot.half_day);
            if mine && named && is_rebuildable(&slot) {
                delete.push(slot);
            }
        }
    }

    let filter = WorklogFilter {
        task_ids: Some(vec![task_id]),
        from: None,
        to: None,
        limit: WORKLOG_FILTER_MAX_LIMIT,
        offset: 0,
    };
    let entries = worklog_repo.list(user_id, &filter).await?;
    refuse_a_truncated_page(entries.len())?;

    let mut local_to_utc: std::collections::HashMap<chrono::NaiveDateTime, DateTime<Utc>> =
        std::collections::HashMap::new();
    let mut local_times = Vec::new();
    for entry in &entries {
        let local = tz.from_utc_datetime(&entry.logged_at.naive_utc()).naive_local();
        let in_scope = half_days.iter().any(|u| {
            u.date == local.date() && u.half_day == half_day_of(local.time().hour())
        });
        if !in_scope {
            continue;
        }
        local_to_utc.insert(local, entry.logged_at);
        local_times.push(local);
    }

    let mut write = Vec::new();
    for block in derive_time_blocks(&local_times) {
        // Both ends came out of `local_times`, so both are in the map. A miss would
        // mean the projection invented a timestamp, and writing a slot from an
        // invented instant is worse than writing none.
        let (Some(start), Some(raw_end)) =
            (local_to_utc.get(&block.start), local_to_utc.get(&block.end))
        else {
            continue;
        };
        let mut end = *raw_end;
        if end <= *start {
            end = *start + chrono::Duration::minutes(MIN_BLOCK_MINUTES);
        }
        write.push(ActivitySlot::from_worklog(
            user_id, task_id, None, *start, end, block.half_day, block.date, now,
        ));
    }

    Ok(RebuildPlan { task_id, delete, write })
}

/// Persist a plan: drop the stale projection, then write the fresh one.
///
/// Deletion precedes writing on purpose. The reverse order would leave a window in
/// which the half-day carries both, and a reader landing there sees doubled hours.
pub async fn apply_task_projection(
    activity_repo: &dyn ActivitySlotRepository,
    plan: &RebuildPlan,
) -> Result<(), AppError> {
    for slot in &plan.delete {
        activity_repo.delete(slot.id).await?;
    }
    for slot in &plan.write {
        activity_repo.save(slot).await?;
    }
    Ok(())
}
```

Imports to add at the top of the file: `use domain::rules::reattribution::{is_rebuildable, AffectedHalfDay};`, `use domain::rules::workload::half_day_of;`, `use chrono::{NaiveDate, Timelike};`. `refuse_a_truncated_page` is `pub(crate)` in `use_cases/reattribution.rs` (plan 1 widened it) — import it, do not copy it.

- [ ] **Step 4: Run to verify they pass**

```bash
cargo test -p application plan_task_projection applying_the_plan
```

Expected: PASS (4 tests).

- [ ] **Step 5: Rewire reattribution onto the primitive**

Replace `reattribution.rs`'s `rebuildable_slots` and `project_slots` with calls to `plan_task_projection` / `apply_task_projection`, one call per task in `{source, destination}`. Its `ReattributionOutcome` figures come from the returned `delete` / `write` vectors — `slot_hours` over `plan.delete` is the "before", over `plan.write` the "after".

**Reattribution's behaviour must not change.** Its test module is large and was written against the inline implementation; it is the proof. Run it in isolation and expect every test green:

```bash
cargo test -p application reattribut
```

If a test goes red, the extraction changed semantics — find the difference and preserve the old behaviour rather than adjusting the test. The one exception is any test that asserted a `Manual` slot gets rebuilt: Task 1 already changed that deliberately.

- [ ] **Step 6: Run the whole suite**

```bash
cargo test --workspace
```

- [ ] **Step 7: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/application/src/use_cases/worklog.rs \
        backend/crates/application/src/use_cases/reattribution.rs
git commit -F - <<'EOF'
Extract the projection rebuild the reattribution already performed

The repair verb had the only correct implementation of "drop this task's
stale projection over these half-days and rewrite it from the entries".
The flush needs the same thing, and two copies of that arithmetic would
drift. Split into plan/apply so the preview and the write share it.
EOF
```

---

## Task 4: Rewire the flush onto the rebuild

**Files:**
- Modify: `backend/crates/application/src/use_cases/worklog.rs` (`materialize_worklog_time`)

**Interfaces:**
- Consumes: `plan_task_projection`, `apply_task_projection`, `RebuildPlan` (Task 3).
- Produces: `materialize_worklog_time` keeping its signature `(worklog_repo, activity_repo, config_repo, user_id, task_id, from, now) -> Result<FlushOutcome, AppError>`, with `from` demoted from watermark to half-day selector.

- [ ] **Step 1: Write the failing tests**

```rust
    /// Two flushes over the same window produce one set of slots. This is the
    /// property the old append-with-a-watermark implementation could not have.
    #[tokio::test]
    async fn flushing_twice_does_not_double_the_half_day() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(7, 30)]).await;

        for _ in 0..2 {
            materialize_worklog_time(
                &worklog, &activity, &config, user_id(), task_id(), t(6, 0), t(12, 0),
            ).await.unwrap();
        }

        let slots = activity.find_by_user_and_date(user_id(), date(2026, 8, 4)).await.unwrap();
        assert_eq!(slots.len(), 1);
    }

    /// An entry logged with a past `logged_at` — under any watermark the caller might
    /// pass — still reaches the projection, because membership is by half-day.
    #[tokio::test]
    async fn a_backdated_entry_is_still_materialized() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(7, 30)]).await;

        materialize_worklog_time(
            &worklog, &activity, &config, user_id(), task_id(),
            t(7, 15), // a window that starts *after* the first entry
            t(12, 0),
        ).await.unwrap();

        let slots = activity.find_by_user_and_date(user_id(), date(2026, 8, 4)).await.unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].start_time, t(7, 0), "the earlier entry set the boundary");
    }

    /// Flushing one task neither reads nor writes another task's slots — the whole
    /// point of the plan: two sessions on two tasks stop losing each other's hours.
    #[tokio::test]
    async fn flushing_one_task_leaves_another_intact() {
        let (activity, worklog, config) = fakes_with_entries(&[t(7, 0), t(7, 30)]).await;
        let other = Uuid::new_v4();
        let theirs = ActivitySlot::from_worklog(
            user_id(), other, None, t(8, 0), t(8, 30),
            HalfDay::Morning, date(2026, 8, 4), t(9, 0),
        );
        activity.save(&theirs).await.unwrap();

        materialize_worklog_time(
            &worklog, &activity, &config, user_id(), task_id(), t(6, 0), t(12, 0),
        ).await.unwrap();

        let still_there = activity.find_by_id(theirs.id).await.unwrap();
        assert!(still_there.is_some(), "another task's slot survives our flush");
    }
```

- [ ] **Step 2: Run to verify they fail**

```bash
cargo test -p application flushing_twice a_backdated_entry flushing_one_task
```

Expected: `flushing_twice_does_not_double_the_half_day` FAILS with 2 slots — the current implementation appends.

- [ ] **Step 3: Rewrite `materialize_worklog_time`**

```rust
/// Materialize the worklog time of `task_id` into closed activity slots.
///
/// `from` is a **selector, not a watermark**: it picks which local half-days to
/// rebuild, and every entry of this task in those half-days then decides what the
/// slots are. That inversion is the point of the whole plan. The old
/// implementation appended slots for entries newer than a single global watermark,
/// so flushing task B advanced the mark for task A too and A's entries were never
/// materialized; and re-running it duplicated whatever it had already written.
///
/// Widening the window is therefore free, and re-running is free: the operation is
/// idempotent because it derives everything from the entries and owns only the slots
/// it wrote (`SlotSource::Worklog`).
pub async fn materialize_worklog_time(
    worklog_repo: &dyn WorklogRepository,
    activity_repo: &dyn ActivitySlotRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    task_id: TaskId,
    from: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<FlushOutcome, AppError> {
    let tz = user_timezone(config_repo, user_id).await?;

    let filter = WorklogFilter {
        task_ids: Some(vec![task_id]),
        from: Some(from),
        to: Some(now),
        limit: WORKLOG_FILTER_MAX_LIMIT,
        offset: 0,
    };
    let entries = worklog_repo.list(user_id, &filter).await?;
    refuse_a_truncated_page(entries.len())?;

    // The window's only job: which half-days did this task touch?
    let mut half_days: Vec<AffectedHalfDay> = Vec::new();
    for entry in &entries {
        let local = tz.from_utc_datetime(&entry.logged_at.naive_utc()).naive_local();
        let unit = AffectedHalfDay {
            date: local.date(),
            half_day: half_day_of(local.time().hour()),
        };
        if !half_days.iter().any(|u| u.date == unit.date && u.half_day == unit.half_day) {
            half_days.push(unit);
        }
    }

    let mut written = 0u32;
    for unit in &half_days {
        let plan = plan_task_projection(
            activity_repo, worklog_repo, user_id, task_id,
            std::slice::from_ref(unit), tz, now,
        )
        .await?;
        written += plan.write.len() as u32;
        apply_task_projection(activity_repo, &plan).await?;
    }

    Ok(FlushOutcome { slots_written: written, active_since: now })
}
```

Rebuilding one half-day at a time rather than all at once keeps a partial failure to a single half-day, and each is independently correct.

- [ ] **Step 4: Run to verify they pass, then everything**

```bash
cargo test -p application
cargo test --workspace
```

Expected: PASS. Any pre-existing `materialize_worklog_time` test that asserted append-semantics — a second call adding slots — was asserting the defect; change it deliberately and say so in your report.

- [ ] **Step 5: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/application/src/use_cases/worklog.rs
git commit -F - <<'EOF'
Make the flush rebuild the projection instead of appending to it

The window now selects which half-days to rebuild; the entries in them
decide what the slots are. Re-running is a no-op and a backdated entry is
picked up, neither of which the watermark comparison could manage.
EOF
```

---

## Task 5: Give the per-session window its first caller

**Files:**
- Modify: `backend/crates/api/src/graphql/mutation.rs` (`flush_worklog_time`)
- Modify: `backend/crates/cli/graphql/flush_worklog_time.graphql`
- Modify: `backend/crates/api/src/graphql/tests.rs`

**Interfaces:**
- Consumes: `SessionRepository::{find_by_id, set_last_flush}` and `Session::flush_window_start()` — all shipped by plan 1 with **no caller**.
- Produces: `flushWorklogTime(taskId: ID!, sessionId: String): FlushResultGql!` reading and advancing the session's own window when a `sessionId` is given, and the human's `aplan.active_since` otherwise.

- [ ] **Step 1: Write the failing tests**

Append to `backend/crates/api/src/graphql/tests.rs`:

```rust
#[tokio::test]
async fn flush_advances_the_sessions_own_window_not_the_global_key() {
    let (schema, task_id) = schema_with_one_task().await;
    schema
        .execute(format!(
            r#"mutation {{ bindSession(sessionId: "s1", taskId: "{task_id}") {{ session {{ id }} }} }}"#
        ))
        .await;
    schema
        .execute(format!(
            r#"mutation {{ addWorklogEntry(taskId: "{task_id}", body: "x", sessionId: "s1") {{ id }} }}"#
        ))
        .await;

    let flushed = schema
        .execute(format!(
            r#"mutation {{ flushWorklogTime(taskId: "{task_id}", sessionId: "s1") {{ slotsWritten }} }}"#
        ))
        .await;
    assert!(flushed.errors.is_empty(), "{:?}", flushed.errors);

    let read = schema.execute(r#"{ claudeSession(id: "s1") { lastFlushAt } }"#).await;
    assert!(
        !read.data.into_json().unwrap()["claudeSession"]["lastFlushAt"].is_null(),
        "the session's own window must have advanced"
    );
}

#[tokio::test]
async fn flushing_one_sessions_task_does_not_move_another_sessions_window() {
    let (schema, task_id) = schema_with_one_task().await;
    for id in ["s1", "s2"] {
        schema
            .execute(format!(
                r#"mutation {{ bindSession(sessionId: "{id}", taskId: "{task_id}") {{ session {{ id }} }} }}"#
            ))
            .await;
    }
    schema
        .execute(format!(
            r#"mutation {{ flushWorklogTime(taskId: "{task_id}", sessionId: "s1") {{ slotsWritten }} }}"#
        ))
        .await;

    let other = schema.execute(r#"{ claudeSession(id: "s2") { lastFlushAt } }"#).await;
    assert!(
        other.data.into_json().unwrap()["claudeSession"]["lastFlushAt"].is_null(),
        "s2's window is not s1's to advance — this is the shared-watermark bug"
    );
}

#[tokio::test]
async fn a_flush_without_a_session_still_uses_the_humans_pointer() {
    let (schema, task_id) = schema_with_one_task().await;
    let flushed = schema
        .execute(format!(
            r#"mutation {{ flushWorklogTime(taskId: "{task_id}") {{ slotsWritten activeSince }} }}"#
        ))
        .await;
    assert!(flushed.errors.is_empty(), "{:?}", flushed.errors);
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cargo test -p api flush_advances_the_sessions_own_window
```

Expected: FAIL — `Unknown argument "sessionId" on field "flushWorklogTime"`.

- [ ] **Step 3: Implement the resolver change**

In `mutation.rs`'s `flush_worklog_time`, add a `session_id: Option<String>` parameter, then choose the window and where to advance it:

```rust
        // A session flushes against its own window; the human keeps the global key.
        // Sharing one key across tasks is what made flushing task B advance the mark
        // for task A, so the pair must never be crossed.
        let session = match &session_id {
            Some(sid) => {
                let sessions = ctx.data::<Arc<dyn SessionRepository>>()?;
                sessions
                    .find_by_id(sid, user_id)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?
            }
            None => None,
        };

        let from = match &session {
            Some(s) => s.flush_window_start(),
            None => config_repo
                .get(user_id, "aplan.active_since")
                .await
                .ok()
                .flatten()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0)
                    .expect("epoch is a valid timestamp")),
        };
```

and after the materialization:

```rust
        match (&session, &session_id) {
            (Some(_), Some(sid)) => {
                let sessions = ctx.data::<Arc<dyn SessionRepository>>()?;
                sessions
                    .set_last_flush(sid, user_id, outcome.active_since)
                    .await
                    .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            }
            // No session, or an id naming no row: the human's pointer answered, so
            // the human's key is the one to advance.
            _ => {
                configuration::set_config(
                    config_repo.as_ref(),
                    user_id,
                    "aplan.active_since",
                    &outcome.active_since.to_rfc3339(),
                )
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
            }
        }
```

Then add `sessionId` to `backend/crates/cli/graphql/flush_worklog_time.graphql`:

```graphql
mutation FlushWorklogTime($taskId: ID!, $sessionId: String) {
  flushWorklogTime(taskId: $taskId, sessionId: $sessionId) {
    slotsWritten
    activeSince
  }
}
```

and regenerate the CLI's schema copy **against a scratch database**, never the real one — `create_sqlite_pool` runs migrations before the `ExportSchema` early return, so pointing it at the live file would migrate the user's database:

```bash
cd ~/appfactory/aggregated_plan/backend
DATABASE_URL=sqlite:/tmp/claude-1000/-home-mbt/f17e71e0-7cc2-4f9a-a087-7658e467f8af/scratchpad/schema-export-p2.db?mode=rwc \
  cargo run -p api -- export-schema > crates/cli/graphql/schema.graphql
sqlite3 aggregated_plan.db "SELECT MAX(version) FROM _sqlx_migrations;"   # must print 14
```

- [ ] **Step 4: Run to verify they pass**

```bash
cargo test -p api
cargo test --workspace
```

- [ ] **Step 5: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add backend/crates/api backend/crates/cli/graphql
git commit -F - <<'EOF'
Flush against the session's own window instead of one global key

sessions.last_flush_at and set_last_flush shipped with no caller in plan 1;
this is it. A session no longer advances a mark shared with every other
task, which is the defect that lost time whenever two tasks interleaved.
EOF
```

---

## Task 6: Make `session bind` flush with its own window

**Files:**
- Modify: `backend/crates/cli/src/commands.rs` (`flush_task`)
- Modify: `backend/crates/cli/src/session_cmd.rs:155-170`
- Modify: `backend/crates/cli/tests/integration.rs`

**Interfaces:**
- Consumes: `flushWorklogTime(taskId, sessionId)` (Task 5).
- Produces: `flush_task(client: &Client, task_id: &str, session: Option<&str>)`.

**Why this is the gate for plan 3:** `session bind` currently flushes the previous task through the global window and lets the server advance that global key, and its comment claims "the same call `aplan start` makes: time behaviour is unchanged" — which is false, because `start` also re-arms the window it consumed. Hand-typed, that is a latent inherited flaw. Once plan 3's hook calls `bind` on every session start, it fires many times a day.

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn session_bind_flushes_the_previous_task_against_its_own_session() {
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("BindSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "bindSession": {
                "session": { "id": "s1", "taskId": "00000000-0000-0000-0000-000000000001",
                             "mode": "TRACKING", "label": null, "endedAt": null },
                "previousTaskId": "00000000-0000-0000-0000-0000000000bb" } }
        })))
        .mount(&server)
        .await;
    // The flush must carry the session id. A body matcher that only accepts
    // "sessionId":"s1" makes an unscoped flush fail the test instead of passing quietly.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(wiremock::matchers::body_string_contains("\"sessionId\":\"s1\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": { "slotsWritten": 1, "activeSince": "2026-08-05T09:00:00+00:00" } }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "--session", "s1", "session", "bind",
               "00000000-0000-0000-0000-000000000001"])
        .assert()
        .success();
}
```

Every test goes through the shared `aplan()` builder, which removes `CLAUDE_CODE_SESSION_ID` by construction; `--session` is passed explicitly here.

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test -p cli session_bind_flushes_the_previous_task_against_its_own_session
```

Expected: FAIL — the flush request carries no `sessionId`, so the matcher never matches and the `expect(1)` is unmet.

- [ ] **Step 3: Thread the session through the flush**

Change `flush_task` in `commands.rs` to take `session: Option<&str>` and pass it as the mutation's `session_id`. Update both call sites — `start` passes `None` (it is the human's pointer by definition, and plan 3 is what changes that), `session_cmd`'s bind passes its own session id.

Then replace the false comment at `session_cmd.rs:155-170` with the truth:

```rust
                    // Flush the task this session is leaving, against *this session's*
                    // window — not the global one. `aplan start` flushes the human's
                    // pointer and re-arms the human's key; a session bind must do the
                    // same for its own, or it consumes a window it does not own and
                    // leaves the next flush of some other task looking at a mark that
                    // already moved. Above the `--json` return on purpose: that is the
                    // path the hooks use.
```

- [ ] **Step 4: Run to verify it passes, plus the env A/B**

```bash
cargo test -p cli
S=/tmp/claude-1000/-home-mbt/f17e71e0-7cc2-4f9a-a087-7658e467f8af/scratchpad
env -u CLAUDE_CODE_SESSION_ID cargo test -p cli > $S/ab-a.txt 2>&1; echo "A exit=$?"
CLAUDE_CODE_SESSION_ID=deadbeef cargo test -p cli > $S/ab-b.txt 2>&1; echo "B exit=$?"
rg -o '[0-9]+ passed; [0-9]+ failed' $S/ab-a.txt
rg -o '[0-9]+ passed; [0-9]+ failed' $S/ab-b.txt
```

Both must be identical. Put both outputs in your report.

- [ ] **Step 5: Run the whole suite and commit**

```bash
cd ~/appfactory/aggregated_plan/backend && cargo test --workspace
cd ~/appfactory/aggregated_plan
git add backend/crates/cli
git commit -F - <<'EOF'
Flush a session's previous task against that session's window

session bind consumed the global window and let the server advance it,
while its comment claimed the behaviour matched aplan start — which also
re-arms what it consumed. Harmless while binds are hand-typed; plan 3
makes the hooks call this on every session start.
EOF
```

---

## Task 7: Documentation

**Files:**
- Modify: `SPEC_TECHNIQUE.md` (§ 7.3, add 7.3.4)
- Modify: `docs/superpowers/specs/2026-08-04-aplan-session-scoped-worklog-design.md`

- [ ] **Step 1: Document the rebuild in `SPEC_TECHNIQUE.md` (French)**

Add § 7.3.4 covering: the window as a selector rather than a watermark; the rebuild scoped to (tâche, demi-journée); `source` as the deletion gate with `manual` and NULL protected; the shared `plan_task_projection` / `apply_task_projection` used by both the flush and the reattribution; the per-session window (`sessions.last_flush_at`) versus the human's `aplan.active_since`; and the three properties that follow — idempotence, backdate safety, and isolation between tasks.

- [ ] **Step 2: Mark the spec's plan-2 staging done**

In the design spec's "Implementation staging" section, mark item 2 complete with the commit range, and note that the flush is no longer watermark-based so the § "Flush becomes an idempotent rebuild" text now describes shipped behaviour rather than intent.

- [ ] **Step 3: Commit**

```bash
cd ~/appfactory/aggregated_plan
git add SPEC_TECHNIQUE.md docs/superpowers/specs/2026-08-04-aplan-session-scoped-worklog-design.md
git commit -m "Document the idempotent rebuild and the per-session flush window"
```

---

## Self-review notes

**Spec coverage.** § "Flush becomes an idempotent rebuild" → Tasks 3, 4, 5. Its three claimed properties each get a named test: order-independence and idempotence (Task 4 `flushing_twice_...`), backdate safety (`a_backdated_entry_...`), isolation (`flushing_one_task_leaves_another_intact`). § "Reattribution alignment" → Tasks 1 and 3; the spec says reattribution restricts itself to `source='worklog'`, which Task 1 delivers through `is_rebuildable`. § "Classifying pre-014 slots" → already shipped in plan 1, and Task 1 is what finally makes it load-bearing.

**Carried in from plan 1's review, not invented here.** Task 2 is ledger item 11, which the whole-branch review said must block this plan's rebuild wiring. Task 6 is the downgrade the Critical hunt identified as the hard gate before plan 3.

**Deliberately out of scope.** Overlap detection and its display, the hook rewrites, `aplan start` binding the session, and the 12-hour idle reaper are all plan 3. `resolve_session_target` stays dead — the CLI owns that refusal and plan 1's review accepted the duplication.

**Known risk.** Task 3's extraction touches the one module that rewrites billing-relevant history, and its test module is the only proof the semantics survived. If any reattribution test goes red for a reason other than Task 1's deliberate provenance change, stop and escalate rather than adjusting the test — that file's header documents invariants a reader cannot re-derive from the code alone.
