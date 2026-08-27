# Break Routine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give aplan a configurable routine of superposed break cadences that notifies the user on the desktop, never fires inside a meeting, and records what the user did about it.

**Architecture:** A pure decision function in `domain` (`decide`) computes, for a tick interval `(since, now]`, which break should fire, which should be deferred behind a meeting, which are absorbed by collision, and which expired. An in-API background job (`run_break_scheduler`, twin of `run_eod_scheduler`) ticks it every 30 s, persists the outcome to `break_events`, and delivers the notification through a `Notifier` trait implemented over `notify-send`. Rules are edited in the React settings screen through GraphQL.

**Tech Stack:** Rust (chrono, sqlx 0.8 runtime queries, async-graphql 7, tokio), SQLite, React 18 + urql + Vitest/RTL, `notify-send` 0.8.8 / swaync 0.12.6.

**Spec:** `docs/superpowers/specs/2026-08-27-break-routine-design.md`

## Global Constraints

- **DDD layers are strict.** `domain` may depend only on chrono/serde/uuid/thiserror. `application` depends on domain only. `infrastructure` implements application traits. `api` depends on all.
- **`chrono_tz` is forbidden in `domain`.** It lives in `application/src/time.rs`. The application resolves timezone, working days and window boundaries and hands `domain` UTC instants. Daily-cadence rules likewise reach `decide` as pre-resolved UTC instants.
- **No `.unwrap()` in production code.** Tests may unwrap.
- **SQLite repos use runtime `sqlx::query`**, never `sqlx::query!`. Map `sqlx::Error` → `RepositoryError::Database(e.to_string())`.
- **All IDs are UUID stored as `TEXT`; all datetimes are RFC 3339 `TEXT`; booleans are `INTEGER` 0/1.**
- **TDD.** Write the failing test first, watch it fail, then implement.
- **Scoped test command.** The `mcp` crate does not compile at HEAD. Always use
  `cargo test -p domain -p application -p infrastructure -p api`, never a bare `cargo test`.
- **User-facing strings are French** (labels, notification text, UI copy). Code, comments and doc comments are English.
- **Specs updated in the same commit as behaviour** — `SPEC_FONCTIONNELLE.md` and `SPEC_TECHNIQUE.md` (both French). Task 11 covers this.

---

### Task 1: Domain types for rules and events

**Files:**
- Create: `backend/crates/domain/src/types/break_rule.rs`
- Create: `backend/crates/domain/src/types/break_event.rs`
- Modify: `backend/crates/domain/src/types/common.rs` (add two ID aliases)
- Modify: `backend/crates/domain/src/types/mod.rs` (declare and re-export the two modules)
- Test: inline `#[cfg(test)] mod tests` in both new files

**Interfaces:**
- Consumes: `UserId` from `domain::types::common`.
- Produces: `BreakRuleId`, `BreakEventId`, `BreakKind`, `BreakCadence`, `BreakUrgency`, `BreakRule`, `BreakOutcome`, `DeferReason`, `BreakEvent`. Every string-backed enum exposes `fn as_str(&self) -> &'static str` and `fn from_str(s: &str) -> Option<Self>`; these exact names are what Tasks 2, 5, 6, 8 and 9 call.

- [ ] **Step 1: Write the failing test for `break_rule.rs`**

Create `backend/crates/domain/src/types/break_rule.rs` with only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_round_trips_through_its_storage_string() {
        for k in [BreakKind::Visual, BreakKind::Posture, BreakKind::Long, BreakKind::Strength] {
            assert_eq!(BreakKind::from_str(k.as_str()), Some(k));
        }
        assert_eq!(BreakKind::from_str("nope"), None);
    }

    #[test]
    fn urgency_round_trips_through_its_storage_string() {
        for u in [BreakUrgency::Low, BreakUrgency::Normal, BreakUrgency::Critical] {
            assert_eq!(BreakUrgency::from_str(u.as_str()), Some(u));
        }
        assert_eq!(BreakUrgency::from_str(""), None);
    }

    /// The cadence enum is what enforces interval-XOR-daily in the type system;
    /// the database CHECK (Task 2) enforces the same thing in storage.
    #[test]
    fn cadence_carries_exactly_one_shape() {
        let i = BreakCadence::Interval { minutes: 20 };
        let d = BreakCadence::Daily { at: NaiveTime::from_hms_opt(14, 0, 0).unwrap() };
        assert_eq!(i.interval_minutes(), Some(20));
        assert_eq!(i.at_time(), None);
        assert_eq!(d.interval_minutes(), None);
        assert_eq!(d.at_time(), Some(NaiveTime::from_hms_opt(14, 0, 0).unwrap()));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd backend && cargo test -p domain break_rule`
Expected: FAIL — `cannot find type BreakKind in this scope`.

- [ ] **Step 3: Implement `break_rule.rs`**

Prepend to the same file, above the test module:

```rust
use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{BreakRuleId, UserId};

/// What a break is for. Drives the notification icon and the seeded copy; it is
/// deliberately a closed set so the UI can render one control per kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakKind {
    Visual,
    Posture,
    Long,
    Strength,
}

impl BreakKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            BreakKind::Visual => "visual",
            BreakKind::Posture => "posture",
            BreakKind::Long => "long",
            BreakKind::Strength => "strength",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "visual" => Some(BreakKind::Visual),
            "posture" => Some(BreakKind::Posture),
            "long" => Some(BreakKind::Long),
            "strength" => Some(BreakKind::Strength),
            _ => None,
        }
    }
}

/// Passed straight through to the notification daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakUrgency {
    Low,
    Normal,
    Critical,
}

impl BreakUrgency {
    pub fn as_str(&self) -> &'static str {
        match self {
            BreakUrgency::Low => "low",
            BreakUrgency::Normal => "normal",
            BreakUrgency::Critical => "critical",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "low" => Some(BreakUrgency::Low),
            "normal" => Some(BreakUrgency::Normal),
            "critical" => Some(BreakUrgency::Critical),
            _ => None,
        }
    }
}

/// How often a rule comes due.
///
/// Modelled as a sum type rather than two nullable fields so the interval-XOR-daily
/// invariant cannot be violated in memory. Storage keeps two nullable columns plus a
/// cross-column CHECK, and the repository is the only place the two shapes meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakCadence {
    /// Anchored on each working window's start, never on the last fire.
    Interval { minutes: u32 },
    /// A wall-clock time in the user's timezone, resolved to UTC by the application.
    Daily { at: NaiveTime },
}

impl BreakCadence {
    pub fn interval_minutes(&self) -> Option<u32> {
        match self {
            BreakCadence::Interval { minutes } => Some(*minutes),
            BreakCadence::Daily { .. } => None,
        }
    }

    pub fn at_time(&self) -> Option<NaiveTime> {
        match self {
            BreakCadence::Interval { .. } => None,
            BreakCadence::Daily { at } => Some(*at),
        }
    }
}

/// One rhythm of the routine. This is what the settings screen edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakRule {
    pub id: BreakRuleId,
    pub user_id: UserId,
    pub kind: BreakKind,
    /// Notification title.
    pub label: String,
    /// Notification body: what to actually do.
    pub body: String,
    pub cadence: BreakCadence,
    pub duration_seconds: u32,
    /// Breaks collision ties when several rules come due in the same tick, and
    /// orders the settings list. Higher wins.
    pub priority: i32,
    pub enabled: bool,
    pub urgency: BreakUrgency,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd backend && cargo test -p domain break_rule`
Expected: PASS (3 tests).

- [ ] **Step 5: Write the failing test for `break_event.rs`**

Create `backend/crates/domain/src/types/break_event.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_round_trips_through_its_storage_string() {
        for o in [
            BreakOutcome::Pending,
            BreakOutcome::Taken,
            BreakOutcome::Snoozed,
            BreakOutcome::Skipped,
            BreakOutcome::Ignored,
            BreakOutcome::Absorbed,
            BreakOutcome::Expired,
        ] {
            assert_eq!(BreakOutcome::from_str(o.as_str()), Some(o));
        }
        assert_eq!(BreakOutcome::from_str("dismissed"), None);
    }

    #[test]
    fn defer_reason_round_trips_through_its_storage_string() {
        for r in [DeferReason::Meeting, DeferReason::Snooze] {
            assert_eq!(DeferReason::from_str(r.as_str()), Some(r));
        }
    }

    /// Adherence counts what the user actually saw. `absorbed` never reached a
    /// screen, so it must count neither for nor against.
    #[test]
    fn only_seen_outcomes_count_towards_adherence() {
        assert!(BreakOutcome::Taken.counts_towards_adherence());
        assert!(BreakOutcome::Skipped.counts_towards_adherence());
        assert!(BreakOutcome::Ignored.counts_towards_adherence());
        assert!(BreakOutcome::Snoozed.counts_towards_adherence());
        assert!(!BreakOutcome::Absorbed.counts_towards_adherence());
        assert!(!BreakOutcome::Expired.counts_towards_adherence());
        assert!(!BreakOutcome::Pending.counts_towards_adherence());
    }
}
```

- [ ] **Step 6: Run the test to verify it fails**

Run: `cd backend && cargo test -p domain break_event`
Expected: FAIL — `cannot find type BreakOutcome in this scope`.

- [ ] **Step 7: Implement `break_event.rs`**

Prepend above the test module:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{BreakEventId, BreakRuleId, UserId};

/// What became of one due slot.
///
/// `Skipped` and `Ignored` are kept apart on purpose: systematically *ignoring* the
/// 20-minute break says the cadence is wrong, while explicitly *skipping* says the
/// timing was wrong. Those are two different fixes, and collapsing them would erase
/// the only signal that tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakOutcome {
    /// Created, unresolved: either deferred, or fired and awaiting an answer.
    Pending,
    Taken,
    Snoozed,
    Skipped,
    /// Fired, closed without a choice.
    Ignored,
    /// Collapsed by coalescing. The user never saw it.
    Absorbed,
    /// Could no longer usefully fire.
    Expired,
}

impl BreakOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            BreakOutcome::Pending => "pending",
            BreakOutcome::Taken => "taken",
            BreakOutcome::Snoozed => "snoozed",
            BreakOutcome::Skipped => "skipped",
            BreakOutcome::Ignored => "ignored",
            BreakOutcome::Absorbed => "absorbed",
            BreakOutcome::Expired => "expired",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(BreakOutcome::Pending),
            "taken" => Some(BreakOutcome::Taken),
            "snoozed" => Some(BreakOutcome::Snoozed),
            "skipped" => Some(BreakOutcome::Skipped),
            "ignored" => Some(BreakOutcome::Ignored),
            "absorbed" => Some(BreakOutcome::Absorbed),
            "expired" => Some(BreakOutcome::Expired),
            _ => None,
        }
    }

    /// Whether this outcome describes a break the user was actually shown.
    pub fn counts_towards_adherence(&self) -> bool {
        matches!(
            self,
            BreakOutcome::Taken
                | BreakOutcome::Snoozed
                | BreakOutcome::Skipped
                | BreakOutcome::Ignored
        )
    }
}

/// Why a slot is waiting instead of firing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeferReason {
    Meeting,
    Snooze,
}

impl DeferReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeferReason::Meeting => "meeting",
            DeferReason::Snooze => "snooze",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "meeting" => Some(DeferReason::Meeting),
            "snooze" => Some(DeferReason::Snooze),
            _ => None,
        }
    }
}

/// One due slot and its fate. Persisting this is what makes deferral survive an
/// API restart, and what makes adherence measurable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakEvent {
    pub id: BreakEventId,
    pub user_id: UserId,
    pub rule_id: BreakRuleId,
    /// The instant the cadence designated.
    pub due_at: DateTime<Utc>,
    /// When the notification actually went out. `None` while deferred, and also
    /// after a delivery failure.
    pub fired_at: Option<DateTime<Utc>>,
    pub deferred_until: Option<DateTime<Utc>>,
    pub defer_reason: Option<DeferReason>,
    /// Audit trail for "why didn't it fire".
    pub suppressed_by_meeting_id: Option<String>,
    pub outcome: BreakOutcome,
    pub responded_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
```

- [ ] **Step 8: Add the ID aliases**

In `backend/crates/domain/src/types/common.rs`, after the existing `pub type TaskLinkId = Uuid;` line:

```rust
pub type BreakRuleId = Uuid;
pub type BreakEventId = Uuid;
```

- [ ] **Step 9: Declare the modules**

In `backend/crates/domain/src/types/mod.rs`, add alongside the existing module declarations and re-exports (match whatever form the file already uses — `pub mod x;` plus `pub use x::*;`):

```rust
pub mod break_event;
pub mod break_rule;

pub use break_event::*;
pub use break_rule::*;
```

- [ ] **Step 10: Run the whole domain suite**

Run: `cd backend && cargo test -p domain`
Expected: PASS, including the 6 new tests.

- [ ] **Step 11: Commit**

```bash
git add backend/crates/domain/src/types/break_rule.rs \
        backend/crates/domain/src/types/break_event.rs \
        backend/crates/domain/src/types/common.rs \
        backend/crates/domain/src/types/mod.rs
git commit -m "Add break rule and break event domain types"
```

---

### Task 2: Migration 019, repository traits, SQLite implementations

**Files:**
- Create: `migrations/sqlite/019_create_break_rules.sql`
- Create: `backend/crates/application/src/repositories/break_repository.rs`
- Create: `backend/crates/infrastructure/src/database/break_repo.rs`
- Modify: `backend/crates/application/src/repositories/mod.rs`
- Modify: `backend/crates/infrastructure/src/database/mod.rs`
- Test: inline `#[cfg(test)] mod tests` in `break_repo.rs`, against `sqlite::memory:`

**Interfaces:**
- Consumes: `BreakRule`, `BreakEvent`, `BreakOutcome`, `DeferReason`, `BreakKind`, `BreakCadence`, `BreakUrgency` (Task 1); `RepositoryError` from `application::errors`.
- Produces: traits `BreakRuleRepository` and `BreakEventRepository` with the exact methods below; structs `SqliteBreakRuleRepository::new(pool)` and `SqliteBreakEventRepository::new(pool)`.

- [ ] **Step 1: Write the migration**

Create `migrations/sqlite/019_create_break_rules.sql`:

```sql
-- Break routine: several superposed cadences, one row each, plus one row per due slot.
--
-- The cadences overlap by construction (20/30/60 all coincide at minute 60), so
-- `priority` is not cosmetic: the engine fires at most one notification per tick and
-- marks the rest absorbed. Without it the user takes three pop-ups every hour and
-- turns the whole thing off within two days.
CREATE TABLE IF NOT EXISTS break_rules (
    id               TEXT PRIMARY KEY,
    user_id          TEXT NOT NULL,
    kind             TEXT NOT NULL CHECK (kind IN ('visual','posture','long','strength')),
    label            TEXT NOT NULL,
    body             TEXT NOT NULL,
    cadence          TEXT NOT NULL CHECK (cadence IN ('interval','daily')),
    interval_minutes INTEGER CHECK (interval_minutes IS NULL OR interval_minutes > 0),
    -- 'HH:MM', read in aplan.timezone by the application.
    at_time          TEXT,
    duration_seconds INTEGER NOT NULL CHECK (duration_seconds > 0),
    priority         INTEGER NOT NULL DEFAULT 0,
    enabled          INTEGER NOT NULL DEFAULT 1,
    urgency          TEXT NOT NULL DEFAULT 'normal' CHECK (urgency IN ('low','normal','critical')),
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    -- The exclusivity of the two cadence shapes is an invariant we do not entrust to
    -- application code alone: a rule with both set has no defined due time at all.
    CHECK ((cadence = 'interval' AND interval_minutes IS NOT NULL AND at_time IS NULL)
        OR (cadence = 'daily'    AND at_time         IS NOT NULL AND interval_minutes IS NULL))
);

CREATE INDEX IF NOT EXISTS idx_break_rules_user_enabled ON break_rules(user_id, enabled);

-- One row per due slot. This is what makes a deferral survive an API restart, and
-- what makes adherence measurable afterwards.
CREATE TABLE IF NOT EXISTS break_events (
    id                       TEXT PRIMARY KEY,
    user_id                  TEXT NOT NULL,
    rule_id                  TEXT NOT NULL REFERENCES break_rules(id) ON DELETE CASCADE,
    due_at                   TEXT NOT NULL,
    fired_at                 TEXT,
    deferred_until           TEXT,
    defer_reason             TEXT CHECK (defer_reason IS NULL OR defer_reason IN ('meeting','snooze')),
    suppressed_by_meeting_id TEXT,
    outcome                  TEXT NOT NULL
        CHECK (outcome IN ('pending','taken','snoozed','skipped','ignored','absorbed','expired')),
    responded_at             TEXT,
    created_at               TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_break_events_rule_due ON break_events(user_id, rule_id, due_at);
CREATE INDEX IF NOT EXISTS idx_break_events_outcome  ON break_events(user_id, outcome);

-- Seeded routine, straight from the ergonomics evidence. The user edits it afterwards
-- in the settings screen; the copy is French because the user reads it in a popup.
--
-- Seeded against the fixed local user id (api::state::DEFAULT_USER_ID_STR) rather than
-- `SELECT ... FROM users`: no migration ever inserts into `users`, so a row-driven seed
-- would silently produce nothing on a fresh database.
INSERT INTO break_rules (id, user_id, kind, label, body, cadence, interval_minutes, at_time,
                         duration_seconds, priority, enabled, urgency, created_at, updated_at)
VALUES
  ('11111111-1111-4111-8111-000000000001', '00000000-0000-0000-0000-000000000001', 'visual',
   'Pause visuelle', 'Regarde au loin 20 s, relâche les épaules.',
   'interval', 20, NULL, 30, 1, 1, 'low',
   '2026-08-27T00:00:00+00:00', '2026-08-27T00:00:00+00:00'),
  ('11111111-1111-4111-8111-000000000002', '00000000-0000-0000-0000-000000000001', 'posture',
   'Change de posture', 'Lève-toi, bouge, marche un instant.',
   'interval', 30, NULL, 120, 2, 1, 'normal',
   '2026-08-27T00:00:00+00:00', '2026-08-27T00:00:00+00:00'),
  ('11111111-1111-4111-8111-000000000003', '00000000-0000-0000-0000-000000000001', 'long',
   'Pause franche', 'Cinq minutes hors écran.',
   'interval', 60, NULL, 300, 3, 1, 'normal',
   '2026-08-27T00:00:00+00:00', '2026-08-27T00:00:00+00:00'),
  ('11111111-1111-4111-8111-000000000004', '00000000-0000-0000-0000-000000000001', 'strength',
   'Renfo épaule', 'Deux minutes d''élastique : rotations externes, rétractions scapulaires.',
   'daily', NULL, '14:00', 120, 4, 1, 'normal',
   '2026-08-27T00:00:00+00:00', '2026-08-27T00:00:00+00:00');
```

