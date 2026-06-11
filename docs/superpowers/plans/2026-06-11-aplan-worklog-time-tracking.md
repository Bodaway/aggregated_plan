# Claude Worklog Logging + Self-Closing Time Blocks — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Claude log progress to the timestamped worklog (not the `notes` field) and record time as closed, per-day/per-half-day activity slots derived from worklog timestamps — never leaving an open slot.

**Architecture:** Worklog entries are the durable source of truth. A pure domain function groups entry timestamps (in the user's local timezone) into per-day/per-half-day blocks; an application use case materializes them as closed `ActivitySlot`s at lifecycle boundaries (session-end / stop / done / task-switch). A config-backed `aplan.active_task_id` + `aplan.active_since` pointer replaces the open activity slot as the session link.

**Tech Stack:** Rust (DDD workspace: domain → application → api), `async-graphql`, `sqlx`/SQLite, `chrono` + `chrono-tz`, `graphql_client` (CLI), `clap` (CLI), bash hooks.

**Reference spec:** `docs/superpowers/specs/2026-06-11-aplan-worklog-time-tracking-design.md`

**Working directory for all backend commands:** `backend/`

---

## File Structure

| File | Responsibility | Action |
|---|---|---|
| `backend/crates/domain/src/rules/worklog_time.rs` | Pure: group local timestamps → per-day/half-day blocks | Create |
| `backend/crates/domain/src/rules/mod.rs` | Register the new rules module | Modify |
| `backend/crates/application/Cargo.toml` | Add `chrono-tz` dependency | Modify |
| `backend/crates/application/src/use_cases/worklog.rs` | `materialize_worklog_time` use case + tests | Modify |
| `backend/crates/api/src/graphql/mutation.rs` | `flush_worklog_time` resolver | Modify |
| `backend/crates/api/src/graphql/types/worklog_entry.rs` | `FlushResultGql` output type | Modify |
| `backend/crates/cli/graphql/schema.graphql` | Regenerated SDL (adds worklog ops) | Regenerate |
| `backend/crates/cli/graphql/add_worklog_entry.graphql` | `aplan log` operation | Create |
| `backend/crates/cli/graphql/flush_worklog_time.graphql` | flush operation | Create |
| `backend/crates/cli/src/queries.rs` | Register new GraphQL ops | Modify |
| `backend/crates/cli/src/cli.rs` | `Log` subcommand | Modify |
| `backend/crates/cli/src/commands.rs` | `log`; repoint `start`/`stop`/`done`/`current` | Modify |
| `backend/crates/cli/src/main.rs` | Dispatch `Log` | Modify |
| `backend/crates/cli/tests/integration.rs` | CLI flow tests | Modify |
| `~/.claude/hooks/aplan-session-end.sh` | Flush on session end | Create |
| `~/.claude/hooks/aplan-session-start.sh` | Read pointer; `note`→`log` | Modify |
| `~/.claude/settings.json` | Register SessionEnd hook | Modify |
| `~/.claude/skills/aplan/SKILL.md` | `log` vs `note`; new lifecycle | Modify |
| `SPEC_FONCTIONNELLE.md`, `SPEC_TECHNIQUE.md` | Document behavior | Modify |

**Config keys introduced:** `aplan.active_task_id` (UUID string), `aplan.active_since` (RFC3339 UTC), `aplan.timezone` (IANA, default `Europe/Paris`).

---

## Phase 1 — Domain: time-block derivation

### Task 1: `derive_time_blocks` pure function

**Files:**
- Create: `backend/crates/domain/src/rules/worklog_time.rs`
- Modify: `backend/crates/domain/src/rules/mod.rs`

- [ ] **Step 1: Register the module**

In `backend/crates/domain/src/rules/mod.rs`, add this line alongside the other `pub mod` declarations:

```rust
pub mod worklog_time;
```

- [ ] **Step 2: Write the failing tests**

Create `backend/crates/domain/src/rules/worklog_time.rs` with ONLY the type, signature stub, and tests:

```rust
use chrono::{NaiveDate, NaiveDateTime};

use crate::types::common::HalfDay;

/// A derived block of worked time, expressed in the user's LOCAL wall-clock.
/// `start`/`end` are local naive datetimes; the caller maps them back to UTC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBlock {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub date: NaiveDate,
    pub half_day: HalfDay,
}

/// Group LOCAL worklog timestamps into one block per (calendar day, half-day).
/// Morning = hour < 13, Afternoon = hour >= 13 (matches `workload::half_day_of`).
/// A group's block runs from its earliest to its latest timestamp. For a group
/// with a single timestamp, `start == end` (the caller gives it a minimal
/// non-zero duration when persisting). Input order does not matter; output is
/// sorted by (date, half_day morning-before-afternoon, start).
pub fn derive_time_blocks(local_times: &[NaiveDateTime]) -> Vec<LocalBlock> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap()
    }

    #[test]
    fn empty_input_yields_no_blocks() {
        assert!(derive_time_blocks(&[]).is_empty());
    }

    #[test]
    fn single_morning_day_one_block() {
        let times = vec![dt(2026, 6, 8, 10, 0), dt(2026, 6, 8, 11, 30)];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start, dt(2026, 6, 8, 10, 0));
        assert_eq!(blocks[0].end, dt(2026, 6, 8, 11, 30));
        assert_eq!(blocks[0].date, NaiveDate::from_ymd_opt(2026, 6, 8).unwrap());
        assert_eq!(blocks[0].half_day, HalfDay::Morning);
    }

    #[test]
    fn crossing_noon_splits_into_two_blocks() {
        // 11:00 (AM) and 14:00 (PM) same day => two blocks
        let times = vec![dt(2026, 6, 8, 11, 0), dt(2026, 6, 8, 14, 0)];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].half_day, HalfDay::Morning);
        assert_eq!(blocks[0].start, dt(2026, 6, 8, 11, 0));
        assert_eq!(blocks[0].end, dt(2026, 6, 8, 11, 0));
        assert_eq!(blocks[1].half_day, HalfDay::Afternoon);
        assert_eq!(blocks[1].start, dt(2026, 6, 8, 14, 0));
    }

    #[test]
    fn multi_day_only_days_with_entries() {
        // Mon two entries, Wed two entries, Tue nothing
        let times = vec![
            dt(2026, 6, 8, 14, 2),
            dt(2026, 6, 8, 15, 30),
            dt(2026, 6, 10, 9, 10),
            dt(2026, 6, 10, 11, 45),
        ];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].date, NaiveDate::from_ymd_opt(2026, 6, 8).unwrap());
        assert_eq!(blocks[0].half_day, HalfDay::Afternoon);
        assert_eq!(blocks[1].date, NaiveDate::from_ymd_opt(2026, 6, 10).unwrap());
        assert_eq!(blocks[1].half_day, HalfDay::Morning);
    }

    #[test]
    fn single_entry_block_has_equal_start_end() {
        let times = vec![dt(2026, 6, 8, 9, 0)];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start, blocks[0].end);
    }

    #[test]
    fn unsorted_input_is_handled() {
        let times = vec![dt(2026, 6, 8, 11, 30), dt(2026, 6, 8, 10, 0)];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks[0].start, dt(2026, 6, 8, 10, 0));
        assert_eq!(blocks[0].end, dt(2026, 6, 8, 11, 30));
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd backend && cargo test -p domain worklog_time`
Expected: compile/panic failure at `todo!()` (tests run but panic).

- [ ] **Step 4: Implement `derive_time_blocks`**

Replace the `todo!()` body:

```rust
pub fn derive_time_blocks(local_times: &[NaiveDateTime]) -> Vec<LocalBlock> {
    use crate::rules::workload::half_day_of;
    use std::collections::BTreeMap;

    // Key sorts naturally: date asc, then half-day flag (false=morning first).
    let mut groups: BTreeMap<(NaiveDate, bool), (NaiveDateTime, NaiveDateTime)> = BTreeMap::new();

    for &t in local_times {
        let date = t.date();
        let half_day = half_day_of(t.time().hour());
        let is_pm = matches!(half_day, HalfDay::Afternoon);
        let entry = groups.entry((date, is_pm)).or_insert((t, t));
        if t < entry.0 {
            entry.0 = t;
        }
        if t > entry.1 {
            entry.1 = t;
        }
    }

    groups
        .into_iter()
        .map(|((date, is_pm), (start, end))| LocalBlock {
            start,
            end,
            date,
            half_day: if is_pm { HalfDay::Afternoon } else { HalfDay::Morning },
        })
        .collect()
}
```

Add the required import at the top of the file (next to the existing `use chrono::...`):

```rust
use chrono::Timelike;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd backend && cargo test -p domain worklog_time`
Expected: all 6 tests PASS.

- [ ] **Step 6: Commit**

```bash
cd backend && git add crates/domain/src/rules/worklog_time.rs crates/domain/src/rules/mod.rs
git commit -m "feat(domain): derive_time_blocks groups local timestamps per day/half-day"
```

---

## Phase 2 — Application: materialize use case

### Task 2: Add `chrono-tz` dependency

**Files:**
- Modify: `backend/crates/application/Cargo.toml`

- [ ] **Step 1: Add the dependency**

In `backend/crates/application/Cargo.toml`, under `[dependencies]`, add:

```toml
chrono-tz = "0.9"
```

- [ ] **Step 2: Verify it resolves**

Run: `cd backend && cargo build -p application`
Expected: builds successfully (downloads `chrono-tz`).

- [ ] **Step 3: Commit**

```bash
cd backend && git add crates/application/Cargo.toml Cargo.lock
git commit -m "chore(application): add chrono-tz for local-timezone time blocks"
```

### Task 3: `materialize_worklog_time` use case

**Files:**
- Modify: `backend/crates/application/src/use_cases/worklog.rs`

- [ ] **Step 1: Write the failing test**

In `backend/crates/application/src/use_cases/worklog.rs`, inside the existing `#[cfg(test)] mod tests`, add an in-memory `ActivitySlotRepository` and `ConfigRepository` plus this test. (The `FakeRepo` for worklog already exists in this module.)

```rust
// --- add near the other use-imports inside mod tests ---
use crate::repositories::{ActivitySlotRepository, ConfigRepository};
use domain::types::{ActivitySlot, ActivitySlotId, HalfDay};
use chrono::NaiveDate;

#[derive(Default)]
struct FakeActivityRepo {
    slots: Mutex<Vec<ActivitySlot>>,
}

#[async_trait]
impl ActivitySlotRepository for FakeActivityRepo {
    async fn find_by_id(&self, id: ActivitySlotId) -> Result<Option<ActivitySlot>, RepositoryError> {
        Ok(self.slots.lock().unwrap().iter().find(|s| s.id == id).cloned())
    }
    async fn find_by_user_and_date(&self, user_id: UserId, date: NaiveDate) -> Result<Vec<ActivitySlot>, RepositoryError> {
        Ok(self.slots.lock().unwrap().iter().filter(|s| s.user_id == user_id && s.date == date).cloned().collect())
    }
    async fn find_active(&self, _user_id: UserId) -> Result<Option<ActivitySlot>, RepositoryError> {
        Ok(None)
    }
    async fn find_by_user_and_date_range(&self, user_id: UserId, start: NaiveDate, end: NaiveDate) -> Result<Vec<ActivitySlot>, RepositoryError> {
        Ok(self.slots.lock().unwrap().iter().filter(|s| s.user_id == user_id && s.date >= start && s.date <= end).cloned().collect())
    }
    async fn save(&self, slot: &ActivitySlot) -> Result<(), RepositoryError> {
        self.slots.lock().unwrap().push(slot.clone());
        Ok(())
    }
    async fn update(&self, _slot: &ActivitySlot) -> Result<(), RepositoryError> { Ok(()) }
    async fn delete(&self, _id: ActivitySlotId) -> Result<(), RepositoryError> { Ok(()) }
}

#[derive(Default)]
struct FakeConfigRepo {
    map: Mutex<std::collections::HashMap<String, String>>,
}

#[async_trait]
impl ConfigRepository for FakeConfigRepo {
    async fn get(&self, _user_id: UserId, key: &str) -> Result<Option<String>, RepositoryError> {
        Ok(self.map.lock().unwrap().get(key).cloned())
    }
    async fn get_all(&self, _user_id: UserId) -> Result<Vec<(String, String)>, RepositoryError> {
        Ok(self.map.lock().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }
    async fn set(&self, _user_id: UserId, key: &str, value: &str) -> Result<(), RepositoryError> {
        self.map.lock().unwrap().insert(key.to_string(), value.to_string());
        Ok(())
    }
}

#[tokio::test]
async fn materialize_writes_one_local_slot_per_half_day() {
    use chrono::TimeZone;
    let wlog = FakeRepo::default();
    let acts = FakeActivityRepo::default();
    let cfg = FakeConfigRepo::default();
    cfg.set(Uuid::new_v4(), "aplan.timezone", "Europe/Paris").await.unwrap();
    let uid = Uuid::new_v4();
    let tid = Uuid::new_v4();
    // 10:00 and 11:30 Paris time on 2026-06-08 == 08:00 and 09:30 UTC (CEST +02)
    let from = Utc.with_ymd_and_hms(2026, 6, 8, 0, 0, 0).unwrap();
    let to = Utc.with_ymd_and_hms(2026, 6, 8, 23, 0, 0).unwrap();
    add_worklog_entry(&wlog, uid, tid, "a".into(), Some(Utc.with_ymd_and_hms(2026, 6, 8, 8, 0, 0).unwrap()), from).await.unwrap();
    add_worklog_entry(&wlog, uid, tid, "b".into(), Some(Utc.with_ymd_and_hms(2026, 6, 8, 9, 30, 0).unwrap()), from).await.unwrap();

    let result = materialize_worklog_time(&wlog, &acts, &cfg, uid, tid, from, to).await.unwrap();

    let slots = acts.slots.lock().unwrap();
    assert_eq!(slots.len(), 1, "one morning block expected");
    assert_eq!(slots[0].half_day, HalfDay::Morning);
    assert_eq!(slots[0].date, NaiveDate::from_ymd_opt(2026, 6, 8).unwrap());
    assert_eq!(slots[0].task_id, Some(tid));
    assert!(slots[0].end_time.unwrap() > slots[0].start_time);
    assert_eq!(result.slots_written, 1);
    assert_eq!(result.active_since, to);
}

#[tokio::test]
async fn materialize_empty_window_writes_nothing() {
    let wlog = FakeRepo::default();
    let acts = FakeActivityRepo::default();
    let cfg = FakeConfigRepo::default();
    let uid = Uuid::new_v4();
    let tid = Uuid::new_v4();
    let from = now();
    let to = now() + chrono::Duration::hours(1);
    let result = materialize_worklog_time(&wlog, &acts, &cfg, uid, tid, from, to).await.unwrap();
    assert_eq!(result.slots_written, 0);
    assert!(acts.slots.lock().unwrap().is_empty());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd backend && cargo test -p application materialize`
Expected: FAIL — `materialize_worklog_time` and `FlushOutcome` not found.

- [ ] **Step 3: Implement the use case**

At the top of `backend/crates/application/src/use_cases/worklog.rs`, ensure these imports exist:

```rust
use chrono::{DateTime, TimeZone, Utc};
use domain::rules::worklog_time::derive_time_blocks;
use domain::types::*;
use crate::repositories::{ActivitySlotRepository, ConfigRepository, WorklogFilter, WorklogRepository, WORKLOG_FILTER_MAX_LIMIT};
```

Add the result type and use case (production code, outside the test module):

```rust
/// Outcome of a flush: how many slots were written and the new watermark.
pub struct FlushOutcome {
    pub slots_written: u32,
    pub active_since: DateTime<Utc>,
}

/// Default timezone when `aplan.timezone` is unset or unparseable.
const DEFAULT_TZ: &str = "Europe/Paris";

/// Materialize worklog entries logged in `[from, now]` for `task_id` into closed
/// activity slots, one per (local day, half-day). Returns the new watermark (`now`)
/// and the number of slots written. Idempotent when the caller advances the stored
/// watermark to `active_since` after each call.
pub async fn materialize_worklog_time(
    worklog_repo: &dyn WorklogRepository,
    activity_repo: &dyn ActivitySlotRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    task_id: TaskId,
    from: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<FlushOutcome, AppError> {
    // Resolve timezone (fall back to default on missing/invalid).
    let tz: chrono_tz::Tz = config_repo
        .get(user_id, "aplan.timezone")
        .await?
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| DEFAULT_TZ.parse().expect("default tz parses"));

    // Pull entries in the window for this task.
    let filter = WorklogFilter {
        task_ids: Some(vec![task_id]),
        from: Some(from),
        to: Some(now),
        limit: WORKLOG_FILTER_MAX_LIMIT,
        offset: 0,
    };
    let entries = worklog_repo.list(user_id, &filter).await?;

    // Map local naive time -> originating UTC instant (for exact, DST-free back-conversion).
    let mut local_to_utc: std::collections::HashMap<chrono::NaiveDateTime, DateTime<Utc>> =
        std::collections::HashMap::new();
    let mut local_times = Vec::with_capacity(entries.len());
    for e in &entries {
        let local = tz.from_utc_datetime(&e.logged_at.naive_utc()).naive_local();
        local_to_utc.insert(local, e.logged_at);
        local_times.push(local);
    }

    let blocks = derive_time_blocks(&local_times);
    let mut written = 0u32;
    for block in blocks {
        let start_utc = local_to_utc[&block.start];
        let mut end_utc = local_to_utc[&block.end];
        if end_utc <= start_utc {
            end_utc = start_utc + chrono::Duration::minutes(1);
        }
        let slot = ActivitySlot {
            id: uuid::Uuid::new_v4(),
            user_id,
            task_id: Some(task_id),
            start_time: start_utc,
            end_time: Some(end_utc),
            half_day: block.half_day,
            date: block.date,
            created_at: Utc::now(),
        };
        activity_repo.save(&slot).await?;
        written += 1;
    }

    Ok(FlushOutcome { slots_written: written, active_since: now })
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd backend && cargo test -p application materialize`
Expected: both tests PASS.

- [ ] **Step 5: Run the full application + domain suites**

Run: `cd backend && cargo test -p application -p domain`
Expected: all PASS (no regressions).

- [ ] **Step 6: Commit**

```bash
cd backend && git add crates/application/src/use_cases/worklog.rs
git commit -m "feat(application): materialize_worklog_time writes closed local slots from entries"
```

---

## Phase 3 — API: flush mutation

### Task 4: `FlushResultGql` output type

**Files:**
- Modify: `backend/crates/api/src/graphql/types/worklog_entry.rs`

- [ ] **Step 1: Add the output type**

Append to `backend/crates/api/src/graphql/types/worklog_entry.rs`:

```rust
use application::use_cases::worklog::FlushOutcome;

/// Result of flushing worklog time into activity slots.
pub struct FlushResultGql(pub FlushOutcome);

#[async_graphql::Object]
impl FlushResultGql {
    /// New watermark: entries at/after this instant are not yet materialized.
    async fn active_since(&self) -> chrono::DateTime<chrono::Utc> {
        self.0.active_since
    }
    /// Number of activity slots written by this flush.
    async fn slots_written(&self) -> i32 {
        self.0.slots_written as i32
    }
}
```

If the existing types in this file are re-exported via `crates/api/src/graphql/types/mod.rs`, add `FlushResultGql` to that re-export list (match the pattern used for `WorklogEntryGql`).

- [ ] **Step 2: Verify it compiles**

Run: `cd backend && cargo check -p api`
Expected: compiles (type unused yet may warn — acceptable until Task 5).

- [ ] **Step 3: Commit**

```bash
cd backend && git add crates/api/src/graphql/types/worklog_entry.rs crates/api/src/graphql/types/mod.rs
git commit -m "feat(api): FlushResultGql output type"
```

### Task 5: `flush_worklog_time` resolver

**Files:**
- Modify: `backend/crates/api/src/graphql/mutation.rs`

- [ ] **Step 1: Add the resolver**

In `backend/crates/api/src/graphql/mutation.rs`, inside `impl MutationRoot`, add (place it next to `add_worklog_entry`). It reads `aplan.active_since` as the `from` bound (defaulting to the Unix epoch if unset), flushes, then advances the stored watermark:

```rust
/// Materialize worklog entries logged since `aplan.active_since` for the given
/// task into closed activity slots. Advances the stored watermark.
async fn flush_worklog_time(
    &self,
    ctx: &Context<'_>,
    task_id: ID,
) -> Result<FlushResultGql> {
    let user_id = *ctx.data::<UserId>()?;
    let worklog_repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
    let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
    let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
    let tid = Uuid::parse_str(&task_id)
        .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {e}")))?;

    let now = chrono::Utc::now();
    let from = config_repo
        .get(user_id, "aplan.active_since")
        .await
        .ok()
        .flatten()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).unwrap());

    let outcome = worklog_uc::materialize_worklog_time(
        worklog_repo.as_ref(),
        activity_repo.as_ref(),
        config_repo.as_ref(),
        user_id,
        tid,
        from,
        now,
    )
    .await
    .map_err(|e| async_graphql::Error::new(e.to_string()))?;

    // Advance the watermark so the same entries are never re-materialized.
    configuration::set_config(config_repo.as_ref(), user_id, "aplan.active_since", &outcome.active_since.to_rfc3339())
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

    Ok(FlushResultGql(outcome))
}
```

Ensure `configuration` is in the `use application::use_cases::{...}` import line at the top of the file (it is, per existing imports) and that `ActivitySlotRepository` and `ConfigRepository` are reachable via `application::repositories::*` (already glob-imported).

- [ ] **Step 2: Build the API**

Run: `cd backend && cargo build -p api`
Expected: compiles.

- [ ] **Step 3: Add a resolver test**

In `backend/crates/api/src/graphql/tests.rs`, add a test that builds the schema with in-memory repos (follow the existing test harness in that file for constructing `Schema` + executing a query), seeds two worklog entries + `aplan.active_since` in the past, executes:

```graphql
mutation { flushWorklogTime(taskId: "<TID>") { slotsWritten activeSince } }
```

and asserts `slotsWritten >= 1`. Mirror the construction style already present in `tests.rs` (do not invent a new harness).

- [ ] **Step 4: Run API tests**

Run: `cd backend && cargo test -p api flush`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd backend && git add crates/api/src/graphql/mutation.rs crates/api/src/graphql/tests.rs
git commit -m "feat(api): flushWorklogTime mutation materializes + advances watermark"
```

### Task 6: Regenerate the CLI SDL

**Files:**
- Regenerate: `backend/crates/cli/graphql/schema.graphql`

- [ ] **Step 1: Export the schema**

Run: `cd backend && cargo run -p api -- export-schema > crates/cli/graphql/schema.graphql`
Expected: file rewritten; `git diff --stat` shows it changed.

- [ ] **Step 2: Confirm worklog ops are present**

Run: `rg -n 'flushWorklogTime|addWorklogEntry|worklogEntries' backend/crates/cli/graphql/schema.graphql`
Expected: all three appear.

- [ ] **Step 3: Commit**

```bash
cd backend && git add crates/cli/graphql/schema.graphql
git commit -m "chore(cli): regenerate GraphQL SDL (adds worklog + flush ops)"
```

---

## Phase 4 — CLI

### Task 7: GraphQL operation files + registration

**Files:**
- Create: `backend/crates/cli/graphql/add_worklog_entry.graphql`
- Create: `backend/crates/cli/graphql/flush_worklog_time.graphql`
- Modify: `backend/crates/cli/src/queries.rs`

- [ ] **Step 1: Create the operation files**

`backend/crates/cli/graphql/add_worklog_entry.graphql`:

```graphql
mutation AddWorklogEntry($taskId: ID!, $body: String!) {
  addWorklogEntry(taskId: $taskId, body: $body) {
    id
    taskId
    loggedAt
  }
}
```

`backend/crates/cli/graphql/flush_worklog_time.graphql`:

```graphql
mutation FlushWorklogTime($taskId: ID!) {
  flushWorklogTime(taskId: $taskId) {
    slotsWritten
    activeSince
  }
}
```

- [ ] **Step 2: Register the ops in `queries.rs`**

In `backend/crates/cli/src/queries.rs`, add two `GraphQLQuery` derives modeled exactly on the existing `AppendTaskNotes` block:

```rust
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/add_worklog_entry.graphql",
    response_derives = "Debug, Clone"
)]
pub struct AddWorklogEntry;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/flush_worklog_time.graphql",
    response_derives = "Debug, Clone"
)]
pub struct FlushWorklogTime;
```

- [ ] **Step 3: Verify codegen compiles against the SDL**

Run: `cd backend && cargo build -p cli`
Expected: compiles (graphql_client validates the ops against `schema.graphql`).

- [ ] **Step 4: Commit**

```bash
cd backend && git add crates/cli/graphql/add_worklog_entry.graphql crates/cli/graphql/flush_worklog_time.graphql crates/cli/src/queries.rs
git commit -m "feat(cli): register addWorklogEntry + flushWorklogTime ops"
```

### Task 8: `aplan log` command

**Files:**
- Modify: `backend/crates/cli/src/cli.rs`
- Modify: `backend/crates/cli/src/commands.rs`
- Modify: `backend/crates/cli/src/main.rs`

- [ ] **Step 1: Add the `Log` subcommand to `cli.rs`**

In `backend/crates/cli/src/cli.rs`, in `enum Commands`, add (right after the `Note { .. }` variant):

```rust
    /// Append a timestamped entry to the worklog of the active task (or --task TARGET).
    Log {
        /// Entry text. Variadic — multiple words are joined with spaces.
        #[arg(required = true)]
        text: Vec<String>,
        /// Override the implicit active-task target.
        #[arg(long)]
        task: Option<String>,
    },
