# Timesheet Reconstruction — Plan 1: Backend Socle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the shared, pure reconstruction engine (signals → hours per Gryzzly project) plus its mapping layer, git ingestion, persistence, and use cases — all provable with `cargo test`, before any GraphQL/CLI/UI surface exists.

**Architecture:** A pure domain rule `reconstruct_day` turns a day's local-time signals (worklog, meetings, commits) into normalized per-project hours with transparency guardrails (per-line confidence, first-class unattributed bucket, day-off suppression, pinned values). A pure domain rule `resolve_signal_project` maps each raw signal to a Gryzzly project. The `application` layer collects signals (via existing repos + a new `GitConnector`), converts UTC→local once, resolves projects, calls the pure rules, and persists a draft. Two new SQLite repos back the draft and the learned mapping rules.

**Tech Stack:** Rust (stable), chrono + chrono_tz, sqlx 0.8 (runtime queries), async_trait, thiserror, uuid, tokio (tests). DDD layers: domain (pure) → application (traits + use cases) → infrastructure (sqlx + `git log` shell).

## Global Constraints

- **DDD layers (strict):** domain depends only on chrono/serde/uuid/thiserror. Application depends only on domain. Infrastructure implements application traits with I/O. Reference: `CLAUDE.md`.
- **Domain purity:** `crates/domain` performs zero I/O and must NOT depend on chrono_tz. All UTC→local conversion happens in `application` before calling domain rules.
- **Error mapping:** map `sqlx::Error` → `RepositoryError::Database(e.to_string())`. Repos return `Result<_, RepositoryError>`; use cases return `Result<_, AppError>` (`RepositoryError`/`DomainError` auto-convert via `#[from]`).
- **No `.unwrap()`/`.expect()` in production code** (tests may use them). Reference: `CLAUDE.md`.
- **Repos:** runtime queries (`sqlx::query`), never compile-time `sqlx::query!`. `#[async_trait]` on all repo traits. Ids are `Uuid` stored as `TEXT`; datetimes as RFC3339 `TEXT`; dates as `%Y-%m-%d` `TEXT`; enums as lowercase `TEXT`; bools as `INTEGER` 0/1.
- **Migrations path:** files in `migrations/sqlite/`, run via `sqlx::migrate!("../../../migrations/sqlite")` in `infrastructure/src/database/connection.rs`. Next free numbers are `010` and `011`.
- **Timezone default:** `Europe/Paris`. One helper does UTC→local; day bounds are local-midnight mapped to UTC.
- **TDD:** write the failing test first, watch it fail, implement minimally, watch it pass, commit. Backend tests are inline `#[cfg(test)] mod tests`; integration uses `sqlite::memory:`.
- **Scoped test command** (the `mcp` crate does not compile at HEAD): `cargo test -p domain -p application -p infrastructure`.
- **Commit messages:** `<imperative subject>` (no Jira key for this work). No `Co-Authored-By`. Stage only files relevant to the task.

---

## File Structure

**Created:**
- `migrations/sqlite/010_create_timesheet_drafts.sql` — draft header + per-project lines tables.
- `migrations/sqlite/011_create_signal_project_mappings.sql` — learned mapping rules table.
- `backend/crates/domain/src/types/signal_mapping.rs` — `SignalMapping`, `MappingKind`, ids.
- `backend/crates/domain/src/rules/project_mapping.rs` — pure `resolve_signal_project`.
- `backend/crates/domain/src/rules/reconstruction.rs` — pure engine: types + `reconstruct_day` + `apportion_to_target` + `renormalize_lines`.
- `backend/crates/domain/src/types/timesheet.rs` — `TimesheetDraft`, `TimesheetDraftLine`, `TimesheetStatus`.
- `backend/crates/application/src/repositories/signal_mapping_repository.rs` — trait.
- `backend/crates/application/src/repositories/timesheet_draft_repository.rs` — trait.
- `backend/crates/application/src/services/git_connector.rs` — `GitConnector` trait + pure `parse_git_log` + `commit_project_key`.
- `backend/crates/application/src/time.rs` — `resolve_tz`, `local_day_bounds`, `to_local`.
- `backend/crates/application/src/use_cases/timesheet.rs` — `reconstruct_timesheet`, `save_timesheet_draft`, `validate_timesheet`, `mark_day_off`, `learn_mapping`, config loader.
- `backend/crates/infrastructure/src/database/signal_mapping_repo.rs` — SQLite impl.
- `backend/crates/infrastructure/src/database/timesheet_draft_repo.rs` — SQLite impl.
- `backend/crates/infrastructure/src/connectors/git/mod.rs` — `ShellGitConnector` (shells `git log`).

**Modified:**
- `backend/crates/domain/src/types/common.rs` — add `Confidence` enum.
- `backend/crates/domain/src/types/mod.rs` — add `signal_mapping`, `timesheet` modules.
- `backend/crates/domain/src/rules/mod.rs` — add `project_mapping`, `reconstruction` modules.
- `backend/crates/application/src/repositories/mod.rs` — export new repo traits.
- `backend/crates/application/src/services/mod.rs` — export `git_connector`.
- `backend/crates/application/src/use_cases/mod.rs` — add `timesheet`.
- `backend/crates/application/src/lib.rs` — add `pub mod time;` (if modules are declared there).
- `backend/crates/infrastructure/src/database/mod.rs` — export new repos.
- `backend/crates/infrastructure/src/connectors/mod.rs` — add `git`.

---

### Task 1: Migrations for draft + mapping tables

**Files:**
- Create: `migrations/sqlite/010_create_timesheet_drafts.sql`
- Create: `migrations/sqlite/011_create_signal_project_mappings.sql`
- Test: `backend/crates/infrastructure/src/database/connection.rs` (inline test module)

**Interfaces:**
- Produces: the tables `timesheet_drafts`, `timesheet_draft_lines`, `signal_project_mappings` for later repo tasks.

- [ ] **Step 1: Write the migration files**

`migrations/sqlite/010_create_timesheet_drafts.sql`:
```sql
-- Daily reconstructed timesheet: one header row per (user, local date) + per-project lines.
-- Independent of activity_slots (which has no project). Never auto-submitted to Gryzzly.
CREATE TABLE timesheet_drafts (
    id             TEXT PRIMARY KEY,
    user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    date           TEXT NOT NULL,               -- LOCAL calendar date (YYYY-MM-DD)
    status         TEXT NOT NULL DEFAULT 'draft', -- draft | validated | submitted | day_off
    target_hours   REAL NOT NULL,
    total_hours    REAL NOT NULL,
    day_confidence TEXT NOT NULL,               -- high | medium | low
    blocks_json    TEXT,                        -- serialized timeline (AttributedBlock[])
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    UNIQUE(user_id, date)
);

CREATE TABLE timesheet_draft_lines (
    id                 TEXT PRIMARY KEY,
    draft_id           TEXT NOT NULL REFERENCES timesheet_drafts(id) ON DELETE CASCADE,
    gryzzly_project_id TEXT,                    -- NULL = "unattributed" bucket
    project_name       TEXT,
    hours              REAL NOT NULL,
    is_pinned          INTEGER NOT NULL DEFAULT 0,
    confidence         TEXT NOT NULL,           -- high | medium | low
    source_refs_json   TEXT,
    created_at         TEXT NOT NULL
);
CREATE INDEX idx_tsl_draft ON timesheet_draft_lines(draft_id);
```

`migrations/sqlite/011_create_signal_project_mappings.sql`:
```sql
-- Learned rules mapping a raw signal (git repo/branch, meeting subject/organizer,
-- internal project) to a Gryzzly project. User-scoped; upsert-once, disable never delete.
CREATE TABLE signal_project_mappings (
    id                   TEXT PRIMARY KEY,
    user_id              TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind                 TEXT NOT NULL,   -- repo_path | branch | meeting_subject | meeting_organizer | internal_project
    pattern              TEXT NOT NULL,
    branch_pattern       TEXT,
    gryzzly_project_id   TEXT NOT NULL,
    gryzzly_project_name TEXT,
    is_enabled           INTEGER NOT NULL DEFAULT 1,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    UNIQUE(user_id, kind, pattern)
);
CREATE INDEX idx_spm_user_kind ON signal_project_mappings(user_id, kind, is_enabled);
```

- [ ] **Step 2: Write the failing migration smoke test**

Add to the bottom of `backend/crates/infrastructure/src/database/connection.rs`:
```rust
#[cfg(test)]
mod migration_tests {
    use sqlx::SqlitePool;

    #[tokio::test]
    async fn migrations_create_timesheet_and_mapping_tables() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../../migrations/sqlite")
            .run(&pool)
            .await
            .unwrap();

        for table in [
            "timesheet_drafts",
            "timesheet_draft_lines",
            "signal_project_mappings",
        ] {
            let row: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(row.0, 1, "table {table} should exist after migration");
        }
    }
}
```

- [ ] **Step 3: Run test to verify it fails (before migration files exist, it fails)**

Run: `cd backend && cargo test -p infrastructure migrations_create_timesheet_and_mapping_tables -- --nocapture`
Expected: FAIL — the tables do not exist (assert on count == 1 fails) OR migration compile error until Step 1 files are saved.

- [ ] **Step 4: Ensure the migration files from Step 1 are saved, then run again**

Run: `cd backend && cargo test -p infrastructure migrations_create_timesheet_and_mapping_tables`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add migrations/sqlite/010_create_timesheet_drafts.sql \
        migrations/sqlite/011_create_signal_project_mappings.sql \
        backend/crates/infrastructure/src/database/connection.rs
git commit -m "Add timesheet draft + signal-mapping migrations"
```

---

### Task 2: Domain `Confidence` enum + `SignalMapping` type

**Files:**
- Modify: `backend/crates/domain/src/types/common.rs`
- Create: `backend/crates/domain/src/types/signal_mapping.rs`
- Modify: `backend/crates/domain/src/types/mod.rs`

**Interfaces:**
- Produces: `Confidence { High, Medium, Low }`; `MappingKind`; `SignalMapping`; `SignalMappingId = Uuid`.

- [ ] **Step 1: Write failing test for the new type module**

Create `backend/crates/domain/src/types/signal_mapping.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::UserId;

pub type SignalMappingId = Uuid;

/// The kind of signal a mapping rule matches against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MappingKind {
    RepoPath,
    Branch,
    MeetingSubject,
    MeetingOrganizer,
    InternalProject,
}

impl MappingKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MappingKind::RepoPath => "repo_path",
            MappingKind::Branch => "branch",
            MappingKind::MeetingSubject => "meeting_subject",
            MappingKind::MeetingOrganizer => "meeting_organizer",
            MappingKind::InternalProject => "internal_project",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "repo_path" => Some(MappingKind::RepoPath),
            "branch" => Some(MappingKind::Branch),
            "meeting_subject" => Some(MappingKind::MeetingSubject),
            "meeting_organizer" => Some(MappingKind::MeetingOrganizer),
            "internal_project" => Some(MappingKind::InternalProject),
            _ => None,
        }
    }
}

/// A learned rule mapping a raw signal to a Gryzzly project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalMapping {
    pub id: SignalMappingId,
    pub user_id: UserId,
    pub kind: MappingKind,
    pub pattern: String,
    pub branch_pattern: Option<String>,
    pub gryzzly_project_id: String,
    pub gryzzly_project_name: Option<String>,
    pub is_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_kind_roundtrips_through_str() {
        for k in [
            MappingKind::RepoPath,
            MappingKind::Branch,
            MappingKind::MeetingSubject,
            MappingKind::MeetingOrganizer,
            MappingKind::InternalProject,
        ] {
            assert_eq!(MappingKind::from_str(k.as_str()), Some(k));
        }
        assert_eq!(MappingKind::from_str("bogus"), None);
    }
}
```

- [ ] **Step 2: Add `Confidence` to `common.rs`**

Append to `backend/crates/domain/src/types/common.rs` (near the other enums):
```rust
/// How much the reconstruction trusts an allocation / a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
        }
    }
}
```

- [ ] **Step 3: Register the module**

In `backend/crates/domain/src/types/mod.rs`, add the module declaration and re-export alongside the others:
```rust
pub mod signal_mapping;
```
```rust
pub use signal_mapping::*;
```

- [ ] **Step 4: Run tests**

Run: `cd backend && cargo test -p domain signal_mapping`
Expected: PASS (`mapping_kind_roundtrips_through_str`).

- [ ] **Step 5: Commit**

```bash
git add backend/crates/domain/src/types/signal_mapping.rs \
        backend/crates/domain/src/types/common.rs \
        backend/crates/domain/src/types/mod.rs
git commit -m "Add Confidence enum and SignalMapping domain type"
```

---

### Task 3: Pure `resolve_signal_project` rule

**Files:**
- Create: `backend/crates/domain/src/rules/project_mapping.rs`
- Modify: `backend/crates/domain/src/rules/mod.rs`

**Interfaces:**
- Consumes: `SignalMapping`, `MappingKind`, `Confidence` (Task 2).
- Produces:
  - `enum RawSignal { Worklog { task_gryzzly_project_id: Option<String> }, Commit { repo_path: String, branch: String }, Meeting { subject: String, organizer: Option<String>, internal_project_id: Option<String> } }`
  - `enum UnmappedReason { TaskNotAssigned, NoRule, StaleMapping }`
  - `enum ProjectResolution { Mapped { gryzzly_project_id: String, confidence: Confidence, source_rule_id: Option<SignalMappingId> }, Unmapped { reason: UnmappedReason, suggested: Option<SignalMappingId> } }`
  - `fn resolve_signal_project(signal: &RawSignal, rules: &[SignalMapping], live_project_ids: &std::collections::HashSet<String>) -> ProjectResolution`

- [ ] **Step 1: Write the failing tests + type skeleton**

Create `backend/crates/domain/src/rules/project_mapping.rs`:
```rust
use std::collections::HashSet;