Fixed UUIDs and fixed timestamps: a migration must be deterministic, and SQLite has no
UUID generator. A second user starts with no rules and adds them from the screen.

- [ ] **Step 2: Write the failing repository tests**

Create `backend/crates/infrastructure/src/database/break_repo.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../../migrations/sqlite")
            .run(&pool)
            .await
            .unwrap();
        pool
    }

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 27, h, m, 0).unwrap()
    }

    fn rule(user_id: UserId, cadence: BreakCadence) -> BreakRule {
        BreakRule {
            id: Uuid::new_v4(),
            user_id,
            kind: BreakKind::Posture,
            label: "Bouge".into(),
            body: "Lève-toi".into(),
            cadence,
            duration_seconds: 120,
            priority: 2,
            enabled: true,
            urgency: BreakUrgency::Normal,
            created_at: at(8, 0),
            updated_at: at(8, 0),
        }
    }

    #[tokio::test]
    async fn insert_then_list_round_trips_an_interval_rule() {
        let pool = pool().await;
        let repo = SqliteBreakRuleRepository::new(pool);
        let user_id = Uuid::new_v4();
        let r = rule(user_id, BreakCadence::Interval { minutes: 30 });
        repo.create(&r).await.unwrap();
        let listed = repo.list(user_id).await.unwrap();
        assert_eq!(listed, vec![r]);
    }

    #[tokio::test]
    async fn insert_then_list_round_trips_a_daily_rule() {
        let pool = pool().await;
        let repo = SqliteBreakRuleRepository::new(pool);
        let user_id = Uuid::new_v4();
        let at_time = NaiveTime::from_hms_opt(14, 0, 0).unwrap();
        let r = rule(user_id, BreakCadence::Daily { at: at_time });
        repo.create(&r).await.unwrap();
        assert_eq!(repo.list(user_id).await.unwrap(), vec![r]);
    }

    #[tokio::test]
    async fn list_enabled_hides_disabled_rules() {
        let pool = pool().await;
        let repo = SqliteBreakRuleRepository::new(pool);
        let user_id = Uuid::new_v4();
        let mut off = rule(user_id, BreakCadence::Interval { minutes: 20 });
        off.enabled = false;
        repo.create(&off).await.unwrap();
        assert!(repo.list_enabled(user_id).await.unwrap().is_empty());
        assert_eq!(repo.list(user_id).await.unwrap().len(), 1);
    }

    /// The invariant the type system already enforces in memory must also hold in
    /// storage, because migrations and hand-edits bypass the type system entirely.
    #[tokio::test]
    async fn database_rejects_a_rule_carrying_both_cadence_shapes() {
        let pool = pool().await;
        let err = sqlx::query(
            "INSERT INTO break_rules (id, user_id, kind, label, body, cadence, interval_minutes,
                                      at_time, duration_seconds, priority, enabled, urgency,
                                      created_at, updated_at)
             VALUES (?, ?, 'posture', 'l', 'b', 'interval', 30, '14:00', 120, 1, 1, 'normal', ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(Uuid::new_v4().to_string())
        .bind(at(8, 0).to_rfc3339())
        .bind(at(8, 0).to_rfc3339())
        .execute(&pool)
        .await;
        assert!(err.is_err(), "CHECK must reject interval+at_time");
    }

    #[tokio::test]
    async fn update_replaces_every_editable_field() {
        let pool = pool().await;
        let repo = SqliteBreakRuleRepository::new(pool);
        let user_id = Uuid::new_v4();
        let mut r = rule(user_id, BreakCadence::Interval { minutes: 30 });
        repo.create(&r).await.unwrap();
        r.label = "Autre".into();
        r.cadence = BreakCadence::Daily { at: NaiveTime::from_hms_opt(9, 30, 0).unwrap() };
        r.enabled = false;
        r.updated_at = at(9, 0);
        repo.update(&r).await.unwrap();
        assert_eq!(repo.list(user_id).await.unwrap(), vec![r]);
    }

    #[tokio::test]
    async fn deleting_a_rule_cascades_to_its_events() {
        let pool = pool().await;
        sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await.unwrap();
        let rules = SqliteBreakRuleRepository::new(pool.clone());
        let events = SqliteBreakEventRepository::new(pool.clone());
        let user_id = Uuid::new_v4();
        let r = rule(user_id, BreakCadence::Interval { minutes: 30 });
        rules.create(&r).await.unwrap();
        let e = BreakEvent {
            id: Uuid::new_v4(),
            user_id,
            rule_id: r.id,
            due_at: at(9, 30),
            fired_at: None,
            deferred_until: None,
            defer_reason: None,
            suppressed_by_meeting_id: None,
            outcome: BreakOutcome::Pending,
            responded_at: None,
            created_at: at(9, 30),
        };
        events.create(&e).await.unwrap();
        rules.delete(user_id, r.id).await.unwrap();
        assert!(events.list_open(user_id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_open_returns_only_pending_events() {
        let pool = pool().await;
        let rules = SqliteBreakRuleRepository::new(pool.clone());
        let events = SqliteBreakEventRepository::new(pool.clone());
        let user_id = Uuid::new_v4();
        let r = rule(user_id, BreakCadence::Interval { minutes: 30 });
        rules.create(&r).await.unwrap();
        let mut open = BreakEvent {
            id: Uuid::new_v4(),
            user_id,
            rule_id: r.id,
            due_at: at(9, 30),
            fired_at: None,
            deferred_until: Some(at(10, 0)),
            defer_reason: Some(DeferReason::Meeting),
            suppressed_by_meeting_id: Some("outlook-1".into()),
            outcome: BreakOutcome::Pending,
            responded_at: None,
            created_at: at(9, 30),
        };
        events.create(&open).await.unwrap();
        let mut done = open.clone();
        done.id = Uuid::new_v4();
        done.outcome = BreakOutcome::Taken;
        events.create(&done).await.unwrap();
        assert_eq!(events.list_open(user_id).await.unwrap(), vec![open.clone()]);

        open.outcome = BreakOutcome::Expired;
        events.set_outcome(open.id, BreakOutcome::Expired, None).await.unwrap();
        assert!(events.list_open(user_id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn counts_between_groups_by_rule_and_outcome() {
        let pool = pool().await;
        let rules = SqliteBreakRuleRepository::new(pool.clone());
        let events = SqliteBreakEventRepository::new(pool.clone());
        let user_id = Uuid::new_v4();
        let r = rule(user_id, BreakCadence::Interval { minutes: 30 });
        rules.create(&r).await.unwrap();
        for (n, outcome) in [(2, BreakOutcome::Taken), (1, BreakOutcome::Ignored)] {
            for _ in 0..n {
                events
                    .create(&BreakEvent {
                        id: Uuid::new_v4(),
                        user_id,
                        rule_id: r.id,
                        due_at: at(10, 0),
                        fired_at: Some(at(10, 0)),
                        deferred_until: None,
                        defer_reason: None,
                        suppressed_by_meeting_id: None,
                        outcome,
                        responded_at: Some(at(10, 1)),
                        created_at: at(10, 0),
                    })
                    .await
                    .unwrap();
            }
        }
        let counts = events.counts_between(user_id, at(0, 0), at(23, 59)).await.unwrap();
        assert_eq!(counts.len(), 2);
        assert!(counts.contains(&(r.id, BreakOutcome::Taken, 2)));
        assert!(counts.contains(&(r.id, BreakOutcome::Ignored, 1)));
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd backend && cargo test -p infrastructure break_repo`
Expected: FAIL — `cannot find type SqliteBreakRuleRepository in this scope`.

- [ ] **Step 4: Write the repository traits**

Create `backend/crates/application/src/repositories/break_repository.rs`:

```rust
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::errors::RepositoryError;
use domain::types::*;

#[async_trait]
pub trait BreakRuleRepository: Send + Sync {
    /// Every rule, enabled or not, ordered by priority — what the settings screen shows.
    async fn list(&self, user_id: UserId) -> Result<Vec<BreakRule>, RepositoryError>;

    /// Only the enabled rules — what the tick evaluates.
    async fn list_enabled(&self, user_id: UserId) -> Result<Vec<BreakRule>, RepositoryError>;

    async fn get(&self, user_id: UserId, id: BreakRuleId)
        -> Result<Option<BreakRule>, RepositoryError>;

    async fn create(&self, rule: &BreakRule) -> Result<(), RepositoryError>;

    async fn update(&self, rule: &BreakRule) -> Result<(), RepositoryError>;

    async fn delete(&self, user_id: UserId, id: BreakRuleId) -> Result<(), RepositoryError>;
}

#[async_trait]
pub trait BreakEventRepository: Send + Sync {
    /// Events still awaiting resolution: deferred, or fired and unanswered.
    async fn list_open(&self, user_id: UserId) -> Result<Vec<BreakEvent>, RepositoryError>;

    async fn create(&self, event: &BreakEvent) -> Result<(), RepositoryError>;

    /// Resolve an event. `responded_at` is `None` for outcomes the user did not choose
    /// (absorbed, expired).
    async fn set_outcome(
        &self,
        id: BreakEventId,
        outcome: BreakOutcome,
        responded_at: Option<DateTime<Utc>>,
    ) -> Result<(), RepositoryError>;

    /// Arm or re-arm a deferral on an existing event.
    async fn set_deferral(
        &self,
        id: BreakEventId,
        until: DateTime<Utc>,
        reason: DeferReason,
        meeting_id: Option<&str>,
    ) -> Result<(), RepositoryError>;

    /// Stamp the moment the notification actually reached the daemon.
    async fn mark_fired(
        &self,
        id: BreakEventId,
        fired_at: DateTime<Utc>,
    ) -> Result<(), RepositoryError>;

    /// `(rule_id, outcome, count)` over `[from, to)`, for the stats panel.
    async fn counts_between(
        &self,
        user_id: UserId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<(BreakRuleId, BreakOutcome, i64)>, RepositoryError>;
}
```

- [ ] **Step 5: Export the traits**

In `backend/crates/application/src/repositories/mod.rs`, add alongside the existing declarations:

```rust
pub mod break_repository;
pub use break_repository::{BreakEventRepository, BreakRuleRepository};
```

- [ ] **Step 6: Implement the SQLite repositories**

Prepend to `backend/crates/infrastructure/src/database/break_repo.rs`, above the test module:

```rust
use async_trait::async_trait;
use chrono::{DateTime, NaiveTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::{BreakEventRepository, BreakRuleRepository};
use domain::types::*;

fn parse_dt(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RepositoryError::Database(format!("bad datetime '{s}': {e}")))
}

fn parse_opt_dt(s: Option<String>) -> Result<Option<DateTime<Utc>>, RepositoryError> {
    s.as_deref().map(parse_dt).transpose()
}

fn parse_uuid(s: &str) -> Result<Uuid, RepositoryError> {
    Uuid::parse_str(s).map_err(|e| RepositoryError::Database(e.to_string()))
}

/// Rebuild the cadence sum type from the two nullable columns the CHECK keeps exclusive.
fn map_cadence(row: &SqliteRow) -> Result<BreakCadence, RepositoryError> {
    let cadence: String = Row::get(row, "cadence");
    match cadence.as_str() {
        "interval" => {
            let minutes: i64 = Row::get(row, "interval_minutes");
            Ok(BreakCadence::Interval { minutes: minutes as u32 })
        }
        "daily" => {
            let at: String = Row::get(row, "at_time");
            let at = NaiveTime::parse_from_str(&at, "%H:%M")
                .map_err(|e| RepositoryError::Database(format!("bad at_time '{at}': {e}")))?;
            Ok(BreakCadence::Daily { at })
        }
        other => Err(RepositoryError::Database(format!("bad cadence '{other}'"))),
    }
}

fn map_rule(row: &SqliteRow) -> Result<BreakRule, RepositoryError> {
    let kind_str: String = Row::get(row, "kind");
    let urgency_str: String = Row::get(row, "urgency");
    let duration: i64 = Row::get(row, "duration_seconds");
    let enabled: i64 = Row::get(row, "enabled");
    Ok(BreakRule {
        id: parse_uuid(&Row::get::<String, _>(row, "id"))?,
        user_id: parse_uuid(&Row::get::<String, _>(row, "user_id"))?,
        kind: BreakKind::from_str(&kind_str)
            .ok_or_else(|| RepositoryError::Database(format!("bad kind '{kind_str}'")))?,
        label: Row::get(row, "label"),
        body: Row::get(row, "body"),
        cadence: map_cadence(row)?,
        duration_seconds: duration as u32,
        priority: Row::get::<i64, _>(row, "priority") as i32,
        enabled: enabled != 0,
        urgency: BreakUrgency::from_str(&urgency_str)
            .ok_or_else(|| RepositoryError::Database(format!("bad urgency '{urgency_str}'")))?,
        created_at: parse_dt(&Row::get::<String, _>(row, "created_at"))?,
        updated_at: parse_dt(&Row::get::<String, _>(row, "updated_at"))?,
    })
}

fn map_event(row: &SqliteRow) -> Result<BreakEvent, RepositoryError> {
    let outcome_str: String = Row::get(row, "outcome");
    let reason: Option<String> = Row::get(row, "defer_reason");
    Ok(BreakEvent {
        id: parse_uuid(&Row::get::<String, _>(row, "id"))?,
        user_id: parse_uuid(&Row::get::<String, _>(row, "user_id"))?,
        rule_id: parse_uuid(&Row::get::<String, _>(row, "rule_id"))?,
        due_at: parse_dt(&Row::get::<String, _>(row, "due_at"))?,
        fired_at: parse_opt_dt(Row::get(row, "fired_at"))?,
        deferred_until: parse_opt_dt(Row::get(row, "deferred_until"))?,
        defer_reason: reason.as_deref().and_then(DeferReason::from_str),
        suppressed_by_meeting_id: Row::get(row, "suppressed_by_meeting_id"),
        outcome: BreakOutcome::from_str(&outcome_str)
            .ok_or_else(|| RepositoryError::Database(format!("bad outcome '{outcome_str}'")))?,
        responded_at: parse_opt_dt(Row::get(row, "responded_at"))?,
        created_at: parse_dt(&Row::get::<String, _>(row, "created_at"))?,
    })
}

pub struct SqliteBreakRuleRepository {
    pool: SqlitePool,
}

impl SqliteBreakRuleRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

const RULE_COLUMNS: &str = "id, user_id, kind, label, body, cadence, interval_minutes, at_time, \
                            duration_seconds, priority, enabled, urgency, created_at, updated_at";

#[async_trait]
impl BreakRuleRepository for SqliteBreakRuleRepository {
    async fn list(&self, user_id: UserId) -> Result<Vec<BreakRule>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM break_rules WHERE user_id = ? ORDER BY priority ASC, created_at ASC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        rows.iter().map(map_rule).collect()
    }

    async fn list_enabled(&self, user_id: UserId) -> Result<Vec<BreakRule>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM break_rules WHERE user_id = ? AND enabled = 1 \
             ORDER BY priority ASC, created_at ASC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        rows.iter().map(map_rule).collect()
    }

    async fn get(
        &self,
        user_id: UserId,
        id: BreakRuleId,
    ) -> Result<Option<BreakRule>, RepositoryError> {
        let row = sqlx::query("SELECT * FROM break_rules WHERE user_id = ? AND id = ?")
            .bind(user_id.to_string())
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        row.as_ref().map(map_rule).transpose()
    }

    async fn create(&self, rule: &BreakRule) -> Result<(), RepositoryError> {
        sqlx::query(&format!(
            "INSERT INTO break_rules ({RULE_COLUMNS}) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
        ))
        .bind(rule.id.to_string())
        .bind(rule.user_id.to_string())
        .bind(rule.kind.as_str())
        .bind(&rule.label)
        .bind(&rule.body)
        .bind(match rule.cadence {
            BreakCadence::Interval { .. } => "interval",
            BreakCadence::Daily { .. } => "daily",
        })
        .bind(rule.cadence.interval_minutes().map(|m| m as i64))
        .bind(rule.cadence.at_time().map(|t| t.format("%H:%M").to_string()))
        .bind(rule.duration_seconds as i64)
        .bind(rule.priority as i64)
        .bind(i64::from(rule.enabled))
        .bind(rule.urgency.as_str())
        .bind(rule.created_at.to_rfc3339())
        .bind(rule.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn update(&self, rule: &BreakRule) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE break_rules SET kind = ?, label = ?, body = ?, cadence = ?, \
             interval_minutes = ?, at_time = ?, duration_seconds = ?, priority = ?, \
             enabled = ?, urgency = ?, updated_at = ? WHERE user_id = ? AND id = ?",
        )
        .bind(rule.kind.as_str())
        .bind(&rule.label)
        .bind(&rule.body)
        .bind(match rule.cadence {
            BreakCadence::Interval { .. } => "interval",
            BreakCadence::Daily { .. } => "daily",
        })
        .bind(rule.cadence.interval_minutes().map(|m| m as i64))
        .bind(rule.cadence.at_time().map(|t| t.format("%H:%M").to_string()))
        .bind(rule.duration_seconds as i64)
        .bind(rule.priority as i64)
        .bind(i64::from(rule.enabled))
        .bind(rule.urgency.as_str())
        .bind(rule.updated_at.to_rfc3339())
        .bind(rule.user_id.to_string())
        .bind(rule.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, user_id: UserId, id: BreakRuleId) -> Result<(), RepositoryError> {
        // Explicit event cleanup rather than relying on ON DELETE CASCADE: SQLite only
        // enforces foreign keys when `PRAGMA foreign_keys` is on, and the pool's pragma
        // state is not this repository's to assume.
        sqlx::query("DELETE FROM break_events WHERE user_id = ? AND rule_id = ?")
            .bind(user_id.to_string())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        sqlx::query("DELETE FROM break_rules WHERE user_id = ? AND id = ?")
            .bind(user_id.to_string())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }
}

pub struct SqliteBreakEventRepository {
    pool: SqlitePool,
}

impl SqliteBreakEventRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BreakEventRepository for SqliteBreakEventRepository {
    async fn list_open(&self, user_id: UserId) -> Result<Vec<BreakEvent>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM break_events WHERE user_id = ? AND outcome = 'pending' \
             ORDER BY due_at ASC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        rows.iter().map(map_event).collect()
    }

    async fn create(&self, event: &BreakEvent) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO break_events (id, user_id, rule_id, due_at, fired_at, deferred_until, \
             defer_reason, suppressed_by_meeting_id, outcome, responded_at, created_at) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(event.id.to_string())
        .bind(event.user_id.to_string())
        .bind(event.rule_id.to_string())
        .bind(event.due_at.to_rfc3339())
        .bind(event.fired_at.map(|d| d.to_rfc3339()))
        .bind(event.deferred_until.map(|d| d.to_rfc3339()))
        .bind(event.defer_reason.map(|r| r.as_str()))
        .bind(event.suppressed_by_meeting_id.as_deref())
        .bind(event.outcome.as_str())
        .bind(event.responded_at.map(|d| d.to_rfc3339()))
        .bind(event.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn set_outcome(
        &self,
        id: BreakEventId,
        outcome: BreakOutcome,
        responded_at: Option<DateTime<Utc>>,
    ) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE break_events SET outcome = ?, responded_at = ? WHERE id = ?")
            .bind(outcome.as_str())
            .bind(responded_at.map(|d| d.to_rfc3339()))
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn set_deferral(
        &self,
        id: BreakEventId,
        until: DateTime<Utc>,
        reason: DeferReason,
        meeting_id: Option<&str>,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE break_events SET deferred_until = ?, defer_reason = ?, \
             suppressed_by_meeting_id = ? WHERE id = ?",
        )
        .bind(until.to_rfc3339())
        .bind(reason.as_str())
        .bind(meeting_id)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn mark_fired(
        &self,
        id: BreakEventId,
        fired_at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE break_events SET fired_at = ?, deferred_until = NULL, defer_reason = NULL \
             WHERE id = ?",
        )
        .bind(fired_at.to_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn counts_between(
        &self,
        user_id: UserId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<(BreakRuleId, BreakOutcome, i64)>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT rule_id, outcome, COUNT(*) AS n FROM break_events \
             WHERE user_id = ? AND due_at >= ? AND due_at < ? GROUP BY rule_id, outcome",
        )
        .bind(user_id.to_string())
        .bind(from.to_rfc3339())
        .bind(to.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        rows.iter()
            .map(|row| {
                let outcome_str: String = Row::get(row, "outcome");
                Ok((
                    parse_uuid(&Row::get::<String, _>(row, "rule_id"))?,
                    BreakOutcome::from_str(&outcome_str).ok_or_else(|| {
                        RepositoryError::Database(format!("bad outcome '{outcome_str}'"))
                    })?,
                    Row::get::<i64, _>(row, "n"),
                ))
            })
            .collect()
    }
}
```

