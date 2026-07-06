# Timesheet Reconstruction — Plan 4: End-of-Day Auto-Reconstruction Job Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A background tokio task that, once per local day after a configurable hour, reconstructs the just-completed workday's timesheet (persisting a draft) and raises a passive `TimesheetReady` alert — so the draft is waiting when the user opens the CLI/app, with zero real-time effort and never any auto-submission to Gryzzly.

**Architecture:** The testable logic lives in the application layer: a pure `compute_target_dates` (which local dates are due this tick) + a `run_eod_pass` use case (resolve tz → pick target dates → per date: `reconstruct_timesheet` + upsert a `TimesheetReady` alert → advance a watermark). A thin `api/src/jobs.rs` scheduler wraps `run_eod_pass` in a 60s `tokio::time::interval` loop, spawned from `main.rs` before `axum::serve`. A new `AlertType::TimesheetReady` variant surfaces through the existing `alerts` query with no new query.

**Tech Stack:** Rust, tokio (multi-threaded `#[tokio::main]`, `spawn` + `time::interval`), chrono/chrono_tz, async-graphql (enum), sqlx. Reuses Plan-1 `reconstruct_timesheet` + `application::time` helpers and Plan-2 GraphQL wiring.

## Global Constraints

- **Base:** branch `feat/timesheet-eod-job` off `main` (which now carries Plans 1+2). Depends on: `application::use_cases::timesheet::reconstruct_timesheet` (10-arg), `application::time::{resolve_tz, to_local}`, `domain::types::{TimesheetStatus, Alert, AlertType, AlertSeverity, RelatedItem}`, repos `TimesheetDraftRepository`/`SignalMappingRepository`/`AlertRepository`/`GryzzlyCatalogRepository`/`WorklogRepository`/`MeetingRepository`/`TaskRepository`/`ConfigRepository`, service `GitConnector` + `ShellGitConnector`.
- **Never submits to Gryzzly.** The job only reconstructs + persists a draft + raises an in-app alert. No external calls beyond what `reconstruct_timesheet` already does.
- **Never clobbers validated/submitted drafts** — `reconstruct_timesheet` already guards this (Plan 1). The job additionally does NOT raise a `TimesheetReady` alert for a validated/submitted day (and resolves a stale one).
- **Notification = passive alert only.** No SSE/push (subscription is `EmptySubscription`); the `TimesheetReady` alert surfaces via the existing `alerts` query. Honest scope.
- **`AlertType` has exactly 3 exhaustive `match` sites** that MUST get the new arm or the crate won't compile: `domain::types::common::AlertType` (enum def), `infrastructure::database::conversions::alert_type_to_str`, `api::graphql::types::enums`'s `From<AlertType> for AlertTypeGql`. Also add the `AlertTypeGql` variant and an explicit `alert_type_from_str` arm.
- **`Alert` has no constructor** — build via struct literal, all 9 fields (`id, user_id, alert_type, severity, message, related_items, date, resolved, created_at`).
- **Determinism/testability:** `run_eod_pass` takes `now_utc: DateTime<Utc>` as a parameter (never calls `Utc::now()` internally) so tests inject a fixed instant. Only the scheduler loop calls `Utc::now()`.
- **Timezone:** reuse `application::time::{resolve_tz,to_local}` (Europe/Paris default). Config keys: `workday.auto_reconstruct_hour` (default 18), `aplan.timesheet.last_auto_run` (watermark, local `%Y-%m-%d`).
- **Scoped tests:** `cargo test -p domain -p application -p infrastructure -p api`. Map `sqlx::Error → RepositoryError::Database`. No `.unwrap()` in prod. `#[async_trait]` on repo mocks. TDD; commit per task; NO `Co-Authored-By`; stage only task-relevant files.

---

## File Structure

**Created:**
- `backend/crates/api/src/jobs.rs` — `EodDeps` bundle + `run_eod_scheduler` (60s interval loop).

**Modified:**
- `backend/crates/domain/src/types/common.rs` — add `AlertType::TimesheetReady`.
- `backend/crates/infrastructure/src/database/conversions.rs` — `alert_type_to_str`/`from_str` arms.
- `backend/crates/api/src/graphql/types/enums.rs` — `AlertTypeGql::TimesheetReady` + `From` arm.
- `backend/crates/application/src/use_cases/timesheet.rs` — `compute_target_dates`, `run_eod_pass`, `upsert_timesheet_ready_alert` + tests.
- `backend/crates/api/src/main.rs` — clone repos before the `SchemaDeps` move; `tokio::spawn(jobs::run_eod_scheduler(...))` before `axum::serve`; `mod jobs;`.
- `SPEC_TECHNIQUE.md`, `SPEC_FONCTIONNELLE.md`.