```

- [ ] **Step 2: Add the `log` handler to `commands.rs`**

In `backend/crates/cli/src/commands.rs`, add the import to the `use crate::queries::{...}` list: `add_worklog_entry, AddWorklogEntry`. Then add this function (model on the existing `note` fn):

```rust
pub fn log(api_url: &str, json: bool, text: &[String], task: Option<&str>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_task(&client, task) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {}", e);
            return e.exit_code();
        }
    };
    let joined = text.join(" ");
    let result = client.run::<AddWorklogEntry>(add_worklog_entry::Variables {
        task_id: target.id.clone(),
        body: joined,
    });
    match result {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {}", e);
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            println!("✎ {}: worklog entry added", target.title);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}
```

- [ ] **Step 3: Dispatch `Log` in `main.rs`**

In `backend/crates/cli/src/main.rs`, find the `match cli.command` arm for `Commands::Note { text, task }` and add an adjacent arm:

```rust
        Commands::Log { text, task } => {
            commands::log(&cli.api_url, cli.json, &text, task.as_deref())
        }
```

- [ ] **Step 4: Build**

Run: `cd backend && cargo build -p cli`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
cd backend && git add crates/cli/src/cli.rs crates/cli/src/commands.rs crates/cli/src/main.rs
git commit -m "feat(cli): aplan log appends a timestamped worklog entry"
```