- [ ] **Step 7: Export the implementations**

In `backend/crates/infrastructure/src/database/mod.rs`, add alongside the existing declarations:

```rust
pub mod break_repo;
pub use break_repo::{SqliteBreakEventRepository, SqliteBreakRuleRepository};
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure break_repo`
Expected: PASS (8 tests).

- [ ] **Step 9: Commit**

```bash
git add migrations/sqlite/019_create_break_rules.sql \
        backend/crates/application/src/repositories/break_repository.rs \
        backend/crates/application/src/repositories/mod.rs \
        backend/crates/infrastructure/src/database/break_repo.rs \
        backend/crates/infrastructure/src/database/mod.rs
git commit -m "Add break_rules and break_events tables with repositories"
```

---

### Task 3: Natural dues and coalescing — `decide` without meetings

**Files:**
- Create: `backend/crates/domain/src/rules/breaks.rs`
- Modify: `backend/crates/domain/src/rules/mod.rs`
- Test: inline `#[cfg(test)] mod tests` in `breaks.rs`

**Interfaces:**
- Consumes: `BreakRule`, `BreakCadence`, `BreakEvent`, `BreakRuleId`, `BreakEventId`, `DeferReason` (Task 1).
- Produces: `Window`, `BusyPeriod`, `Candidate`, `FireBreak`, `DeferBreak`, `AbsorbBreak`, `BreakTick`, `BreakTickInput`, `pub fn decide(input: BreakTickInput<'_>) -> BreakTick`, and the helpers `natural_dues` / `next_natural_due_after`. Task 4 extends this same function; Task 6 calls it.

**Three refinements over the spec, settled while planning:**

0. The notification is **awaited inline by the tick**, not spawned as a detached task as
   the spec's section 3 assumed. Detaching would need the spawned task to own clones of
   the repositories and to write back concurrently with the next tick — real complexity,
   bought for nothing: because `decide` anchors on the wall clock, a tick that starts
   late loses no dues, it only delays them. `expire_after` caps the block at the rule's
   duration plus five minutes.

1. `decide` does **not** resolve `Daily` cadences itself — it cannot, since `chrono_tz` is banned in `domain`. The application resolves today's instant for each daily rule and passes them in as `daily_dues: &[(BreakRuleId, DateTime<Utc>)]`.
2. `decide` takes no `snooze` parameter. A snooze is written by the use case as an ordinary deferral with `defer_reason = Snooze`; `decide` only ever sees it later as a wake-up.

- [ ] **Step 1: Write the failing tests**

Create `backend/crates/domain/src/rules/breaks.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 27, h, m, 0).unwrap()
    }

    fn interval_rule(minutes: u32, priority: i32) -> BreakRule {
        BreakRule {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            kind: BreakKind::Posture,
            label: format!("{minutes} min"),
            body: "bouge".into(),
            cadence: BreakCadence::Interval { minutes },
            duration_seconds: 60,
            priority,
            enabled: true,
            urgency: BreakUrgency::Normal,
            created_at: at(0, 0),
            updated_at: at(0, 0),
        }
    }

    fn morning() -> Vec<Window> {
        vec![Window { start: at(8, 0), end: at(12, 0) }]
    }

    fn input<'a>(
        now: DateTime<Utc>,
        since: DateTime<Utc>,
        windows: &'a [Window],
        rules: &'a [BreakRule],
    ) -> BreakTickInput<'a> {
        BreakTickInput {
            now,
            since,
            windows,
            rules,
            daily_dues: &[],
            busy: &[],
            open: &[],
            grace: Duration::minutes(3),
        }
    }

    /// The clock is anchored on the window, not on the last fire. That is what makes
    /// it a wall clock: 08:20, 08:40, 09:00 … whatever happened in between.
    #[test]
    fn interval_dues_are_anchored_on_the_window_start() {
        let rules = vec![interval_rule(20, 1)];
        let w = morning();
        let dues = natural_dues(&rules[0], &w, at(8, 0), at(9, 0));
        assert_eq!(dues, vec![at(8, 20), at(8, 40), at(9, 0)]);
    }

    /// Arriving at 08:00 does not earn a break at 08:00.
    #[test]
    fn the_first_due_of_a_window_is_one_interval_in() {
        let rules = vec![interval_rule(30, 1)];
        let w = morning();
        assert_eq!(natural_dues(&rules[0], &w, at(7, 0), at(8, 0)), vec![]);
        assert_eq!(natural_dues(&rules[0], &w, at(8, 0), at(8, 30)), vec![at(8, 30)]);
    }

    #[test]
    fn each_window_re_anchors_on_its_own_start() {
        let windows = vec![
            Window { start: at(8, 0), end: at(12, 0) },
            Window { start: at(13, 0), end: at(17, 0) },
        ];
        let rule = interval_rule(30, 1);
        assert_eq!(natural_dues(&rule, &windows, at(12, 0), at(13, 45)), vec![at(13, 30)]);
    }

    #[test]
    fn dues_never_fall_outside_their_window() {
        let rule = interval_rule(30, 1);
        let w = morning();
        // 12:00 is the window end and is included; 12:30 is not.
        assert_eq!(natural_dues(&rule, &w, at(11, 45), at(13, 0)), vec![at(12, 0)]);
    }

    /// The collision the whole `priority` column exists for: at minute 60 the three
    /// interval rules are due together and the user must see exactly one popup.
    #[test]
    fn simultaneous_dues_collapse_to_the_highest_priority() {
        let rules = vec![interval_rule(20, 1), interval_rule(30, 2), interval_rule(60, 3)];
        let w = morning();
        let tick = decide(input(at(9, 0), at(8, 59), &w, &rules));
        let fired = tick.fire.expect("one break fires");
        assert_eq!(fired.candidate.rule_id(), rules[2].id, "the hourly rule wins");
        assert_eq!(tick.absorb.len(), 2);
        assert!(tick.defer.is_empty());
    }

    /// After a suspend the tick interval can span hours. Six missed dues must not
    /// become six popups.
    #[test]
    fn a_long_gap_fires_once_and_absorbs_the_rest() {
        let rules = vec![interval_rule(20, 1)];
        let w = morning();
        let tick = decide(input(at(11, 0), at(9, 0), &w, &rules));
        assert!(tick.fire.is_some());
        assert_eq!(tick.absorb.len(), 5, "08:00-anchored dues 09:20..11:00 minus the one fired");
    }

    #[test]
    fn outside_every_window_nothing_fires() {
        let rules = vec![interval_rule(20, 1)];
        let w = morning();
        let tick = decide(input(at(19, 0), at(18, 0), &w, &rules));
        assert!(tick.fire.is_none());
        assert!(tick.absorb.is_empty());
    }

    /// A non-working day has no windows at all.
    #[test]
    fn a_day_with_no_windows_fires_nothing() {
        let rules = vec![interval_rule(20, 1)];
        let tick = decide(input(at(10, 0), at(9, 0), &[], &rules));
        assert!(tick.fire.is_none());
    }

    #[test]
    fn disabled_rules_are_the_callers_problem_not_ours() {
        // `rules` is documented as already filtered; passing an empty slice must be inert.
        let tick = decide(input(at(9, 0), at(8, 0), &morning(), &[]));
        assert!(tick.fire.is_none());
    }

    /// Daily rules arrive pre-resolved because `domain` cannot know the timezone.
    #[test]
    fn a_daily_due_inside_the_window_fires() {
        let rule = BreakRule {
            cadence: BreakCadence::Daily {
                at: chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            },
            ..interval_rule(0, 9)
        };
        let rules = vec![rule.clone()];
        let w = morning();
        let daily = vec![(rule.id, at(10, 0))];
        let tick = decide(BreakTickInput {
            now: at(10, 1),
            since: at(9, 59),
            windows: &w,
            rules: &rules,
            daily_dues: &daily,
            busy: &[],
            open: &[],
            grace: Duration::minutes(3),
        });
        assert_eq!(tick.fire.expect("fires").candidate.rule_id(), rule.id);
    }

    #[test]
    fn next_natural_due_after_walks_forward_within_the_window() {
        let rule = interval_rule(20, 1);
        let w = morning();
        assert_eq!(next_natural_due_after(&rule, &w, at(8, 25)), Some(at(8, 40)));
        // Past the last due of the window, the next one is in the following window.
        let windows = vec![
            Window { start: at(8, 0), end: at(12, 0) },
            Window { start: at(13, 0), end: at(17, 0) },
        ];
        assert_eq!(next_natural_due_after(&rule, &windows, at(12, 0)), Some(at(13, 20)));
        assert_eq!(next_natural_due_after(&rule, &w, at(12, 0)), None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test -p domain breaks`
Expected: FAIL — `cannot find function natural_dues in this scope`.

- [ ] **Step 3: Implement the types and the meeting-free path**

Prepend to `backend/crates/domain/src/rules/breaks.rs`, above the test module:

```rust
use chrono::{DateTime, Duration, Utc};

use crate::types::{BreakCadence, BreakEvent, BreakEventId, BreakRule, BreakRuleId, DeferReason};

/// A stretch of working time, already resolved to UTC by the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl Window {
    fn contains(&self, t: DateTime<Utc>) -> bool {
        self.start < t && t <= self.end
    }
}

/// A meeting that suppresses breaks. The caller has already filtered on `show_as`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusyPeriod {
    pub meeting_id: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl BusyPeriod {
    fn covers(&self, t: DateTime<Utc>) -> bool {
        self.start <= t && t < self.end
    }
}

/// Something that wants to fire on this tick: either a due that has no row yet, or a
/// deferred event whose wait is over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Candidate {
    New { rule_id: BreakRuleId, due_at: DateTime<Utc> },
    Wake { event_id: BreakEventId, rule_id: BreakRuleId, due_at: DateTime<Utc> },
}

impl Candidate {
    pub fn rule_id(&self) -> BreakRuleId {
        match self {
            Candidate::New { rule_id, .. } | Candidate::Wake { rule_id, .. } => *rule_id,
        }
    }

    pub fn due_at(&self) -> DateTime<Utc> {
        match self {
            Candidate::New { due_at, .. } | Candidate::Wake { due_at, .. } => *due_at,
        }
    }

    pub fn event_id(&self) -> Option<BreakEventId> {
        match self {
            Candidate::New { .. } => None,
            Candidate::Wake { event_id, .. } => Some(*event_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireBreak {
    pub candidate: Candidate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferBreak {
    pub candidate: Candidate,
    pub until: DateTime<Utc>,
    pub reason: DeferReason,
    pub meeting_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsorbBreak {
    pub candidate: Candidate,
}

/// Everything one tick decided. The caller only has to execute it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BreakTick {
    /// At most one, always: the user sees one popup per tick or none.
    pub fire: Option<FireBreak>,
    pub defer: Vec<DeferBreak>,
    pub absorb: Vec<AbsorbBreak>,
    pub expire: Vec<BreakEventId>,
}

pub struct BreakTickInput<'a> {
    pub now: DateTime<Utc>,
    /// The previous tick. `(since, now]` is the interval examined, which is what makes
    /// the engine survive a suspend or a restart without firing a burst.
    pub since: DateTime<Utc>,
    /// Today's working windows in UTC. Empty on a non-working day.
    pub windows: &'a [Window],
    /// Enabled rules only — filtering is the caller's job.
    pub rules: &'a [BreakRule],
    /// Today's UTC instant for each enabled `Daily` rule, resolved by the caller
    /// because `domain` has no timezone database.
    pub daily_dues: &'a [(BreakRuleId, DateTime<Utc>)],
    /// Meetings already filtered on `show_as`.
    pub busy: &'a [BusyPeriod],
    /// Events still pending: deferred, or fired and unanswered.
    pub open: &'a [BreakEvent],
    pub grace: Duration,
}

/// Every instant `rule` comes due inside `(since, now]`, anchored on each window's start.
///
/// Anchoring on the window rather than on the last fire is the whole meaning of "wall
/// clock": a break that was missed, snoozed or absorbed does not shift the grid.
pub fn natural_dues(
    rule: &BreakRule,
    windows: &[Window],
    since: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Vec<DateTime<Utc>> {
    let Some(minutes) = rule.cadence.interval_minutes() else {
        return Vec::new();
    };
    let step = Duration::minutes(minutes as i64);
    let mut out = Vec::new();
    for w in windows {
        let mut due = w.start + step;
        while due <= w.end {
            if due > since && due <= now {
                out.push(due);
            }
            if due > now {
                break;
            }
            due = due + step;
        }
    }
    out.sort();
    out
}

/// The first instant after `t` at which `rule` next comes due, if any remains today.
///
/// `None` for a `Daily` rule: it has no "next" today, so a deferral of it is never
/// culled by the expiry rule and only ends at the close of the working day.
pub fn next_natural_due_after(
    rule: &BreakRule,
    windows: &[Window],
    t: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let minutes = rule.cadence.interval_minutes()?;
    let step = Duration::minutes(minutes as i64);
    for w in windows {
        let mut due = w.start + step;
        while due <= w.end {
            if due > t {
                return Some(due);
            }
            due = due + step;
        }
    }
    None
}

pub fn decide(input: BreakTickInput<'_>) -> BreakTick {
    let mut tick = BreakTick::default();

    // 1. Outside every working window nothing fires, and whatever was still waiting is
    //    cleaned up: a break deferred at 17:55 has no meaning at 19:00.
    let in_window = input.windows.iter().any(|w| w.contains(input.now));
    if !in_window {
        tick.expire = input.open.iter().map(|e| e.id).collect();
        return tick;
    }

    // 2. Candidates: natural dues in (since, now], plus today's daily dues.
    let mut candidates: Vec<Candidate> = Vec::new();
    for rule in input.rules {
        for due_at in natural_dues(rule, input.windows, input.since, input.now) {
            candidates.push(Candidate::New { rule_id: rule.id, due_at });
        }
    }
    for (rule_id, due_at) in input.daily_dues {
        let inside = input.windows.iter().any(|w| w.contains(*due_at));
        if inside && *due_at > input.since && *due_at <= input.now {
            candidates.push(Candidate::New { rule_id: *rule_id, due_at: *due_at });
        }
    }

    // 3. Coalescing: the highest priority fires, the rest are absorbed. Ties go to the
    //    oldest due, so a backlog drains in order.
    finish(&mut tick, candidates, input.rules);
    tick
}

/// Pick the one candidate that fires and absorb the others.
fn finish(tick: &mut BreakTick, mut candidates: Vec<Candidate>, rules: &[BreakRule]) {
    if candidates.is_empty() {
        return;
    }
    let priority_of = |c: &Candidate| {
        rules
            .iter()
            .find(|r| r.id == c.rule_id())
            .map(|r| r.priority)
            .unwrap_or(i32::MIN)
    };
    candidates.sort_by(|a, b| {
        priority_of(b)
            .cmp(&priority_of(a))
            .then(a.due_at().cmp(&b.due_at()))
    });
    let winner = candidates.remove(0);
    tick.fire = Some(FireBreak { candidate: winner });
    tick.absorb = candidates.into_iter().map(|c| AbsorbBreak { candidate: c }).collect();
}
```