---

### Task 1: Add `AlertType::TimesheetReady` across all fan-out sites

**Files:**
- Modify: `backend/crates/domain/src/types/common.rs`
- Modify: `backend/crates/infrastructure/src/database/conversions.rs`
- Modify: `backend/crates/api/src/graphql/types/enums.rs`

**Interfaces:**
- Produces: `AlertType::TimesheetReady` usable across domain/infra/api; string form `"timesheet_ready"`; GraphQL `AlertTypeGql::TimesheetReady`.

- [ ] **Step 1: Add the domain variant**

In `backend/crates/domain/src/types/common.rs`, add `TimesheetReady` to the enum:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertType {
    Deadline,
    Overload,
    Conflict,
    TimesheetReady,
}
```

- [ ] **Step 2: Add the DB (de)serialization arms + failing test**

In `backend/crates/infrastructure/src/database/conversions.rs`, add the arm to BOTH functions:
```rust
pub fn alert_type_to_str(a: AlertType) -> &'static str {
    match a {
        AlertType::Deadline => "deadline",
        AlertType::Overload => "overload",
        AlertType::Conflict => "conflict",
        AlertType::TimesheetReady => "timesheet_ready",
    }
}
```
```rust
pub fn alert_type_from_str(s: &str) -> AlertType {
    match s {
        "deadline" => AlertType::Deadline,
        "overload" => AlertType::Overload,
        "conflict" => AlertType::Conflict,
        "timesheet_ready" => AlertType::TimesheetReady,
        _ => AlertType::Conflict,
    }
}
```
Add a round-trip test to the `#[cfg(test)] mod tests` in `conversions.rs` (create the module if absent — follow the file's existing test style if present):
```rust
    #[test]
    fn alert_type_timesheet_ready_roundtrips() {
        assert_eq!(alert_type_to_str(AlertType::TimesheetReady), "timesheet_ready");
        assert_eq!(alert_type_from_str("timesheet_ready"), AlertType::TimesheetReady);
    }
```

- [ ] **Step 3: Add the GraphQL enum variant + From arm**

In `backend/crates/api/src/graphql/types/enums.rs`:
```rust
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum AlertTypeGql {
    Deadline,
    Overload,
    Conflict,
    TimesheetReady,
}

impl From<types::AlertType> for AlertTypeGql {
    fn from(a: types::AlertType) -> Self {
        match a {
            types::AlertType::Deadline => AlertTypeGql::Deadline,
            types::AlertType::Overload => AlertTypeGql::Overload,
            types::AlertType::Conflict => AlertTypeGql::Conflict,
            types::AlertType::TimesheetReady => AlertTypeGql::TimesheetReady,
        }
    }
}
```

- [ ] **Step 4: Build + test**

Run: `cd backend && cargo test -p infrastructure alert_type_timesheet_ready_roundtrips && cargo build -p domain -p infrastructure -p api`
Expected: test passes; all three crates build (the exhaustive matches now cover the new variant).

- [ ] **Step 5: Commit**

```bash
git add backend/crates/domain/src/types/common.rs \
        backend/crates/infrastructure/src/database/conversions.rs \
        backend/crates/api/src/graphql/types/enums.rs
git commit -m "Add AlertType::TimesheetReady variant across domain/infra/api"
```

---

### Task 2: Pure `compute_target_dates`

**Files:**
- Modify: `backend/crates/application/src/use_cases/timesheet.rs`

**Interfaces:**
- Produces: `pub fn compute_target_dates(last_auto_run: Option<NaiveDate>, local_today: NaiveDate, local_hour: u32, trigger_hour: u32, cap: usize) -> Vec<NaiveDate>` (ascending; the dates the EOD job should process this tick).

- [ ] **Step 1: Write the failing tests + function**