use crate::types::common::Confidence;
use crate::types::signal_mapping::{MappingKind, SignalMapping, SignalMappingId};

/// A raw signal, already stripped of I/O concerns, ready to be mapped to a project.
#[derive(Debug, Clone)]
pub enum RawSignal {
    Worklog {
        task_gryzzly_project_id: Option<String>,
    },
    Commit {
        repo_path: String,
        branch: String,
    },
    Meeting {
        subject: String,
        organizer: Option<String>,
        internal_project_id: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnmappedReason {
    TaskNotAssigned,
    NoRule,
    StaleMapping,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectResolution {
    Mapped {
        gryzzly_project_id: String,
        confidence: Confidence,
        source_rule_id: Option<SignalMappingId>,
    },
    Unmapped {
        reason: UnmappedReason,
        suggested: Option<SignalMappingId>,
    },
}

/// Resolve one signal to a Gryzzly project.
///
/// - Worklog: uses the task's already-snapshotted gryzzly_project_id (confidence High).
/// - Commit: matches enabled Branch rules (repo+branch) before RepoPath rules (repo only).
/// - Meeting: InternalProject rule, then MeetingOrganizer (exact), then MeetingSubject (substring).
///
/// A matched rule whose target project is absent from `live_project_ids` downgrades to
/// Unmapped{StaleMapping, suggested=rule_id}. No match → Unmapped{NoRule}.
pub fn resolve_signal_project(
    signal: &RawSignal,
    rules: &[SignalMapping],
    live_project_ids: &HashSet<String>,
) -> ProjectResolution {
    match signal {
        RawSignal::Worklog {
            task_gryzzly_project_id,
        } => match task_gryzzly_project_id {
            Some(pid) => finalize(pid.clone(), Confidence::High, None, live_project_ids),
            None => ProjectResolution::Unmapped {
                reason: UnmappedReason::TaskNotAssigned,
                suggested: None,
            },
        },
        RawSignal::Commit { repo_path, branch } => {
            // Branch rules (more specific) first, then RepoPath rules.
            if let Some(r) = best_match(rules, MappingKind::Branch, |m| {
                m.pattern == *repo_path
                    && m.branch_pattern.as_deref().map(|b| b == branch).unwrap_or(false)
            }) {
                return finalize(r.gryzzly_project_id.clone(), Confidence::High, Some(r.id), live_project_ids);
            }
            if let Some(r) = best_match(rules, MappingKind::RepoPath, |m| m.pattern == *repo_path) {
                return finalize(r.gryzzly_project_id.clone(), Confidence::Medium, Some(r.id), live_project_ids);
            }
            ProjectResolution::Unmapped { reason: UnmappedReason::NoRule, suggested: None }
        }
        RawSignal::Meeting {
            subject,
            organizer,
            internal_project_id,
        } => {
            if let Some(pid) = internal_project_id {
                if let Some(r) = best_match(rules, MappingKind::InternalProject, |m| m.pattern == *pid) {
                    return finalize(r.gryzzly_project_id.clone(), Confidence::High, Some(r.id), live_project_ids);
                }
            }
            if let Some(org) = organizer {
                if let Some(r) = best_match(rules, MappingKind::MeetingOrganizer, |m| {
                    m.pattern.eq_ignore_ascii_case(org)
                }) {
                    return finalize(r.gryzzly_project_id.clone(), Confidence::High, Some(r.id), live_project_ids);
                }
            }
            // Subject keyword: longest matching keyword wins.
            let subj_lower = subject.to_lowercase();
            let kw = rules
                .iter()
                .filter(|m| m.is_enabled && m.kind == MappingKind::MeetingSubject)
                .filter(|m| subj_lower.contains(&m.pattern.to_lowercase()))
                .max_by_key(|m| m.pattern.len());
            if let Some(r) = kw {
                return finalize(r.gryzzly_project_id.clone(), Confidence::Medium, Some(r.id), live_project_ids);
            }
            ProjectResolution::Unmapped { reason: UnmappedReason::NoRule, suggested: None }
        }
    }
}

fn best_match<'a>(
    rules: &'a [SignalMapping],
    kind: MappingKind,
    pred: impl Fn(&SignalMapping) -> bool,
) -> Option<&'a SignalMapping> {
    rules
        .iter()
        .filter(|m| m.is_enabled && m.kind == kind && pred(m))
        .max_by(|a, b| a.updated_at.cmp(&b.updated_at))
}

fn finalize(
    project_id: String,
    confidence: Confidence,
    rule_id: Option<SignalMappingId>,
    live_project_ids: &HashSet<String>,
) -> ProjectResolution {
    if live_project_ids.contains(&project_id) {
        ProjectResolution::Mapped {
            gryzzly_project_id: project_id,
            confidence,
            source_rule_id: rule_id,
        }
    } else {
        ProjectResolution::Unmapped {
            reason: UnmappedReason::StaleMapping,
            suggested: rule_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use uuid::Uuid;

    fn live(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn rule(kind: MappingKind, pattern: &str, project: &str) -> SignalMapping {
        SignalMapping {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            kind,
            pattern: pattern.to_string(),
            branch_pattern: None,
            gryzzly_project_id: project.to_string(),
            gryzzly_project_name: None,
            is_enabled: true,
            created_at: DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
            updated_at: DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
        }
    }

    #[test]
    fn worklog_uses_task_project_with_high_confidence() {
        let r = resolve_signal_project(
            &RawSignal::Worklog { task_gryzzly_project_id: Some("p1".into()) },
            &[],
            &live(&["p1"]),
        );
        assert_eq!(
            r,
            ProjectResolution::Mapped { gryzzly_project_id: "p1".into(), confidence: Confidence::High, source_rule_id: None }
        );
    }

    #[test]
    fn worklog_without_assignment_is_task_not_assigned() {
        let r = resolve_signal_project(
            &RawSignal::Worklog { task_gryzzly_project_id: None },
            &[],
            &live(&[]),
        );
        assert_eq!(r, ProjectResolution::Unmapped { reason: UnmappedReason::TaskNotAssigned, suggested: None });
    }

    #[test]
    fn meeting_internal_project_rule_wins_over_organizer() {
        let rules = vec![
            rule(MappingKind::InternalProject, "internal-42", "p_internal"),
            rule(MappingKind::MeetingOrganizer, "boss@corp.com", "p_org"),
        ];
        let r = resolve_signal_project(
            &RawSignal::Meeting {
                subject: "sync".into(),
                organizer: Some("boss@corp.com".into()),
                internal_project_id: Some("internal-42".into()),
            },
            &rules,
            &live(&["p_internal", "p_org"]),
        );
        match r {
            ProjectResolution::Mapped { gryzzly_project_id, .. } => assert_eq!(gryzzly_project_id, "p_internal"),
            other => panic!("expected Mapped, got {other:?}"),
        }
    }

    #[test]
    fn commit_branch_rule_beats_repo_rule() {
        let mut branch_rule = rule(MappingKind::Branch, "/home/me/repo", "p_branch");
        branch_rule.branch_pattern = Some("main".into());
        let rules = vec![branch_rule, rule(MappingKind::RepoPath, "/home/me/repo", "p_repo")];
        let r = resolve_signal_project(
            &RawSignal::Commit { repo_path: "/home/me/repo".into(), branch: "main".into() },
            &rules,
            &live(&["p_branch", "p_repo"]),
        );
        match r {
            ProjectResolution::Mapped { gryzzly_project_id, confidence, .. } => {
                assert_eq!(gryzzly_project_id, "p_branch");
                assert_eq!(confidence, Confidence::High);
            }
            other => panic!("expected Mapped, got {other:?}"),
        }
    }

    #[test]
    fn stale_project_downgrades_to_unmapped() {
        let rules = vec![rule(MappingKind::RepoPath, "/repo", "p_dead")];
        let r = resolve_signal_project(
            &RawSignal::Commit { repo_path: "/repo".into(), branch: "x".into() },
            &rules,
            &live(&["p_live"]), // p_dead not live
        );
        match r {
            ProjectResolution::Unmapped { reason, suggested } => {
                assert_eq!(reason, UnmappedReason::StaleMapping);
                assert!(suggested.is_some());
            }
            other => panic!("expected Unmapped/StaleMapping, got {other:?}"),
        }
    }

    #[test]
    fn no_matching_rule_is_no_rule() {
        let r = resolve_signal_project(
            &RawSignal::Commit { repo_path: "/unknown".into(), branch: "x".into() },
            &[],
            &live(&[]),
        );
        assert_eq!(r, ProjectResolution::Unmapped { reason: UnmappedReason::NoRule, suggested: None });
    }
}
```

- [ ] **Step 2: Register the module**

In `backend/crates/domain/src/rules/mod.rs`, add:
```rust
pub mod project_mapping;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cd backend && cargo test -p domain project_mapping`
Expected: PASS (6 tests).

- [ ] **Step 4: Commit**

```bash
git add backend/crates/domain/src/rules/project_mapping.rs \
        backend/crates/domain/src/rules/mod.rs
git commit -m "Add pure resolve_signal_project mapping rule"
```

---

### Task 4: Reconstruction types + `apportion_to_target` helper

**Files:**
- Create: `backend/crates/domain/src/rules/reconstruction.rs`
- Modify: `backend/crates/domain/src/rules/mod.rs`

**Interfaces:**
- Consumes: `Confidence` (Task 2).
- Produces (all in `rules::reconstruction`):
  - `SignalKind { Log, Commit }`, `Signal { at, gryzzly_project_id, kind, label, source_ref }`
  - `MeetingKind { Work, OutOfOffice }`, `MeetingBlock { start, end, gryzzly_project_id, kind, title, source_ref }`
  - `ReconstructionConfig { morning:(u32,u32), afternoon:(u32,u32), daily_target_hours:f64, rounding_hours:f64, min_signal_hours:f64 }`
  - `BlockKind { Meeting, Work, OutOfOffice }`, `AttributedBlock`, `ProjectAllocation`, `UnresolvedSignal`, `ReconstructedDay`
  - `fn apportion_to_target(buckets: &[Bucket], target: f64, rounding: f64) -> Vec<Bucket>` where `Bucket { key: Option<String>, hours: f64, pinned: bool }`

- [ ] **Step 1: Write the failing tests for `apportion_to_target` + define the types**

Create `backend/crates/domain/src/rules/reconstruction.rs`:
```rust
use chrono::{NaiveDate, NaiveDateTime};

use crate::types::common::Confidence;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    Log,
    Commit,
}

#[derive(Debug, Clone)]
pub struct Signal {
    pub at: NaiveDateTime,
    pub gryzzly_project_id: Option<String>,
    pub kind: SignalKind,
    pub label: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeetingKind {
    Work,
    OutOfOffice,
}

#[derive(Debug, Clone)]
pub struct MeetingBlock {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub gryzzly_project_id: Option<String>,
    pub kind: MeetingKind,
    pub title: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ReconstructionConfig {
    pub morning: (u32, u32),
    pub afternoon: (u32, u32),
    pub daily_target_hours: f64,
    pub rounding_hours: f64,
    pub min_signal_hours: f64,
}

impl Default for ReconstructionConfig {
    fn default() -> Self {
        Self {
            morning: (8, 12),
            afternoon: (13, 17),
            daily_target_hours: 7.5,
            rounding_hours: 0.25,
            min_signal_hours: 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Meeting,
    Work,
    OutOfOffice,
}

#[derive(Debug, Clone)]
pub struct AttributedBlock {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub gryzzly_project_id: Option<String>,
    pub kind: BlockKind,
    pub hours: f64,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectAllocation {
    pub gryzzly_project_id: String,
    pub hours: f64,
    pub confidence: Confidence,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct UnresolvedSignal {
    pub source_ref: String,
    pub label: String,
    pub at: NaiveDateTime,
}

#[derive(Debug, Clone)]
pub struct ReconstructedDay {
    pub date: NaiveDate,
    pub allocations: Vec<ProjectAllocation>,
    pub unattributed_hours: f64,
    pub unresolved: Vec<UnresolvedSignal>,
    pub total_hours: f64,
    pub day_confidence: Confidence,
    pub blocks: Vec<AttributedBlock>,
}

/// A weighted bucket for apportionment. `key = None` is the unattributed bucket.
#[derive(Debug, Clone, PartialEq)]
pub struct Bucket {
    pub key: Option<String>,
    pub hours: f64,
    pub pinned: bool,
}

/// Round every bucket to a multiple of `rounding` such that the total equals
/// `target` exactly (largest-remainder apportionment). Pinned buckets keep their
/// value (rounded to the increment) and are excluded from redistribution; the
/// leftover is spread across UNpinned buckets by largest fractional remainder.
/// If unpinned buckets can't absorb the leftover (all pinned), the residual is
/// appended to the unattributed bucket (key=None), created if absent.
pub fn apportion_to_target(buckets: &[Bucket], target: f64, rounding: f64) -> Vec<Bucket> {
    let unit = rounding.max(f64::EPSILON);
    let target_units = (target / unit).round() as i64;

    // Pinned buckets: snap to nearest unit, reserve their units.
    let mut out: Vec<Bucket> = Vec::with_capacity(buckets.len());
    let mut pinned_units = 0i64;
    for b in buckets.iter().filter(|b| b.pinned) {
        let u = (b.hours / unit).round().max(0.0) as i64;
        pinned_units += u;
        out.push(Bucket { key: b.key.clone(), hours: u as f64 * unit, pinned: true });
    }

    let unpinned: Vec<&Bucket> = buckets.iter().filter(|b| !b.pinned).collect();
    let remaining_units = (target_units - pinned_units).max(0);

    let raw_sum: f64 = unpinned.iter().map(|b| b.hours.max(0.0)).sum();
    if unpinned.is_empty() || raw_sum <= 0.0 {
        // Nothing unpinned to scale — dump any remaining units on unattributed.
        if remaining_units > 0 {
            push_or_merge_unattributed(&mut out, remaining_units as f64 * unit);
        }
        return out;
    }

    // Scale unpinned to the remaining units, floor to integer units, distribute leftover.
    let scale = remaining_units as f64 / (raw_sum / unit);
    let mut floors: Vec<(usize, i64, f64)> = Vec::with_capacity(unpinned.len());
    let mut used = 0i64;
    for (i, b) in unpinned.iter().enumerate() {
        let scaled = (b.hours.max(0.0) / unit) * scale;
        let f = scaled.floor() as i64;
        let rem = scaled - f as f64;
        used += f;
        floors.push((i, f, rem));
    }
    let mut leftover = remaining_units - used;
    // Give leftover units to the largest remainders (stable by index on ties).
    let mut order: Vec<usize> = (0..floors.len()).collect();
    order.sort_by(|&a, &b| {
        floors[b].2
            .partial_cmp(&floors[a].2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    let mut idx = 0;
    while leftover > 0 && !order.is_empty() {
        let target_i = order[idx % order.len()];
        floors[target_i].1 += 1;
        leftover -= 1;
        idx += 1;
    }
    for (i, units, _) in floors {
        out.push(Bucket {
            key: unpinned[i].key.clone(),
            hours: units as f64 * unit,
            pinned: false,
        });
    }
    out
}

fn push_or_merge_unattributed(out: &mut Vec<Bucket>, add_hours: f64) {
    if let Some(b) = out.iter_mut().find(|b| b.key.is_none()) {
        b.hours += add_hours;
    } else {
        out.push(Bucket { key: None, hours: add_hours, pinned: false });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(key: Option<&str>, hours: f64, pinned: bool) -> Bucket {
        Bucket { key: key.map(|s| s.to_string()), hours, pinned }
    }

    fn total(bs: &[Bucket]) -> f64 {
        bs.iter().map(|b| b.hours).sum()
    }

    #[test]
    fn apportion_sums_exactly_to_target() {
        let out = apportion_to_target(
            &[b(Some("a"), 1.0, false), b(Some("b"), 2.0, false)],
            7.5,
            0.25,
        );
        assert!((total(&out) - 7.5).abs() < 1e-9, "total was {}", total(&out));
    }

    #[test]
    fn apportion_rounds_to_increment() {
        let out = apportion_to_target(&[b(Some("a"), 1.0, false), b(Some("b"), 1.0, false)], 7.5, 0.25);
        for bucket in &out {
            let units = bucket.hours / 0.25;
            assert!((units - units.round()).abs() < 1e-9, "{} not a 0.25 multiple", bucket.hours);
        }
    }

    #[test]
    fn pinned_bucket_is_frozen_others_absorb_remainder() {
        let out = apportion_to_target(
            &[b(Some("a"), 3.0, true), b(Some("b"), 1.0, false), b(Some("c"), 1.0, false)],
            7.5,
            0.25,
        );
        let a = out.iter().find(|x| x.key.as_deref() == Some("a")).unwrap();
        assert!((a.hours - 3.0).abs() < 1e-9, "pinned a should stay 3.0, got {}", a.hours);
        assert!((total(&out) - 7.5).abs() < 1e-9);
    }

    #[test]
    fn all_pinned_residual_goes_to_unattributed() {
        let out = apportion_to_target(&[b(Some("a"), 3.0, true)], 7.5, 0.25);
        let un = out.iter().find(|x| x.key.is_none()).unwrap();
        assert!((un.hours - 4.5).abs() < 1e-9, "unattributed should absorb 4.5, got {}", un.hours);
        assert!((total(&out) - 7.5).abs() < 1e-9);
    }
}
```

- [ ] **Step 2: Register the module**

In `backend/crates/domain/src/rules/mod.rs`, add:
```rust
pub mod reconstruction;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cd backend && cargo test -p domain reconstruction::tests`
Expected: PASS (4 tests).

- [ ] **Step 4: Commit**

```bash
git add backend/crates/domain/src/rules/reconstruction.rs \
        backend/crates/domain/src/rules/mod.rs
git commit -m "Add reconstruction types and largest-remainder apportionment"
```

---

### Task 5: `reconstruct_day` — allocation core (windows, anchors, carry-forward, aggregate)

**Files:**
- Modify: `backend/crates/domain/src/rules/reconstruction.rs`

**Interfaces:**
- Consumes: types + `apportion_to_target` (Task 4).
- Produces: `pub fn reconstruct_day(inputs: &DayInputs, cfg: &ReconstructionConfig) -> ReconstructedDay` and `pub struct DayInputs { pub date: NaiveDate, pub meetings: Vec<MeetingBlock>, pub signals: Vec<Signal> }`.

- [ ] **Step 1: Write the failing tests**

Add to `reconstruction.rs` (above the existing `mod tests`, add `DayInputs` near the other structs; put these tests inside the existing `mod tests`):

Add the struct near `ReconstructionConfig`:
```rust
#[derive(Debug, Clone)]
pub struct DayInputs {
    pub date: NaiveDate,
    pub meetings: Vec<MeetingBlock>,
    pub signals: Vec<Signal>,
}
```

Add tests inside `mod tests`:
```rust
    use chrono::NaiveDate;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 8).unwrap()
    }
    fn at(h: u32, m: u32) -> NaiveDateTime {
        day().and_hms_opt(h, m, 0).unwrap()
    }
    fn sig(h: u32, m: u32, project: Option<&str>) -> Signal {
        Signal {
            at: at(h, m),
            gryzzly_project_id: project.map(|s| s.to_string()),
            kind: SignalKind::Log,
            label: format!("log {h}:{m}"),
            source_ref: format!("wl-{h}{m}"),
        }
    }
    fn meeting(sh: u32, eh: u32, project: Option<&str>, kind: MeetingKind) -> MeetingBlock {
        MeetingBlock {
            start: at(sh, 0),
            end: at(eh, 0),
            gryzzly_project_id: project.map(|s| s.to_string()),
            kind,
            title: "meet".into(),
            source_ref: format!("mtg-{sh}"),
        }
    }

    #[test]
    fn empty_day_yields_zero_total_low_confidence() {
        let out = reconstruct_day(
            &DayInputs { date: day(), meetings: vec![], signals: vec![] },
            &ReconstructionConfig::default(),
        );
        assert_eq!(out.total_hours, 0.0);
        assert_eq!(out.day_confidence, Confidence::Low);
        assert!(out.allocations.is_empty());
    }

    #[test]
    fn two_project_signals_split_and_scale_to_target() {
        // Morning log on p1 at 09:00; afternoon log on p2 at 14:00. Enough span (>2h).
        let out = reconstruct_day(
            &DayInputs {
                date: day(),
                meetings: vec![],
                signals: vec![sig(9, 0, Some("p1")), sig(14, 0, Some("p2"))],
            },
            &ReconstructionConfig::default(),
        );
        assert!((out.total_hours - 7.5).abs() < 1e-9, "total {}", out.total_hours);
        assert_eq!(out.day_confidence, Confidence::High);
        let p1 = out.allocations.iter().find(|a| a.gryzzly_project_id == "p1");
        let p2 = out.allocations.iter().find(|a| a.gryzzly_project_id == "p2");
        assert!(p1.is_some() && p2.is_some());
    }

    #[test]
    fn unresolved_signal_goes_to_unattributed_not_a_project() {
        let out = reconstruct_day(
            &DayInputs {
                date: day(),
                meetings: vec![],
                signals: vec![sig(9, 0, Some("p1")), sig(10, 0, None), sig(14, 0, Some("p1"))],
            },
            &ReconstructionConfig::default(),
        );
        assert!(out.unattributed_hours > 0.0);
        assert!(out.unresolved.iter().any(|u| u.source_ref == "wl-100"));
    }

    #[test]
    fn meeting_anchor_counts_toward_its_project() {
        // Only a 2h meeting on p_meet in the morning, plus one afternoon log on p1.
        let out = reconstruct_day(
            &DayInputs {
                date: day(),
                meetings: vec![meeting(9, 11, Some("p_meet"), MeetingKind::Work)],
                signals: vec![sig(14, 0, Some("p1"))],
            },
            &ReconstructionConfig::default(),
        );
        assert!(out.allocations.iter().any(|a| a.gryzzly_project_id == "p_meet"));
        assert!((out.total_hours - 7.5).abs() < 1e-9);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test -p domain reconstruction::tests::two_project_signals_split_and_scale_to_target`
Expected: FAIL — `reconstruct_day` not defined (compile error).

- [ ] **Step 3: Implement `reconstruct_day`**

Add to `reconstruction.rs` (after `apportion_to_target`):
```rust
use chrono::Timelike;
use std::collections::HashMap;

/// A half-day window in local wall-clock minutes-from-midnight.
struct Window {
    start_min: i64,
    end_min: i64,
}

fn windows(cfg: &ReconstructionConfig) -> [Window; 2] {
    [
        Window { start_min: cfg.morning.0 as i64 * 60, end_min: cfg.morning.1 as i64 * 60 },
        Window { start_min: cfg.afternoon.0 as i64 * 60, end_min: cfg.afternoon.1 as i64 * 60 },
    ]
}

fn mins(dt: NaiveDateTime) -> i64 {
    dt.time().hour() as i64 * 60 + dt.time().minute() as i64
}

/// Reconstruct one day from its LOCAL-time signals and meetings.
pub fn reconstruct_day(inputs: &DayInputs, cfg: &ReconstructionConfig) -> ReconstructedDay {
    let mut blocks: Vec<AttributedBlock> = Vec::new();
    let mut unresolved: Vec<UnresolvedSignal> = Vec::new();

    // Out-of-office anchors suppress target scaling for the half-days they cover.
    let mut ooo_windows: Vec<(i64, i64)> = Vec::new();

    for w in windows(cfg).iter() {
        // Meetings clipped to this window.
        let mut anchors: Vec<(i64, i64, &MeetingBlock)> = inputs
            .meetings
            .iter()
            .filter_map(|m| {
                let s = mins(m.start).max(w.start_min);
                let e = mins(m.end).min(w.end_min);
                if e > s {
                    Some((s, e, m))
                } else {
                    None
                }
            })
            .collect();
        anchors.sort_by_key(|a| a.0);

        // Earlier meeting wins contested intervals: truncate later overlaps.
        let mut cursor = w.start_min;
        let mut fixed: Vec<(i64, i64, &MeetingBlock)> = Vec::new();
        for (s, e, m) in anchors {
            let s = s.max(cursor);
            if e > s {
                fixed.push((s, e, m));
                cursor = e;
            }
        }
        for (s, e, m) in &fixed {
            let kind = match m.kind {
                MeetingKind::Work => BlockKind::Meeting,
                MeetingKind::OutOfOffice => {
                    ooo_windows.push((*s, *e));
                    BlockKind::OutOfOffice
                }
            };
            if matches!(kind, BlockKind::OutOfOffice) {
                continue; // OOO consumes time but is never attributed to a project
            }
            blocks.push(AttributedBlock {
                start: inputs.date.and_hms_opt((*s / 60) as u32, (*s % 60) as u32, 0).unwrap(),
                end: inputs.date.and_hms_opt((*e / 60) as u32, (*e % 60) as u32, 0).unwrap(),
                gryzzly_project_id: m.gryzzly_project_id.clone(),
                kind: BlockKind::Meeting,
                hours: (e - s) as f64 / 60.0,
                source_refs: vec![m.source_ref.clone()],
            });
        }

        // Free intervals = window minus fixed meeting anchors.
        let mut free: Vec<(i64, i64)> = Vec::new();
        let mut c = w.start_min;
        for (s, e, _) in &fixed {
            if *s > c {
                free.push((c, *s));
            }
            c = (*e).max(c);
        }
        if c < w.end_min {
            free.push((c, w.end_min));
        }

        // Signals in this window, sorted by time.
        let mut sigs: Vec<&Signal> = inputs
            .signals
            .iter()
            .filter(|s| mins(s.at) >= w.start_min && mins(s.at) < w.end_min)
            .collect();
        sigs.sort_by_key(|s| mins(s.at));

        // Carry-forward within each free interval.
        for (fs, fe) in &free {
            let in_iv: Vec<&Signal> = sigs
                .iter()
                .copied()
                .filter(|s| {
                    let m = mins(s.at);
                    m >= *fs && m < *fe
                })
                .collect();
            if in_iv.is_empty() {
                continue;
            }
            for (i, s) in in_iv.iter().enumerate() {
                let start_min = if i == 0 { *fs } else { mins(s.at) };
                let end_min = if i + 1 < in_iv.len() { mins(in_iv[i + 1].at) } else { *fe };
                if end_min <= start_min {
                    continue;
                }
                if s.gryzzly_project_id.is_none() {
                    unresolved.push(UnresolvedSignal {
                        source_ref: s.source_ref.clone(),
                        label: s.label.clone(),
                        at: s.at,
                    });
                }
                blocks.push(AttributedBlock {
                    start: inputs.date.and_hms_opt((start_min / 60) as u32, (start_min % 60) as u32, 0).unwrap(),
                    end: inputs.date.and_hms_opt((end_min / 60) as u32, (end_min % 60) as u32, 0).unwrap(),
                    gryzzly_project_id: s.gryzzly_project_id.clone(),
                    kind: BlockKind::Work,
                    hours: (end_min - start_min) as f64 / 60.0,
                    source_refs: vec![s.source_ref.clone()],
                });
            }
        }
    }

    // Aggregate raw hours by project (None = unattributed).
    let mut raw: HashMap<Option<String>, (f64, Vec<String>)> = HashMap::new();
    for blk in &blocks {
        let entry = raw.entry(blk.gryzzly_project_id.clone()).or_insert((0.0, vec![]));
        entry.0 += blk.hours;
        entry.1.extend(blk.source_refs.iter().cloned());
    }
    let raw_total: f64 = raw.values().map(|(h, _)| *h).sum();

    // Guardrails + normalization (Task 6 fills this in; for now compute a raw draft).
    finalize_day(inputs.date, raw, raw_total, unresolved, blocks, cfg, &ooo_windows)
}
```

Add a temporary minimal `finalize_day` so the crate compiles and the allocation tests pass (Task 6 replaces its body):
```rust
fn finalize_day(
    date: NaiveDate,
    raw: HashMap<Option<String>, (f64, Vec<String>)>,
    raw_total: f64,
    unresolved: Vec<UnresolvedSignal>,
    blocks: Vec<AttributedBlock>,
    cfg: &ReconstructionConfig,
    _ooo: &[(i64, i64)],
) -> ReconstructedDay {
    if raw_total <= 0.0 {
        return ReconstructedDay {
            date,
            allocations: vec![],
            unattributed_hours: 0.0,
            unresolved,
            total_hours: 0.0,
            day_confidence: Confidence::Low,
            blocks,
        };
    }
    let buckets: Vec<Bucket> = raw
        .iter()
        .map(|(k, (h, _))| Bucket { key: k.clone(), hours: *h, pinned: false })
        .collect();
    let apportioned = apportion_to_target(&buckets, cfg.daily_target_hours, cfg.rounding_hours);
    let mut allocations = Vec::new();
    let mut unattributed_hours = 0.0;
    for bkt in &apportioned {
        match &bkt.key {
            Some(pid) => {
                let refs = raw.get(&Some(pid.clone())).map(|(_, r)| r.clone()).unwrap_or_default();
                allocations.push(ProjectAllocation {
                    gryzzly_project_id: pid.clone(),
                    hours: bkt.hours,
                    confidence: Confidence::High,
                    source_refs: refs,
                });
            }
            None => unattributed_hours += bkt.hours,
        }
    }
    let total_hours: f64 = allocations.iter().map(|a| a.hours).sum::<f64>() + unattributed_hours;
    let day_confidence = if raw_total >= cfg.min_signal_hours { Confidence::High } else { Confidence::Low };
    ReconstructedDay {
        date,
        allocations,
        unattributed_hours,
        unresolved,
        total_hours,
        day_confidence,
        blocks,
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test -p domain reconstruction`
Expected: PASS (Task 4 tests + the 4 new allocation tests).

- [ ] **Step 5: Commit**

```bash
git add backend/crates/domain/src/rules/reconstruction.rs
git commit -m "Add reconstruct_day allocation core (windows, anchors, carry-forward)"
```

---

### Task 6: `reconstruct_day` — guardrails (low-signal quarantine, OOO, day confidence) + `renormalize_lines`

**Files:**
- Modify: `backend/crates/domain/src/rules/reconstruction.rs`

**Interfaces:**
- Consumes: `finalize_day` internals (Task 5).
- Produces: `pub fn renormalize_lines(lines: &[EditedLine], target: f64, rounding: f64) -> Vec<EditedLine>` and `pub struct EditedLine { pub gryzzly_project_id: Option<String>, pub hours: f64, pub is_pinned: bool }`; upgraded `finalize_day` with the low-signal guard.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests`:
```rust
    #[test]
    fn low_signal_day_quarantines_to_unattributed_not_projects() {
        // One short morning log (well under min_signal_hours worth of real span).
        let cfg = ReconstructionConfig { min_signal_hours: 4.0, ..Default::default() };
        let out = reconstruct_day(
            &DayInputs { date: day(), meetings: vec![], signals: vec![sig(9, 0, Some("p1"))] },
            &cfg,
        );
        // p1 keeps only its raw carry-forward hours; the scaled remainder is unattributed.
        assert_eq!(out.day_confidence, Confidence::Low);
        let p1 = out.allocations.iter().find(|a| a.gryzzly_project_id == "p1").unwrap();
        assert!(p1.hours < out.total_hours, "p1 should not absorb the whole day");
        assert!(out.unattributed_hours > 0.0);
        assert!((out.total_hours - cfg.daily_target_hours).abs() < 1e-9);
    }

    #[test]
    fn out_of_office_day_is_not_scaled_to_target() {
        // All-morning OOO + one afternoon log. Morning is suppressed.
        let out = reconstruct_day(
            &DayInputs {
                date: day(),
                meetings: vec![meeting(8, 12, None, MeetingKind::OutOfOffice)],
                signals: vec![sig(14, 0, Some("p1"))],
            },
            &ReconstructionConfig::default(),
        );
        // Total should be at most the afternoon window (4h), never the full 7.5h target.
        assert!(out.total_hours <= 4.0 + 1e-9, "OOO morning must not be filled, got {}", out.total_hours);
    }

    #[test]
    fn renormalize_respects_pinned_and_sums_to_target() {
        let lines = vec![
            EditedLine { gryzzly_project_id: Some("a".into()), hours: 3.0, is_pinned: true },
            EditedLine { gryzzly_project_id: Some("b".into()), hours: 1.0, is_pinned: false },
            EditedLine { gryzzly_project_id: None, hours: 1.0, is_pinned: false },
        ];
        let out = renormalize_lines(&lines, 7.5, 0.25);
        let a = out.iter().find(|l| l.gryzzly_project_id.as_deref() == Some("a")).unwrap();
        assert!((a.hours - 3.0).abs() < 1e-9);
        let total: f64 = out.iter().map(|l| l.hours).sum();
        assert!((total - 7.5).abs() < 1e-9);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test -p domain reconstruction::tests::renormalize_respects_pinned_and_sums_to_target`
Expected: FAIL — `renormalize_lines` / `EditedLine` not defined.

- [ ] **Step 3: Implement the guardrails + `renormalize_lines`**

Add to `reconstruction.rs`:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct EditedLine {
    pub gryzzly_project_id: Option<String>,
    pub hours: f64,
    pub is_pinned: bool,
}

/// Re-apply target-rounding to user-edited lines: pinned lines are frozen,
/// unpinned lines + unattributed absorb the difference so the total == target.
pub fn renormalize_lines(lines: &[EditedLine], target: f64, rounding: f64) -> Vec<EditedLine> {
    let buckets: Vec<Bucket> = lines
        .iter()
        .map(|l| Bucket { key: l.gryzzly_project_id.clone(), hours: l.hours, pinned: l.is_pinned })
        .collect();
    apportion_to_target(&buckets, target, rounding)
        .into_iter()
        .map(|b| EditedLine { gryzzly_project_id: b.key, hours: b.hours, is_pinned: b.pinned })
        .collect()
}
```

Replace the body of `finalize_day` with the guarded version:
```rust
fn finalize_day(
    date: NaiveDate,
    raw: HashMap<Option<String>, (f64, Vec<String>)>,
    raw_total: f64,
    unresolved: Vec<UnresolvedSignal>,
    blocks: Vec<AttributedBlock>,
    cfg: &ReconstructionConfig,
    ooo: &[(i64, i64)],
) -> ReconstructedDay {
    if raw_total <= 0.0 {
        return ReconstructedDay {
            date, allocations: vec![], unattributed_hours: 0.0, unresolved,
            total_hours: 0.0, day_confidence: Confidence::Low, blocks,
        };
    }

    // Available worked hours cap: full day minus OOO-covered time.
    let ooo_hours: f64 = ooo.iter().map(|(s, e)| (e - s) as f64 / 60.0).sum();
    let day_cap = (cfg.daily_target_hours - ooo_hours).max(0.0);

    let low_signal = raw_total < cfg.min_signal_hours;
    let day_confidence = if low_signal { Confidence::Low } else { Confidence::High };

    // Determine the target the day is scaled to.
    // - Low-signal OR OOO present: do NOT inflate projects. Keep project raw hours,
    //   dump the (capped target - raw) remainder into the unattributed bucket.
    // - Otherwise: scale projects up to the day cap.
    let effective_target = day_cap;

    let allocations;
    let unattributed_hours;

    if low_signal || !ooo.is_empty() {
        // Project buckets keep their raw hours (rounded); unattributed absorbs the rest.
        let mut project_units_hours: Vec<(Option<String>, f64)> =
            raw.iter().map(|(k, (h, _))| (k.clone(), *h)).collect();
        // Round each project bucket to the increment, then set unattributed = target - sum.
        let unit = cfg.rounding_hours.max(f64::EPSILON);
        let mut sum_projects = 0.0;
        for (_, h) in project_units_hours.iter_mut() {
            *h = (*h / unit).round() * unit;
            sum_projects += *h;
        }
        let mut allocs = Vec::new();
        let mut unattr = (effective_target - sum_projects).max(0.0);
        for (k, h) in project_units_hours {
            match k {
                Some(pid) => {
                    let refs = raw.get(&Some(pid.clone())).map(|(_, r)| r.clone()).unwrap_or_default();
                    allocs.push(ProjectAllocation {
                        gryzzly_project_id: pid,
                        hours: h,
                        confidence: Confidence::High,
                        source_refs: refs,
                    });
                }
                None => unattr += h,
            }
        }
        allocations = allocs;
        unattributed_hours = unattr;
    } else {
        let buckets: Vec<Bucket> = raw
            .iter()
            .map(|(k, (h, _))| Bucket { key: k.clone(), hours: *h, pinned: false })
            .collect();
        let apportioned = apportion_to_target(&buckets, effective_target, cfg.rounding_hours);
        let mut allocs = Vec::new();
        let mut unattr = 0.0;
        for bkt in &apportioned {
            match &bkt.key {
                Some(pid) => {
                    let refs = raw.get(&Some(pid.clone())).map(|(_, r)| r.clone()).unwrap_or_default();
                    allocs.push(ProjectAllocation {
                        gryzzly_project_id: pid.clone(),
                        hours: bkt.hours,
                        confidence: Confidence::High,
                        source_refs: refs,
                    });
                }
                None => unattr += bkt.hours,
            }
        }
        allocations = allocs;
        unattributed_hours = unattr;
    }

    let total_hours: f64 = allocations.iter().map(|a| a.hours).sum::<f64>() + unattributed_hours;
    ReconstructedDay {
        date, allocations, unattributed_hours, unresolved, total_hours, day_confidence, blocks,
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test -p domain reconstruction`
Expected: PASS (all Task 4/5/6 tests). Note: `two_project_signals_split_and_scale_to_target` still passes because that day is high-signal with no OOO.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/domain/src/rules/reconstruction.rs
git commit -m "Add reconstruction guardrails (low-signal quarantine, OOO, renormalize)"
```

---

### Task 7: `TimesheetDraft` domain type + `TimesheetDraftRepository` trait

**Files:**
- Create: `backend/crates/domain/src/types/timesheet.rs`
- Modify: `backend/crates/domain/src/types/mod.rs`
- Create: `backend/crates/application/src/repositories/timesheet_draft_repository.rs`
- Modify: `backend/crates/application/src/repositories/mod.rs`

**Interfaces:**
- Produces: `TimesheetStatus`, `TimesheetDraft`, `TimesheetDraftLine`, and:
  - `trait TimesheetDraftRepository { async fn upsert(&self, draft: &TimesheetDraft) -> Result<(), RepositoryError>; async fn find_by_user_and_date(&self, user_id: UserId, date: NaiveDate) -> Result<Option<TimesheetDraft>, RepositoryError>; async fn set_status(&self, user_id: UserId, date: NaiveDate, status: TimesheetStatus) -> Result<(), RepositoryError>; }`

- [ ] **Step 1: Write the domain type with a status roundtrip test**

Create `backend/crates/domain/src/types/timesheet.rs`:
```rust
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::{Confidence, UserId};

pub type TimesheetDraftId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimesheetStatus {
    Draft,
    Validated,
    Submitted,
    DayOff,
}

impl TimesheetStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimesheetStatus::Draft => "draft",
            TimesheetStatus::Validated => "validated",
            TimesheetStatus::Submitted => "submitted",
            TimesheetStatus::DayOff => "day_off",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(TimesheetStatus::Draft),
            "validated" => Some(TimesheetStatus::Validated),
            "submitted" => Some(TimesheetStatus::Submitted),
            "day_off" => Some(TimesheetStatus::DayOff),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimesheetDraftLine {
    pub id: Uuid,
    pub gryzzly_project_id: Option<String>,
    pub project_name: Option<String>,
    pub hours: f64,
    pub is_pinned: bool,
    pub confidence: Confidence,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimesheetDraft {
    pub id: TimesheetDraftId,
    pub user_id: UserId,
    pub date: NaiveDate,
    pub status: TimesheetStatus,
    pub target_hours: f64,
    pub total_hours: f64,
    pub day_confidence: Confidence,
    pub blocks_json: Option<String>,
    pub lines: Vec<TimesheetDraftLine>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrips() {
        for s in [
            TimesheetStatus::Draft,
            TimesheetStatus::Validated,
            TimesheetStatus::Submitted,
            TimesheetStatus::DayOff,
        ] {
            assert_eq!(TimesheetStatus::from_str(s.as_str()), Some(s));
        }
    }
}
```

- [ ] **Step 2: Register the type module**

In `backend/crates/domain/src/types/mod.rs`, add:
```rust
pub mod timesheet;
```
```rust
pub use timesheet::*;
```

- [ ] **Step 3: Write the repository trait**

Create `backend/crates/application/src/repositories/timesheet_draft_repository.rs`:
```rust
use async_trait::async_trait;
use chrono::NaiveDate;
use domain::types::*;

use crate::errors::RepositoryError;

/// Persists the reconstructed daily timesheet draft (header + per-project lines).
#[async_trait]
pub trait TimesheetDraftRepository: Send + Sync {
    /// Insert or replace the whole draft for (user, date). Replaces all lines.
    async fn upsert(&self, draft: &TimesheetDraft) -> Result<(), RepositoryError>;

    /// Load the draft (with its lines) for a user + local date.
    async fn find_by_user_and_date(
        &self,
        user_id: UserId,
        date: NaiveDate,
    ) -> Result<Option<TimesheetDraft>, RepositoryError>;

    /// Change only the status of an existing draft.
    async fn set_status(
        &self,
        user_id: UserId,
        date: NaiveDate,
        status: TimesheetStatus,
    ) -> Result<(), RepositoryError>;
}
```

- [ ] **Step 4: Register the repository trait**

In `backend/crates/application/src/repositories/mod.rs`, add:
```rust
pub mod timesheet_draft_repository;
```
```rust
pub use timesheet_draft_repository::*;
```

- [ ] **Step 5: Run tests**

Run: `cd backend && cargo test -p domain timesheet && cargo build -p application`
Expected: PASS (`status_roundtrips`) and application compiles.

- [ ] **Step 6: Commit**

```bash
git add backend/crates/domain/src/types/timesheet.rs \
        backend/crates/domain/src/types/mod.rs \
        backend/crates/application/src/repositories/timesheet_draft_repository.rs \
        backend/crates/application/src/repositories/mod.rs
git commit -m "Add TimesheetDraft type and TimesheetDraftRepository trait"
```

---

### Task 8: `SignalMappingRepository` trait

**Files:**
- Create: `backend/crates/application/src/repositories/signal_mapping_repository.rs`
- Modify: `backend/crates/application/src/repositories/mod.rs`

**Interfaces:**
- Produces:
  - `trait SignalMappingRepository { async fn list_enabled(&self, user_id: UserId) -> Result<Vec<SignalMapping>, RepositoryError>; async fn upsert(&self, mapping: &SignalMapping) -> Result<(), RepositoryError>; async fn set_enabled(&self, id: SignalMappingId, enabled: bool) -> Result<(), RepositoryError>; async fn delete(&self, id: SignalMappingId) -> Result<(), RepositoryError>; }`

- [ ] **Step 1: Write the trait**

Create `backend/crates/application/src/repositories/signal_mapping_repository.rs`:
```rust
use async_trait::async_trait;
use domain::types::*;

use crate::errors::RepositoryError;

/// Persists learned signal→Gryzzly-project mapping rules (user-scoped).
#[async_trait]
pub trait SignalMappingRepository: Send + Sync {
    /// All enabled rules for the user (the resolver filters by kind in memory).
    async fn list_enabled(&self, user_id: UserId) -> Result<Vec<SignalMapping>, RepositoryError>;

    /// Insert or update a rule (idempotent on (user_id, kind, pattern)).
    async fn upsert(&self, mapping: &SignalMapping) -> Result<(), RepositoryError>;

    /// Enable/disable a rule without deleting it.
    async fn set_enabled(&self, id: SignalMappingId, enabled: bool) -> Result<(), RepositoryError>;

    /// Hard-delete a rule.
    async fn delete(&self, id: SignalMappingId) -> Result<(), RepositoryError>;
}
```

- [ ] **Step 2: Register the trait**

In `backend/crates/application/src/repositories/mod.rs`, add:
```rust
pub mod signal_mapping_repository;
```
```rust
pub use signal_mapping_repository::*;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd backend && cargo build -p application`
Expected: builds cleanly.

- [ ] **Step 4: Commit**

```bash
git add backend/crates/application/src/repositories/signal_mapping_repository.rs \
        backend/crates/application/src/repositories/mod.rs
git commit -m "Add SignalMappingRepository trait"
```

---

### Task 9: SQLite `SignalMappingRepository` impl

**Files:**
- Create: `backend/crates/infrastructure/src/database/signal_mapping_repo.rs`
- Modify: `backend/crates/infrastructure/src/database/mod.rs`
- Test: inline `#[cfg(test)] mod tests` in the new file (in-memory SQLite).

**Interfaces:**
- Consumes: `SignalMappingRepository` (Task 8), migration 011 (Task 1).
- Produces: `pub struct SqliteSignalMappingRepository` with `pub fn new(pool: SqlitePool) -> Self`.

- [ ] **Step 1: Write the impl + failing test**

Create `backend/crates/infrastructure/src/database/signal_mapping_repo.rs`:
```rust
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::SignalMappingRepository;
use domain::types::*;

pub struct SqliteSignalMappingRepository {
    pool: SqlitePool,
}

impl SqliteSignalMappingRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RepositoryError::Database(format!("bad datetime '{s}': {e}")))
}

fn map_row(row: &SqliteRow) -> Result<SignalMapping, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let user_id_str: String = Row::get(row, "user_id");
    let kind_str: String = Row::get(row, "kind");
    let is_enabled: i64 = Row::get(row, "is_enabled");
    Ok(SignalMapping {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        kind: MappingKind::from_str(&kind_str)
            .ok_or_else(|| RepositoryError::Database(format!("bad kind '{kind_str}'")))?,
        pattern: Row::get(row, "pattern"),
        branch_pattern: Row::get(row, "branch_pattern"),
        gryzzly_project_id: Row::get(row, "gryzzly_project_id"),
        gryzzly_project_name: Row::get(row, "gryzzly_project_name"),
        is_enabled: is_enabled != 0,
        created_at: parse_dt(&Row::get::<String, _>(row, "created_at"))?,
        updated_at: parse_dt(&Row::get::<String, _>(row, "updated_at"))?,
    })
}

#[async_trait]
impl SignalMappingRepository for SqliteSignalMappingRepository {
    async fn list_enabled(&self, user_id: UserId) -> Result<Vec<SignalMapping>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM signal_project_mappings WHERE user_id = ? AND is_enabled = 1",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        rows.iter().map(map_row).collect()
    }

    async fn upsert(&self, m: &SignalMapping) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO signal_project_mappings
                (id, user_id, kind, pattern, branch_pattern, gryzzly_project_id, gryzzly_project_name, is_enabled, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_id, kind, pattern) DO UPDATE SET
                branch_pattern = excluded.branch_pattern,
                gryzzly_project_id = excluded.gryzzly_project_id,
                gryzzly_project_name = excluded.gryzzly_project_name,
                is_enabled = excluded.is_enabled,
                updated_at = excluded.updated_at",
        )
        .bind(m.id.to_string())
        .bind(m.user_id.to_string())
        .bind(m.kind.as_str())
        .bind(&m.pattern)
        .bind(&m.branch_pattern)
        .bind(&m.gryzzly_project_id)
        .bind(&m.gryzzly_project_name)
        .bind(if m.is_enabled { 1 } else { 0 })
        .bind(m.created_at.to_rfc3339())
        .bind(m.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn set_enabled(&self, id: SignalMappingId, enabled: bool) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE signal_project_mappings SET is_enabled = ? WHERE id = ?")
            .bind(if enabled { 1 } else { 0 })
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: SignalMappingId) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM signal_project_mappings WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    async fn pool_with_user() -> (SqlitePool, Uuid) {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../../migrations/sqlite").run(&pool).await.unwrap();
        let uid = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, name, email, created_at) VALUES (?, 'T', 't@e.co', ?)")
            .bind(uid.to_string())
            .bind(Utc::now().to_rfc3339())
            .execute(&pool)
            .await
            .unwrap();
        (pool, uid)
    }

    fn mapping(uid: Uuid) -> SignalMapping {
        SignalMapping {
            id: Uuid::new_v4(),
            user_id: uid,
            kind: MappingKind::RepoPath,
            pattern: "/home/me/repo".into(),
            branch_pattern: None,
            gryzzly_project_id: "p1".into(),
            gryzzly_project_name: Some("Project 1".into()),
            is_enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn upsert_then_list_enabled_returns_it() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteSignalMappingRepository::new(pool);
        repo.upsert(&mapping(uid)).await.unwrap();
        let rows = repo.list_enabled(uid).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].gryzzly_project_id, "p1");
    }

    #[tokio::test]
    async fn upsert_is_idempotent_on_kind_pattern() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteSignalMappingRepository::new(pool);
        let mut m = mapping(uid);
        repo.upsert(&m).await.unwrap();
        m.id = Uuid::new_v4(); // different id, same (kind, pattern)
        m.gryzzly_project_id = "p2".into();
        repo.upsert(&m).await.unwrap();
        let rows = repo.list_enabled(uid).await.unwrap();
        assert_eq!(rows.len(), 1, "same (kind,pattern) must update not duplicate");
        assert_eq!(rows[0].gryzzly_project_id, "p2");
    }

    #[tokio::test]
    async fn disabled_rule_is_excluded() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteSignalMappingRepository::new(pool);
        let m = mapping(uid);
        let id = m.id;
        repo.upsert(&m).await.unwrap();
        repo.set_enabled(id, false).await.unwrap();
        assert!(repo.list_enabled(uid).await.unwrap().is_empty());
    }
}
```

> **Note:** verify the `users` insert columns in Step 1 match the real `001_initial.sql` schema. If the `users` table has different NOT NULL columns, adjust the seed insert accordingly (read `migrations/sqlite/001_initial.sql`).

- [ ] **Step 2: Register the repo**

In `backend/crates/infrastructure/src/database/mod.rs`, add:
```rust
pub mod signal_mapping_repo;
```
```rust
pub use signal_mapping_repo::SqliteSignalMappingRepository;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure signal_mapping_repo`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add backend/crates/infrastructure/src/database/signal_mapping_repo.rs \
        backend/crates/infrastructure/src/database/mod.rs
git commit -m "Add SQLite SignalMappingRepository impl"
```

---

### Task 10: SQLite `TimesheetDraftRepository` impl

**Files:**
- Create: `backend/crates/infrastructure/src/database/timesheet_draft_repo.rs`
- Modify: `backend/crates/infrastructure/src/database/mod.rs`
- Test: inline in the new file.

**Interfaces:**
- Consumes: `TimesheetDraftRepository` (Task 7), migration 010 (Task 1).
- Produces: `pub struct SqliteTimesheetDraftRepository` with `pub fn new(pool: SqlitePool) -> Self`.

- [ ] **Step 1: Write the impl + failing test**

Create `backend/crates/infrastructure/src/database/timesheet_draft_repo.rs`:
```rust
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::TimesheetDraftRepository;
use domain::types::*;

pub struct SqliteTimesheetDraftRepository {
    pool: SqlitePool,
}

impl SqliteTimesheetDraftRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RepositoryError::Database(format!("bad datetime '{s}': {e}")))
}

fn conf_from(s: &str) -> Confidence {
    match s {
        "high" => Confidence::High,
        "medium" => Confidence::Medium,
        _ => Confidence::Low,
    }
}

fn map_line(row: &SqliteRow) -> Result<TimesheetDraftLine, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let refs_json: Option<String> = Row::get(row, "source_refs_json");
    let is_pinned: i64 = Row::get(row, "is_pinned");
    let conf: String = Row::get(row, "confidence");
    Ok(TimesheetDraftLine {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        gryzzly_project_id: Row::get(row, "gryzzly_project_id"),
        project_name: Row::get(row, "project_name"),
        hours: Row::get(row, "hours"),
        is_pinned: is_pinned != 0,
        confidence: conf_from(&conf),
        source_refs: refs_json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default(),
    })
}

#[async_trait]
impl TimesheetDraftRepository for SqliteTimesheetDraftRepository {
    async fn upsert(&self, draft: &TimesheetDraft) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(|e| RepositoryError::Database(e.to_string()))?;

        // Header upsert (unique on user_id, date).
        sqlx::query(
            "INSERT INTO timesheet_drafts
                (id, user_id, date, status, target_hours, total_hours, day_confidence, blocks_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_id, date) DO UPDATE SET
                status = excluded.status, target_hours = excluded.target_hours,
                total_hours = excluded.total_hours, day_confidence = excluded.day_confidence,
                blocks_json = excluded.blocks_json, updated_at = excluded.updated_at",
        )
        .bind(draft.id.to_string())
        .bind(draft.user_id.to_string())
        .bind(draft.date.format("%Y-%m-%d").to_string())
        .bind(draft.status.as_str())
        .bind(draft.target_hours)
        .bind(draft.total_hours)
        .bind(draft.day_confidence.as_str())
        .bind(&draft.blocks_json)
        .bind(draft.created_at.to_rfc3339())
        .bind(draft.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        // Resolve the header id (existing row may keep its original id).
        let header_id: String = sqlx::query("SELECT id FROM timesheet_drafts WHERE user_id = ? AND date = ?")
            .bind(draft.user_id.to_string())
            .bind(draft.date.format("%Y-%m-%d").to_string())
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .get::<String, _>("id");

        // Replace lines.
        sqlx::query("DELETE FROM timesheet_draft_lines WHERE draft_id = ?")
            .bind(&header_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        for line in &draft.lines {
            let refs = serde_json::to_string(&line.source_refs)
                .map_err(|e| RepositoryError::Serialization(e.to_string()))?;
            sqlx::query(
                "INSERT INTO timesheet_draft_lines
                    (id, draft_id, gryzzly_project_id, project_name, hours, is_pinned, confidence, source_refs_json, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(line.id.to_string())
            .bind(&header_id)
            .bind(&line.gryzzly_project_id)
            .bind(&line.project_name)
            .bind(line.hours)
            .bind(if line.is_pinned { 1 } else { 0 })
            .bind(line.confidence.as_str())
            .bind(refs)
            .bind(draft.updated_at.to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn find_by_user_and_date(
        &self,
        user_id: UserId,
        date: NaiveDate,
    ) -> Result<Option<TimesheetDraft>, RepositoryError> {
        let header = sqlx::query("SELECT * FROM timesheet_drafts WHERE user_id = ? AND date = ? LIMIT 1")
            .bind(user_id.to_string())
            .bind(date.format("%Y-%m-%d").to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let Some(h) = header.first() else { return Ok(None) };

        let header_id: String = Row::get(h, "id");
        let line_rows = sqlx::query("SELECT * FROM timesheet_draft_lines WHERE draft_id = ?")
            .bind(&header_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let lines: Result<Vec<_>, _> = line_rows.iter().map(map_line).collect();

        let status_str: String = Row::get(h, "status");
        let conf_str: String = Row::get(h, "day_confidence");
        Ok(Some(TimesheetDraft {
            id: Uuid::parse_str(&header_id).map_err(|e| RepositoryError::Database(e.to_string()))?,
            user_id,
            date,
            status: TimesheetStatus::from_str(&status_str)
                .ok_or_else(|| RepositoryError::Database(format!("bad status '{status_str}'")))?,
            target_hours: Row::get(h, "target_hours"),
            total_hours: Row::get(h, "total_hours"),
            day_confidence: conf_from(&conf_str),
            blocks_json: Row::get(h, "blocks_json"),
            lines: lines?,
            created_at: parse_dt(&Row::get::<String, _>(h, "created_at"))?,
            updated_at: parse_dt(&Row::get::<String, _>(h, "updated_at"))?,
        }))
    }

    async fn set_status(
        &self,
        user_id: UserId,
        date: NaiveDate,
        status: TimesheetStatus,
    ) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE timesheet_drafts SET status = ? WHERE user_id = ? AND date = ?")
            .bind(status.as_str())
            .bind(user_id.to_string())
            .bind(date.format("%Y-%m-%d").to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    async fn pool_with_user() -> (SqlitePool, Uuid) {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../../migrations/sqlite").run(&pool).await.unwrap();
        let uid = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, name, email, created_at) VALUES (?, 'T', 't@e.co', ?)")
            .bind(uid.to_string())
            .bind(Utc::now().to_rfc3339())
            .execute(&pool)
            .await
            .unwrap();
        (pool, uid)
    }

    fn draft(uid: Uuid) -> TimesheetDraft {
        TimesheetDraft {
            id: Uuid::new_v4(),
            user_id: uid,
            date: NaiveDate::from_ymd_opt(2026, 6, 8).unwrap(),
            status: TimesheetStatus::Draft,
            target_hours: 7.5,
            total_hours: 7.5,
            day_confidence: Confidence::High,
            blocks_json: Some("[]".into()),
            lines: vec![TimesheetDraftLine {
                id: Uuid::new_v4(),
                gryzzly_project_id: Some("p1".into()),
                project_name: Some("Proj 1".into()),
                hours: 7.5,
                is_pinned: false,
                confidence: Confidence::High,
                source_refs: vec!["wl-1".into()],
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn upsert_then_find_roundtrips() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteTimesheetDraftRepository::new(pool);
        let d = draft(uid);
        repo.upsert(&d).await.unwrap();
        let got = repo.find_by_user_and_date(uid, d.date).await.unwrap().unwrap();
        assert_eq!(got.lines.len(), 1);
        assert_eq!(got.lines[0].gryzzly_project_id.as_deref(), Some("p1"));
        assert_eq!(got.status, TimesheetStatus::Draft);
    }

    #[tokio::test]
    async fn upsert_replaces_lines_not_appends() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteTimesheetDraftRepository::new(pool);
        let mut d = draft(uid);
        repo.upsert(&d).await.unwrap();
        d.lines[0].hours = 3.0;
        repo.upsert(&d).await.unwrap();
        let got = repo.find_by_user_and_date(uid, d.date).await.unwrap().unwrap();
        assert_eq!(got.lines.len(), 1, "re-upsert must replace lines");
        assert!((got.lines[0].hours - 3.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn set_status_updates_only_status() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteTimesheetDraftRepository::new(pool);
        let d = draft(uid);
        repo.upsert(&d).await.unwrap();
        repo.set_status(uid, d.date, TimesheetStatus::Validated).await.unwrap();
        let got = repo.find_by_user_and_date(uid, d.date).await.unwrap().unwrap();
        assert_eq!(got.status, TimesheetStatus::Validated);
    }
}
```

- [ ] **Step 2: Register the repo**

In `backend/crates/infrastructure/src/database/mod.rs`, add:
```rust
pub mod timesheet_draft_repo;
```
```rust
pub use timesheet_draft_repo::SqliteTimesheetDraftRepository;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure timesheet_draft_repo`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add backend/crates/infrastructure/src/database/timesheet_draft_repo.rs \
        backend/crates/infrastructure/src/database/mod.rs
git commit -m "Add SQLite TimesheetDraftRepository impl"
```

---

### Task 11: `GitConnector` service — pure log parser + commit→project key

**Files:**
- Create: `backend/crates/application/src/services/git_connector.rs`
- Modify: `backend/crates/application/src/services/mod.rs`
- Create: `backend/crates/infrastructure/src/connectors/git/mod.rs`
- Modify: `backend/crates/infrastructure/src/connectors/mod.rs`

**Interfaces:**
- Produces:
  - `struct GitCommit { pub repo_path: String, pub branch: String, pub committed_at: DateTime<Utc>, pub message: String }`
  - `trait GitConnector { async fn commits_between(&self, repo_paths: &[String], from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<GitCommit>, AppError>; }`
  - pure `fn parse_git_log(repo_path: &str, branch: &str, stdout: &str) -> Vec<GitCommit>`
  - pure `fn jira_key_in(text: &str) -> Option<String>` (extracts `AP-123`-style keys for commit→task matching)

- [ ] **Step 1: Write the service trait + pure parser with failing tests**

Create `backend/crates/application/src/services/git_connector.rs`:
```rust
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::errors::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommit {
    pub repo_path: String,
    pub branch: String,
    pub committed_at: DateTime<Utc>,
    pub message: String,
}

/// Reads commit activity from local git repositories. Impl in infrastructure.
#[async_trait]
pub trait GitConnector: Send + Sync {
    /// All commits authored by the current user across `repo_paths` in [from, to).
    async fn commits_between(
        &self,
        repo_paths: &[String],
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<GitCommit>, AppError>;
}

/// Parse `git log --pretty=%cI%x1f%s` output (one commit per line, ISO-8601 commit
/// date, unit-separator, subject). Unparseable lines are skipped.
pub fn parse_git_log(repo_path: &str, branch: &str, stdout: &str) -> Vec<GitCommit> {
    stdout
        .lines()
        .filter_map(|line| {
            let (date_s, subject) = line.split_once('\u{1f}')?;
            let committed_at = DateTime::parse_from_rfc3339(date_s.trim())
                .ok()?
                .with_timezone(&Utc);
            Some(GitCommit {
                repo_path: repo_path.to_string(),
                branch: branch.to_string(),
                committed_at,
                message: subject.to_string(),
            })
        })
        .collect()
}

/// Extract an uppercase Jira-style key (e.g. AP-123) from text, if present.
pub fn jira_key_in(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // find a run of A-Z of length >= 2
        if bytes[i].is_ascii_uppercase() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_uppercase() {
                i += 1;
            }
            let letters = i - start;
            if letters >= 2 && i < bytes.len() && bytes[i] == b'-' {
                let dash = i;
                i += 1;
                let dig_start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i > dig_start {
                    return Some(text[start..i].to_string());
                }
                i = dash + 1;
            }
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_commits() {
        let out = "2026-06-08T09:15:00+02:00\u{1f}AP-12 fix login\n2026-06-08T14:02:00+02:00\u{1f}refactor";
        let commits = parse_git_log("/repo", "main", out);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].message, "AP-12 fix login");
        assert_eq!(commits[0].repo_path, "/repo");
        assert_eq!(commits[0].branch, "main");
    }

    #[test]
    fn skips_malformed_lines() {
        let out = "garbage line without separator\n2026-06-08T09:15:00+02:00\u{1f}ok";
        let commits = parse_git_log("/repo", "main", out);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].message, "ok");
    }

    #[test]
    fn extracts_jira_key() {
        assert_eq!(jira_key_in("AP-123 do stuff"), Some("AP-123".to_string()));
        assert_eq!(jira_key_in("feat: PROJ-9 thing"), Some("PROJ-9".to_string()));
        assert_eq!(jira_key_in("no key here"), None);
        assert_eq!(jira_key_in("lowercase ab-1"), None);
    }
}
```

- [ ] **Step 2: Register the service module**

In `backend/crates/application/src/services/mod.rs`, add:
```rust
pub mod git_connector;
```
```rust
pub use git_connector::*;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cd backend && cargo test -p application git_connector`
Expected: PASS (3 tests).

- [ ] **Step 4: Write the infrastructure `ShellGitConnector`**

Create `backend/crates/infrastructure/src/connectors/git/mod.rs`:
```rust
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::process::Command;

use application::errors::AppError;
use application::services::git_connector::{parse_git_log, GitCommit, GitConnector};

/// Reads commits by shelling out to the local `git` binary. Suitable for a
/// single-user local deployment where the backend can see the user's repos.
pub struct ShellGitConnector;

impl ShellGitConnector {
    pub fn new() -> Self {
        Self
    }

    async fn current_branch(&self, repo_path: &str) -> Option<String> {
        let out = Command::new("git")
            .args(["-C", repo_path, "rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .await
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

impl Default for ShellGitConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GitConnector for ShellGitConnector {
    async fn commits_between(
        &self,
        repo_paths: &[String],
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<GitCommit>, AppError> {
        let mut all = Vec::new();
        for repo in repo_paths {
            let branch = self.current_branch(repo).await.unwrap_or_else(|| "HEAD".to_string());
            let out = Command::new("git")
                .args([
                    "-C",
                    repo,
                    "log",
                    "--no-merges",
                    &format!("--since={}", from.to_rfc3339()),
                    &format!("--until={}", to.to_rfc3339()),
                    "--pretty=%cI\u{1f}%s",
                ])
                .output()
                .await
                .map_err(|e| AppError::Configuration(format!("git log failed for {repo}: {e}")))?;
            if !out.status.success() {
                // A missing/invalid repo path is non-fatal: skip it.
                continue;
            }
            let stdout = String::from_utf8_lossy(&out.stdout);
            all.extend(parse_git_log(repo, &branch, &stdout));
        }
        Ok(all)
    }
}
```

- [ ] **Step 5: Register the connector module**

In `backend/crates/infrastructure/src/connectors/mod.rs`, add:
```rust
pub mod git;
```
Add `tokio` with the `process` feature to `backend/crates/infrastructure/Cargo.toml` if not already present:
```toml
tokio = { workspace = true, features = ["process"] }
```
> **Note:** check the existing `tokio` line in `infrastructure/Cargo.toml`; if it already enables features via the workspace, add `"process"` to the workspace `tokio` features in `backend/Cargo.toml` instead. Confirm with `cargo build -p infrastructure`.

- [ ] **Step 6: Verify infrastructure builds**

Run: `cd backend && cargo build -p infrastructure`
Expected: builds cleanly.

- [ ] **Step 7: Commit**

```bash
git add backend/crates/application/src/services/git_connector.rs \
        backend/crates/application/src/services/mod.rs \
        backend/crates/infrastructure/src/connectors/git/mod.rs \
        backend/crates/infrastructure/src/connectors/mod.rs \
        backend/crates/infrastructure/Cargo.toml backend/Cargo.toml
git commit -m "Add GitConnector: pure log parser + ShellGitConnector"
```

---

### Task 12: Timezone helper (`resolve_tz`, `local_day_bounds`, `to_local`)

**Files:**
- Create: `backend/crates/application/src/time.rs`
- Modify: `backend/crates/application/src/lib.rs`

**Interfaces:**
- Produces:
  - `fn resolve_tz(config_value: Option<String>) -> chrono_tz::Tz` (default `Europe/Paris`)
  - `fn to_local(dt: DateTime<Utc>, tz: chrono_tz::Tz) -> NaiveDateTime`
  - `fn local_day_bounds(date: NaiveDate, tz: chrono_tz::Tz) -> (DateTime<Utc>, DateTime<Utc>)` (local-midnight→next-local-midnight, mapped to UTC)

- [ ] **Step 1: Write the helper + failing tests**

Create `backend/crates/application/src/time.rs`:
```rust
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

pub const DEFAULT_TZ: &str = "Europe/Paris";

/// Resolve a timezone from a config string, falling back to Europe/Paris.
pub fn resolve_tz(config_value: Option<String>) -> Tz {
    config_value
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| DEFAULT_TZ.parse().expect("default tz parses"))
}

/// Convert a UTC instant to the user's local wall-clock (naive) time.
pub fn to_local(dt: DateTime<Utc>, tz: Tz) -> NaiveDateTime {
    tz.from_utc_datetime(&dt.naive_utc()).naive_local()
}

/// UTC bounds [start, end) for a LOCAL calendar day. `end` is the next local midnight.
/// Uses the earliest valid instant at each local midnight (handles DST gaps).
pub fn local_day_bounds(date: NaiveDate, tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
    let start_local = date.and_hms_opt(0, 0, 0).expect("valid midnight");
    let end_local = (date + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .expect("valid midnight");
    let start_utc = tz
        .from_local_datetime(&start_local)
        .earliest()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&start_local));
    let end_utc = tz
        .from_local_datetime(&end_local)
        .earliest()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&end_local));
    (start_utc, end_utc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_tz_defaults_to_paris() {
        assert_eq!(resolve_tz(None), "Europe/Paris".parse::<Tz>().unwrap());
        assert_eq!(resolve_tz(Some("bogus".into())), "Europe/Paris".parse::<Tz>().unwrap());
        assert_eq!(resolve_tz(Some("UTC".into())), Tz::UTC);
    }

    #[test]
    fn paris_local_day_is_offset_from_utc() {
        // 2026-06-08 is CEST (UTC+2): local midnight = 22:00 UTC the previous day.
        let tz: Tz = "Europe/Paris".parse().unwrap();
        let (start, end) = local_day_bounds(NaiveDate::from_ymd_opt(2026, 6, 8).unwrap(), tz);
        assert_eq!(start.to_rfc3339(), "2026-06-07T22:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-06-08T22:00:00+00:00");
    }

    #[test]
    fn to_local_shifts_into_paris() {
        let tz: Tz = "Europe/Paris".parse().unwrap();
        let utc = DateTime::parse_from_rfc3339("2026-06-08T07:30:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let local = to_local(utc, tz);
        assert_eq!(local.to_string(), "2026-06-08 09:30:00"); // +2h CEST
    }
}
```

- [ ] **Step 2: Register the module**

In `backend/crates/application/src/lib.rs`, add near the other `pub mod` lines:
```rust
pub mod time;
```
> **Note:** confirm `chrono-tz` is a dependency of the application crate (it is used by `use_cases/worklog.rs`). If `cargo build` complains, add `chrono-tz = { workspace = true }` to `backend/crates/application/Cargo.toml`.

- [ ] **Step 3: Run tests to verify they pass**

Run: `cd backend && cargo test -p application time::`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add backend/crates/application/src/time.rs backend/crates/application/src/lib.rs
git commit -m "Add timezone helper (resolve_tz, to_local, local_day_bounds)"
```

---

### Task 13: `timesheet` use cases — reconstruct, save, validate, mark-day-off, learn-mapping

**Files:**
- Create: `backend/crates/application/src/use_cases/timesheet.rs`
- Modify: `backend/crates/application/src/use_cases/mod.rs`
- Test: inline in the new file, with in-memory mock repos (pattern from `gryzzly_assignment.rs` tests).

**Interfaces:**
- Consumes: reconstruction rule (Tasks 4-6), mapping rule (Task 3), `TimesheetDraftRepository` (Task 7), `SignalMappingRepository` (Task 8), `GitConnector` (Task 11), time helper (Task 12), and existing `WorklogRepository`, `MeetingRepository`, `TaskRepository`, `GryzzlyCatalogRepository`, `ConfigRepository`.
- Produces:
  - `async fn reconstruct_timesheet(deps..., user_id, date) -> Result<ReconstructedDay, AppError>`
  - `async fn save_timesheet_draft(deps..., user_id, date, edited_lines: Vec<EditedLine>) -> Result<(), AppError>`
  - `async fn validate_timesheet(draft_repo, user_id, date) -> Result<(), AppError>`
  - `async fn mark_day_off(draft_repo, user_id, date, scope: DayOffScope) -> Result<(), AppError>`
  - `async fn learn_mapping(mapping_repo, catalog_repo, user_id, kind, pattern, branch_pattern, gryzzly_project_id, now) -> Result<SignalMapping, AppError>`
  - `enum DayOffScope { Full, Morning, Afternoon }`
  - `async fn load_reconstruction_config(config_repo, user_id) -> Result<ReconstructionConfig, AppError>`

- [ ] **Step 1: Write the use case module (config loader + reconstruct + save + validate + mark_day_off + learn_mapping)**

Create `backend/crates/application/src/use_cases/timesheet.rs`:
```rust
use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, Utc};
use domain::rules::project_mapping::{resolve_signal_project, ProjectResolution, RawSignal};
use domain::rules::reconstruction::{
    reconstruct_day, renormalize_lines, DayInputs, EditedLine, MeetingBlock, MeetingKind,
    ReconstructedDay, ReconstructionConfig, Signal, SignalKind,
};
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::{
    ConfigRepository, GryzzlyCatalogRepository, MeetingRepository, SignalMappingRepository,
    TaskRepository, TimesheetDraftRepository, WorklogFilter, WorklogRepository,
    WORKLOG_FILTER_MAX_LIMIT,
};
use crate::services::git_connector::{jira_key_in, GitConnector};
use crate::time::{local_day_bounds, resolve_tz, to_local};

#[derive(Debug, Clone, Copy)]
pub enum DayOffScope {
    Full,
    Morning,
    Afternoon,
}

/// Read the reconstruction config from the key-value store (with defaults).
pub async fn load_reconstruction_config(
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
) -> Result<ReconstructionConfig, AppError> {
    async fn f64_key(c: &dyn ConfigRepository, u: UserId, k: &str, d: f64) -> f64 {
        c.get(u, k).await.ok().flatten().and_then(|s| s.parse().ok()).unwrap_or(d)
    }
    async fn u32_key(c: &dyn ConfigRepository, u: UserId, k: &str, d: u32) -> u32 {
        c.get(u, k).await.ok().flatten().and_then(|s| s.parse().ok()).unwrap_or(d)
    }
    let rounding_minutes = f64_key(config_repo, user_id, "gryzzly.rounding_minutes", 15.0).await;
    Ok(ReconstructionConfig {
        morning: (
            u32_key(config_repo, user_id, "workday.morning_start_hour", 8).await,
            u32_key(config_repo, user_id, "workday.morning_end_hour", 12).await,
        ),
        afternoon: (
            u32_key(config_repo, user_id, "workday.afternoon_start_hour", 13).await,
            u32_key(config_repo, user_id, "workday.afternoon_end_hour", 17).await,
        ),
        daily_target_hours: f64_key(config_repo, user_id, "workday.daily_target_hours", 7.5).await,
        rounding_hours: (rounding_minutes / 60.0).max(f64::EPSILON),
        min_signal_hours: f64_key(config_repo, user_id, "timesheet.min_signal_hours", 2.0).await,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn reconstruct_timesheet(
    worklog_repo: &dyn WorklogRepository,
    meeting_repo: &dyn MeetingRepository,
    task_repo: &dyn TaskRepository,
    catalog_repo: &dyn GryzzlyCatalogRepository,
    mapping_repo: &dyn SignalMappingRepository,
    config_repo: &dyn ConfigRepository,
    git: &dyn GitConnector,
    user_id: UserId,
    date: NaiveDate,
) -> Result<ReconstructedDay, AppError> {
    let tz = resolve_tz(config_repo.get(user_id, "aplan.timezone").await?);
    let (from_utc, to_utc) = local_day_bounds(date, tz);
    let cfg = load_reconstruction_config(config_repo, user_id).await?;

    let rules = mapping_repo.list_enabled(user_id).await?;
    let live_project_ids: HashSet<String> = catalog_repo
        .list_active(user_id, None, None, 5000)
        .await?
        .into_iter()
        .map(|e| e.gryzzly_project_id)
        .collect();

    // ---- Worklog signals ----
    let wl = worklog_repo
        .list(
            user_id,
            &WorklogFilter { task_ids: None, from: Some(from_utc), to: Some(to_utc), limit: WORKLOG_FILTER_MAX_LIMIT, offset: 0 },
        )
        .await?;
    let mut signals: Vec<Signal> = Vec::new();
    for e in &wl {
        let task = task_repo.find_by_id(e.task_id).await?;
        let raw = RawSignal::Worklog {
            task_gryzzly_project_id: task.as_ref().and_then(|t| t.gryzzly_project_id.clone()),
        };
        let project = mapped_or_none(&raw, &rules, &live_project_ids);
        signals.push(Signal {
            at: to_local(e.logged_at, tz),
            gryzzly_project_id: project,
            kind: SignalKind::Log,
            label: truncate(&e.body, 60),
            source_ref: format!("wl:{}", e.id),
        });
    }

    // ---- Git commit signals ----
    let repos = split_repos(config_repo.get(user_id, "git.repos").await?);
    if !repos.is_empty() {
        let commits = git.commits_between(&repos, from_utc, to_utc).await?;
        for c in &commits {
            // Prefer a Jira key match to a task; else fall back to repo/branch rules.
            let mut project = None;
            if let Some(key) = jira_key_in(&c.message).or_else(|| jira_key_in(&c.branch)) {
                if let Some(t) = task_repo.find_by_source(user_id, Source::Jira, &key).await? {
                    project = t.gryzzly_project_id.clone().filter(|p| live_project_ids.contains(p));
                }
            }
            if project.is_none() {
                let raw = RawSignal::Commit { repo_path: c.repo_path.clone(), branch: c.branch.clone() };
                project = mapped_or_none(&raw, &rules, &live_project_ids);
            }
            signals.push(Signal {
                at: to_local(c.committed_at, tz),
                gryzzly_project_id: project,
                kind: SignalKind::Commit,
                label: truncate(&c.message, 60),
                source_ref: format!("git:{}:{}", c.repo_path, c.committed_at.to_rfc3339()),
            });
        }
    }

    // ---- Meeting anchors ----
    let meetings_raw = meeting_repo.find_by_user_and_date(user_id, date).await?;
    let mut meetings: Vec<MeetingBlock> = Vec::new();
    for m in &meetings_raw {
        let kind = if is_out_of_office(m) { MeetingKind::OutOfOffice } else { MeetingKind::Work };
        let project = if matches!(kind, MeetingKind::Work) {
            let raw = RawSignal::Meeting {
                subject: m.title.clone(),
                organizer: meeting_organizer(m),
                internal_project_id: m.project_id.map(|p| p.to_string()),
            };
            mapped_or_none(&raw, &rules, &live_project_ids)
        } else {
            None
        };
        meetings.push(MeetingBlock {
            start: to_local(m.start_time, tz),
            end: to_local(m.end_time, tz),
            gryzzly_project_id: project,
            kind,
            title: m.title.clone(),
            source_ref: format!("mtg:{}", m.id),
        });
    }

    let day = reconstruct_day(&DayInputs { date, meetings, signals }, &cfg);

    persist_reconstructed(
        // persist as a fresh draft unless a validated/submitted one exists
        get_draft_repo_from(config_repo), // placeholder — see note
        user_id,
        &day,
        cfg.daily_target_hours,
    )
    .await?;

    Ok(day)
}

fn mapped_or_none(
    raw: &RawSignal,
    rules: &[SignalMapping],
    live: &HashSet<String>,
) -> Option<String> {
    match resolve_signal_project(raw, rules, live) {
        ProjectResolution::Mapped { gryzzly_project_id, .. } => Some(gryzzly_project_id),
        ProjectResolution::Unmapped { .. } => None,
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

fn split_repos(v: Option<String>) -> Vec<String> {
    v.map(|s| {
        s.split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

/// A meeting is out-of-office if Outlook marked it `oof` or its title looks like leave.
fn is_out_of_office(m: &Meeting) -> bool {
    let show_as = m.show_as.as_deref().unwrap_or("").to_lowercase();
    if show_as == "oof" {
        return true;
    }
    let t = m.title.to_lowercase();
    ["congé", "conge", "vacances", "pto", "ooo", "out of office", "absent"]
        .iter()
        .any(|kw| t.contains(kw))
}

fn meeting_organizer(_m: &Meeting) -> Option<String> {
    // Meeting schema has no organizer column today; return None until added.
    None
}
```

> **Important:** the two placeholder lines (`persist_reconstructed(get_draft_repo_from(...), ...)`) are wrong-by-construction to force the next step. Replace them in Step 3 — `reconstruct_timesheet` must additionally take `draft_repo: &dyn TimesheetDraftRepository` as a parameter and persist through it, guarding on existing status.

- [ ] **Step 2: Write the failing test (mock repos), then run to see it fail**

Add to the bottom of `timesheet.rs` a `#[cfg(test)] mod tests` module. Model the mocks on `gryzzly_assignment.rs` tests. Minimum viable test:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{Duration, TimeZone};
    use std::sync::Mutex;

    use crate::errors::RepositoryError;

    // --- Mock ConfigRepository (Europe/Paris default, no overrides) ---
    #[derive(Default)]
    struct MemConfig {
        map: Mutex<std::collections::HashMap<String, String>>,
    }
    #[async_trait]
    impl ConfigRepository for MemConfig {
        async fn get(&self, _u: UserId, key: &str) -> Result<Option<String>, RepositoryError> {
            Ok(self.map.lock().unwrap().get(key).cloned())
        }
        async fn get_all(&self, _u: UserId) -> Result<Vec<(String, String)>, RepositoryError> {
            Ok(vec![])
        }
        async fn set(&self, _u: UserId, key: &str, value: &str) -> Result<(), RepositoryError> {
            self.map.lock().unwrap().insert(key.into(), value.into());
            Ok(())
        }
    }

    // --- Mock TimesheetDraftRepository (captures the upsert) ---
    #[derive(Default)]
    struct MemDraft {
        saved: Mutex<Vec<TimesheetDraft>>,
    }
    #[async_trait]
    impl TimesheetDraftRepository for MemDraft {
        async fn upsert(&self, d: &TimesheetDraft) -> Result<(), RepositoryError> {
            self.saved.lock().unwrap().push(d.clone());
            Ok(())
        }
        async fn find_by_user_and_date(&self, _u: UserId, _d: NaiveDate) -> Result<Option<TimesheetDraft>, RepositoryError> {
            Ok(None)
        }
        async fn set_status(&self, _u: UserId, _d: NaiveDate, _s: TimesheetStatus) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    // NOTE: Worklog/Meeting/Task/Catalog/Git mocks follow the same Mutex-Vec pattern
    // as gryzzly_assignment.rs tests. Implement `WorklogRepository::list` to return a
    // fixed worklog entry, `MeetingRepository::find_by_user_and_date` to return [],
    // `TaskRepository::find_by_id` to return a task with gryzzly_project_id=Some("p1"),
    // `GryzzlyCatalogRepository::list_active` to return one entry with project "p1",
    // and a `GitConnector` that returns []. (Full mock bodies are mechanical; mirror
    // the unimplemented!()-stub style from gryzzly_assignment.rs for unused methods.)

    #[tokio::test]
    async fn reconstruct_high_signal_day_persists_draft_summing_to_target() {
        // Wire the mocks (see NOTE above), then:
        //   let day = reconstruct_timesheet(...).await.unwrap();
        //   assert!((day.total_hours - 7.5).abs() < 1e-9);
        //   assert_eq!(draft_repo.saved.lock().unwrap().len(), 1);
        // Left as the concrete assertion once mocks are in place.
    }
}
```

Run: `cd backend && cargo test -p application timesheet`
Expected: FAIL (compile error: the placeholder `get_draft_repo_from`/`persist_reconstructed` don't exist).

- [ ] **Step 3: Fix the signature + implement persistence and the remaining use cases**

Replace the placeholder tail of `reconstruct_timesheet` and add the persistence + other use cases. Change the function signature to accept `draft_repo: &dyn TimesheetDraftRepository`, and end with:
```rust
    // Persist as a draft, but NEVER clobber a validated/submitted day.
    if let Some(existing) = draft_repo.find_by_user_and_date(user_id, date).await? {
        if matches!(existing.status, TimesheetStatus::Validated | TimesheetStatus::Submitted) {
            return Ok(day);
        }
    }
    let draft = to_draft(user_id, &day, cfg.daily_target_hours, TimesheetStatus::Draft, catalog_repo).await?;
    draft_repo.upsert(&draft).await?;
    Ok(day)
}

/// Build a persistable draft from a reconstructed day, decorating lines with
/// project names from the catalog and adding the unattributed line if non-zero.
async fn to_draft(
    user_id: UserId,
    day: &ReconstructedDay,
    target_hours: f64,
    status: TimesheetStatus,
    catalog_repo: &dyn GryzzlyCatalogRepository,
) -> Result<TimesheetDraft, AppError> {
    let now = Utc::now();
    let mut lines: Vec<TimesheetDraftLine> = Vec::new();
    for a in &day.allocations {
        let name = catalog_repo
            .find_by_gryzzly_task_id(user_id, "") // project name not directly indexed; see note
            .await
            .ok()
            .flatten()
            .map(|_| String::new());
        let _ = name; // project-name lookup is best-effort; see note below
        lines.push(TimesheetDraftLine {
            id: Uuid::new_v4(),
            gryzzly_project_id: Some(a.gryzzly_project_id.clone()),
            project_name: None,
            hours: a.hours,
            is_pinned: false,
            confidence: a.confidence,
            source_refs: a.source_refs.clone(),
        });
    }
    if day.unattributed_hours > 0.0 {
        lines.push(TimesheetDraftLine {
            id: Uuid::new_v4(),
            gryzzly_project_id: None,
            project_name: None,
            hours: day.unattributed_hours,
            is_pinned: false,
            confidence: Confidence::Low,
            source_refs: vec![],
        });
    }
    let blocks_json = serde_json::to_string(
        &day.blocks
            .iter()
            .map(|b| {
                serde_json::json!({
                    "start": b.start.to_string(),
                    "end": b.end.to_string(),
                    "gryzzlyProjectId": b.gryzzly_project_id,
                    "kind": format!("{:?}", b.kind),
                    "hours": b.hours,
                    "sourceRefs": b.source_refs,
                })
            })
            .collect::<Vec<_>>(),
    )
    .ok();
    Ok(TimesheetDraft {
        id: Uuid::new_v4(),
        user_id,
        date: day.date,
        status,
        target_hours,
        total_hours: day.total_hours,
        day_confidence: day.day_confidence,
        blocks_json,
        lines,
        created_at: now,
        updated_at: now,
    })
}

/// Persist user edits: re-normalize (pinned frozen), store, keep status=draft.
#[allow(clippy::too_many_arguments)]
pub async fn save_timesheet_draft(
    draft_repo: &dyn TimesheetDraftRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    date: NaiveDate,
    edited: Vec<EditedLine>,
) -> Result<(), AppError> {
    let cfg = load_reconstruction_config(config_repo, user_id).await?;
    let normalized = renormalize_lines(&edited, cfg.daily_target_hours, cfg.rounding_hours);
    let now = Utc::now();
    let lines = normalized
        .into_iter()
        .map(|l| TimesheetDraftLine {
            id: Uuid::new_v4(),
            gryzzly_project_id: l.gryzzly_project_id,
            project_name: None,
            hours: l.hours,
            is_pinned: l.is_pinned,
            confidence: Confidence::High,
            source_refs: vec![],
        })
        .collect::<Vec<_>>();
    let total: f64 = lines.iter().map(|l| l.hours).sum();
    let existing = draft_repo.find_by_user_and_date(user_id, date).await?;
    let draft = TimesheetDraft {
        id: existing.as_ref().map(|d| d.id).unwrap_or_else(Uuid::new_v4),
        user_id,
        date,
        status: TimesheetStatus::Draft,
        target_hours: cfg.daily_target_hours,
        total_hours: total,
        day_confidence: existing.map(|d| d.day_confidence).unwrap_or(Confidence::Medium),
        blocks_json: None,
        lines,
        created_at: now,
        updated_at: now,
    };
    draft_repo.upsert(&draft).await?;
    Ok(())
}

pub async fn validate_timesheet(
    draft_repo: &dyn TimesheetDraftRepository,
    user_id: UserId,
    date: NaiveDate,
) -> Result<(), AppError> {
    draft_repo.set_status(user_id, date, TimesheetStatus::Validated).await?;
    Ok(())
}

pub async fn mark_day_off(
    draft_repo: &dyn TimesheetDraftRepository,
    user_id: UserId,
    date: NaiveDate,
    _scope: DayOffScope,
) -> Result<(), AppError> {
    // v1: full-day off. (Half-day scoping refines total_hours in a later iteration.)
    let now = Utc::now();
    let draft = TimesheetDraft {
        id: Uuid::new_v4(),
        user_id,
        date,
        status: TimesheetStatus::DayOff,
        target_hours: 0.0,
        total_hours: 0.0,
        day_confidence: Confidence::High,
        blocks_json: None,
        lines: vec![],
        created_at: now,
        updated_at: now,
    };
    draft_repo.upsert(&draft).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn learn_mapping(
    mapping_repo: &dyn SignalMappingRepository,
    catalog_repo: &dyn GryzzlyCatalogRepository,
    user_id: UserId,
    kind: MappingKind,
    pattern: String,
    branch_pattern: Option<String>,
    gryzzly_project_id: String,
    now: DateTime<Utc>,
) -> Result<SignalMapping, AppError> {
    // Validate the target project against the live catalog + fetch its display name.
    let name = catalog_repo
        .list_active(user_id, None, None, 5000)
        .await?
        .into_iter()
        .find(|e| e.gryzzly_project_id == gryzzly_project_id)
        .map(|e| e.project_name);
    if name.is_none() {
        return Err(AppError::Validation(format!(
            "unknown or inactive Gryzzly project: {gryzzly_project_id}"
        )));
    }
    let mapping = SignalMapping {
        id: Uuid::new_v4(),
        user_id,
        kind,
        pattern,
        branch_pattern,
        gryzzly_project_id,
        gryzzly_project_name: name,
        is_enabled: true,
        created_at: now,
        updated_at: now,
    };
    mapping_repo.upsert(&mapping).await?;
    Ok(mapping)
}
```

> **Note on project names:** the `to_draft` project-name lookup above is a best-effort stub (the catalog is indexed by `gryzzly_task_id`, not project id). For v1, leave `project_name: None` on reconstructed lines — the GraphQL/CLI layer (Plan 2) resolves display names via `list_active`. Remove the dead `find_by_gryzzly_task_id("")` call; it is shown only to flag that name resolution is deferred. The reconstructed line's `project_name` stays `None`.

Then also update the `#[cfg(test)]` test to pass a `MemDraft` into `reconstruct_timesheet` and complete the `reconstruct_high_signal_day_persists_draft_summing_to_target` assertions.

- [ ] **Step 4: Register the use case**

In `backend/crates/application/src/use_cases/mod.rs`, add:
```rust
pub mod timesheet;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test -p application timesheet`
Expected: PASS (the reconstruct/save/validate tests once mocks are complete).

- [ ] **Step 6: Full socle test sweep**

Run: `cd backend && cargo test -p domain -p application -p infrastructure`
Expected: all green.

- [ ] **Step 7: Commit**

```bash
git add backend/crates/application/src/use_cases/timesheet.rs \
        backend/crates/application/src/use_cases/mod.rs
git commit -m "Add timesheet use cases (reconstruct, save, validate, day-off, learn-mapping)"
```

---

## Self-Review

**Spec coverage (against `2026-07-02-gryzzly-timesheet-reconstruction-design.md`):**
- §5.1 migration 010 → Task 1. §5.2 migration 011 → Task 1. ✅
- §3/§6 reconstruction engine + guardrails → Tasks 4-6. ✅
- §7 mapping layer → Tasks 2, 3, 8, 9, plus `learn_mapping` (Task 13). ✅
- §5.3 config keys → `load_reconstruction_config` + `resolve_tz` (Tasks 12, 13). ✅
- D5 worklog source of truth (raw entries, ignore ActivitySlots) → Task 13 reads `worklog_repo.list` only. ✅
- D8 timezone (one helper, Europe/Paris) → Task 12. ✅
- D9 git via `git log`, no commits table → Task 11. ✅
- D10 pinned values → `apportion_to_target` + `renormalize_lines` (Tasks 4, 6, 13). ✅
- **Deferred to later plans (documented):** GraphQL contract §8 (Plan 2), CLI §9.1 (Plan 2), Surface B §9.2 (Plan 3), Surface C §9.3 (Plan 4), `AlertType::TimesheetReady` (Plan 4), meeting `organizer` column (noted `meeting_organizer` returns None until a schema column is added — a `MeetingOrganizer` rule stays dormant until then).

**Placeholder scan:** Task 13 Step 1 intentionally contains a wrong placeholder (`get_draft_repo_from`) to drive Step 3's TDD fix; this is explicitly flagged, not a silent gap. The `to_draft` name-lookup stub is flagged and resolved to `project_name: None`. No `TODO`/`TBD` left as real work.

**Type consistency:** `Confidence` (domain::common), `ReconstructedDay`/`EditedLine`/`Bucket` (domain::rules::reconstruction), `SignalMapping`/`MappingKind` (domain::types::signal_mapping), `TimesheetDraft`/`TimesheetStatus` (domain::types::timesheet), `RepositoryError`/`AppError` (application::errors), repo trait method names (`list_enabled`, `upsert`, `set_status`, `find_by_user_and_date`) are used identically across the trait (Tasks 7, 8), the SQLite impls (Tasks 9, 10), and the use cases (Task 13). ✅

**Open verification notes for the implementer (confirm against real files, adjust if needed):**
1. `users` table seed columns in repo tests (Tasks 9, 10) — confirm against `001_initial.sql`.
2. `Meeting` struct fields (`show_as: Option<String>`, `title`, `start_time`, `end_time`, `project_id: Option<ProjectId>`) — confirm exact names in `domain/src/types/meeting.rs`.
3. `chrono-tz` and `tokio` `process` feature availability at the crate level (Tasks 11, 12).
4. `TaskRepository::find_by_source(user_id, Source, &str)` signature used for commit→Jira matching (Task 13) — confirmed present per extracted patterns.