- [ ] **Step 4: Declare the module**

In `backend/crates/domain/src/rules/mod.rs`, add alongside the existing declarations:

```rust
pub mod breaks;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd backend && cargo test -p domain breaks`
Expected: PASS (10 tests).

- [ ] **Step 6: Commit**

```bash
git add backend/crates/domain/src/rules/breaks.rs backend/crates/domain/src/rules/mod.rs
git commit -m "Add break cadence engine: natural dues and collision coalescing"
```

---

### Task 4: Meeting suppression, deferral, expiry

**Files:**
- Modify: `backend/crates/domain/src/rules/breaks.rs` (extend `decide`; add tests)

**Interfaces:**
- Consumes: everything Task 3 produced.
- Produces: no new public names. `decide` now populates `BreakTick::defer` and `BreakTick::expire`, and honours `BreakTickInput::busy` / `open` / `grace`.

- [ ] **Step 1: Write the failing tests**

Append to the existing `mod tests` in `backend/crates/domain/src/rules/breaks.rs`:

```rust
    fn busy(id: &str, from: (u32, u32), to: (u32, u32)) -> BusyPeriod {
        BusyPeriod {
            meeting_id: id.into(),
            start: at(from.0, from.1),
            end: at(to.0, to.1),
        }
    }

    fn open_event(rule_id: BreakRuleId, due: DateTime<Utc>, until: DateTime<Utc>) -> BreakEvent {
        BreakEvent {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            rule_id,
            due_at: due,
            fired_at: None,
            deferred_until: Some(until),
            defer_reason: Some(DeferReason::Meeting),
            suppressed_by_meeting_id: Some("m1".into()),
            outcome: crate::types::BreakOutcome::Pending,
            responded_at: None,
            created_at: due,
        }
    }

    #[test]
    fn a_due_inside_a_meeting_is_deferred_to_its_end_plus_grace() {
        let rules = vec![interval_rule(60, 3)];
        let w = morning();
        let busy = vec![busy("m1", (8, 50), (9, 50))];
        let tick = decide(BreakTickInput {
            now: at(9, 1),
            since: at(8, 59),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &busy,
            open: &[],
            grace: Duration::minutes(3),
        });
        assert!(tick.fire.is_none(), "nothing fires during the meeting");
        assert_eq!(tick.defer.len(), 1);
        assert_eq!(tick.defer[0].until, at(9, 53));
        assert_eq!(tick.defer[0].reason, DeferReason::Meeting);
        assert_eq!(tick.defer[0].meeting_id.as_deref(), Some("m1"));
    }

    /// The point of deferring rather than skipping: an hour of meeting must not cost
    /// the hourly break.
    #[test]
    fn a_deferred_break_fires_when_its_wait_is_over() {
        let rules = vec![interval_rule(60, 3)];
        let w = morning();
        let ev = open_event(rules[0].id, at(9, 0), at(9, 53));
        let open = vec![ev.clone()];
        let tick = decide(BreakTickInput {
            now: at(9, 53),
            since: at(9, 52),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &[],
            open: &open,
            grace: Duration::minutes(3),
        });
        assert_eq!(tick.fire.expect("fires").candidate.event_id(), Some(ev.id));
        assert!(tick.expire.is_empty());
    }

    /// Back-to-back calls: the wake-up lands inside the next meeting and is re-deferred
    /// onto it rather than firing over it.
    #[test]
    fn a_wake_up_inside_another_meeting_is_re_deferred() {
        let rules = vec![interval_rule(60, 3)];
        let w = morning();
        let ev = open_event(rules[0].id, at(9, 0), at(9, 53));
        let open = vec![ev.clone()];
        let busy = vec![busy("m2", (9, 50), (10, 30))];
        let tick = decide(BreakTickInput {
            now: at(9, 53),
            since: at(9, 52),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &busy,
            open: &open,
            grace: Duration::minutes(3),
        });
        assert!(tick.fire.is_none());
        assert_eq!(tick.defer.len(), 1);
        assert_eq!(tick.defer[0].until, at(10, 33));
        assert_eq!(tick.defer[0].meeting_id.as_deref(), Some("m2"));
    }

    /// A deferral that can no longer beat its own rule's next due is pointless: the
    /// fresh one is 4 minutes away. This is what stops deferrals piling up without
    /// having to count them.
    #[test]
    fn a_deferral_overtaken_by_the_next_natural_due_expires() {
        let rules = vec![interval_rule(20, 1)];
        let w = morning();
        // Due at 09:00, meeting ran to 10:50, so the wait ends at 10:53 — long past
        // the 09:20 that would have replaced it.
        let ev = open_event(rules[0].id, at(9, 0), at(10, 53));
        let open = vec![ev.clone()];
        let tick = decide(BreakTickInput {
            now: at(10, 53),
            since: at(10, 52),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &[],
            open: &open,
            grace: Duration::minutes(3),
        });
        assert_eq!(tick.expire, vec![ev.id]);
        assert!(tick.fire.is_none() || tick.fire.as_ref().unwrap().candidate.event_id() != Some(ev.id));
    }

    /// Only one deferral per rule may be alive. The case that needs the guard is a
    /// single tick carrying several dues of the same rule — after a suspend, or simply
    /// a long meeting — which must arm one deferral, not three.
    ///
    /// (Across *separate* ticks the expiry rule already does this on its own: each new
    /// due overtakes the previous deferral and replaces it. The guard is what covers
    /// several dues arriving at once.)
    #[test]
    fn several_dues_inside_one_meeting_arm_only_one_deferral() {
        let rules = vec![interval_rule(20, 1)];
        let w = morning();
        let busy = vec![busy("m1", (8, 55), (11, 30))];
        let tick = decide(BreakTickInput {
            now: at(10, 0),
            since: at(9, 0),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &busy,
            open: &[],
            grace: Duration::minutes(3),
        });
        assert_eq!(tick.defer.len(), 1, "one deferral for the rule");
        assert_eq!(tick.defer[0].until, at(11, 33));
        assert_eq!(tick.absorb.len(), 2, "the 09:40 and 10:00 dues are absorbed");
    }

    /// A deferral computed into the past resolves on this very tick instead of writing
    /// a row that is already overdue.
    #[test]
    fn a_due_inside_a_meeting_that_has_since_ended_fires_now() {
        let rules = vec![interval_rule(60, 3)];
        let w = morning();
        let busy = vec![busy("m1", (8, 50), (9, 50))];
        let tick = decide(BreakTickInput {
            now: at(10, 30),
            since: at(8, 30),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &busy,
            open: &[],
            grace: Duration::minutes(3),
        });
        assert!(tick.fire.is_some(), "the 09:00 due, unblocked, resolves immediately");
        assert!(tick.defer.is_empty());
    }

    #[test]
    fn the_end_of_the_day_expires_everything_still_waiting() {
        let rules = vec![interval_rule(20, 1)];
        let w = morning();
        let ev = open_event(rules[0].id, at(11, 40), at(11, 58));
        let open = vec![ev.clone()];
        let tick = decide(BreakTickInput {
            now: at(12, 30),
            since: at(12, 29),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &[],
            open: &open,
            grace: Duration::minutes(3),
        });
        assert_eq!(tick.expire, vec![ev.id]);
    }

    /// A snooze is not a special case: it is a deferral with another reason, and it
    /// takes the same path — expiry included.
    #[test]
    fn a_snoozed_event_wakes_like_any_other_deferral() {
        let rules = vec![interval_rule(60, 3)];
        let w = morning();
        let mut ev = open_event(rules[0].id, at(9, 0), at(9, 10));
        ev.defer_reason = Some(DeferReason::Snooze);
        ev.suppressed_by_meeting_id = None;
        let open = vec![ev.clone()];
        let tick = decide(BreakTickInput {
            now: at(9, 10),
            since: at(9, 9),
            windows: &w,
            rules: &rules,
            daily_dues: &[],
            busy: &[],
            open: &open,
            grace: Duration::minutes(3),
        });
        assert_eq!(tick.fire.expect("fires").candidate.event_id(), Some(ev.id));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test -p domain breaks`
Expected: FAIL — the new tests fail (deferrals are never produced; `tick.defer` is empty).

- [ ] **Step 3: Extend `decide`**

Replace the body of `decide` in `backend/crates/domain/src/rules/breaks.rs` with:

```rust
pub fn decide(input: BreakTickInput<'_>) -> BreakTick {
    let mut tick = BreakTick::default();

    // 1. Outside every working window nothing fires, and whatever was still waiting is
    //    cleaned up: a break deferred at 17:55 has no meaning at 19:00.
    let in_window = input.windows.iter().any(|w| w.contains(input.now));
    if !in_window {
        tick.expire = input.open.iter().map(|e| e.id).collect();
        return tick;
    }

    let rule_of = |id: BreakRuleId| input.rules.iter().find(|r| r.id == id);

    // 2. Cull deferrals that can no longer beat their own rule's next due, plus any
    //    whose rule has since been disabled or deleted. Doing this before anything else
    //    is what keeps "one live deferral per rule" true in step 4 without counting.
    let mut live_deferrals: Vec<&BreakEvent> = Vec::new();
    for event in input.open {
        let Some(rule) = rule_of(event.rule_id) else {
            tick.expire.push(event.id);
            continue;
        };
        let Some(until) = event.deferred_until else {
            // Fired and unanswered: the notifier owns its fate, not the tick.
            continue;
        };
        match next_natural_due_after(rule, input.windows, event.due_at) {
            Some(next) if until >= next => tick.expire.push(event.id),
            _ => live_deferrals.push(event),
        }
    }

    // 3. Candidates: woken deferrals, then natural dues in (since, now], then the daily
    //    instants the caller resolved.
    let mut candidates: Vec<Candidate> = Vec::new();
    for event in &live_deferrals {
        if let Some(until) = event.deferred_until {
            if until <= input.now {
                candidates.push(Candidate::Wake {
                    event_id: event.id,
                    rule_id: event.rule_id,
                    due_at: event.due_at,
                });
            }
        }
    }
    for rule in input.rules {
        for due_at in natural_dues(rule, input.windows, input.since, input.now) {
            candidates.push(Candidate::New { rule_id: rule.id, due_at });
        }
    }
    for (rule_id, due_at) in input.daily_dues {
        let inside = input.windows.iter().any(|w| w.contains(*due_at));
        if inside && *due_at > input.since && *due_at <= input.now {
            candidates.push(Candidate::New { rule_id: *rule_id, due_at: *due_at });
        }
    }

    // 4. Meeting suppression. A candidate is judged at the instant it wants the user's
    //    attention: its due time for a fresh one, `now` for a wake-up.
    //
    //    Rules that already hold a live deferral do not get a second one — their extra
    //    dues are absorbed. Without that, a two-hour meeting would arm six deferrals of
    //    the 20-minute rule and empty them all at once when it ended.
    let mut deferred_rules: Vec<BreakRuleId> = live_deferrals.iter().map(|e| e.rule_id).collect();
    let mut runnable: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        let judged_at = match &candidate {
            Candidate::New { due_at, .. } => *due_at,
            Candidate::Wake { .. } => input.now,
        };
        let Some(period) = input.busy.iter().find(|b| b.covers(judged_at)) else {
            runnable.push(candidate);
            continue;
        };
        let until = period.end + input.grace;
        if until <= input.now {
            // The blocking meeting is already over; resolve on this tick rather than
            // writing a deferral that is born overdue.
            runnable.push(candidate);
            continue;
        }
        let already = deferred_rules.contains(&candidate.rule_id())
            && matches!(candidate, Candidate::New { .. });
        if already {
            tick.absorb.push(AbsorbBreak { candidate });
            continue;
        }
        deferred_rules.push(candidate.rule_id());
        tick.defer.push(DeferBreak {
            candidate,
            until,
            reason: DeferReason::Meeting,
            meeting_id: Some(period.meeting_id.clone()),
        });
    }

    // 5. Coalescing: the highest priority fires, the rest are absorbed. Ties go to the
    //    oldest due, so a backlog drains in order.
    finish(&mut tick, runnable, input.rules);
    tick
}
```

`finish` must now append rather than overwrite, since step 4 may already have absorbed candidates. Replace its last line:

```rust
    tick.absorb
        .extend(candidates.into_iter().map(|c| AbsorbBreak { candidate: c }));
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd backend && cargo test -p domain breaks`
Expected: PASS (18 tests — the 10 from Task 3 plus 8 new).

- [ ] **Step 5: Run the whole domain suite for regressions**

Run: `cd backend && cargo test -p domain`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/crates/domain/src/rules/breaks.rs
git commit -m "Add meeting suppression, deferral and expiry to the break engine"
```

---

### Task 5: Notifier trait and the notify-send adapter

**Files:**
- Create: `backend/crates/application/src/services/notifier.rs`
- Create: `backend/crates/infrastructure/src/notify/mod.rs`
- Create: `backend/crates/infrastructure/src/notify/notify_send.rs`
- Modify: `backend/crates/application/src/services/mod.rs`
- Modify: `backend/crates/infrastructure/src/lib.rs`
- Test: inline `#[cfg(test)] mod tests` in `notify_send.rs`

**Interfaces:**
- Consumes: `AppError` from `application::errors`; `BreakUrgency` (Task 1).
- Produces: `Notification`, `NotificationOutcome`, trait `Notifier`, `NullNotifier`, `NotifySendNotifier::new()`, and the two pure helpers `command_args(&Notification) -> Vec<String>` and `parse_outcome(stdout: &str) -> NotificationOutcome`. Task 6 depends on `Notifier` and `NotificationOutcome`; Task 7 constructs `NotifySendNotifier`.

- [ ] **Step 1: Write the trait**

Create `backend/crates/application/src/services/notifier.rs`:

```rust
use async_trait::async_trait;
use std::time::Duration;

use crate::errors::AppError;
use domain::types::BreakUrgency;

/// A desktop notification with optional buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub urgency: BreakUrgency,
    pub icon: Option<String>,
    /// How long to wait for an answer before giving up and closing.
    pub expire_after: Duration,
    /// `(key, label)` — the key is what comes back in `NotificationOutcome::Action`.
    pub actions: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationOutcome {
    /// The user pressed a button; carries its key.
    Action(String),
    /// Closed without choosing.
    Dismissed,
    /// Never answered within `expire_after`.
    Expired,
}

/// Delivers a notification and waits for what the user does about it.
///
/// Implementations block for as long as the notification is on screen, so callers must
/// treat `notify` as long-running and never hold a tick open on it.
#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, n: Notification) -> Result<NotificationOutcome, AppError>;
}

/// Records nothing, shows nothing, always reports a dismissal.
///
/// Used in tests, and selected at wiring time when no session bus is reachable: a
/// headless API must still keep its books, and must not spam the log every 30 seconds
/// with a failure it cannot fix.
pub struct NullNotifier;

#[async_trait]
impl Notifier for NullNotifier {
    async fn notify(&self, _n: Notification) -> Result<NotificationOutcome, AppError> {
        Ok(NotificationOutcome::Dismissed)
    }
}
```