Add to `backend/crates/application/src/use_cases/timesheet.rs` (ensure `use chrono::{NaiveDate, Timelike};` — `NaiveDate` is already used; add `Timelike` only where `run_eod_pass` needs `.hour()` in Task 3):
```rust
/// The local dates the end-of-day job should (re)process on this tick, ascending.
///
/// - Every missed local date STRICTLY after `last_auto_run` and STRICTLY before `local_today`
///   (catch-up for days the machine was off) — but only when a watermark exists; with no
///   watermark we never backfill history.
/// - Plus `local_today` itself, IFF `local_hour >= trigger_hour` AND today isn't already the
///   watermark (so today is processed at most once per day, not every tick).
/// - Capped to the most recent `cap` dates (avoid reconstructing months after a long absence).
pub fn compute_target_dates(
    last_auto_run: Option<NaiveDate>,
    local_today: NaiveDate,
    local_hour: u32,
    trigger_hour: u32,
    cap: usize,
) -> Vec<NaiveDate> {
    let mut dates = Vec::new();
    if let Some(last) = last_auto_run {
        let mut d = match last.succ_opt() {
            Some(n) => n,
            None => return dates,
        };
        while d < local_today {
            dates.push(d);
            d = match d.succ_opt() {
                Some(n) => n,
                None => break,
            };
        }
    }
    let already_ran_today = last_auto_run == Some(local_today);
    if !already_ran_today && local_hour >= trigger_hour {
        dates.push(local_today);
    }
    if dates.len() > cap {
        dates = dates.split_off(dates.len() - cap);
    }
    dates
}

#[cfg(test)]
mod eod_target_tests {
    use super::compute_target_dates;
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn no_watermark_before_trigger_is_empty() {
        assert!(compute_target_dates(None, d(2026, 6, 8), 9, 18, 7).is_empty());
    }

    #[test]
    fn no_watermark_after_trigger_is_today_only() {
        assert_eq!(compute_target_dates(None, d(2026, 6, 8), 18, 18, 7), vec![d(2026, 6, 8)]);
    }

    #[test]
    fn caught_up_after_trigger_is_today() {
        assert_eq!(
            compute_target_dates(Some(d(2026, 6, 7)), d(2026, 6, 8), 20, 18, 7),
            vec![d(2026, 6, 8)]
        );
    }

    #[test]
    fn missed_days_are_caught_up_plus_today() {
        assert_eq!(
            compute_target_dates(Some(d(2026, 6, 5)), d(2026, 6, 8), 20, 18, 7),
            vec![d(2026, 6, 6), d(2026, 6, 7), d(2026, 6, 8)]
        );
    }

    #[test]
    fn missed_days_caught_up_even_before_trigger_but_not_today() {
        assert_eq!(
            compute_target_dates(Some(d(2026, 6, 5)), d(2026, 6, 8), 9, 18, 7),
            vec![d(2026, 6, 6), d(2026, 6, 7)]
        );
    }

    #[test]
    fn already_ran_today_is_empty() {
        assert!(compute_target_dates(Some(d(2026, 6, 8)), d(2026, 6, 8), 20, 18, 7).is_empty());
    }

    #[test]
    fn catch_up_is_capped_to_most_recent() {
        let out = compute_target_dates(Some(d(2026, 5, 1)), d(2026, 6, 8), 20, 18, 7);
        assert_eq!(out.len(), 7);
        assert_eq!(*out.last().unwrap(), d(2026, 6, 8));
        assert_eq!(*out.first().unwrap(), d(2026, 6, 2)); // last 7: Jun 2..Jun 8
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cd backend && cargo test -p application eod_target_tests`
Expected: PASS (7 tests).

- [ ] **Step 3: Commit**

```bash
git add backend/crates/application/src/use_cases/timesheet.rs
git commit -m "Add pure compute_target_dates for the end-of-day job"
```

---

### Task 3: `run_eod_pass` + `upsert_timesheet_ready_alert`

**Files:**
- Modify: `backend/crates/application/src/use_cases/timesheet.rs`

**Interfaces:**
- Consumes: `compute_target_dates` (Task 2), `reconstruct_timesheet` (Plan 1), `AlertRepository`, `AlertType::TimesheetReady` (Task 1), `application::time`.
- Produces:
  - `pub async fn run_eod_pass(worklog_repo, meeting_repo, task_repo, catalog_repo, mapping_repo, config_repo, git, draft_repo, alert_repo, user_id, now_utc: DateTime<Utc>) -> Result<Vec<NaiveDate>, AppError>` (returns processed dates)
  - `async fn upsert_timesheet_ready_alert(alert_repo, draft_repo, user_id, date, now_utc) -> Result<(), AppError>`