### Task 9: Repoint `start`/`stop`/`done`/`current` to the config pointer

**Files:**
- Modify: `backend/crates/cli/src/commands.rs`

The pointer keys are written/read via the existing `UpdateConfiguration` / `GetConfiguration` ops (already registered and imported in `commands.rs`). The flush uses `FlushWorklogTime`.

- [ ] **Step 1: Add a private helper to set/clear/read the pointer**

In `backend/crates/cli/src/commands.rs`, add helpers near the top (after imports). Add `flush_worklog_time, FlushWorklogTime` to the `use crate::queries::{...}` import list.

```rust
/// Read `aplan.active_task_id` from configuration, if set and non-empty.
fn active_task_id(client: &Client) -> Option<String> {
    let r = client.run::<GetConfiguration>(get_configuration::Variables {}).ok()?;
    r.data
        .configuration
        .get("aplan.active_task_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Set a single config key (best-effort; logs a warning on failure).
fn set_config_key(client: &Client, key: &str, value: &str) {
    if let Err(e) = client.run::<UpdateConfiguration>(update_configuration::Variables {
        key: key.to_string(),
        value: value.to_string(),
    }) {
        eprintln!("warning: failed to set {}: {}", key, e);
    }
}

/// Flush the worklog window of `task_id` into closed activity slots.
fn flush_task(client: &Client, task_id: &str) {
    if let Err(e) = client.run::<FlushWorklogTime>(flush_worklog_time::Variables {
        task_id: task_id.to_string(),
    }) {
        eprintln!("warning: failed to flush worklog time: {}", e);
    }
}
```