- [ ] **Step 2: Export it**

In `backend/crates/application/src/services/mod.rs`, add:

```rust
pub mod notifier;
pub use notifier::{Notification, NotificationOutcome, Notifier, NullNotifier};
```

- [ ] **Step 3: Write the failing tests for the adapter**

Create `backend/crates/infrastructure/src/notify/notify_send.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sample() -> Notification {
        Notification {
            title: "Pause visuelle".into(),
            body: "Regarde au loin 20 s.".into(),
            urgency: BreakUrgency::Low,
            icon: Some("appointment-soon".into()),
            expire_after: Duration::from_secs(90),
            actions: vec![
                ("taken".into(), "Pris".into()),
                ("snoozed".into(), "Plus tard".into()),
                ("skipped".into(), "Passer".into()),
            ],
        }
    }

    #[test]
    fn args_carry_app_name_urgency_icon_and_every_action() {
        let args = command_args(&sample());
        assert!(args.contains(&"--app-name=aplan".to_string()));
        assert!(args.contains(&"--urgency=low".to_string()));
        assert!(args.contains(&"--icon=appointment-soon".to_string()));
        assert!(args.contains(&"--action=taken=Pris".to_string()));
        assert!(args.contains(&"--action=snoozed=Plus tard".to_string()));
        assert!(args.contains(&"--action=skipped=Passer".to_string()));
        // Title and body are positional and must come last, in that order.
        assert_eq!(args[args.len() - 2], "Pause visuelle");
        assert_eq!(args[args.len() - 1], "Regarde au loin 20 s.");
    }

    #[test]
    fn args_omit_the_icon_when_there_is_none() {
        let mut n = sample();
        n.icon = None;
        assert!(!command_args(&n).iter().any(|a| a.starts_with("--icon")));
    }

    /// `notify-send` prints the chosen action key on stdout, and nothing at all when
    /// the notification is dismissed.
    #[test]
    fn stdout_maps_to_an_outcome() {
        assert_eq!(parse_outcome("taken\n"), NotificationOutcome::Action("taken".into()));
        assert_eq!(parse_outcome("snoozed"), NotificationOutcome::Action("snoozed".into()));
        assert_eq!(parse_outcome(""), NotificationOutcome::Dismissed);
        assert_eq!(parse_outcome("   \n"), NotificationOutcome::Dismissed);
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cd backend && cargo test -p infrastructure notify_send`
Expected: FAIL — `cannot find function command_args in this scope`.

- [ ] **Step 5: Implement the adapter**

Prepend to `backend/crates/infrastructure/src/notify/notify_send.rs`:

```rust
use async_trait::async_trait;
use tokio::process::Command;

use application::errors::AppError;
use application::services::{Notification, NotificationOutcome, Notifier};
use domain::types::BreakUrgency;

/// Build the `notify-send` argv for a notification.
///
/// Split out as a pure function on purpose: this is the part with actual logic, and
/// it can be tested without a session bus. The spawn below is a three-line shell.
pub fn command_args(n: &Notification) -> Vec<String> {
    let mut args = vec![
        "--app-name=aplan".to_string(),
        format!("--urgency={}", n.urgency.as_str()),
        format!("--expire-time={}", n.expire_after.as_millis()),
    ];
    if let Some(icon) = &n.icon {
        args.push(format!("--icon={icon}"));
    }
    for (key, label) in &n.actions {
        args.push(format!("--action={key}={label}"));
    }
    args.push(n.title.clone());
    args.push(n.body.clone());
    args
}

/// `notify-send` writes the chosen action's key to stdout, and writes nothing when the
/// notification was closed without a choice.
pub fn parse_outcome(stdout: &str) -> NotificationOutcome {
    let key = stdout.trim();
    if key.is_empty() {
        NotificationOutcome::Dismissed
    } else {
        NotificationOutcome::Action(key.to_string())
    }
}

/// Delivers through `notify-send`, which `--action` puts into `--wait` mode: the
/// process stays alive until the user answers or the notification expires.
pub struct NotifySendNotifier;

impl NotifySendNotifier {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NotifySendNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Notifier for NotifySendNotifier {
    async fn notify(&self, n: Notification) -> Result<NotificationOutcome, AppError> {
        let output = Command::new("notify-send")
            .args(command_args(&n))
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("notify-send failed to run: {e}")))?;
        if !output.status.success() {
            return Err(AppError::Internal(format!(
                "notify-send exited with {}",
                output.status
            )));
        }
        Ok(parse_outcome(&String::from_utf8_lossy(&output.stdout)))
    }
}
```

`AppError` has **no catch-all variant today** — its variants are `Domain`, `Repository`, `Connector`, `Configuration`, `NotFound`, `Ambiguous`, `Validation`, and none of them describes "the notification daemon could not be reached". Add one to `backend/crates/application/src/errors.rs`, beside the others:

```rust
    #[error("Internal error: {0}")]
    Internal(String),
```

Stage `errors.rs` with this task's commit.

- [ ] **Step 6: Create the module and export it**

Create `backend/crates/infrastructure/src/notify/mod.rs`:

```rust
pub mod notify_send;
pub use notify_send::{command_args, parse_outcome, NotifySendNotifier};
```

In `backend/crates/infrastructure/src/lib.rs`, add `pub mod notify;` alongside the existing module declarations.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure notify_send`
Expected: PASS (3 tests).

- [ ] **Step 8: Commit**

```bash
git add backend/crates/application/src/services/notifier.rs \
        backend/crates/application/src/services/mod.rs \
        backend/crates/infrastructure/src/notify/ \
        backend/crates/infrastructure/src/lib.rs
git commit -m "Add Notifier trait with a notify-send adapter and a null implementation"
```

---

### Task 6: The `run_break_tick` use case

**Files:**
- Create: `backend/crates/application/src/use_cases/breaks.rs`
- Modify: `backend/crates/application/src/use_cases/mod.rs`
- Test: inline `#[cfg(test)] mod tests` in `breaks.rs`, with in-memory fakes

**Interfaces:**
- Consumes: `decide`, `Window`, `BusyPeriod`, `BreakTickInput` (Tasks 3–4); `BreakRuleRepository`, `BreakEventRepository` (Task 2); `Notifier`, `Notification`, `NotificationOutcome` (Task 5); `ConfigRepository`, `MeetingRepository`; `application::time::{resolve_tz, local_to_utc, to_local}`.
- Produces: `pub struct BreakTickDeps<'a>`, `pub async fn run_break_tick(deps: BreakTickDeps<'_>, user_id: UserId, now: DateTime<Utc>) -> Result<BreakTickReport, AppError>`, `pub struct BreakTickReport { pub fired: Option<BreakEventId>, pub deferred: usize, pub absorbed: usize, pub expired: usize }`, and `pub async fn resolve_windows(...) -> Result<Vec<Window>, AppError>`. Task 7 calls `run_break_tick`.

- [ ] **Step 1: Write the failing tests**

Create `backend/crates/application/src/use_cases/breaks.rs` with only the test module. The fakes are deliberately in-memory rather than SQLite: this task tests orchestration, and Task 2 already proved the SQL.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeNotifier {
        sent: Mutex<Vec<Notification>>,
        answer: Mutex<Option<NotificationOutcome>>,
    }

    #[async_trait::async_trait]
    impl Notifier for FakeNotifier {
        async fn notify(&self, n: Notification) -> Result<NotificationOutcome, AppError> {
            self.sent.lock().unwrap().push(n);
            Ok(self
                .answer
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(NotificationOutcome::Dismissed))
        }
    }

    // InMemoryBreakRuleRepository / InMemoryBreakEventRepository / InMemoryConfigRepository
    // / InMemoryMeetingRepository: straightforward Mutex<Vec<_>> implementations of the
    // four traits, each method operating on the vector. Write them here in full; they are
    // ~120 lines of mechanical code with no branching.

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 27, h, m, 0).unwrap()
    }

    /// The happy path end to end: a due arrives, a row is written, the notification goes
    /// out, and the user's answer lands back on the row.
    #[tokio::test]
    async fn a_fired_break_is_recorded_and_the_answer_is_written_back() {
        let fixture = Fixture::new().await;                    // seeds one 30-min rule
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("taken".into()));
        fixture.set_last_tick(at(8, 29)).await;

        let report = fixture.tick(at(8, 30)).await.unwrap();

        assert!(report.fired.is_some());
        assert_eq!(fixture.notifier.sent.lock().unwrap().len(), 1);
        let events = fixture.all_events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].outcome, BreakOutcome::Taken);
        assert!(events[0].fired_at.is_some());
        assert!(events[0].responded_at.is_some());
    }

    /// Dismissing without choosing is `ignored`, not `skipped`: the distinction is the
    /// whole reason both outcomes exist.
    #[tokio::test]
    async fn a_dismissed_notification_is_recorded_as_ignored() {
        let fixture = Fixture::new().await;
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Dismissed);
        fixture.set_last_tick(at(8, 29)).await;
        fixture.tick(at(8, 30)).await.unwrap();
        assert_eq!(fixture.all_events().await[0].outcome, BreakOutcome::Ignored);
    }

    /// "Plus tard" resolves the current slot and arms a fresh deferral, which is how a
    /// snooze re-enters `decide` without being a special case there.
    #[tokio::test]
    async fn a_snooze_resolves_the_slot_and_arms_a_new_deferral() {
        let fixture = Fixture::new().await;
        *fixture.notifier.answer.lock().unwrap() = Some(NotificationOutcome::Action("snoozed".into()));
        fixture.set_last_tick(at(8, 29)).await;
        fixture.tick(at(8, 30)).await.unwrap();
        let events = fixture.all_events().await;
        assert_eq!(events.len(), 2, "the snoozed slot plus its follow-up");
        let follow_up = events.iter().find(|e| e.outcome == BreakOutcome::Pending).unwrap();
        assert_eq!(follow_up.defer_reason, Some(DeferReason::Snooze));
        assert_eq!(follow_up.deferred_until, Some(at(8, 40)));   // snooze_minutes = 10
    }

    /// The tick is the only writer of `last_tick`, and it must advance it even when it
    /// decided nothing — otherwise re-enabling after a pause replays days of dues.
    #[tokio::test]
    async fn the_tick_advances_last_tick_even_when_disabled() {
        let fixture = Fixture::new().await;
        fixture.set_config("aplan.breaks.enabled", "false").await;
        fixture.set_last_tick(at(8, 0)).await;
        let report = fixture.tick(at(10, 0)).await.unwrap();
        assert!(report.fired.is_none());
        assert!(fixture.all_events().await.is_empty());
        assert_eq!(fixture.last_tick().await, Some(at(10, 0)));
    }

    /// A first-ever run must not invent a backlog.
    #[tokio::test]
    async fn a_missing_last_tick_starts_the_clock_at_now() {
        let fixture = Fixture::new().await;
        let report = fixture.tick(at(11, 0)).await.unwrap();
        assert!(report.fired.is_none());
        assert_eq!(fixture.last_tick().await, Some(at(11, 0)));
    }

    /// Running the same tick twice must not double anything.
    #[tokio::test]
    async fn a_repeated_tick_is_inert() {
        let fixture = Fixture::new().await;
        fixture.set_last_tick(at(8, 29)).await;
        fixture.tick(at(8, 30)).await.unwrap();
        let before = fixture.all_events().await.len();
        fixture.tick(at(8, 30)).await.unwrap();
        assert_eq!(fixture.all_events().await.len(), before);
    }

    /// Delivery failure keeps the books and lets the expiry rule clean up; it must not
    /// fail the tick.
    #[tokio::test]
    async fn a_notifier_error_leaves_the_event_unfired_but_does_not_fail_the_tick() {
        let fixture = Fixture::new().await;
        fixture.notifier_always_errors();
        fixture.set_last_tick(at(8, 29)).await;
        let report = fixture.tick(at(8, 30)).await;
        assert!(report.is_ok());
        let events = fixture.all_events().await;
        assert_eq!(events[0].outcome, BreakOutcome::Pending);
        assert!(events[0].fired_at.is_none());
    }

    /// Only meetings whose show_as is in the configured list suppress.
    #[tokio::test]
    async fn a_free_meeting_does_not_suppress() {
        let fixture = Fixture::new().await;
        fixture.add_meeting("m1", at(8, 20), at(9, 0), Some("free")).await;
        fixture.set_last_tick(at(8, 29)).await;
        let report = fixture.tick(at(8, 30)).await.unwrap();
        assert!(report.fired.is_some());
    }

    #[tokio::test]
    async fn a_busy_meeting_suppresses_and_defers() {
        let fixture = Fixture::new().await;
        fixture.add_meeting("m1", at(8, 20), at(9, 0), Some("busy")).await;
        fixture.set_last_tick(at(8, 29)).await;
        let report = fixture.tick(at(8, 30)).await.unwrap();
        assert!(report.fired.is_none());
        assert_eq!(report.deferred, 1);
        let events = fixture.all_events().await;
        assert_eq!(events[0].deferred_until, Some(at(9, 3)));   // grace = 3
        assert_eq!(events[0].suppressed_by_meeting_id.as_deref(), Some("m1"));
    }

    /// Windows come from the existing workday config, read in the user's timezone.
    #[tokio::test]
    async fn windows_come_from_the_workday_config_in_local_time() {
        let fixture = Fixture::new().await;   // Europe/Paris, 08-12 and 13-17 local
        let windows = fixture.windows(at(10, 0)).await.unwrap();
        assert_eq!(windows.len(), 2);
        // August in Paris is UTC+2.
        assert_eq!(windows[0].start, at(6, 0));
        assert_eq!(windows[0].end, at(10, 0));
        assert_eq!(windows[1].start, at(11, 0));
        assert_eq!(windows[1].end, at(15, 0));
    }

    /// A day not in `general.working_days` has no windows, so nothing can fire.
    #[tokio::test]
    async fn a_non_working_day_yields_no_windows() {
        let fixture = Fixture::new().await;
        fixture.set_config("general.working_days", "mon,tue,wed,thu").await;
        // 2026-08-27 is a Thursday; make it a Friday-only config instead.
        fixture.set_config("general.working_days", "fri").await;
        assert!(fixture.windows(at(10, 0)).await.unwrap().is_empty());
    }
}
```

Write the `Fixture` helper and the four in-memory repositories in full — they are mechanical. `Fixture::new()` seeds: `aplan.timezone = Europe/Paris`, `general.working_days = mon,tue,wed,thu,fri`, `workday.morning_start_hour = 8`, `morning_end_hour = 12`, `afternoon_start_hour = 13`, `afternoon_end_hour = 17`, `aplan.breaks.enabled = true`, `meeting_grace_minutes = 3`, `snooze_minutes = 10`, `suppressing_show_as = busy,oof`, and one enabled `Interval { minutes: 30 }` rule with `priority = 2`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test -p application breaks`
Expected: FAIL — `cannot find function run_break_tick in this scope`.

- [ ] **Step 3: Implement the use case**

Prepend to `backend/crates/application/src/use_cases/breaks.rs`:

```rust
use chrono::{DateTime, Datelike, Duration, NaiveTime, Utc, Weekday};
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::{
    BreakEventRepository, BreakRuleRepository, ConfigRepository, MeetingRepository,
};
use crate::services::{Notification, NotificationOutcome, Notifier};
use crate::time::{local_to_utc, resolve_tz, to_local};
use domain::rules::breaks::{decide, BreakTickInput, BusyPeriod, Candidate, Window};
use domain::types::*;

const KEY_ENABLED: &str = "aplan.breaks.enabled";
const KEY_GRACE: &str = "aplan.breaks.meeting_grace_minutes";
const KEY_SNOOZE: &str = "aplan.breaks.snooze_minutes";
const KEY_SHOW_AS: &str = "aplan.breaks.suppressing_show_as";
const KEY_LAST_TICK: &str = "aplan.breaks.last_tick";

const DEFAULT_GRACE_MINUTES: i64 = 3;
const DEFAULT_SNOOZE_MINUTES: i64 = 10;
const DEFAULT_SHOW_AS: &str = "busy,oof";

pub struct BreakTickDeps<'a> {
    pub rules: &'a dyn BreakRuleRepository,
    pub events: &'a dyn BreakEventRepository,
    pub meetings: &'a dyn MeetingRepository,
    pub config: &'a dyn ConfigRepository,
    pub notifier: &'a dyn Notifier,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BreakTickReport {
    pub fired: Option<BreakEventId>,
    pub deferred: usize,
    pub absorbed: usize,
    pub expired: usize,
}

/// Read an integer configuration value, falling back to `default`.
///
/// Deliberately a local helper rather than a shared one: `use_cases/timesheet.rs` has its
/// own private `u32_key`, nested inside a function and not reachable from here. What must
/// stay shared is not the code but the **defaults** — 8 / 12 / 13 / 17 for the workday
/// bounds, identical to timesheet's — because a break engine and a timesheet that disagree
/// about when the workday starts would each be individually correct and jointly absurd.
async fn config_i64(
    config: &dyn ConfigRepository,
    user_id: UserId,
    key: &str,
    default: i64,
) -> Result<i64, AppError> {
    Ok(config
        .get(user_id, key)
        .await?
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default))
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    match s.trim().to_lowercase().as_str() {
        "mon" | "monday" | "lundi" => Some(Weekday::Mon),
        "tue" | "tuesday" | "mardi" => Some(Weekday::Tue),
        "wed" | "wednesday" | "mercredi" => Some(Weekday::Wed),
        "thu" | "thursday" | "jeudi" => Some(Weekday::Thu),
        "fri" | "friday" | "vendredi" => Some(Weekday::Fri),
        "sat" | "saturday" | "samedi" => Some(Weekday::Sat),
        "sun" | "sunday" | "dimanche" => Some(Weekday::Sun),
        _ => None,
    }
}

/// Today's working windows in UTC, from the workday configuration the rest of the
/// cockpit already uses.
///
/// This is where the timezone lives. `domain` gets UTC instants and never learns that
/// zones exist — the same split `use_cases/worklog.rs` uses for half-day projection.
pub async fn resolve_windows(
    config: &dyn ConfigRepository,
    user_id: UserId,
    now: DateTime<Utc>,
) -> Result<Vec<Window>, AppError> {
    let tz = resolve_tz(config.get(user_id, "aplan.timezone").await?);
    let local_now = to_local(now, tz);
    let today = local_now.date();

    let days = config
        .get(user_id, "general.working_days")
        .await?
        .unwrap_or_else(|| "mon,tue,wed,thu,fri".to_string());
    let working: Vec<Weekday> = days.split(',').filter_map(parse_weekday).collect();
    if !working.contains(&today.weekday()) {
        return Ok(Vec::new());
    }

    let mut windows = Vec::new();
    for (start_key, end_key, default_start, default_end) in [
        ("workday.morning_start_hour", "workday.morning_end_hour", 8, 12),
        ("workday.afternoon_start_hour", "workday.afternoon_end_hour", 13, 17),
    ] {
        let start_h = config_i64(config, user_id, start_key, default_start).await?;
        let end_h = config_i64(config, user_id, end_key, default_end).await?;
        let (Some(start_t), Some(end_t)) = (
            NaiveTime::from_hms_opt(start_h.clamp(0, 23) as u32, 0, 0),
            NaiveTime::from_hms_opt(end_h.clamp(0, 23) as u32, 0, 0),
        ) else {
            continue;
        };
        if end_t <= start_t {
            continue;
        }
        windows.push(Window {
            start: local_to_utc(tz, today.and_time(start_t)),
            end: local_to_utc(tz, today.and_time(end_t)),
        });
    }
    Ok(windows)
}

/// Resolve today's UTC instant for every enabled `Daily` rule.
async fn resolve_daily_dues(
    config: &dyn ConfigRepository,
    user_id: UserId,
    rules: &[BreakRule],
    now: DateTime<Utc>,
) -> Result<Vec<(BreakRuleId, DateTime<Utc>)>, AppError> {
    let tz = resolve_tz(config.get(user_id, "aplan.timezone").await?);
    let today = to_local(now, tz).date();
    Ok(rules
        .iter()
        .filter_map(|r| r.cadence.at_time().map(|t| (r.id, local_to_utc(tz, today.and_time(t)))))
        .collect())
}

/// One pass of the break engine.
///
/// Never fails on a delivery problem: a notification that could not be shown leaves its
/// row unfired, and the engine's own expiry rule clears it at the next natural due. That
/// is deliberately the same path a meeting-deferred break takes — one cleanup mechanism,
/// not two.
pub async fn run_break_tick(
    deps: BreakTickDeps<'_>,
    user_id: UserId,
    now: DateTime<Utc>,
) -> Result<BreakTickReport, AppError> {
    let mut report = BreakTickReport::default();

    let since = match deps.config.get(user_id, KEY_LAST_TICK).await? {
        Some(s) => DateTime::parse_from_rfc3339(&s)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or(now),
        // First run ever: start the clock here rather than invent a backlog.
        None => now,
    };
    // Advance the watermark whatever happens below, including when the feature is off:
    // otherwise re-enabling after a week replays a week of dues in one tick.
    let advance = || async {
        deps.config
            .set(user_id, KEY_LAST_TICK, &now.to_rfc3339())
            .await
    };

    let enabled = deps
        .config
        .get(user_id, KEY_ENABLED)
        .await?
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);
    if !enabled {
        advance().await?;
        return Ok(report);
    }

    let rules = deps.rules.list_enabled(user_id).await?;
    let windows = resolve_windows(deps.config, user_id, now).await?;
    let daily_dues = resolve_daily_dues(deps.config, user_id, &rules, now).await?;
    let open = deps.events.list_open(user_id).await?;

    let grace_minutes = config_i64(deps.config, user_id, KEY_GRACE, DEFAULT_GRACE_MINUTES).await?;
    let show_as_filter = deps
        .config
        .get(user_id, KEY_SHOW_AS)
        .await?
        .unwrap_or_else(|| DEFAULT_SHOW_AS.to_string());
    let suppressing: Vec<String> = show_as_filter
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    // `MeetingRepository` exposes `find_by_user_and_range(user_id, start: NaiveDate,
    // end: NaiveDate)` — local calendar dates, not a UTC instant range. Breaks only ever
    // fire inside today's windows, so today's local date on both ends is the exact query.
    let tz = resolve_tz(deps.config.get(user_id, "aplan.timezone").await?);
    let today = to_local(now, tz).date();
    let busy: Vec<BusyPeriod> = deps
        .meetings
        .find_by_user_and_range(user_id, today, today)
        .await?
        .into_iter()
        .filter(|m| {
            m.show_as
                .as_deref()
                .map(|s| suppressing.contains(&s.to_lowercase()))
                .unwrap_or(false)
        })
        .map(|m| BusyPeriod {
            meeting_id: m.outlook_id.clone(),
            start: m.start_time,
            end: m.end_time,
        })
        .collect();

    let tick = decide(BreakTickInput {
        now,
        since,
        windows: &windows,
        rules: &rules,
        daily_dues: &daily_dues,
        busy: &busy,
        open: &open,
        grace: Duration::minutes(grace_minutes),
    });

    for id in &tick.expire {
        deps.events.set_outcome(*id, BreakOutcome::Expired, None).await?;
        report.expired += 1;
    }

    for absorbed in &tick.absorb {
        let id = match absorbed.candidate.event_id() {
            Some(id) => id,
            None => {
                let id = Uuid::new_v4();
                deps.events
                    .create(&new_event(id, user_id, &absorbed.candidate, now, BreakOutcome::Absorbed))
                    .await?;
                report.absorbed += 1;
                continue;
            }
        };
        deps.events.set_outcome(id, BreakOutcome::Absorbed, None).await?;
        report.absorbed += 1;
    }

    for deferred in &tick.defer {
        let id = match deferred.candidate.event_id() {
            Some(id) => id,
            None => {
                let id = Uuid::new_v4();
                deps.events
                    .create(&new_event(id, user_id, &deferred.candidate, now, BreakOutcome::Pending))
                    .await?;
                id
            }
        };
        deps.events
            .set_deferral(id, deferred.until, deferred.reason, deferred.meeting_id.as_deref())
            .await?;
        report.deferred += 1;
    }

    let Some(fire) = tick.fire else {
        advance().await?;
        return Ok(report);
    };

    let event_id = match fire.candidate.event_id() {
        Some(id) => id,
        None => {
            let id = Uuid::new_v4();
            deps.events
                .create(&new_event(id, user_id, &fire.candidate, now, BreakOutcome::Pending))
                .await?;
            id
        }
    };
    report.fired = Some(event_id);

    let Some(rule) = rules.iter().find(|r| r.id == fire.candidate.rule_id()) else {
        advance().await?;
        return Ok(report);
    };

    let notification = Notification {
        title: rule.label.clone(),
        body: rule.body.clone(),
        urgency: rule.urgency,
        icon: Some(icon_for(rule.kind).to_string()),
        expire_after: std::time::Duration::from_secs(rule.duration_seconds as u64 + 300),
        actions: vec![
            ("taken".to_string(), "Pris".to_string()),
            ("snoozed".to_string(), "Plus tard".to_string()),
            ("skipped".to_string(), "Passer".to_string()),
        ],
    };

    match deps.notifier.notify(notification).await {
        Ok(outcome) => {
            deps.events.mark_fired(event_id, now).await?;
            apply_outcome(&deps, user_id, event_id, rule, outcome, now).await?;
        }
        Err(e) => {
            // Books kept, no state invented: `fired_at` stays NULL and the expiry rule
            // clears the row at the rule's next natural due. Logged rather than
            // returned, because a daemon that is not there must not fail the tick —
            // but it must not be silent either, or a routine that stopped notifying
            // looks identical to a routine with nothing to say.
            tracing::warn!(error = %e, rule = %rule.label, "break notification not delivered");
        }
    }

    advance().await?;
    Ok(report)
}

fn icon_for(kind: BreakKind) -> &'static str {
    match kind {
        BreakKind::Visual => "eye",
        BreakKind::Posture => "user-available",
        BreakKind::Long => "appointment-soon",
        BreakKind::Strength => "weather-clear",
    }
}

fn new_event(
    id: BreakEventId,
    user_id: UserId,
    candidate: &Candidate,
    now: DateTime<Utc>,
    outcome: BreakOutcome,
) -> BreakEvent {
    BreakEvent {
        id,
        user_id,
        rule_id: candidate.rule_id(),
        due_at: candidate.due_at(),
        fired_at: None,
        deferred_until: None,
        defer_reason: None,
        suppressed_by_meeting_id: None,
        outcome,
        responded_at: if outcome == BreakOutcome::Pending { None } else { Some(now) },
        created_at: now,
    }
}

/// Translate what the user pressed into stored state.
///
/// A snooze resolves the current slot and arms a fresh deferral, which is how it
/// re-enters `decide` without `decide` knowing snoozes exist.
async fn apply_outcome(
    deps: &BreakTickDeps<'_>,
    user_id: UserId,
    event_id: BreakEventId,
    rule: &BreakRule,
    outcome: NotificationOutcome,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    let resolved = match &outcome {
        NotificationOutcome::Action(key) => {
            BreakOutcome::from_str(key).unwrap_or(BreakOutcome::Ignored)
        }
        NotificationOutcome::Dismissed => BreakOutcome::Ignored,
        NotificationOutcome::Expired => BreakOutcome::Ignored,
    };
    deps.events.set_outcome(event_id, resolved, Some(now)).await?;

    if resolved == BreakOutcome::Snoozed {
        let minutes =
            config_i64(deps.config, user_id, KEY_SNOOZE, DEFAULT_SNOOZE_MINUTES).await?;
        let follow_up = Uuid::new_v4();
        deps.events
            .create(&BreakEvent {
                id: follow_up,
                user_id,
                rule_id: rule.id,
                due_at: now,
                fired_at: None,
                deferred_until: Some(now + Duration::minutes(minutes)),
                defer_reason: Some(DeferReason::Snooze),
                suppressed_by_meeting_id: None,
                outcome: BreakOutcome::Pending,
                responded_at: None,
                created_at: now,
            })
            .await?;
    }
    Ok(())
}
```

Check `MeetingRepository` for the exact name and signature of the "meetings in a UTC range" method before writing `list_between`; use whatever the trait already exposes rather than adding a method.

- [ ] **Step 4: Declare the module**

In `backend/crates/application/src/use_cases/mod.rs`, add `pub mod breaks;`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd backend && cargo test -p application breaks`
Expected: PASS (11 tests).

- [ ] **Step 6: Run the full backend suite for regressions**

Run: `cd backend && cargo test -p domain -p application -p infrastructure -p api`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add backend/crates/application/src/use_cases/breaks.rs \
        backend/crates/application/src/use_cases/mod.rs
git commit -m "Add run_break_tick: window resolution, persistence and notification dispatch"
```

---

### Task 7: The background job and its wiring

**Files:**
- Modify: `backend/crates/application/src/jobs.rs` (add `RetryPolicy::breaks()`)
- Modify: `backend/crates/api/src/jobs.rs` (add `BreakDeps` and `run_break_scheduler`)
- Modify: `backend/crates/api/src/main.rs` (construct the repos, pick the notifier, spawn the job)
- Test: inline test for `RetryPolicy::breaks()` in `application/src/jobs.rs`

**Interfaces:**
- Consumes: `run_break_tick`, `BreakTickDeps`, `BreakTickReport` (Task 6); `SqliteBreakRuleRepository`, `SqliteBreakEventRepository` (Task 2); `NotifySendNotifier`, `NullNotifier` (Task 5); the existing `RetryPolicy` / `JobHealth`.
- Produces: `api::jobs::BreakDeps`, `api::jobs::run_break_scheduler(deps, user_id)`.

- [ ] **Step 1: Write the failing test for the retry policy**

In `backend/crates/application/src/jobs.rs`, inside the existing `#[cfg(test)] mod tests`:

```rust
    /// Breaks need a much finer tick than the end-of-day pass: a 5-minute tick would
    /// place a 20-minute break anywhere in a 5-minute band, and the deferral wake-up
    /// would inherit the same slop.
    #[test]
    fn break_policy_ticks_every_thirty_seconds_while_healthy() {
        assert_eq!(backoff_delay(0, &RetryPolicy::breaks()), Duration::from_secs(30));
    }

    #[test]
    fn break_policy_backs_off_to_five_minutes() {
        let p = RetryPolicy::breaks();
        assert_eq!(backoff_delay(1, &p), Duration::from_secs(30));
        assert_eq!(backoff_delay(3, &p), Duration::from_secs(120));
        // base * 2^(n-1) saturates at the ceiling from the fifth failure on.
        assert_eq!(backoff_delay(5, &p), Duration::from_secs(300));
        assert_eq!(backoff_delay(50, &p), Duration::from_secs(300));
    }
```

The real API, already read for you: `RetryPolicy { base, ceiling, escalate_after, reminder_every }`, built by `const fn` constructors; the delay comes from the free function `backoff_delay(consecutive_failures, &policy)` = `base * 2^(n-1)` capped at `ceiling`, with `n == 0` returning `base`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd backend && cargo test -p application break_policy`
Expected: FAIL — `no function or associated item named breaks found`.

- [ ] **Step 3: Add the policy**

In `backend/crates/application/src/jobs.rs`, beside `RetryPolicy::end_of_day()`:

```rust
    /// The break engine: a tick every 30 seconds while healthy, backing off to 5
    /// minutes.
    ///
    /// Far finer than `end_of_day()`'s 5-minute base because here the granularity of
    /// the tick is the granularity of every break: at a 5-minute tick a deferral armed
    /// for 09:53 lands somewhere in 09:53–09:58, which is exactly the sloppiness that
    /// makes a reminder feel arbitrary.
    pub const fn breaks() -> Self {
        Self {
            base: Duration::from_secs(30),
            ceiling: Duration::from_secs(5 * 60),
            escalate_after: 3,
            reminder_every: 12,
        }
    }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd backend && cargo test -p application break_policy`
Expected: PASS.

- [ ] **Step 5: Add the scheduler**

In `backend/crates/api/src/jobs.rs`, after `run_eod_scheduler`:

```rust
/// Dependencies the break scheduler needs.
pub struct BreakDeps {
    pub rule_repo: Arc<dyn BreakRuleRepository>,
    pub event_repo: Arc<dyn BreakEventRepository>,
    pub meeting_repo: Arc<dyn MeetingRepository>,
    pub config_repo: Arc<dyn ConfigRepository>,
    pub notifier: Arc<dyn Notifier>,
}