- [ ] **Step 1: Write the functions**

Add to `use_cases/timesheet.rs`. Ensure imports at top include `use chrono::{DateTime, NaiveDate, Timelike, Utc};` (extend the existing chrono import), `use crate::time::{resolve_tz, to_local};`, `use crate::repositories::AlertRepository;`, `use domain::types::{Alert, AlertSeverity, AlertType};`.
```rust
const EOD_CATCHUP_CAP: usize = 7;
const DEFAULT_AUTO_RECONSTRUCT_HOUR: u32 = 18;

/// One end-of-day pass for `user_id` as of `now_utc`. Reconstructs each due local day
/// (persisting a draft; never clobbering validated/submitted), raises/settles a
/// TimesheetReady alert, and advances the `aplan.timesheet.last_auto_run` watermark.
/// Returns the dates processed. NEVER submits to Gryzzly.
#[allow(clippy::too_many_arguments)]
pub async fn run_eod_pass(
    worklog_repo: &dyn WorklogRepository,
    meeting_repo: &dyn MeetingRepository,
    task_repo: &dyn TaskRepository,
    catalog_repo: &dyn GryzzlyCatalogRepository,
    mapping_repo: &dyn SignalMappingRepository,
    config_repo: &dyn ConfigRepository,
    git: &dyn GitConnector,
    draft_repo: &dyn TimesheetDraftRepository,
    alert_repo: &dyn AlertRepository,
    user_id: UserId,
    now_utc: DateTime<Utc>,
) -> Result<Vec<NaiveDate>, AppError> {
    let tz = resolve_tz(config_repo.get(user_id, "aplan.timezone").await?);
    let local_now = to_local(now_utc, tz);
    let local_today = local_now.date();
    let local_hour = local_now.time().hour();
    let trigger_hour = config_repo
        .get(user_id, "workday.auto_reconstruct_hour")
        .await?
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_AUTO_RECONSTRUCT_HOUR);
    let last_auto_run = config_repo
        .get(user_id, "aplan.timesheet.last_auto_run")
        .await?
        .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());

    let targets = compute_target_dates(last_auto_run, local_today, local_hour, trigger_hour, EOD_CATCHUP_CAP);

    for date in &targets {
        reconstruct_timesheet(
            worklog_repo, meeting_repo, task_repo, catalog_repo, mapping_repo, config_repo,
            git, draft_repo, user_id, *date,
        )
        .await?;
        upsert_timesheet_ready_alert(alert_repo, draft_repo, user_id, *date, now_utc).await?;
    }

    if let Some(max) = targets.last() {
        config_repo
            .set(user_id, "aplan.timesheet.last_auto_run", &max.format("%Y-%m-%d").to_string())
            .await?;
    }

    Ok(targets)
}

/// Raise a single passive TimesheetReady alert for a day with a non-empty draft (deduped),
/// or resolve any stale one if the day is now validated/submitted or empty.
async fn upsert_timesheet_ready_alert(
    alert_repo: &dyn AlertRepository,
    draft_repo: &dyn TimesheetDraftRepository,
    user_id: UserId,
    date: NaiveDate,
    now_utc: DateTime<Utc>,
) -> Result<(), AppError> {
    let draft = draft_repo.find_by_user_and_date(user_id, date).await?;
    let mut existing: Vec<Alert> = alert_repo
        .find_by_user(user_id, Some(false))
        .await?
        .into_iter()
        .filter(|a| a.alert_type == AlertType::TimesheetReady && a.date == date)
        .collect();

    let should_alert = matches!(
        &draft,
        Some(d) if d.total_hours > 0.0
            && !matches!(d.status, TimesheetStatus::Validated | TimesheetStatus::Submitted)
    );

    if should_alert {
        if existing.is_empty() {
            let d = draft.expect("checked Some above");
            let project_count = d.lines.iter().filter(|l| l.gryzzly_project_id.is_some()).count();
            let alert = Alert {
                id: Uuid::new_v4(),
                user_id,
                alert_type: AlertType::TimesheetReady,
                severity: AlertSeverity::Information,
                message: format!(
                    "Timesheet draft ready for {date} ({:.1}h across {project_count} project(s)) — review and copy into Gryzzly",
                    d.total_hours
                ),
                related_items: vec![],
                date,
                resolved: false,
                created_at: now_utc,
            };
            alert_repo.save(&alert).await?;
        }
    } else {
        // Day is validated/submitted/empty → settle any stale ready-alert.
        for a in existing.iter_mut() {
            a.resolved = true;
            alert_repo.update(a).await?;
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Write the failing tests**

Add to the existing `#[cfg(test)] mod tests` in `use_cases/timesheet.rs` a mock `AlertRepository` (mirror the existing `MemDraft`/`MemConfig` mock style already in that module), then tests. If the test module already has mocks for worklog/meeting/task/catalog/mapping/git/config/draft (from Plan 1 Task 13), REUSE them; add only `MemAlert`:
```rust
    #[derive(Default)]
    struct MemAlert {
        saved: std::sync::Mutex<Vec<Alert>>,
    }
    #[async_trait]
    impl AlertRepository for MemAlert {
        async fn find_by_id(&self, _id: domain::types::AlertId) -> Result<Option<Alert>, RepositoryError> { Ok(None) }
        async fn find_unresolved(&self, _u: UserId) -> Result<Vec<Alert>, RepositoryError> {
            Ok(self.saved.lock().unwrap().iter().filter(|a| !a.resolved).cloned().collect())
        }
        async fn find_by_user(&self, _u: UserId, resolved: Option<bool>) -> Result<Vec<Alert>, RepositoryError> {
            let all = self.saved.lock().unwrap().clone();
            Ok(match resolved {
                Some(r) => all.into_iter().filter(|a| a.resolved == r).collect(),
                None => all,
            })
        }
        async fn save(&self, a: &Alert) -> Result<(), RepositoryError> {
            self.saved.lock().unwrap().push(a.clone()); Ok(())
        }
        async fn save_batch(&self, alerts: &[Alert]) -> Result<(), RepositoryError> {
            self.saved.lock().unwrap().extend_from_slice(alerts); Ok(())
        }
        async fn update(&self, a: &Alert) -> Result<(), RepositoryError> {
            let mut g = self.saved.lock().unwrap();
            if let Some(slot) = g.iter_mut().find(|x| x.id == a.id) { *slot = a.clone(); }
            Ok(())
        }
        async fn delete_resolved(&self, _u: UserId) -> Result<u64, RepositoryError> { Ok(0) }
    }

    fn utc(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        chrono::TimeZone::with_ymd_and_hms(&Utc, y, m, d, h, 0, 0).unwrap()
    }

    #[tokio::test]
    async fn eod_before_trigger_processes_nothing() {
        // 09:00 UTC = 11:00 Paris, before the default 18:00 trigger, no watermark.
        let (worklog, meeting, task, catalog, mapping, config, git, draft) = eod_mocks(); // build the Plan-1 mocks (empty)
        let alert = MemAlert::default();
        let processed = run_eod_pass(
            &worklog, &meeting, &task, &catalog, &mapping, &config, &git, &draft, &alert,
            test_user_id(), utc(2026, 6, 8, 9),
        ).await.unwrap();
        assert!(processed.is_empty());
        assert!(alert.saved.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn eod_after_trigger_processes_today_and_advances_watermark() {
        // 20:00 UTC = 22:00 Paris, after trigger. Empty signals → draft total 0 → NO alert, but watermark advances.
        let (worklog, meeting, task, catalog, mapping, config, git, draft) = eod_mocks();
        let alert = MemAlert::default();
        let processed = run_eod_pass(
            &worklog, &meeting, &task, &catalog, &mapping, &config, &git, &draft, &alert,
            test_user_id(), utc(2026, 6, 8, 20),
        ).await.unwrap();
        assert_eq!(processed.len(), 1);
        // watermark set to the local date (2026-06-08 Paris)
        assert_eq!(
            config.get(test_user_id(), "aplan.timesheet.last_auto_run").await.unwrap().as_deref(),
            Some("2026-06-08")
        );
        // empty day → no alert (total 0)
        assert!(alert.saved.lock().unwrap().is_empty());
    }
```
> **Mock note:** implement `eod_mocks()` and `test_user_id()` helpers in the test module that return the Plan-1 in-memory mocks (worklog/meeting/task/catalog/mapping/git/draft) seeded EMPTY, plus a fixed user id. If the Plan-1 Task-13 tests already define equivalent mock structs in this module, construct them directly instead of adding a helper. The `MemConfig` must return `None` for all keys (→ Europe/Paris, trigger 18) OR pre-seed `aplan.timezone` if the existing mock supports it. Add ONE more test that seeds a draft with `total_hours > 0` (via `draft_repo.upsert`) for a date then calls `upsert_timesheet_ready_alert` directly and asserts exactly one alert is created (and a second call creates no duplicate); and one that seeds a Validated draft and asserts no alert (and a pre-existing one is resolved).