- [ ] **Step 2: Rewrite `start` to set the pointer (no open slot), flushing a previous task**

Replace the body of `pub fn start(...)` in `commands.rs` with:

```rust
pub fn start(api_url: &str, json: bool, task: &str) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_task(&client, Some(task)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {}", e);
            return e.exit_code();
        }
    };

    // If a different task is active, flush it before repointing.
    if let Some(prev) = active_task_id(&client) {
        if prev != target.id {
            flush_task(&client, &prev);
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    set_config_key(&client, "aplan.active_task_id", &target.id);
    set_config_key(&client, "aplan.active_since", &now);

    if json {
        let payload = serde_json::json!({ "activeTaskId": target.id, "activeSince": now });
        if let Err(e) = print_json(&payload) {
            eprintln!("error writing output: {}", e);
            return ExitCode::Generic;
        }
        return ExitCode::Success;
    }
    println!("▶ tracking: {}", target.title);
    ExitCode::Success
}
```

- [ ] **Step 3: Rewrite `stop` to flush then clear the pointer**

Replace the body of `pub fn stop(...)`:

```rust
pub fn stop(api_url: &str, json: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let active = active_task_id(&client);
    if let Some(ref tid) = active {
        flush_task(&client, tid);
    }
    set_config_key(&client, "aplan.active_task_id", "");

    if json {
        let payload = serde_json::json!({ "stopped": active });
        if let Err(e) = print_json(&payload) {
            eprintln!("error writing output: {}", e);
            return ExitCode::Generic;
        }
        return ExitCode::Success;
    }
    match active {
        Some(_) => println!("⏹ stopped — worklog time flushed, tracking cleared"),
        None => println!("(no task was being tracked)"),
    }
    ExitCode::Success
}
```