/// Long-lived background task: run one break tick for `user_id`, then wait as long as
/// `RetryPolicy::breaks()` says to — 30 s while healthy, 5 minutes while not.
///
/// The tick itself is what owns the notification, including the wait for the user's
/// answer: `notify-send --action` implies `--wait`, so a tick that fired a break can
/// stay open for minutes. That is fine and deliberate — the next tick simply starts
/// late, and the wall-clock anchoring in `decide` means a late tick loses no dues.
pub async fn run_break_scheduler(deps: BreakDeps, user_id: UserId) {
    let policy = RetryPolicy::breaks();
    let mut health = JobHealth::default();
    loop {
        let attempt = run_break_tick(
            BreakTickDeps {
                rules: deps.rule_repo.as_ref(),
                events: deps.event_repo.as_ref(),
                meetings: deps.meeting_repo.as_ref(),
                config: deps.config_repo.as_ref(),
                notifier: deps.notifier.as_ref(),
            },
            user_id,
            Utc::now(),
        )
        .await;

        let failure = match &attempt {
            Ok(report) => {
                if report.fired.is_some() || report.deferred > 0 {
                    tracing::info!(
                        fired = ?report.fired,
                        deferred = report.deferred,
                        absorbed = report.absorbed,
                        expired = report.expired,
                        "break tick"
                    );
                }
                None
            }
            Err(e) => {
                tracing::warn!(error = %e, "break tick failed");
                Some(e.to_string())
            }
        };

        // Identical to `run_eod_scheduler`'s block, so both jobs age their health the
        // same way and print through the same journal helper.
        let observed = match &failure {
            Some(signature) => AttemptOutcome::Failed { signature },
            None => AttemptOutcome::Succeeded,
        };
        let (next_health, decision) = health.observe(observed, Utc::now(), &policy);
        health = next_health;
        report("break routine", decision.log, failure.as_deref(), decision.retry_in);

        tokio::time::sleep(decision.retry_in).await;
    }
}
```

`report` is the private helper already in this file — reuse it, do not write a second one.

Add to that file's imports: `BreakEventRepository`, `BreakRuleRepository` to the `application::repositories` list; `application::services::Notifier`; `application::use_cases::breaks::{run_break_tick, BreakTickDeps}`.

- [ ] **Step 6: Wire it in `main.rs`**

In `backend/crates/api/src/main.rs`, alongside the existing scheduler spawns:

```rust
    // The notifier is chosen once, at startup: a headless run keeps its books silently
    // rather than failing every 30 seconds on a bus that is not there.
    let notifier: Arc<dyn Notifier> = if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok() {
        Arc::new(NotifySendNotifier::new())
    } else {
        tracing::info!("no session bus: break notifications will be recorded, not shown");
        Arc::new(NullNotifier)
    };

    // Built once in main.rs and handed to both consumers: the background job below and
    // the GraphQL `SchemaDeps` of Tasks 8-9, so there is one instance, not two.
    //
    // PLACEMENT MATTERS: put these two bindings ABOVE the `let deps = SchemaDeps { ... }`
    // literal (currently around main.rs:116, with `build_schema` at :138), not down beside
    // the scheduler spawns near :204. Task 8 adds them to that literal, and a binding
    // declared after it would not be in scope there.
    let break_rule_repo: Arc<dyn BreakRuleRepository> =
        Arc::new(SqliteBreakRuleRepository::new(pool.clone()));
    let break_event_repo: Arc<dyn BreakEventRepository> =
        Arc::new(SqliteBreakEventRepository::new(pool.clone()));

    tokio::spawn(run_break_scheduler(
        BreakDeps {
            rule_repo: break_rule_repo.clone(),
            event_repo: break_event_repo.clone(),
            meeting_repo: meeting_repo.clone(),
            config_repo: config_repo.clone(),
            notifier,
        },
        default_user_id,
    ));
```

Do **not** put them on `AppState` — `AppState` carries only the schema, the config repo, the OAuth pieces and the default user id, and the GraphQL layer does not read repositories through it. Tasks 8 and 9 will instead add these two `Arc`s to `SchemaDeps` in `backend/crates/api/src/graphql/schema.rs` and inject them with `.data(...)`, which is how every other repository reaches a resolver. Leave that to Task 8; this task only needs the two bindings to exist in `main.rs` so Task 8 can pass them along.

Use whatever names `main.rs` already binds for the pool, the meeting repo, the config repo and the default user id.

- [ ] **Step 7: Build and run the suite**

Run: `cd backend && cargo build -p api && cargo test -p domain -p application -p infrastructure -p api`
Expected: build OK, tests PASS.

- [ ] **Step 8: Commit**

```bash
git add backend/crates/application/src/jobs.rs backend/crates/api/src/jobs.rs \
        backend/crates/api/src/main.rs
git commit -m "Run the break engine as a background job in the API"
```

---

### Task 8: GraphQL CRUD for break rules

**Files:**
- Modify: `backend/crates/api/src/graphql/query.rs`
- Modify: `backend/crates/api/src/graphql/mutation.rs`
- Modify: `backend/crates/api/src/types/` (add the GraphQL object and inputs, following the existing file layout there)
- Modify: `backend/crates/api/src/graphql/schema.rs` (two new `SchemaDeps` fields, destructured and `.data(...)`-injected)
- Modify: `backend/crates/api/src/main.rs` (pass the two bindings Task 7 created into `SchemaDeps`)
- Test: `backend/crates/api/src/graphql/tests.rs`

**How this layer actually works** — read before writing anything: there is **no `AppState` in the GraphQL layer**. Resolvers read `ctx.data::<UserId>()?` for the user and `ctx.data::<Arc<dyn SomeRepository>>()?` for each repository; `build_schema` injects them all with `.data(...)`, and `graphql/tests.rs` builds its schema the same way over **in-memory repository fakes**, not over SQLite. Migration 019's seed is therefore invisible to these tests — every test seeds what it needs.

**The test-harness change, exactly** (the file has three builders: `build_test_schema_with` at ~:1332, `build_test_schema_with_memory` at ~:1352, and the zero-arg `build_test_schema()` at ~:1415):

1. Add `InMemoryBreakRuleRepository` and `InMemoryBreakEventRepository` to `tests.rs`, both `Default`-constructible, each a `Mutex<Vec<_>>` implementing its trait — same shape as the other in-memory fakes already in the file.
2. Add two parameters to `build_test_schema_with_memory` (the innermost builder) for them, and `.data(...)` both into the chain.
3. Update its only caller, `build_test_schema_with`, to pass `Arc::new(InMemoryBreakRuleRepository::default())` / `Arc::new(InMemoryBreakEventRepository::default())`. Every existing test then keeps working untouched — that is the point of threading it through the innermost builder rather than adding a fourth one.
4. Add `fn build_test_schema_with_breaks(break_rules: Arc<InMemoryBreakRuleRepository>, break_events: Arc<InMemoryBreakEventRepository>) -> TestSchema`, mirroring `build_test_schema()`'s defaults for everything else and threading these two through. Task 9's stats test needs a handle on the event store, exactly as the memory tests needed one on `InMemoryMemoryStore` — which is why `build_test_schema_with_memory` exists at all. Follow that precedent.

Task 8's own tests need no handle: they seed through the `createBreakRule` mutation and use the plain zero-arg `build_test_schema()`.

**Interfaces:**
- Consumes: `BreakRuleRepository` (Task 2), the domain types (Task 1).
- Produces: GraphQL type `BreakRule` with fields `id, kind, label, body, cadence, intervalMinutes, atTime, durationSeconds, priority, enabled, urgency`; query `breakRules`; mutations `createBreakRule(input: BreakRuleInput!)`, `updateBreakRule(id: ID!, input: BreakRuleInput!)`, `deleteBreakRule(id: ID!)`. `BreakRuleInput` carries the same fields minus `id`. Task 10 consumes this schema.

- [ ] **Step 1: Write the failing tests**

In `backend/crates/api/src/graphql/tests.rs`, using the file's own `build_test_schema_with_memory` helper (extended with the two new fakes) rather than any SQLite pool:

```rust
/// Both cadence shapes must survive the round trip to GraphQL: an interval rule reports
/// `intervalMinutes` and a null `atTime`, a daily rule the reverse. Collapsing them into
/// one nullable pair is how a rule with no defined due time would reach the database.
#[tokio::test]
async fn break_rules_query_renders_each_cadence_in_its_own_shape() {
    let schema = build_test_schema();
    // Seeded through the mutation, not through a repository handle: this test is about
    // the query, and going in the front door proves both halves agree on the shape.
    for input in [
        r#"kind: VISUAL, label: "Visuelle", body: "Regarde au loin",
           cadence: INTERVAL, intervalMinutes: 20,
           durationSeconds: 30, priority: 1, enabled: true, urgency: LOW"#,
        r#"kind: STRENGTH, label: "Renfo", body: "Élastique",
           cadence: DAILY, atTime: "14:00",
           durationSeconds: 120, priority: 4, enabled: true, urgency: NORMAL"#,
    ] {
        let created = schema
            .execute(format!("mutation {{ createBreakRule(input: {{ {input} }}) {{ id }} }}"))
            .await;
        assert!(created.errors.is_empty(), "{:?}", created.errors);
    }

    let res = schema
        .execute("{ breakRules { kind label intervalMinutes atTime priority enabled } }")
        .await;
    assert!(res.errors.is_empty(), "{:?}", res.errors);
    let rules = res.data.into_json().unwrap()["breakRules"].as_array().unwrap().clone();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0]["kind"], "VISUAL");
    assert_eq!(rules[0]["intervalMinutes"], 20);
    assert!(rules[0]["atTime"].is_null());
    assert_eq!(rules[1]["kind"], "STRENGTH");
    assert_eq!(rules[1]["atTime"], "14:00");
    assert!(rules[1]["intervalMinutes"].is_null());
}

#[tokio::test]
async fn create_update_delete_round_trips_a_rule() {
    let schema = build_test_schema();
    let created = schema
        .execute(
            r#"mutation { createBreakRule(input: {
                 kind: POSTURE, label: "Test", body: "Bouge",
                 cadence: INTERVAL, intervalMinutes: 45,
                 durationSeconds: 90, priority: 7, enabled: true, urgency: NORMAL
               }) { id label priority } }"#,
        )
        .await;
    assert!(created.errors.is_empty(), "{:?}", created.errors);
    let id = created.data.into_json().unwrap()["createBreakRule"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let updated = schema
        .execute(&format!(
            r#"mutation {{ updateBreakRule(id: "{id}", input: {{
                 kind: POSTURE, label: "Renommé", body: "Bouge",
                 cadence: INTERVAL, intervalMinutes: 45,
                 durationSeconds: 90, priority: 7, enabled: false, urgency: NORMAL
               }}) {{ label enabled }} }}"#
        ))
        .await;
    assert!(updated.errors.is_empty(), "{:?}", updated.errors);
    let u = updated.data.into_json().unwrap();
    assert_eq!(u["updateBreakRule"]["label"], "Renommé");
    assert_eq!(u["updateBreakRule"]["enabled"], false);

    let deleted = schema
        .execute(&format!(r#"mutation {{ deleteBreakRule(id: "{id}") }}"#))
        .await;
    assert!(deleted.errors.is_empty(), "{:?}", deleted.errors);
    let listed = schema.execute("{ breakRules { id } }").await;
    let n = listed.data.into_json().unwrap()["breakRules"].as_array().unwrap().len();
    assert_eq!(n, 0, "the fake started empty and the rule is gone");
}

/// The XOR the database enforces must be refused at the edge too, with a message the
/// settings screen can show.
#[tokio::test]
async fn creating_a_rule_with_both_cadence_shapes_is_rejected() {
    let schema = build_test_schema();
    let res = schema
        .execute(
            r#"mutation { createBreakRule(input: {
                 kind: POSTURE, label: "Bad", body: "b",
                 cadence: INTERVAL, intervalMinutes: 30, atTime: "14:00",
                 durationSeconds: 90, priority: 1, enabled: true, urgency: NORMAL
               }) { id } }"#,
        )
        .await;
    assert!(!res.errors.is_empty());
}

#[tokio::test]
async fn creating_an_interval_rule_without_an_interval_is_rejected() {
    let schema = build_test_schema();
    let res = schema
        .execute(
            r#"mutation { createBreakRule(input: {
                 kind: POSTURE, label: "Bad", body: "b",
                 cadence: INTERVAL,
                 durationSeconds: 90, priority: 1, enabled: true, urgency: NORMAL
               }) { id } }"#,
        )
        .await;
    assert!(!res.errors.is_empty());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test -p api break_rule`
Expected: FAIL — unknown field `breakRules`.

- [ ] **Step 3: Add the GraphQL types**

In the API's types module, following the file layout already there:

```rust
use async_graphql::{Enum, InputObject, SimpleObject};

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum GqlBreakKind { Visual, Posture, Long, Strength }

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum GqlBreakUrgency { Low, Normal, Critical }

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum GqlBreakCadence { Interval, Daily }

#[derive(SimpleObject)]
pub struct GqlBreakRule {
    pub id: async_graphql::ID,
    pub kind: GqlBreakKind,
    pub label: String,
    pub body: String,
    pub cadence: GqlBreakCadence,
    pub interval_minutes: Option<i32>,
    /// `HH:MM` in the user's timezone.
    pub at_time: Option<String>,
    pub duration_seconds: i32,
    pub priority: i32,
    pub enabled: bool,
    pub urgency: GqlBreakUrgency,
}

#[derive(InputObject)]
pub struct BreakRuleInput {
    pub kind: GqlBreakKind,
    pub label: String,
    pub body: String,
    pub cadence: GqlBreakCadence,
    pub interval_minutes: Option<i32>,
    pub at_time: Option<String>,
    pub duration_seconds: i32,
    pub priority: i32,
    pub enabled: bool,
    pub urgency: GqlBreakUrgency,
}

impl BreakRuleInput {
    /// Reject the shapes the database would reject anyway, but with a message a form
    /// can display. The CHECK stays as the backstop, not as the user experience.
    pub fn to_cadence(&self) -> Result<domain::types::BreakCadence, String> {
        match (self.cadence, self.interval_minutes, self.at_time.as_deref()) {
            (GqlBreakCadence::Interval, Some(m), None) if m > 0 => {
                Ok(domain::types::BreakCadence::Interval { minutes: m as u32 })
            }
            (GqlBreakCadence::Interval, _, Some(_)) => {
                Err("une règle par intervalle ne peut pas porter d'heure fixe".into())
            }
            (GqlBreakCadence::Interval, _, None) => {
                Err("intervalMinutes est requis et doit être positif".into())
            }
            (GqlBreakCadence::Daily, None, Some(t)) => chrono::NaiveTime::parse_from_str(t, "%H:%M")
                .map(|at| domain::types::BreakCadence::Daily { at })
                .map_err(|_| "atTime doit être au format HH:MM".to_string()),
            (GqlBreakCadence::Daily, Some(_), _) => {
                Err("une règle quotidienne ne peut pas porter d'intervalle".into())
            }
            (GqlBreakCadence::Daily, None, None) => Err("atTime est requis".into()),
        }
    }
}
```

Add the two `From` conversions between `GqlBreakKind`/`GqlBreakUrgency` and the domain enums, and a `From<BreakRule> for GqlBreakRule`.

- [ ] **Step 4: Add the query and mutations**

In `query.rs`:

```rust
    /// The routine, ordered by priority — what the settings screen lists.
    async fn break_rules(&self, ctx: &Context<'_>) -> Result<Vec<GqlBreakRule>> {
        let user_id = ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn BreakRuleRepository>>()?;
        Ok(repo
            .list(*user_id)
            .await?
            .into_iter()
            .map(GqlBreakRule::from)
            .collect())
    }
```

In `mutation.rs`, `create_break_rule`, `update_break_rule` and `delete_break_rule`, each reading `ctx.data::<UserId>()?` and `ctx.data::<Arc<dyn BreakRuleRepository>>()?` the same way, and each resolving `input.to_cadence()?` first, returning its message as a GraphQL error. `update_break_rule` loads the existing rule (to preserve `created_at`), sets `updated_at = Utc::now()`, and returns the updated `GqlBreakRule`. `delete_break_rule` returns `bool`.

- [ ] **Step 5: Inject the repositories into the schema**

Add two fields to `SchemaDeps` in `backend/crates/api/src/graphql/schema.rs`:

```rust
    pub break_rule_repo: Arc<dyn BreakRuleRepository>,
    pub break_event_repo: Arc<dyn BreakEventRepository>,
```

destructure them in `build_schema` alongside the others, and add `.data(break_rule_repo)` / `.data(break_event_repo)` to the builder chain. Then pass Task 7's two `main.rs` bindings into the `SchemaDeps` literal there. Do not touch `AppState`.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd backend && cargo test -p api break_rule`
Expected: PASS (4 tests).

- [ ] **Step 7: Regenerate the GraphQL schema file if the repo keeps one**

Run: `cd backend && cargo run -p api -- export-schema`
**Warning:** this command builds the pool first, so it applies pending migrations to the real `backend/aggregated_plan.db`. That is expected here — migration 019 needs to reach the dev database anyway — but back up `aggregated_plan.db-wal` / `-shm` first, as the repo's existing `.bak-*` files show is the habit.

- [ ] **Step 8: Commit**

```bash
git add backend/crates/api/src/graphql/query.rs backend/crates/api/src/graphql/mutation.rs \
        backend/crates/api/src/types backend/crates/api/src/state.rs \
        backend/crates/api/src/graphql/tests.rs
git commit -m "Expose break rules over GraphQL"
```

---

### Task 9: GraphQL break statistics

**Files:**
- Modify: `backend/crates/api/src/graphql/query.rs`
- Modify: `backend/crates/api/src/types/` (add `GqlBreakStats`, `GqlBreakRuleStats`)
- Test: `backend/crates/api/src/graphql/tests.rs`

**Interfaces:**
- Consumes: `BreakEventRepository::counts_between` (Task 2), `BreakOutcome::counts_towards_adherence` (Task 1).
- Produces: query `breakStats(from: String!, to: String!): BreakStats!` where `BreakStats { perRule: [BreakRuleStats!]! }` and `BreakRuleStats { ruleId, label, taken, snoozed, skipped, ignored, absorbed, expired, adherence }`. `adherence = taken / (taken + snoozed + skipped + ignored)`, `null` when that denominator is zero.

- [ ] **Step 1: Write the failing test**

In `backend/crates/api/src/graphql/tests.rs`:

```rust
/// Adherence counts only what the user was actually shown: absorbed slots never
/// reached a screen and must not dilute the rate.
#[tokio::test]
async fn break_stats_computes_adherence_over_seen_outcomes_only() {
    let (rules, events) = (Arc::new(InMemoryBreakRuleRepository::default()),
                           Arc::new(InMemoryBreakEventRepository::default()));
    let schema = build_test_schema_with_breaks(rules.clone(), events.clone());
    let rule_id = seed_break_events(
        &rules,
        &events,
        &[
            (BreakOutcome::Taken, 3),
            (BreakOutcome::Ignored, 1),
            (BreakOutcome::Absorbed, 10),
            (BreakOutcome::Expired, 5),
        ],
    )
    .await;

    let res = schema
        .execute(
            r#"{ breakStats(from: "2026-08-01T00:00:00+00:00", to: "2026-09-01T00:00:00+00:00") {
                   perRule { ruleId taken ignored absorbed expired adherence } } }"#,
        )
        .await;
    assert!(res.errors.is_empty(), "{:?}", res.errors);
    let per = res.data.into_json().unwrap()["breakStats"]["perRule"][0].clone();
    assert_eq!(per["ruleId"], rule_id.to_string());
    assert_eq!(per["taken"], 3);
    assert_eq!(per["absorbed"], 10);
    assert_eq!(per["expired"], 5);
    assert_eq!(per["adherence"], 0.75, "3 taken out of 4 seen");
}

#[tokio::test]
async fn break_stats_reports_null_adherence_when_nothing_was_seen() {
    let (rules, events) = (Arc::new(InMemoryBreakRuleRepository::default()),
                           Arc::new(InMemoryBreakEventRepository::default()));
    let schema = build_test_schema_with_breaks(rules.clone(), events.clone());
    seed_break_events(&rules, &events, &[(BreakOutcome::Absorbed, 4)]).await;
    let res = schema
        .execute(
            r#"{ breakStats(from: "2026-08-01T00:00:00+00:00", to: "2026-09-01T00:00:00+00:00") {
                   perRule { adherence } } }"#,
        )
        .await;
    assert!(res.data.into_json().unwrap()["breakStats"]["perRule"][0]["adherence"].is_null());
}
```

Both stats tests build their schema with `build_test_schema_with_breaks(rules, events)` (the helper added in Task 8), keeping the two `Arc<InMemory...>` handles. `seed_break_events(&rules, &events, &[(outcome, n), ...])` then writes directly through those handles: one rule, then `n` events per outcome with `due_at` inside August 2026, returning the rule id. It must go through the handles rather than the API — no mutation creates a `break_event`, because events are produced by the tick, never by a user.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd backend && cargo test -p api break_stats`
Expected: FAIL — unknown field `breakStats`.

- [ ] **Step 3: Implement the types and resolver**

Types:

```rust
#[derive(SimpleObject)]
pub struct GqlBreakRuleStats {
    pub rule_id: async_graphql::ID,
    pub label: String,
    pub taken: i32,
    pub snoozed: i32,
    pub skipped: i32,
    pub ignored: i32,
    pub absorbed: i32,
    pub expired: i32,
    /// `taken / seen`, or `null` when nothing was seen. Absorbed and expired slots are
    /// excluded from both sides: the user never had the chance to answer them, so
    /// counting them would drown a real signal in scheduling noise.
    pub adherence: Option<f64>,
}

#[derive(SimpleObject)]
pub struct GqlBreakStats {
    pub per_rule: Vec<GqlBreakRuleStats>,
}
```

Resolver in `query.rs`:

```rust
    async fn break_stats(
        &self,
        ctx: &Context<'_>,
        from: String,
        to: String,
    ) -> Result<GqlBreakStats> {
        let user_id = ctx.data::<UserId>()?;
        let events = ctx.data::<Arc<dyn BreakEventRepository>>()?;
        let rules_repo = ctx.data::<Arc<dyn BreakRuleRepository>>()?;
        let parse = |s: &str| {
            DateTime::parse_from_rfc3339(s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| async_graphql::Error::new(format!("bad date '{s}': {e}")))
        };
        let counts = events
            .counts_between(*user_id, parse(&from)?, parse(&to)?)
            .await?;
        let rules = rules_repo.list(*user_id).await?;

        let mut per_rule = Vec::new();
        for rule in rules {
            let mut row = GqlBreakRuleStats {
                rule_id: rule.id.to_string().into(),
                label: rule.label.clone(),
                taken: 0,
                snoozed: 0,
                skipped: 0,
                ignored: 0,
                absorbed: 0,
                expired: 0,
                adherence: None,
            };
            let mut seen = 0;
            for (rule_id, outcome, n) in counts.iter().filter(|(id, _, _)| *id == rule.id) {
                let _ = rule_id;
                let n = *n as i32;
                match outcome {
                    BreakOutcome::Taken => row.taken = n,
                    BreakOutcome::Snoozed => row.snoozed = n,
                    BreakOutcome::Skipped => row.skipped = n,
                    BreakOutcome::Ignored => row.ignored = n,
                    BreakOutcome::Absorbed => row.absorbed = n,
                    BreakOutcome::Expired => row.expired = n,
                    BreakOutcome::Pending => {}
                }
                if outcome.counts_towards_adherence() {
                    seen += n;
                }
            }
            if seen > 0 {
                row.adherence = Some(f64::from(row.taken) / f64::from(seen));
            }
            per_rule.push(row);
        }
        Ok(GqlBreakStats { per_rule })
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd backend && cargo test -p api break_stats`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add backend/crates/api/src/graphql/query.rs backend/crates/api/src/types \
        backend/crates/api/src/graphql/tests.rs
git commit -m "Add breakStats query with adherence over seen outcomes"
```

---

### Task 10: React settings screen

**Files:**
- Create: `frontend/src/graphql/queries/break-rules.ts`
- Create: `frontend/src/graphql/mutations/break-rules.ts`
- Create: `frontend/src/hooks/use-break-rules.ts`
- Create: `frontend/src/components/breaks/BreakRuleRow.tsx`
- Create: `frontend/src/components/breaks/BreakRoutineSettings.tsx`
- Create: `frontend/src/components/breaks/BreakRoutineSettings.test.tsx`
- Modify: `frontend/src/pages/SettingsPage.tsx` (mount the section; add the four config keys)

**Interfaces:**
- Consumes: the GraphQL schema from Tasks 8–9.
- Produces: `useBreakRules()` returning `{ rules, stats, loading, error, createRule, updateRule, deleteRule }`; the two components. Nothing else consumes them.

**Facts about this codebase, already checked for you:**
- `SettingsSection` is a **local, non-exported** function inside `SettingsPage.tsx`, with props `{ title: string; icon: React.ReactNode; children; defaultOpen?: boolean }`. `icon` is required. You mount your component *from* `SettingsPage`, so you never import `SettingsSection` — do not try.
- The four scalars go through the existing `useSettings()` hook (`src/hooks/use-settings.ts`), which returns `{ configuration, syncStatuses, loading, error, syncing, updateConfig, forceSync, refetch }`. The setter is **`updateConfig(key: string, value: string)`** — use it; do not write a second configuration mutation.
- `useSettings()` already exposes `configuration` as a plain key→value object, so the four break keys are readable from it once they exist in `CONFIG_KEYS`.
- Tests in this repo mock with `vi.mock` and import through the `@/` path alias (see `src/pages/MemoryPage.test.tsx`). Prefer `vi.mock('@/hooks/use-break-rules', ...)` over a relative path, to match.

- [ ] **Step 1: Write the failing component test**

Create `frontend/src/components/breaks/BreakRoutineSettings.test.tsx`, following the mocking pattern `frontend/src/pages/MemoryPage.test.tsx` already uses:

```tsx
import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { BreakRoutineSettings } from './BreakRoutineSettings';

const updateRule = vi.fn();

vi.mock('@/hooks/use-break-rules', () => ({
  useBreakRules: () => ({
    rules: [
      { id: 'r1', kind: 'VISUAL', label: 'Pause visuelle', body: 'Regarde au loin',
        cadence: 'INTERVAL', intervalMinutes: 20, atTime: null,
        durationSeconds: 30, priority: 1, enabled: true, urgency: 'LOW' },
      { id: 'r4', kind: 'STRENGTH', label: 'Renfo épaule', body: 'Élastique',
        cadence: 'DAILY', intervalMinutes: null, atTime: '14:00',
        durationSeconds: 120, priority: 4, enabled: false, urgency: 'NORMAL' },
    ],
    stats: { perRule: [{ ruleId: 'r1', label: 'Pause visuelle', taken: 3, snoozed: 0,
                         skipped: 1, ignored: 0, absorbed: 9, expired: 2, adherence: 0.75 }] },
    loading: false,
    error: undefined,
    createRule: vi.fn(),
    updateRule,
    deleteRule: vi.fn(),
  }),
}));

describe('BreakRoutineSettings', () => {
  it('lists every rule with its cadence rendered in its own shape', () => {
    render(<BreakRoutineSettings />);
    expect(screen.getByDisplayValue('Pause visuelle')).toBeDefined();
    expect(screen.getByLabelText(/intervalle/i)).toHaveValue(20);
    expect(screen.getByLabelText(/heure/i)).toHaveValue('14:00');
  });

  it('toggling a rule calls updateRule with the flipped flag', () => {
    render(<BreakRoutineSettings />);
    fireEvent.click(screen.getAllByRole('checkbox', { name: /activ/i })[0]);
    expect(updateRule).toHaveBeenCalledWith('r1', expect.objectContaining({ enabled: false }));
  });

  it('shows adherence as a percentage of what the user actually saw', () => {
    render(<BreakRoutineSettings />);
    expect(screen.getByText('75 %')).toBeDefined();
  });

  it('refuses to save an interval rule with a non-positive interval', () => {
    render(<BreakRoutineSettings />);
    fireEvent.change(screen.getByLabelText(/intervalle/i), { target: { value: '0' } });
    expect(screen.getByText(/doit être positif/i)).toBeDefined();
    expect(updateRule).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend && pnpm test BreakRoutineSettings`
Expected: FAIL — cannot resolve `./BreakRoutineSettings`.

- [ ] **Step 3: Write the GraphQL documents**

`frontend/src/graphql/queries/break-rules.ts`:

```ts
export const BREAK_RULES_QUERY = `
  query BreakRules {
    breakRules {
      id kind label body cadence intervalMinutes atTime
      durationSeconds priority enabled urgency
    }
  }
`;

export const BREAK_STATS_QUERY = `
  query BreakStats($from: String!, $to: String!) {
    breakStats(from: $from, to: $to) {
      perRule { ruleId label taken snoozed skipped ignored absorbed expired adherence }
    }
  }
`;
```

`frontend/src/graphql/mutations/break-rules.ts`:

```ts
export const CREATE_BREAK_RULE = `
  mutation CreateBreakRule($input: BreakRuleInput!) {
    createBreakRule(input: $input) { id }
  }
`;

export const UPDATE_BREAK_RULE = `
  mutation UpdateBreakRule($id: ID!, $input: BreakRuleInput!) {
    updateBreakRule(id: $id, input: $input) { id }
  }
`;

export const DELETE_BREAK_RULE = `
  mutation DeleteBreakRule($id: ID!) {
    deleteBreakRule(id: $id)
  }
`;
```

- [ ] **Step 4: Write the hook**

`frontend/src/hooks/use-break-rules.ts` — urql `useQuery` for both documents (stats over the last 30 days) and `useMutation` for the three mutations, exposing `createRule(input)`, `updateRule(id, input)`, `deleteRule(id)`. Mirror the shape of an existing hook such as `use-settings.ts` for the loading/error contract and for how it re-executes queries after a mutation.

- [ ] **Step 5: Write `BreakRuleRow.tsx`**

One row per rule: an enable checkbox labelled "Activée", a kind select, label and body text inputs, a cadence select, then **either** a number input labelled "Intervalle (min)" **or** a time input labelled "Heure", a duration input, a priority input, an urgency select, and a delete button. Validation is local and blocking: an `INTERVAL` rule with `intervalMinutes <= 0` renders the message "doit être positif" and does not call `updateRule`; a `DAILY` rule with an unparseable `atTime` renders "format HH:MM attendu". Changing the cadence select clears the field belonging to the other shape, so the input sent to the API always satisfies the XOR the server enforces.

- [ ] **Step 6: Write `BreakRoutineSettings.tsx`**

Renders, in order: the master switch and the four scalars (wired to the existing configuration mutation, not to the break mutations), then a `BreakRuleRow` per rule sorted by priority, then an "Ajouter une pause" button that creates a rule with sensible defaults (`kind: POSTURE`, `cadence: INTERVAL`, `intervalMinutes: 30`, `durationSeconds: 120`, `priority: max + 1`, `enabled: true`, `urgency: NORMAL`), then the 30-day stats panel. In the panel, adherence renders as `Math.round(a * 100) + ' %'` and as "—" when null; absorbed and expired are shown apart from the seen outcomes, labelled so it is clear they never reached a screen.

- [ ] **Step 7: Run the test to verify it passes**

Run: `cd frontend && pnpm test BreakRoutineSettings`
Expected: PASS (4 tests).

- [ ] **Step 8: Mount the section**

In `frontend/src/pages/SettingsPage.tsx`, add the four keys to `CONFIG_KEYS`:

```ts
  BREAKS_ENABLED: 'aplan.breaks.enabled',
  BREAKS_GRACE: 'aplan.breaks.meeting_grace_minutes',
  BREAKS_SNOOZE: 'aplan.breaks.snooze_minutes',
  BREAKS_SHOW_AS: 'aplan.breaks.suppressing_show_as',
```

and mount the section alongside the existing ones, reusing the file's own `SettingsSection` component and following how its siblings pass a title and an icon.

`SettingsSection`'s `icon` prop is **required**, and there is no `PauseIcon` in the codebase. Write one as a local function inside `SettingsPage.tsx`, in the exact style of the `GearIcon` / `SyncIcon` / `OutlookIcon` functions already there: a bare inline `<svg className="w-5 h-5 text-gray-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>` with heroicons-style `<path strokeLinecap="round" strokeLinejoin="round" d="..." />` children. Do not add an icon library.

- [ ] **Step 9: Type-check, build and run the frontend suite**

Run: `cd frontend && pnpm build && pnpm test --run`

Expected: build OK; your new tests PASS.

**Known pre-existing failure — not yours, do not fix it:** `src/presentation/app.test.tsx`
fails to even transform, with `Failed to resolve import "@application/index" from
"src/presentation/app.tsx"`. It was already failing before this branch touched the frontend
(baseline: 1 failed suite, 35 passed, 282 tests passing). Leave it exactly as it is — it is
unrelated to breaks, and fixing it is out of scope. Your bar is: still 1 failed suite (that
one), and every other suite green including yours.

- [ ] **Step 10: Commit**

```bash
git add frontend/src/graphql/queries/break-rules.ts \
        frontend/src/graphql/mutations/break-rules.ts \
        frontend/src/hooks/use-break-rules.ts \
        frontend/src/components/breaks/ \
        frontend/src/pages/SettingsPage.tsx
git commit -m "Add the break routine settings section"
```

---

### Task 11: Specification updates

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md`
- Modify: `SPEC_TECHNIQUE.md`
- Modify: `CLAUDE.md` (table count and the new tables' one-paragraph note)

**Interfaces:**
- Consumes: everything above.
- Produces: nothing code-facing.

**Where things go, already checked for you:**
- `SPEC_FONCTIONNELLE.md` runs `## 1.` … `## 12.` (1 Contexte … 7 Règles métier … 12 Glossaire). Business rules live in **§7**, numbered `R01`…`R64`; the next free id is **R65**. Add the break rules there, in a new `### 7.x` subsection, and extend §8 (Données) with the two tables.
- `SPEC_TECHNIQUE.md` runs `## 1.` … `## 20.` (20 = Recurring Tasks). The next free top-level number is **21**.
- Both documents are in **French**. Match their tone and table style; do not translate the surrounding text.

- [ ] **Step 1: Update `SPEC_FONCTIONNELLE.md`**

Add a section (French, matching the document's existing tone and numbering) covering: the routine as N configurable cadences; the seeded four; the wall-clock anchoring on the workday windows; the meeting suppression and its deferral to the meeting's end plus a grace period; the one-popup-per-tick rule and why collisions are absorbed; the three notification buttons and the six outcomes, with the `skipped` / `ignored` distinction spelled out; the adherence statistic and what it excludes.

- [ ] **Step 2: Update `SPEC_TECHNIQUE.md`**

Add (French): the two tables with their columns and CHECK constraints; the five configuration keys and their defaults; the `decide` decision order; the layering rule that keeps `chrono_tz` out of `domain` and resolves daily cadences in `application`; the `Notifier` trait and its two implementations, including the `DBUS_SESSION_BUS_ADDRESS` selection at startup; `RetryPolicy::breaks()` and the 30 s / 5 min cadence; the GraphQL surface.

- [ ] **Step 3: Update `CLAUDE.md`**

Change "23 tables" to "25 tables" and add `break_rules`, `break_events` to the list, plus a short paragraph in the style of the existing sessions/timesheet notes explaining that the break engine is wall-clock anchored on the workday windows, that `priority` exists to collapse the built-in cadence collisions, and that `absorbed` and `expired` are excluded from adherence.

- [ ] **Step 4: Verify the whole thing builds and passes**

Run: `cd backend && cargo test -p domain -p application -p infrastructure -p api && cargo clippy -p domain -p application -p infrastructure -p api`
Run: `cd frontend && pnpm test && pnpm build`
Expected: all PASS, no new clippy warnings.

- [ ] **Step 5: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md CLAUDE.md
git commit -m "Document the break routine in the specs"
```