- [ ] **Step 3: Run tests**

Run: `cd backend && cargo test -p application timesheet`
Expected: the `eod_*` tests plus the existing timesheet tests pass.

- [ ] **Step 4: Commit**

```bash
git add backend/crates/application/src/use_cases/timesheet.rs
git commit -m "Add run_eod_pass + TimesheetReady alert upsert for the end-of-day job"
```

---

### Task 4: Scheduler (`api/src/jobs.rs`) + `main.rs` wiring

**Files:**
- Create: `backend/crates/api/src/jobs.rs`
- Modify: `backend/crates/api/src/main.rs`

**Interfaces:**
- Consumes: `run_eod_pass` (Task 3), all repos + git connector.
- Produces: `pub struct EodDeps { ... }`, `pub async fn run_eod_scheduler(deps: EodDeps, user_id: UserId)`.

- [ ] **Step 1: Write the scheduler**

Create `backend/crates/api/src/jobs.rs`:
```rust
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;

use application::repositories::{
    AlertRepository, ConfigRepository, GryzzlyCatalogRepository, MeetingRepository,
    SignalMappingRepository, TaskRepository, TimesheetDraftRepository, WorklogRepository,
};
use application::services::git_connector::GitConnector;
use application::use_cases::timesheet::run_eod_pass;
use domain::types::UserId;

/// Dependencies the end-of-day scheduler needs (Arc clones of the app's repos).
pub struct EodDeps {
    pub worklog_repo: Arc<dyn WorklogRepository>,
    pub meeting_repo: Arc<dyn MeetingRepository>,
    pub task_repo: Arc<dyn TaskRepository>,
    pub catalog_repo: Arc<dyn GryzzlyCatalogRepository>,
    pub mapping_repo: Arc<dyn SignalMappingRepository>,
    pub config_repo: Arc<dyn ConfigRepository>,
    pub git: Arc<dyn GitConnector>,
    pub draft_repo: Arc<dyn TimesheetDraftRepository>,
    pub alert_repo: Arc<dyn AlertRepository>,
}

/// Long-lived background task: every 60s, run one end-of-day pass for `user_id`.
/// Errors are logged, never fatal. Idempotent via the last_auto_run watermark.
pub async fn run_eod_scheduler(deps: EodDeps, user_id: UserId) {
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    loop {
        ticker.tick().await;
        match run_eod_pass(
            deps.worklog_repo.as_ref(),
            deps.meeting_repo.as_ref(),
            deps.task_repo.as_ref(),
            deps.catalog_repo.as_ref(),
            deps.mapping_repo.as_ref(),
            deps.config_repo.as_ref(),
            deps.git.as_ref(),
            deps.draft_repo.as_ref(),
            deps.alert_repo.as_ref(),
            user_id,
            Utc::now(),
        )
        .await
        {
            Ok(dates) if !dates.is_empty() => {
                tracing::info!(?dates, "end-of-day timesheet reconstruction completed")
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "end-of-day timesheet reconstruction failed"),
        }
    }
}
```