- [ ] **Step 4: Rewrite `current` to read the pointer**

Replace the body of `pub fn current(...)`:

```rust
pub fn current(api_url: &str, json: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let active = active_task_id(&client);
    if json {
        // Resolve to a task object when possible so the SessionStart hook can read
        // `.currentActivity.task.{id,title,sourceId}` exactly as before.
        let payload = match &active {
            Some(id) => match resolve_task(&client, Some(id)) {
                Ok(t) => serde_json::json!({ "currentActivity": { "task": { "id": t.id, "title": t.title, "sourceId": t.source_id } } }),
                Err(_) => serde_json::json!({ "currentActivity": null }),
            },
            None => serde_json::json!({ "currentActivity": null }),
        };
        if let Err(e) = print_json(&payload) {
            eprintln!("error writing output: {}", e);
            return ExitCode::Generic;
        }
        return ExitCode::Success;
    }
    match active {
        Some(id) => match resolve_task(&client, Some(&id)) {
            Ok(t) => println!("▶ tracking: {}", t.title),
            Err(_) => println!("▶ tracking task {}", id),
        },
        None => println!("(no task being tracked)"),
    }
    ExitCode::Success
}
```

NOTE: `resolve_task` returns a struct with `id`, `title`, `source_id` — confirm the field name for the Jira key in `lookup.rs` (it is `source_id`) and adjust the JSON key/field above to match exactly.

- [ ] **Step 5: Rewrite `done` to flush + clear the pointer**

In `pub fn done(...)`, replace the activity-slot stop logic (the `CurrentActivity` fetch and the `StopActivity` call) with pointer-based flush. Keep the `complete_task` call. After completing the task:

```rust
    // Flush + clear the pointer iff it was tracking this task (unless --keep-running).
    let active = active_task_id(&client);
    let was_tracking_target = active.as_deref() == Some(target_id.as_str());
    if !keep_running && was_tracking_target {
        flush_task(&client, &target_id);
        set_config_key(&client, "aplan.active_task_id", "");
    }
```

Remove the now-unused `CurrentActivity` / `StopActivity` references from `done` and from the `use crate::queries::{...}` import list **only if** no other command still uses them (`StartActivity`/`StopActivity`/`CurrentActivity` are no longer used after this task — remove their imports and their `queries.rs` derives in a follow-up cleanup commit only after `cargo build` confirms they are unused; if removal causes errors, leave them).

Update the human-output branch of `done` to not print stopped-minutes (it no longer has them): print `✓ {label} done`.

- [ ] **Step 6: Build**

Run: `cd backend && cargo build -p cli`
Expected: compiles (warnings about unused `StartActivity` etc. are acceptable here).

- [ ] **Step 7: Commit**