- [ ] **Step 2: Wire into `main.rs`**

In `backend/crates/api/src/main.rs`:
1. Add `mod jobs;` near the other `mod` declarations.
2. BEFORE the `let deps = SchemaDeps { ... };` move, capture clones for the scheduler (the repos are otherwise moved into `deps`):
```rust
    let eod_deps = jobs::EodDeps {
        worklog_repo: worklog_repo.clone(),
        meeting_repo: meeting_repo.clone(),
        task_repo: task_repo.clone(),
        catalog_repo: gryzzly_catalog_repo.clone(),
        mapping_repo: signal_mapping_repo.clone(),
        config_repo: config_repo.clone(),
        git: git_connector.clone(),
        draft_repo: timesheet_draft_repo.clone(),
        alert_repo: alert_repo.clone(),
    };
```
   (Place this immediately before the `let deps = SchemaDeps { ... };` line so all those bindings are still in scope.)
3. AFTER the `let app = ...` block and BEFORE `axum::serve(...)`, spawn the scheduler:
```rust
    tokio::spawn(jobs::run_eod_scheduler(eod_deps, default_user_id));
```

- [ ] **Step 3: Build + verify existing tests**

Run: `cd backend && cargo build -p api && cargo test -p api`
Expected: builds cleanly; existing api tests still pass (the scheduler is a spawned loop, not exercised by unit tests — its logic is covered by Task 3's `run_eod_pass` tests).

- [ ] **Step 4: Commit**

```bash
git add backend/crates/api/src/jobs.rs backend/crates/api/src/main.rs
git commit -m "Spawn end-of-day timesheet reconstruction scheduler (60s tokio interval)"
```

---

### Task 5: Update specifications (French)

**Files:**
- Modify: `SPEC_TECHNIQUE.md`, `SPEC_FONCTIONNELLE.md`

- [ ] **Step 1: Document the job (technique)**

In `SPEC_TECHNIQUE.md`, document: the tokio 60s scheduler (`api/src/jobs.rs`), `run_eod_pass` semantics (tz-resolved local day, `workday.auto_reconstruct_hour` default 18, `aplan.timesheet.last_auto_run` watermark, 7-day catch-up cap, never clobbers validated/submitted, never submits to Gryzzly), and the new `AlertType::TimesheetReady` (`"timesheet_ready"`, severity Information) surfaced via the existing `alerts` query.

- [ ] **Step 2: Document the behaviour (fonctionnelle, French)**

In `SPEC_FONCTIONNELLE.md`, describe: en fin de journée (après l'heure configurée), aplan reconstruit automatiquement le brouillon du jour et lève une alerte passive « brouillon de feuille de temps prêt » ; aucune soumission automatique vers Gryzzly ; l'utilisateur revoit via `aplan timesheet` ou l'écran /timesheet. Note the honesty limitation (passive alert only, no OS/push notification).

- [ ] **Step 3: Commit**

```bash
git add SPEC_TECHNIQUE.md SPEC_FONCTIONNELLE.md
git commit -m "Document end-of-day auto-reconstruction job + TimesheetReady alert"
```

---

## Self-Review

**Spec coverage (design §9.3 Surface C):**
- Tokio EOD job, idempotent watermark, 7-day catch-up, trigger hour → Tasks 2-4. ✅
- Reconstructs draft, never clobbers validated/submitted (Plan-1 guard) → Task 3 reuses `reconstruct_timesheet`. ✅
- Passive `TimesheetReady` alert via existing `alerts` query, never auto-submits → Tasks 1, 3. ✅
- Honest "no real push" scope → documented Task 5. ✅

**Placeholder scan:** Task 3 Step 2 references `eod_mocks()`/`test_user_id()` helpers "built from the Plan-1 mocks" — this is a concrete instruction to reuse the existing in-module mocks, not a TODO; the mock bodies for `MemAlert` are given in full. The implementer must confirm the exact Plan-1 mock struct names in `timesheet.rs` tests and construct them (empty-seeded).

**Type consistency:** `AlertType::TimesheetReady` added to all 3 exhaustive sites + `AlertTypeGql` + `from_str`. `run_eod_pass` calls `reconstruct_timesheet` with the exact 10-arg order from Plan 1. `Alert` built via struct literal with all 9 fields. `compute_target_dates` signature identical across Tasks 2/3. `AlertRepository` mock implements the exact 7-method trait from recon.

**Open verification notes for the implementer:**
1. Exact names of the Plan-1 in-memory mock structs in `use_cases/timesheet.rs` tests (`MemConfig`/`MemDraft` + worklog/meeting/task/catalog/mapping/git) — reuse them for `eod_mocks()`.
2. `MemConfig` must let `run_eod_pass` read/write `aplan.timesheet.last_auto_run` (the mock's `set` must persist so the watermark assertion works — the Plan-1 `MemConfig` already backs a `HashMap`, confirm).
3. `conversions.rs` may not yet have a `#[cfg(test)] mod tests` — create one if absent (with the needed `use super::*;`).
4. In `main.rs`, confirm the binding names (`gryzzly_catalog_repo`, `signal_mapping_repo`, `timesheet_draft_repo`, `git_connector`, `alert_repo`, `worklog_repo`, `meeting_repo`, `task_repo`, `config_repo`) match the actual locals before the `SchemaDeps` move; capture `eod_deps` before that move.