```bash
cd backend && git add crates/cli/src/commands.rs
git commit -m "feat(cli): start/stop/done/current use config pointer + worklog flush"
```

### Task 10: CLI integration test for the flow

**Files:**
- Modify: `backend/crates/cli/tests/integration.rs`

- [ ] **Step 1: Read the existing harness**

Open `backend/crates/cli/tests/integration.rs` and identify how it starts an in-process/loopback API (it already tests `worklog` per the earlier file scan). Reuse that exact harness.

- [ ] **Step 2: Add the flow test**

Add a test that: creates a task; runs `aplan start <task>`; asserts `aplan current --json` reports that task; runs `aplan log "did a thing"`; runs `aplan stop --json`; asserts `aplan current --json` reports no task. Use the harness's existing command-invocation helper (do not shell out to a built binary unless the harness already does).

- [ ] **Step 3: Run**

Run: `cd backend && cargo test -p cli`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
cd backend && git add crates/cli/tests/integration.rs
git commit -m "test(cli): start/log/stop pointer lifecycle"
```

---

## Phase 5 — Hooks & skill

### Task 11: `aplan flush` verb (materialize without clearing)

The SessionEnd hook (Task 12) must flush time **without** clearing the pointer, so the task stays linked for the next session. `aplan stop` clears; we need a non-clearing verb.

**Files:**
- Modify: `backend/crates/cli/src/cli.rs`
- Modify: `backend/crates/cli/src/commands.rs`
- Modify: `backend/crates/cli/src/main.rs`

- [ ] **Step 1: Add the `Flush` subcommand to `cli.rs`**

In `enum Commands`, after the `Stop` variant:

```rust
    /// Flush the worklog time of TASK into closed activity slots, WITHOUT
    /// clearing the active-task pointer. Used by the SessionEnd hook.
    Flush {
        /// Task to flush: UUID, Jira-style key, or fuzzy title.
        task: String,
    },
```

- [ ] **Step 2: Add the `flush` handler to `commands.rs`**

```rust
pub fn flush(api_url: &str, json: bool, task: &str) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_task(&client, Some(task)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {}", e);
            return e.exit_code();
        }
    };
    match client.run::<FlushWorklogTime>(flush_worklog_time::Variables { task_id: target.id.clone() }) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {}", e);
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            println!("⤓ {}: worklog time flushed", target.title);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}
```

- [ ] **Step 3: Dispatch `Flush` in `main.rs`**

```rust
        Commands::Flush { task } => commands::flush(&cli.api_url, cli.json, &task),
```

- [ ] **Step 4: Build**

Run: `cd backend && cargo build -p cli`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
cd backend && git add crates/cli/src/cli.rs crates/cli/src/commands.rs crates/cli/src/main.rs
git commit -m "feat(cli): aplan flush verb (materialize without clearing pointer)"
```

### Task 12: SessionEnd flush hook

**Files:**
- Create: `~/.claude/hooks/aplan-session-end.sh`
- Modify: `~/.claude/settings.json`

- [ ] **Step 1: Write the hook**

Create `~/.claude/hooks/aplan-session-end.sh`:

```bash
#!/usr/bin/env bash
# SessionEnd hook: flush the active aplan task's worklog time into closed slots.
# Keeps the active-task pointer (the task stays linked for the next session);
# only the watermark advances. Silent no-op when the CLI/backend is unavailable.

set -u

command -v aplan >/dev/null 2>&1 || exit 0
command -v jq    >/dev/null 2>&1 || exit 0

current_json=$(aplan current --json 2>/dev/null) || exit 0
task_id=$(printf '%s' "$current_json" | jq -r '.currentActivity.task.id // empty')
[ -z "$task_id" ] && exit 0

aplan flush --json "$task_id" >/dev/null 2>&1 || exit 0
```

- [ ] **Step 2: Make it executable**

Run: `chmod +x ~/.claude/hooks/aplan-session-end.sh`
Expected: no output.

- [ ] **Step 3: Register the hook in settings.json**

In `~/.claude/settings.json`, under `"hooks"`, add a `"SessionEnd"` array mirroring the existing `"SessionStart"` entry:

```json
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "bash /home/mbt/.claude/hooks/aplan-session-end.sh",
            "timeout": 10
          }
        ]
      }
    ]
```

- [ ] **Step 4: Validate JSON**

Run: `jq . ~/.claude/settings.json >/dev/null && echo OK`
Expected: `OK`.

- [ ] **Step 5: Record the manual update**

The hook + settings.json live under `~/.claude`, outside the repo. Note in the final summary that they were updated manually (no repo commit).

### Task 13: Update the SessionStart hook

**Files:**
- Modify: `~/.claude/hooks/aplan-session-start.sh`

- [ ] **Step 1: Swap the logging verb in `base_rules`**

In the `base_rules` heredoc, change every `aplan note` to `aplan log`, and update the wording: replace "the running worklog IS the link" with "the active-task pointer IS the link". Change the "Notes are concatenated in the task's notes field…" sentence to: "Each `aplan log` entry is a timestamped worklog record — one per finding/decision/action; these also drive automatic time tracking." Change the exit-4 line to reference `aplan log`.

- [ ] **Step 2: Confirm the detection still works**

The hook already keys off `aplan current --json` → `.currentActivity.task.id`, which Task 9 preserves. No structural change needed.

- [ ] **Step 3: Smoke-test the hook**

Run: `bash ~/.claude/hooks/aplan-session-start.sh | jq .hookSpecificOutput.hookEventName`
Expected: `"SessionStart"` (with the backend running) or empty/no-op (backend down).

- [ ] **Step 4: Commit**

The file is outside the repo; note the manual update. No repo commit required, but record it in the final summary.

### Task 14: Update the `aplan` skill

**Files:**
- Modify: `~/.claude/skills/aplan/SKILL.md`

- [ ] **Step 1: Update the hot-path recipe table**

Change the row `"log a note about X" (active worklog) | aplan note --json "X"` to use `aplan log`:

```markdown
| "log progress / a worklog entry" (active task) | `aplan log --json "X"` |
| "log a worklog entry on AP-1234" | `aplan log --json --task AP-1234 "X"` |
| "append a manual note to the notes field" | `aplan note --json "X"` |
```

- [ ] **Step 2: Document the lifecycle**

Add a short section after the recipe table:

```markdown
## Worklog & time tracking

- `aplan log` appends a **timestamped worklog entry**. Use it for incremental
  progress logging — this is the default logging verb for Claude.
- `aplan note` still appends free text to the task's **notes** field (manual notes).
- `aplan start <task>` links the session to a task (a config pointer — no open timer).
- Time is recorded as **closed** activity slots, derived per day/half-day from your
  worklog-entry timestamps, materialized on `aplan stop` / `aplan done` / session end.
  There is never an open, running slot.
```

- [ ] **Step 3: Update `current` description**

Change the `aplan current` line to: "the task this session is linked to (the active-task pointer)."

- [ ] **Step 4: Commit**

File is outside the repo; record the manual update in the final summary.

---

## Phase 6 — Specs

### Task 15: Update functional + technical specs

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md`
- Modify: `SPEC_TECHNIQUE.md`

- [ ] **Step 1: Functional spec (French)**

In `SPEC_FONCTIONNELLE.md`, in the activity-tracking / worklog section, document: Claude journalise via le worklog horodaté (`aplan log`); le temps est enregistré en créneaux fermés (demi-journée) dérivés des horodatages, jamais de créneau ouvert; le fuseau `aplan.timezone` (défaut `Europe/Paris`) définit les bornes de journée/demi-journée.

- [ ] **Step 2: Technical spec**

In `SPEC_TECHNIQUE.md`, document: `flushWorklogTime` mutation; `derive_time_blocks` domain rule; `materialize_worklog_time` use case; config keys `aplan.active_task_id` / `aplan.active_since` / `aplan.timezone`; SessionEnd hook.

- [ ] **Step 3: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "docs(spec): worklog logging + self-closing time blocks"
```

---

## Final verification

- [ ] **Full backend test suite**

Run: `cd backend && cargo test`
Expected: all PASS.

- [ ] **Clippy**

Run: `cd backend && cargo clippy`
Expected: no new warnings in changed crates (fix any introduced).

- [ ] **Manual smoke test (backend running)**

```bash
cd backend && cargo run -p api &   # in one shell
aplan start <some-task>
aplan log "investigated the thing"
aplan stop --json                  # should report stopped + flushed
aplan journal --json               # should show a closed slot for today
aplan current --json               # should report no task
```
Expected: a closed activity slot appears in `journal`; `current` is empty after `stop`.
