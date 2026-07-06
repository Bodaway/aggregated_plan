# Timesheet Reconstruction — Plan 2: GraphQL Contract + `aplan timesheet` CLI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose the Plan-1 reconstruction socle over GraphQL (one `ReconstructedDay` contract + mutations) and add a flag-driven `aplan timesheet` / `aplan map` CLI so the daily timesheet can be reconstructed, reviewed, corrected, and validated from the terminal.

**Architecture:** New async-graphql types/enums wrap the Plan-1 domain/use-case values; resolver methods on the existing `QueryRoot`/`MutationRoot` call the Plan-1 `use_cases::timesheet` functions with deps pulled from `ctx.data`. `SchemaDeps` gains the two new repos + the git connector. The GraphQL SDL is regenerated into the CLI crate, then `graphql_client`-derived operations back thin, flag-driven CLI command functions (no interactive REPL).

**Tech Stack:** Rust, async-graphql 7 (Object/SimpleObject/InputObject/Enum, `NaiveDate` scalar), Axum, graphql_client 0.14 (compile-time SDL validation), clap 4, reqwest blocking. Backend tests: in-memory `build_test_schema` + `schema.execute`. CLI tests: assert_cmd + wiremock.

## Global Constraints

- **Depends on Plan 1** (branch `feat/gryzzly-timesheet-reconstruction`, base of this plan = Plan 1's HEAD). Plan-1 symbols consumed: `application::use_cases::timesheet::{reconstruct_timesheet, save_timesheet_draft, validate_timesheet, mark_day_off, learn_mapping, DayOffScope}`; `domain::rules::reconstruction::{ReconstructedDay, ProjectAllocation, UnresolvedSignal, AttributedBlock, BlockKind, EditedLine}`; `domain::types::{TimesheetDraft, TimesheetDraftLine, TimesheetStatus, Confidence, SignalMapping, MappingKind}`; repos `TimesheetDraftRepository`, `SignalMappingRepository`; service `GitConnector` + `infrastructure::connectors::git::ShellGitConnector`; repo impls `SqliteTimesheetDraftRepository`, `SqliteSignalMappingRepository`.
- **DDD layers:** api depends on all layers; no business logic in resolvers — they only marshal `ctx.data` + call use cases + map errors via `.map_err(|e| async_graphql::Error::new(e.to_string()))`.
- **GraphQL field naming:** async-graphql auto-renames snake_case Rust to camelCase GraphQL (e.g. `gryzzly_project_id` → `gryzzlyProjectId`). Enum variants render SCREAMING_SNAKE by default unless `#[graphql(name=...)]`; match the existing enum pattern in `types/enums.rs`.
- **`NaiveDate`** is used directly as a resolver arg/return (feature `chrono` on async-graphql); no custom scalar.
- **SDL regeneration is manual and MANDATORY** after any GraphQL surface change: `cargo run -p api -- export-schema > backend/crates/cli/graphql/schema.graphql`. The CLI's `graphql_client` derive validates every `.graphql` op against this file at compile time — CLI ops will NOT compile until the SDL includes the new types.
- **No interactive REPL / no new CLI dependency.** `aplan` is automation-facing (stable `ExitCode`, `--json`, invoked by the aplan skill/hook). Editing is via explicit subcommands. (Refinement of spec §9.1; rich interactive editing is Surface B / Plan 3.)
- **Scoped test command** (workspace `mcp` crate doesn't compile at HEAD): `cargo test -p domain -p application -p infrastructure -p api` for backend; `cargo test -p cli` for CLI.
- **Commit messages:** imperative subject, no Jira key, NO `Co-Authored-By`. Stage only task-relevant files.
- **TDD** throughout; bite-sized steps; frequent commits.
- **Spec maintenance:** update `SPEC_FONCTIONNELLE.md` / `SPEC_TECHNIQUE.md` (French) in the final task.

---

## File Structure

**Created:**
- `backend/crates/api/src/graphql/types/timesheet.rs` — `ReconstructedDayGql`, `TimesheetLineGql`, `AttributedBlockGql`, `UnresolvedSignalGql`, `SignalMappingGql`, `TimesheetLineInput`, + conversions.
- `backend/crates/cli/graphql/reconstruct_timesheet.graphql`, `timesheet_draft.graphql`, `save_timesheet_draft.graphql`, `validate_timesheet.graphql`, `mark_day_off.graphql`, `learn_mapping.graphql`, `signal_mappings.graphql`, `delete_signal_mapping.graphql`, `gryzzly_projects.graphql` (or reuse an existing gryzzly op).
- `backend/crates/cli/src/timesheet_cmd.rs` — `aplan timesheet` + `aplan map` command functions + rendering helpers (keep `commands.rs` from growing unwieldy).

**Modified:**
- `backend/crates/api/src/graphql/types/enums.rs` — add `ConfidenceGql`, `TimesheetStatusGql`, `UnresolvedReasonGql`, `MappingKindGql`, `DayOffScopeGql`, `BlockKindGql`.
- `backend/crates/api/src/graphql/types/mod.rs` — register `timesheet` module.
- `backend/crates/api/src/graphql/query.rs` — add `timesheet_draft`, `signal_mappings` resolvers.
- `backend/crates/api/src/graphql/mutation.rs` — add `run_timesheet_reconstruction`, `save_timesheet_draft`, `validate_timesheet`, `mark_day_off`, `learn_mapping` resolvers.
- `backend/crates/api/src/graphql/schema.rs` — extend `SchemaDeps` + `build_schema`.
- `backend/crates/api/src/main.rs` — construct + inject the 3 new deps.
- `backend/crates/api/src/graphql/tests.rs` — extend `build_test_schema` with the new deps; add integration tests.
- `backend/crates/cli/graphql/schema.graphql` — regenerated SDL.
- `backend/crates/cli/src/queries.rs` — new `#[derive(GraphQLQuery)]` structs.
- `backend/crates/cli/src/cli.rs` — `Timesheet` + `Map` subcommands.
- `backend/crates/cli/src/main.rs` — dispatch the new subcommands.
- `backend/crates/cli/src/mod`/`lib` wiring — declare `mod timesheet_cmd;`.
- `SPEC_FONCTIONNELLE.md`, `SPEC_TECHNIQUE.md`.

---

### Task 1: GraphQL enums

**Files:**
- Modify: `backend/crates/api/src/graphql/types/enums.rs`

**Interfaces:**
- Produces: `ConfidenceGql`, `TimesheetStatusGql`, `MappingKindGql`, `DayOffScopeGql`, `BlockKindGql` (all `#[derive(Enum)]`), each with `From` conversions to/from the domain/application enum as needed. (No `UnresolvedReasonGql` — `UnresolvedSignal` has no reason field and nothing consumes it.)

- [ ] **Step 1: Write the enums + conversions + failing unit test**

Append to `backend/crates/api/src/graphql/types/enums.rs` (mirror the existing `HalfDayGql` pattern — `use domain::types as types;` is already imported there; add `use application::use_cases::timesheet::DayOffScope;` and `use domain::rules::reconstruction::BlockKind;`):
```rust
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum ConfidenceGql {
    High,
    Medium,
    Low,
}
impl From<types::Confidence> for ConfidenceGql {
    fn from(c: types::Confidence) -> Self {
        match c {
            types::Confidence::High => ConfidenceGql::High,
            types::Confidence::Medium => ConfidenceGql::Medium,
            types::Confidence::Low => ConfidenceGql::Low,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum TimesheetStatusGql {
    Draft,
    Validated,
    Submitted,
    DayOff,
}
impl From<types::TimesheetStatus> for TimesheetStatusGql {
    fn from(s: types::TimesheetStatus) -> Self {
        match s {
            types::TimesheetStatus::Draft => TimesheetStatusGql::Draft,
            types::TimesheetStatus::Validated => TimesheetStatusGql::Validated,
            types::TimesheetStatus::Submitted => TimesheetStatusGql::Submitted,
            types::TimesheetStatus::DayOff => TimesheetStatusGql::DayOff,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum BlockKindGql {
    Meeting,
    Work,
    OutOfOffice,
}
impl From<BlockKind> for BlockKindGql {
    fn from(b: BlockKind) -> Self {
        match b {
            BlockKind::Meeting => BlockKindGql::Meeting,
            BlockKind::Work => BlockKindGql::Work,
            BlockKind::OutOfOffice => BlockKindGql::OutOfOffice,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum MappingKindGql {
    RepoPath,
    Branch,
    MeetingSubject,
    MeetingOrganizer,
    InternalProject,
}
impl From<types::MappingKind> for MappingKindGql {
    fn from(k: types::MappingKind) -> Self {
        match k {
            types::MappingKind::RepoPath => MappingKindGql::RepoPath,
            types::MappingKind::Branch => MappingKindGql::Branch,
            types::MappingKind::MeetingSubject => MappingKindGql::MeetingSubject,
            types::MappingKind::MeetingOrganizer => MappingKindGql::MeetingOrganizer,
            types::MappingKind::InternalProject => MappingKindGql::InternalProject,
        }
    }
}
impl From<MappingKindGql> for types::MappingKind {
    fn from(k: MappingKindGql) -> Self {
        match k {
            MappingKindGql::RepoPath => types::MappingKind::RepoPath,
            MappingKindGql::Branch => types::MappingKind::Branch,
            MappingKindGql::MeetingSubject => types::MappingKind::MeetingSubject,
            MappingKindGql::MeetingOrganizer => types::MappingKind::MeetingOrganizer,
            MappingKindGql::InternalProject => types::MappingKind::InternalProject,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum DayOffScopeGql {
    Full,
    Morning,
    Afternoon,
}
impl From<DayOffScopeGql> for DayOffScope {
    fn from(s: DayOffScopeGql) -> Self {
        match s {
            DayOffScopeGql::Full => DayOffScope::Full,
            DayOffScopeGql::Morning => DayOffScope::Morning,
            DayOffScopeGql::Afternoon => DayOffScope::Afternoon,
        }
    }
}

#[cfg(test)]
mod timesheet_enum_tests {
    use super::*;

    #[test]
    fn confidence_maps() {
        assert_eq!(ConfidenceGql::from(types::Confidence::Low), ConfidenceGql::Low);
    }
    #[test]
    fn mapping_kind_roundtrips() {
        for k in [
            types::MappingKind::RepoPath,
            types::MappingKind::Branch,
            types::MappingKind::MeetingSubject,
            types::MappingKind::MeetingOrganizer,
            types::MappingKind::InternalProject,
        ] {
            let g: MappingKindGql = k.into();
            let back: types::MappingKind = g.into();
            assert_eq!(back, k);
        }
    }
    #[test]
    fn day_off_scope_maps() {
        assert!(matches!(DayOffScope::from(DayOffScopeGql::Morning), DayOffScope::Morning));
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cd backend && cargo test -p api timesheet_enum_tests`
Expected: PASS (3 tests). (If `enums.rs` lacks the `use` aliases, add them at the top as noted.)

- [ ] **Step 3: Commit**

```bash
git add backend/crates/api/src/graphql/types/enums.rs
git commit -m "Add GraphQL enums for timesheet (Confidence, TimesheetStatus, MappingKind, BlockKind, UnresolvedReason, DayOffScope)"
```

---

### Task 2: GraphQL timesheet types + conversions

**Files:**
- Create: `backend/crates/api/src/graphql/types/timesheet.rs`
- Modify: `backend/crates/api/src/graphql/types/mod.rs`

**Interfaces:**
- Consumes: Task 1 enums; domain `ReconstructedDay`/`ProjectAllocation`/`UnresolvedSignal`/`AttributedBlock` (rules::reconstruction); `TimesheetDraft`/`TimesheetDraftLine`/`SignalMapping` (domain::types).
- Produces:
  - `ReconstructedDayGql` (SimpleObject) with `date, status, targetHours, roundingIncrement, totalHours, dayConfidence, lines: Vec<TimesheetLineGql>, unattributedHours, unresolved: Vec<UnresolvedSignalGql>, blocks: Vec<AttributedBlockGql>`
  - `TimesheetLineGql`, `AttributedBlockGql`, `UnresolvedSignalGql`, `SignalMappingGql` (SimpleObject)
  - `TimesheetLineInput` (InputObject: `gryzzlyProjectId: Option<ID>, hours: f64, isPinned: bool`)
  - `ReconstructedDayGql::from_reconstructed(day: ReconstructedDay, target_hours: f64, rounding_hours: f64, status: TimesheetStatus) -> Self`
  - `ReconstructedDayGql::from_draft(draft: TimesheetDraft, rounding_hours: f64) -> Self`
  - `SignalMappingGql: From<SignalMapping>`

- [ ] **Step 1: Write the types + conversions + failing unit test**

Create `backend/crates/api/src/graphql/types/timesheet.rs`:
```rust
use async_graphql::{InputObject, SimpleObject, ID};
use chrono::{NaiveDate, NaiveDateTime};

use domain::rules::reconstruction::{
    AttributedBlock, EditedLine, ProjectAllocation, ReconstructedDay, UnresolvedSignal,
};
use domain::types::{Confidence, SignalMapping, TimesheetDraft, TimesheetDraftLine, TimesheetStatus};

use super::enums::{BlockKindGql, ConfidenceGql, MappingKindGql, TimesheetStatusGql};

#[derive(SimpleObject)]
pub struct TimesheetLineGql {
    pub gryzzly_project_id: Option<String>,
    pub project_name: Option<String>,
    pub hours: f64,
    pub is_pinned: bool,
    pub confidence: ConfidenceGql,
    pub source_refs: Vec<String>,
}

#[derive(SimpleObject)]
pub struct AttributedBlockGql {
    pub start_time: NaiveDateTime,
    pub end_time: NaiveDateTime,
    pub gryzzly_project_id: Option<String>,
    pub kind: BlockKindGql,
    pub hours: f64,
    pub source_refs: Vec<String>,
}

impl From<AttributedBlock> for AttributedBlockGql {
    fn from(b: AttributedBlock) -> Self {
        Self {
            start_time: b.start,
            end_time: b.end,
            gryzzly_project_id: b.gryzzly_project_id,
            kind: b.kind.into(),
            hours: b.hours,
            source_refs: b.source_refs,
        }
    }
}

#[derive(SimpleObject)]
pub struct UnresolvedSignalGql {
    pub source_ref: String,
    pub label: String,
    pub at: NaiveDateTime,
}

impl From<UnresolvedSignal> for UnresolvedSignalGql {
    fn from(u: UnresolvedSignal) -> Self {
        Self { source_ref: u.source_ref, label: u.label, at: u.at }
    }
}

#[derive(SimpleObject)]
pub struct SignalMappingGql {
    pub id: ID,
    pub kind: MappingKindGql,
    pub pattern: String,
    pub branch_pattern: Option<String>,
    pub gryzzly_project_id: String,
    pub gryzzly_project_name: Option<String>,
    pub is_enabled: bool,
}

impl From<SignalMapping> for SignalMappingGql {
    fn from(m: SignalMapping) -> Self {
        Self {
            id: ID(m.id.to_string()),
            kind: m.kind.into(),
            pattern: m.pattern,
            branch_pattern: m.branch_pattern,
            gryzzly_project_id: m.gryzzly_project_id,
            gryzzly_project_name: m.gryzzly_project_name,
            is_enabled: m.is_enabled,
        }
    }
}

#[derive(SimpleObject)]
pub struct ReconstructedDayGql {
    pub date: NaiveDate,
    pub status: TimesheetStatusGql,
    pub target_hours: f64,
    pub rounding_increment: f64,
    pub total_hours: f64,
    pub day_confidence: ConfidenceGql,
    pub lines: Vec<TimesheetLineGql>,
    pub unattributed_hours: f64,
    pub unresolved: Vec<UnresolvedSignalGql>,
    pub blocks: Vec<AttributedBlockGql>,
}

impl ReconstructedDayGql {
    /// Build from the live reconstruction (has structured unresolved + blocks).
    pub fn from_reconstructed(
        day: ReconstructedDay,
        target_hours: f64,
        rounding_hours: f64,
        status: TimesheetStatus,
    ) -> Self {
        let mut lines: Vec<TimesheetLineGql> = day
            .allocations
            .into_iter()
            .map(|a: ProjectAllocation| TimesheetLineGql {
                gryzzly_project_id: Some(a.gryzzly_project_id),
                project_name: None,
                hours: a.hours,
                is_pinned: false,
                confidence: a.confidence.into(),
                source_refs: a.source_refs,
            })
            .collect();
        if day.unattributed_hours > 0.0 {
            lines.push(TimesheetLineGql {
                gryzzly_project_id: None,
                project_name: None,
                hours: day.unattributed_hours,
                is_pinned: false,
                confidence: ConfidenceGql::Low,
                source_refs: vec![],
            });
        }
        Self {
            date: day.date,
            status: status.into(),
            target_hours,
            rounding_increment: rounding_hours,
            total_hours: day.total_hours,
            day_confidence: day.day_confidence.into(),
            lines,
            unattributed_hours: day.unattributed_hours,
            unresolved: day.unresolved.into_iter().map(Into::into).collect(),
            blocks: day.blocks.into_iter().map(Into::into).collect(),
        }
    }

    /// Build from a persisted draft (unresolved not persisted → empty; blocks from blocks_json).
    pub fn from_draft(draft: TimesheetDraft, rounding_hours: f64) -> Self {
        let unattributed_hours: f64 = draft
            .lines
            .iter()
            .filter(|l| l.gryzzly_project_id.is_none())
            .map(|l| l.hours)
            .sum();
        let lines = draft
            .lines
            .into_iter()
            .map(|l: TimesheetDraftLine| TimesheetLineGql {
                gryzzly_project_id: l.gryzzly_project_id,
                project_name: l.project_name,
                hours: l.hours,
                is_pinned: l.is_pinned,
                confidence: l.confidence.into(),
                source_refs: l.source_refs,
            })
            .collect();
        // blocks_json is a best-effort display aid; ignore parse failures (empty timeline).
        let blocks = draft
            .blocks_json
            .as_deref()
            .and_then(parse_blocks_json)
            .unwrap_or_default();
        Self {
            date: draft.date,
            status: draft.status.into(),
            target_hours: draft.target_hours,
            rounding_increment: rounding_hours,
            total_hours: draft.total_hours,
            day_confidence: draft.day_confidence.into(),
            lines,
            unattributed_hours,
            unresolved: vec![],
            blocks,
        }
    }
}

/// Parse the persisted blocks_json (written by Plan-1 `to_draft`) into display blocks.
/// Shape: [{start,end,gryzzlyProjectId,kind,hours,sourceRefs}]. Returns None on any error.
fn parse_blocks_json(json: &str) -> Option<Vec<AttributedBlockGql>> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let arr = v.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for b in arr {
        let start = NaiveDateTime::parse_from_str(b.get("start")?.as_str()?, "%Y-%m-%d %H:%M:%S").ok()?;
        let end = NaiveDateTime::parse_from_str(b.get("end")?.as_str()?, "%Y-%m-%d %H:%M:%S").ok()?;
        let kind = match b.get("kind")?.as_str()? {
            "Meeting" => BlockKindGql::Meeting,
            "OutOfOffice" => BlockKindGql::OutOfOffice,
            _ => BlockKindGql::Work,
        };
        out.push(AttributedBlockGql {
            start_time: start,
            end_time: end,
            gryzzly_project_id: b.get("gryzzlyProjectId").and_then(|x| x.as_str()).map(String::from),
            kind,
            hours: b.get("hours").and_then(|x| x.as_f64()).unwrap_or(0.0),
            source_refs: b
                .get("sourceRefs")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
                .unwrap_or_default(),
        });
    }
    Some(out)
}

#[derive(InputObject)]
pub struct TimesheetLineInput {
    pub gryzzly_project_id: Option<ID>,
    pub hours: f64,
    pub is_pinned: bool,
}

impl From<TimesheetLineInput> for EditedLine {
    fn from(i: TimesheetLineInput) -> Self {
        EditedLine {
            gryzzly_project_id: i.gryzzly_project_id.map(|id| id.to_string()),
            hours: i.hours,
            is_pinned: i.is_pinned,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn from_draft_computes_unattributed_and_maps_lines() {
        let draft = TimesheetDraft {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            date: NaiveDate::from_ymd_opt(2026, 6, 8).unwrap(),
            status: TimesheetStatus::Draft,
            target_hours: 7.5,
            total_hours: 7.5,
            day_confidence: Confidence::High,
            blocks_json: Some("[]".into()),
            lines: vec![
                TimesheetDraftLine {
                    id: Uuid::new_v4(),
                    gryzzly_project_id: Some("p1".into()),
                    project_name: Some("Proj 1".into()),
                    hours: 5.0,
                    is_pinned: false,
                    confidence: Confidence::High,
                    source_refs: vec!["wl:1".into()],
                },
                TimesheetDraftLine {
                    id: Uuid::new_v4(),
                    gryzzly_project_id: None,
                    project_name: None,
                    hours: 2.5,
                    is_pinned: false,
                    confidence: Confidence::Low,
                    source_refs: vec![],
                },
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let gql = ReconstructedDayGql::from_draft(draft, 0.25);
        assert_eq!(gql.lines.len(), 2);
        assert!((gql.unattributed_hours - 2.5).abs() < 1e-9);
        assert!(matches!(gql.status, TimesheetStatusGql::Draft));
    }

    #[test]
    fn line_input_maps_to_edited_line() {
        let input = TimesheetLineInput {
            gryzzly_project_id: Some(ID("p1".into())),
            hours: 3.0,
            is_pinned: true,
        };
        let edited: EditedLine = input.into();
        assert_eq!(edited.gryzzly_project_id.as_deref(), Some("p1"));
        assert!(edited.is_pinned);
    }
}
```

- [ ] **Step 2: Register the module**

In `backend/crates/api/src/graphql/types/mod.rs`, add alongside the others:
```rust
pub mod timesheet;
```
```rust
pub use timesheet::*;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cd backend && cargo test -p api graphql::types::timesheet`
Expected: PASS (2 tests).
> If `EditedLine`/`ReconstructedDay` aren't re-exported at `domain::rules::reconstruction`, confirm the exact path (they are `pub` in that module per Plan 1).

- [ ] **Step 4: Commit**

```bash
git add backend/crates/api/src/graphql/types/timesheet.rs backend/crates/api/src/graphql/types/mod.rs
git commit -m "Add GraphQL timesheet types (ReconstructedDayGql, lines/blocks/mappings, input) + conversions"
```

---

### Task 3: Extend `SchemaDeps` + `build_schema` + `main.rs` wiring

**Files:**
- Modify: `backend/crates/api/src/graphql/schema.rs`
- Modify: `backend/crates/api/src/main.rs`

**Interfaces:**
- Consumes: `SqliteTimesheetDraftRepository`, `SqliteSignalMappingRepository`, `ShellGitConnector` (Plan 1).
- Produces: `ctx.data::<Arc<dyn TimesheetDraftRepository>>()`, `ctx.data::<Arc<dyn SignalMappingRepository>>()`, `ctx.data::<Arc<dyn GitConnector>>()` available to resolvers (Task 4).

- [ ] **Step 1: Extend `SchemaDeps` and `build_schema`**

In `backend/crates/api/src/graphql/schema.rs`, add three fields to `SchemaDeps` (after `gryzzly_catalog_repo`):
```rust
    pub timesheet_draft_repo: Arc<dyn TimesheetDraftRepository>,
    pub signal_mapping_repo: Arc<dyn SignalMappingRepository>,
    pub git_connector: Arc<dyn application::services::git_connector::GitConnector>,
```
Add the matching names to the `let SchemaDeps { ... } = deps;` destructure, and add three `.data(...)` calls before `.finish()`:
```rust
    .data(timesheet_draft_repo)
    .data(signal_mapping_repo)
    .data(git_connector)
```
Ensure the imports at the top of `schema.rs` include the two repo traits (they live in `application::repositories`; the file already imports the others via the same path).

- [ ] **Step 2: Construct + inject the deps in `main.rs`**

In `backend/crates/api/src/main.rs`, after the `gryzzly_catalog_repo` construction and before `let oauth = ...`, add:
```rust
    let timesheet_draft_repo: Arc<dyn application::repositories::TimesheetDraftRepository> =
        Arc::new(SqliteTimesheetDraftRepository::new(db_pool.clone()));
    let signal_mapping_repo: Arc<dyn application::repositories::SignalMappingRepository> =
        Arc::new(SqliteSignalMappingRepository::new(db_pool.clone()));
    let git_connector: Arc<dyn application::services::git_connector::GitConnector> =
        Arc::new(infrastructure::connectors::git::ShellGitConnector::new());
```
Add the three fields to the `SchemaDeps { ... }` initializer:
```rust
        timesheet_draft_repo,
        signal_mapping_repo,
        git_connector,
```
Add `use` imports for `SqliteTimesheetDraftRepository`, `SqliteSignalMappingRepository` (they are re-exported from `infrastructure::database`) next to the existing `SqliteX` imports. `ShellGitConnector` is referenced by full path above.

- [ ] **Step 3: Verify it builds**

Run: `cd backend && cargo build -p api`
Expected: builds cleanly.
> `cargo build` skips `#[cfg(test)]`; the `build_test_schema` in `tests.rs` is now missing the 3 new `.data()` deps but that only breaks `cargo test`, which Task 5 fixes. Do NOT run `cargo test -p api` here.

- [ ] **Step 4: Commit**

```bash
git add backend/crates/api/src/graphql/schema.rs backend/crates/api/src/main.rs
git commit -m "Wire timesheet draft repo, signal-mapping repo, and git connector into the GraphQL schema"
```

---

### Task 4: Query + Mutation resolvers

**Files:**
- Modify: `backend/crates/api/src/graphql/query.rs`
- Modify: `backend/crates/api/src/graphql/mutation.rs`

**Interfaces:**
- Consumes: Task 2 types, Task 3 `ctx.data`, Plan-1 use cases.
- Produces GraphQL ops: `timesheetDraft(date)`, `signalMappings`, `runTimesheetReconstruction(date)`, `saveTimesheetDraft(date, lines)`, `validateTimesheet(date)`, `markDayOff(date, scope)`, `learnMapping(kind, pattern, branchPattern, gryzzlyProjectId)`.

- [ ] **Step 1: Add the query resolvers**

Add these methods inside `#[Object] impl QueryRoot` in `query.rs` (imports needed at top: `use crate::graphql::types::{ReconstructedDayGql, SignalMappingGql};`, `use application::repositories::{TimesheetDraftRepository, SignalMappingRepository, ConfigRepository};`, `use application::use_cases::timesheet::load_reconstruction_config;`):
```rust
    /// Load the persisted timesheet draft for a local date (null if none reconstructed yet).
    async fn timesheet_draft(
        &self,
        ctx: &Context<'_>,
        date: NaiveDate,
    ) -> Result<Option<ReconstructedDayGql>> {
        let user_id = *ctx.data::<UserId>()?;
        let draft_repo = ctx.data::<Arc<dyn TimesheetDraftRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        let cfg = load_reconstruction_config(config_repo.as_ref(), user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let draft = draft_repo
            .find_by_user_and_date(user_id, date)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(draft.map(|d| ReconstructedDayGql::from_draft(d, cfg.rounding_hours)))
    }

    /// List the current user's enabled signal→project mapping rules.
    async fn signal_mappings(&self, ctx: &Context<'_>) -> Result<Vec<SignalMappingGql>> {
        let user_id = *ctx.data::<UserId>()?;
        let repo = ctx.data::<Arc<dyn SignalMappingRepository>>()?;
        let rows = repo
            .list_enabled(user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(rows.into_iter().map(SignalMappingGql::from).collect())
    }
```

- [ ] **Step 2: Add the mutation resolvers**

Add these methods inside `#[Object] impl MutationRoot` in `mutation.rs` (imports: the Task-2 types, `use application::use_cases::timesheet::{self as timesheet_uc, load_reconstruction_config};`, `use application::repositories::{TimesheetDraftRepository, SignalMappingRepository, MeetingRepository, TaskRepository, GryzzlyCatalogRepository, WorklogRepository, ConfigRepository};`, `use application::services::git_connector::GitConnector;`, `use crate::graphql::types::enums::{DayOffScopeGql, MappingKindGql};`, `use domain::types::TimesheetStatus;`):
```rust
    /// Reconstruct the day from ambient signals, persist the draft, return the full result.
    async fn run_timesheet_reconstruction(
        &self,
        ctx: &Context<'_>,
        date: NaiveDate,
    ) -> Result<ReconstructedDayGql> {
        let user_id = *ctx.data::<UserId>()?;
        let worklog_repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let meeting_repo = ctx.data::<Arc<dyn MeetingRepository>>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let catalog_repo = ctx.data::<Arc<dyn GryzzlyCatalogRepository>>()?;
        let mapping_repo = ctx.data::<Arc<dyn SignalMappingRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        let git = ctx.data::<Arc<dyn GitConnector>>()?;
        let draft_repo = ctx.data::<Arc<dyn TimesheetDraftRepository>>()?;

        let cfg = load_reconstruction_config(config_repo.as_ref(), user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let day = timesheet_uc::reconstruct_timesheet(
            worklog_repo.as_ref(),
            meeting_repo.as_ref(),
            task_repo.as_ref(),
            catalog_repo.as_ref(),
            mapping_repo.as_ref(),
            config_repo.as_ref(),
            git.as_ref(),
            draft_repo.as_ref(),
            user_id,
            date,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        // If a validated/submitted draft already existed, reconstruct_timesheet did NOT
        // overwrite it — return the PERSISTED draft, not the recomputed (unpersisted) `day`,
        // so the client never sees fresh allocations mislabeled as validated.
        let existing = draft_repo
            .find_by_user_and_date(user_id, date)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        match existing {
            Some(d) if matches!(d.status, TimesheetStatus::Validated | TimesheetStatus::Submitted) => {
                Ok(ReconstructedDayGql::from_draft(d, cfg.rounding_hours))
            }
            _ => Ok(ReconstructedDayGql::from_reconstructed(
                day,
                cfg.daily_target_hours,
                cfg.rounding_hours,
                TimesheetStatus::Draft,
            )),
        }
    }

    /// Persist user edits (pinned lines frozen; rejects pinned > target); returns the saved draft.
    async fn save_timesheet_draft(
        &self,
        ctx: &Context<'_>,
        date: NaiveDate,
        lines: Vec<TimesheetLineInput>,
    ) -> Result<ReconstructedDayGql> {
        let user_id = *ctx.data::<UserId>()?;
        let draft_repo = ctx.data::<Arc<dyn TimesheetDraftRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        let edited = lines.into_iter().map(Into::into).collect();
        timesheet_uc::save_timesheet_draft(draft_repo.as_ref(), config_repo.as_ref(), user_id, date, edited)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let cfg = load_reconstruction_config(config_repo.as_ref(), user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let draft = draft_repo
            .find_by_user_and_date(user_id, date)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .ok_or_else(|| async_graphql::Error::new("draft missing after save"))?;
        Ok(ReconstructedDayGql::from_draft(draft, cfg.rounding_hours))
    }

    /// Mark a day's draft validated (ready to copy into Gryzzly).
    async fn validate_timesheet(&self, ctx: &Context<'_>, date: NaiveDate) -> Result<ReconstructedDayGql> {
        let user_id = *ctx.data::<UserId>()?;
        let draft_repo = ctx.data::<Arc<dyn TimesheetDraftRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        timesheet_uc::validate_timesheet(draft_repo.as_ref(), user_id, date)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let cfg = load_reconstruction_config(config_repo.as_ref(), user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let draft = draft_repo
            .find_by_user_and_date(user_id, date)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .ok_or_else(|| async_graphql::Error::new("no draft to validate"))?;
        Ok(ReconstructedDayGql::from_draft(draft, cfg.rounding_hours))
    }

    /// Mark a whole/half day off (suppresses reconstruction fill).
    async fn mark_day_off(
        &self,
        ctx: &Context<'_>,
        date: NaiveDate,
        scope: DayOffScopeGql,
    ) -> Result<ReconstructedDayGql> {
        let user_id = *ctx.data::<UserId>()?;
        let draft_repo = ctx.data::<Arc<dyn TimesheetDraftRepository>>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        timesheet_uc::mark_day_off(draft_repo.as_ref(), user_id, date, scope.into())
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let cfg = load_reconstruction_config(config_repo.as_ref(), user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let draft = draft_repo
            .find_by_user_and_date(user_id, date)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .ok_or_else(|| async_graphql::Error::new("no draft after mark_day_off"))?;
        Ok(ReconstructedDayGql::from_draft(draft, cfg.rounding_hours))
    }

    /// Learn a signal→Gryzzly-project mapping rule (validated against the live catalog).
    async fn learn_mapping(
        &self,
        ctx: &Context<'_>,
        kind: MappingKindGql,
        pattern: String,
        branch_pattern: Option<String>,
        gryzzly_project_id: ID,
    ) -> Result<SignalMappingGql> {
        let user_id = *ctx.data::<UserId>()?;
        let mapping_repo = ctx.data::<Arc<dyn SignalMappingRepository>>()?;
        let catalog_repo = ctx.data::<Arc<dyn GryzzlyCatalogRepository>>()?;
        let now = chrono::Utc::now();
        let mapping = timesheet_uc::learn_mapping(
            mapping_repo.as_ref(),
            catalog_repo.as_ref(),
            user_id,
            kind.into(),
            pattern,
            branch_pattern,
            gryzzly_project_id.to_string(),
            now,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(SignalMappingGql::from(mapping))
    }
```

- [ ] **Step 2b: Run to verify it builds** (test task follows in Task 5)

Run: `cd backend && cargo build -p api`
Expected: builds cleanly.

- [ ] **Step 3: Commit**

```bash
git add backend/crates/api/src/graphql/query.rs backend/crates/api/src/graphql/mutation.rs
git commit -m "Add timesheet GraphQL resolvers (reconstruct/draft/save/validate/day-off/learn-mapping)"
```

---

### Task 5: API integration tests (extend `build_test_schema` + execute)

**Files:**
- Modify: `backend/crates/api/src/graphql/tests.rs`

**Interfaces:**
- Consumes: Tasks 1-4.
- Produces: passing schema-execution tests proving the timesheet ops work end-to-end.

- [ ] **Step 1: Add in-memory/stub repos for the new deps + extend `build_test_schema`**

In `tests.rs`, add three test doubles near the other in-memory repos, then inject them into `build_test_schema`. `build_test_schema` currently omits `gryzzly_catalog_repo` — add an in-memory one too (the timesheet resolvers need it). Complete code:
```rust
// ---- Timesheet-draft in-memory repo (captures upserts) ----
struct InMemoryTimesheetDraftRepository {
    drafts: Mutex<HashMap<(UserId, chrono::NaiveDate), domain::types::TimesheetDraft>>,
}
impl InMemoryTimesheetDraftRepository {
    fn new() -> Self { Self { drafts: Mutex::new(HashMap::new()) } }
}
#[async_trait]
impl application::repositories::TimesheetDraftRepository for InMemoryTimesheetDraftRepository {
    async fn upsert(&self, draft: &domain::types::TimesheetDraft) -> Result<(), RepositoryError> {
        self.drafts.lock().unwrap().insert((draft.user_id, draft.date), draft.clone());
        Ok(())
    }
    async fn find_by_user_and_date(
        &self, user_id: UserId, date: chrono::NaiveDate,
    ) -> Result<Option<domain::types::TimesheetDraft>, RepositoryError> {
        Ok(self.drafts.lock().unwrap().get(&(user_id, date)).cloned())
    }
    async fn set_status(
        &self, user_id: UserId, date: chrono::NaiveDate, status: domain::types::TimesheetStatus,
    ) -> Result<(), RepositoryError> {
        if let Some(d) = self.drafts.lock().unwrap().get_mut(&(user_id, date)) { d.status = status; }
        Ok(())
    }
}

// ---- Signal-mapping in-memory repo ----
struct InMemorySignalMappingRepository { rows: Mutex<Vec<domain::types::SignalMapping>> }
impl InMemorySignalMappingRepository { fn new() -> Self { Self { rows: Mutex::new(vec![]) } } }
#[async_trait]
impl application::repositories::SignalMappingRepository for InMemorySignalMappingRepository {
    async fn list_enabled(&self, user_id: UserId) -> Result<Vec<domain::types::SignalMapping>, RepositoryError> {
        Ok(self.rows.lock().unwrap().iter().filter(|m| m.user_id == user_id && m.is_enabled).cloned().collect())
    }
    async fn upsert(&self, m: &domain::types::SignalMapping) -> Result<(), RepositoryError> {
        let mut rows = self.rows.lock().unwrap();
        rows.retain(|r| !(r.user_id == m.user_id && r.kind == m.kind && r.pattern == m.pattern));
        rows.push(m.clone());
        Ok(())
    }
    async fn set_enabled(&self, _id: domain::types::SignalMappingId, _enabled: bool) -> Result<(), RepositoryError> { Ok(()) }
    async fn delete(&self, _id: domain::types::SignalMappingId) -> Result<(), RepositoryError> { Ok(()) }
}

// ---- Gryzzly catalog in-memory repo ----
struct InMemoryGryzzlyCatalogRepository { rows: Mutex<Vec<domain::types::GryzzlyCatalogEntry>> }
impl InMemoryGryzzlyCatalogRepository { fn new() -> Self { Self { rows: Mutex::new(vec![]) } } }
#[async_trait]
impl application::repositories::GryzzlyCatalogRepository for InMemoryGryzzlyCatalogRepository {
    async fn upsert(&self, e: &domain::types::GryzzlyCatalogEntry) -> Result<(), RepositoryError> {
        self.rows.lock().unwrap().push(e.clone()); Ok(())
    }
    async fn soft_prune_missing(&self, _u: UserId, _keep: &[String]) -> Result<u64, RepositoryError> { Ok(0) }
    async fn list_active(
        &self, user_id: UserId, _search: Option<&str>, _project: Option<&str>, _limit: i64,
    ) -> Result<Vec<domain::types::GryzzlyCatalogEntry>, RepositoryError> {
        Ok(self.rows.lock().unwrap().iter().filter(|e| e.user_id == user_id && e.is_active).cloned().collect())
    }
    async fn find_by_gryzzly_task_id(
        &self, user_id: UserId, gid: &str,
    ) -> Result<Option<domain::types::GryzzlyCatalogEntry>, RepositoryError> {
        Ok(self.rows.lock().unwrap().iter().find(|e| e.user_id == user_id && e.gryzzly_task_id == gid).cloned())
    }
}

// ---- Stub GitConnector (no commits) ----
struct StubGitConnector;
#[async_trait]
impl application::services::git_connector::GitConnector for StubGitConnector {
    async fn commits_between(
        &self, _repos: &[String], _from: chrono::DateTime<chrono::Utc>, _to: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<application::services::git_connector::GitCommit>, application::errors::AppError> {
        Ok(vec![])
    }
}
```
Then extend `build_test_schema`: construct these four, and add `.data(...)` for each (`timesheet_draft_repo`, `signal_mapping_repo`, `git_connector`, and `gryzzly_catalog_repo`). Keep the existing config repo — but note `StubConfigRepository.get` returns `None`, so `resolve_tz`→Europe/Paris and `load_reconstruction_config`→defaults (target 7.5, rounding 0.25). If the existing `StubConfigRepository` is not `Clone`/shareable enough, construct fresh `Arc`s. Also expose a way to seed the in-memory repos: return them from a second builder or make the seeded repos module-level in the test. Simplest: add a `build_test_schema_with(worklog, task, catalog, draft)` variant that accepts pre-seeded `Arc`s, and keep `build_test_schema()` delegating with empty ones.

- [ ] **Step 2: Write the failing integration tests**

Add:
```rust
#[tokio::test]
async fn run_reconstruction_on_empty_day_returns_zero() {
    let schema = build_test_schema();
    let res = schema
        .execute(r#"mutation { runTimesheetReconstruction(date: "2026-06-08") { totalHours dayConfidence status } }"#)
        .await;
    assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
    let data = res.data.into_json().unwrap();
    assert_eq!(data["runTimesheetReconstruction"]["totalHours"], 0.0);
    assert_eq!(data["runTimesheetReconstruction"]["dayConfidence"], "LOW");
}

#[tokio::test]
async fn timesheet_draft_is_null_before_reconstruction() {
    let schema = build_test_schema();
    let res = schema.execute(r#"{ timesheetDraft(date: "2026-06-08") { totalHours } }"#).await;
    assert!(res.errors.is_empty(), "errors: {:?}", res.errors);
    let data = res.data.into_json().unwrap();
    assert!(data["timesheetDraft"].is_null());
}

#[tokio::test]
async fn learn_mapping_rejects_unknown_project() {
    let schema = build_test_schema();
    let res = schema
        .execute(r#"mutation { learnMapping(kind: REPO_PATH, pattern: "/repo", gryzzlyProjectId: "nope") { id } }"#)
        .await;
    assert!(!res.errors.is_empty(), "expected validation error for unknown project");
}
```
Then ONE seeded happy-path test (using the `build_test_schema_with` variant): seed a `GryzzlyCatalogEntry` (project "p1"), a `Task` with `gryzzly_project_id = Some("p1")`, and a `WorklogEntry` on that task. **Timezone caution:** the stub config returns `None`, so `resolve_tz` → Europe/Paris (UTC+2 in June), NOT UTC. Timestamp the worklog's `logged_at` so it lands in the Paris morning window — e.g. `09:00:00Z` = 11:00 Paris (morning). Then run `runTimesheetReconstruction` and assert `totalHours ≈ 7.5` (high-signal? no — one signal → low_signal → total 7.5 with unattributed fill) OR assert `unattributedHours > 0` and a `p1` line exists. Since a single worklog is low-signal, assert: `status == DRAFT`, a line with `gryzzlyProjectId == "p1"` exists, and `unattributedHours > 0`. Then `validateTimesheet` and assert `status == VALIDATED`. Derive exact assertions from the Plan-1 low-signal semantics (project keeps ~raw hours, fill → unattributed, total == 7.5).

- [ ] **Step 3: Run to verify RED then GREEN**

Run: `cd backend && cargo test -p api timesheet`
Expected: after Step 1-2, all timesheet integration tests pass. If `run_reconstruction_on_empty_day` fails because `StubConfigRepository` isn't wired, verify the `.data(config_repo)` is present and returns defaults.

- [ ] **Step 4: Commit**

```bash
git add backend/crates/api/src/graphql/tests.rs
git commit -m "Add GraphQL integration tests for timesheet ops (in-memory schema)"
```

---

### Task 6: Regenerate the CLI GraphQL SDL

**Files:**
- Modify: `backend/crates/cli/graphql/schema.graphql`

**Interfaces:**
- Consumes: Tasks 1-4 (new types must be in the schema).
- Produces: an SDL that includes `ReconstructedDay`, `TimesheetLine`, `SignalMapping`, the new queries/mutations — so Task 7's CLI ops compile.

- [ ] **Step 1: Regenerate the SDL**

Run:
```bash
cd backend && cargo run -p api -- export-schema > crates/cli/graphql/schema.graphql
```

- [ ] **Step 2: Sanity-check the diff**

Run: `cd backend && git diff --stat crates/cli/graphql/schema.graphql && grep -E "runTimesheetReconstruction|ReconstructedDay|learnMapping|timesheetDraft" crates/cli/graphql/schema.graphql`
Expected: the new type/field names appear. `cargo build -p cli` should still pass (no new ops reference them yet).

- [ ] **Step 3: Commit**

```bash
git add backend/crates/cli/graphql/schema.graphql
git commit -m "Regenerate CLI GraphQL SDL (adds timesheet + mapping ops)"
```

---

### Task 7: CLI GraphQL operations + `queries.rs` derives

**Files:**
- Create: `backend/crates/cli/graphql/*.graphql` (listed below)
- Modify: `backend/crates/cli/src/queries.rs`

**Interfaces:**
- Consumes: the regenerated SDL (Task 6).
- Produces: `ReconstructTimesheet`, `TimesheetDraft`, `SaveTimesheetDraft`, `ValidateTimesheet`, `MarkDayOff`, `LearnMapping`, `SignalMappings`, `GryzzlyProjects` GraphQLQuery structs.

- [ ] **Step 1: Write the operation files**

`reconstruct_timesheet.graphql`:
```graphql
mutation ReconstructTimesheet($date: NaiveDate!) {
  runTimesheetReconstruction(date: $date) {
    date status targetHours roundingIncrement totalHours dayConfidence unattributedHours
    lines { gryzzlyProjectId projectName hours isPinned confidence }
    unresolved { sourceRef label at }
    blocks { startTime endTime gryzzlyProjectId kind hours }
  }
}
```
`timesheet_draft.graphql` (same selection set, query `timesheetDraft(date)`), `save_timesheet_draft.graphql`:
```graphql
mutation SaveTimesheetDraft($date: NaiveDate!, $lines: [TimesheetLineInput!]!) {
  saveTimesheetDraft(date: $date, lines: $lines) {
    date status totalHours targetHours
    lines { gryzzlyProjectId projectName hours isPinned }
  }
}
```
`validate_timesheet.graphql` (`validateTimesheet(date){ date status totalHours }`), `mark_day_off.graphql` (`markDayOff(date, scope){ date status totalHours }`), `learn_mapping.graphql`:
```graphql
mutation LearnMapping($kind: MappingKindGql!, $pattern: String!, $branchPattern: String, $gryzzlyProjectId: ID!) {
  learnMapping(kind: $kind, pattern: $pattern, branchPattern: $branchPattern, gryzzlyProjectId: $gryzzlyProjectId) {
    id kind pattern gryzzlyProjectId gryzzlyProjectName isEnabled
  }
}
```
`signal_mappings.graphql` (`signalMappings { id kind pattern branchPattern gryzzlyProjectId gryzzlyProjectName isEnabled }`), `gryzzly_projects.graphql` (reuse the existing query: `gryzzlyTasks(search: $search) { gryzzlyProjectId projectName customerName }`).
> **Confirm enum type names** against the regenerated SDL — async-graphql names the `MappingKindGql` enum by its Rust type name (`MappingKindGql`) unless renamed; use whatever the SDL shows for `$kind` and `$scope` (likely `MappingKindGql!` and `DayOffScopeGql!`). Adjust the `.graphql` variable types to match the SDL exactly, or graphql_client codegen fails.

- [ ] **Step 2: Add the derive structs to `queries.rs`**

FIRST, at the top of `queries.rs` where the scalar aliases live (`type NaiveDate = String;`, `type ID = String;` already exist), ADD:
```rust
#[allow(non_camel_case_types)]
type NaiveDateTime = String;
```
The timesheet ops select `NaiveDateTime` scalar fields (`blocks.startTime`/`endTime`, `unresolved.at`) and graphql_client needs a Rust type for every selected custom scalar — without this alias the derive fails with "cannot find type NaiveDateTime". Then append (mirroring the existing pattern):
```rust
#[derive(GraphQLQuery)]
#[graphql(schema_path = "graphql/schema.graphql", query_path = "graphql/reconstruct_timesheet.graphql", response_derives = "Debug, Clone")]
pub struct ReconstructTimesheet;

#[derive(GraphQLQuery)]
#[graphql(schema_path = "graphql/schema.graphql", query_path = "graphql/timesheet_draft.graphql", response_derives = "Debug, Clone")]
pub struct TimesheetDraft;

#[derive(GraphQLQuery)]
#[graphql(schema_path = "graphql/schema.graphql", query_path = "graphql/save_timesheet_draft.graphql", response_derives = "Debug, Clone")]
pub struct SaveTimesheetDraft;

#[derive(GraphQLQuery)]
#[graphql(schema_path = "graphql/schema.graphql", query_path = "graphql/validate_timesheet.graphql", response_derives = "Debug, Clone")]
pub struct ValidateTimesheet;

#[derive(GraphQLQuery)]
#[graphql(schema_path = "graphql/schema.graphql", query_path = "graphql/mark_day_off.graphql", response_derives = "Debug, Clone")]
pub struct MarkDayOff;

#[derive(GraphQLQuery)]
#[graphql(schema_path = "graphql/schema.graphql", query_path = "graphql/learn_mapping.graphql", response_derives = "Debug, Clone")]
pub struct LearnMapping;

#[derive(GraphQLQuery)]
#[graphql(schema_path = "graphql/schema.graphql", query_path = "graphql/signal_mappings.graphql", response_derives = "Debug, Clone")]
pub struct SignalMappings;

#[derive(GraphQLQuery)]
#[graphql(schema_path = "graphql/schema.graphql", query_path = "graphql/gryzzly_projects.graphql", response_derives = "Debug, Clone")]
pub struct GryzzlyProjects;
```

- [ ] **Step 3: Verify codegen compiles**

Run: `cd backend && cargo build -p cli`
Expected: builds cleanly (graphql_client validates every op against the SDL). If it fails, the op's field/enum/variable names don't match the SDL — fix the `.graphql` file to match the regenerated schema exactly.

- [ ] **Step 4: Commit**

```bash
git add backend/crates/cli/graphql/*.graphql backend/crates/cli/src/queries.rs
git commit -m "Add CLI GraphQL operations for timesheet + mapping"
```

---

### Task 8: `aplan timesheet` command (flag-driven review) + tests

**Files:**
- Create: `backend/crates/cli/src/timesheet_cmd.rs`
- Modify: `backend/crates/cli/src/cli.rs`, `backend/crates/cli/src/main.rs`, and the module list in `main.rs` (`mod timesheet_cmd;`)

**Interfaces:**
- Consumes: Task 7 ops, `Client`/`ExitCode`/`print_json` (existing).
- Produces: `aplan timesheet [--date D] [--json]` (reconstruct + display), `aplan timesheet validate [--date D]`, `aplan timesheet set <project> <hours> [--date D]`, `aplan timesheet off [--am|--pm] [--date D]`.

- [ ] **Step 1: Add the clap subcommand**

In `cli.rs`, add to `enum Commands`:
```rust
    /// Reconstruct + review the day's Gryzzly timesheet (defaults to today).
    Timesheet {
        #[arg(long)]
        date: Option<String>,
        #[command(subcommand)]
        action: Option<TimesheetAction>,
    },
```
and a new subcommand enum:
```rust
#[derive(Subcommand, Debug)]
pub enum TimesheetAction {
    /// Validate the day's draft (ready to copy into Gryzzly).
    Validate,
    /// Pin a project to an exact number of hours.
    Set { project: String, hours: f64 },
    /// Mark the day (or half-day) off.
    Off {
        #[arg(long, conflicts_with = "pm")]
        am: bool,
        #[arg(long)]
        pm: bool,
    },
}
```

- [ ] **Step 2: Implement the command (reconstruct + render)**

Create `timesheet_cmd.rs`. The default action reconstructs and renders a per-project table + an ASCII timeline + an unattributed/confidence footer. Complete code (rendering helpers included; uses the `ReconstructTimesheet` op and, for edits, `SaveTimesheetDraft`/`ValidateTimesheet`/`MarkDayOff`):
```rust
use crate::client::Client;
use crate::output::{print_json, ExitCode};
use crate::queries::{
    mark_day_off, reconstruct_timesheet, save_timesheet_draft, validate_timesheet,
    MarkDayOff, ReconstructTimesheet, SaveTimesheetDraft, ValidateTimesheet,
};

fn today() -> String {
    chrono::Utc::now().date_naive().to_string()
}

/// `aplan timesheet [--date] [--json]` — reconstruct and display the day.
pub fn timesheet(api_url: &str, json: bool, date: Option<&str>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let date = date.map(String::from).unwrap_or_else(today);
    let res = client.run::<ReconstructTimesheet>(reconstruct_timesheet::Variables { date: date.clone() });
    match res {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) { eprintln!("error writing output: {e}"); return ExitCode::Generic; }
                return ExitCode::Success;
            }
            render_day(&r.data.run_timesheet_reconstruction);
            ExitCode::Success
        }
        Err(e) => { eprintln!("error: {e}"); ExitCode::Generic }
    }
}

fn render_day(d: &reconstruct_timesheet::ReconstructTimesheetRunTimesheetReconstruction) {
    println!("== timesheet {} ==  [{:?}]  {:.2}h / {:.1}h target",
        d.date, d.status, d.total_hours, d.target_hours);
    println!("\nhours × project:");
    for l in &d.lines {
        let label = l.gryzzly_project_id.clone().unwrap_or_else(|| "?? unassigned".into());
        let name = l.project_name.clone().unwrap_or_default();
        let pin = if l.is_pinned { "*" } else { " " };
        println!("  {}{:<8.2}h  {:<24} {}", pin, l.hours, label, name);
    }
    let delta = d.total_hours - d.target_hours;
    let badge = if delta.abs() < 1e-6 { "✓ balanced".to_string() }
        else if delta > 0.0 { format!("⚠ +{delta:.2}h over") } else { format!("⚠ {delta:.2}h short") };
    println!("  ── total {:.2}h  ({badge})", d.total_hours);
    if d.unattributed_hours > 1e-9 {
        println!("  !! {:.2}h unattributed — assign with `aplan timesheet set <project> <hours>`", d.unattributed_hours);
    }
    if !d.blocks.is_empty() {
        println!("\ntimeline:");
        let mut blocks: Vec<_> = d.blocks.iter().collect();
        blocks.sort_by(|a, b| a.start_time.cmp(&b.start_time));
        for b in blocks {
            let glyph = match b.kind { reconstruct_timesheet::BlockKindGql::MEETING => "▓ meet",
                reconstruct_timesheet::BlockKindGql::OUT_OF_OFFICE => "░ off ",
                _ => "· work" };
            let proj = b.gryzzly_project_id.clone().unwrap_or_else(|| "-".into());
            println!("  {}–{}  {}  {:.2}h  {}", b.start_time, b.end_time, glyph, b.hours, proj);
        }
    }
    if !d.unresolved.is_empty() {
        println!("\nunresolved signals ({}):", d.unresolved.len());
        for u in &d.unresolved { println!("  {} {}", u.at, u.label); }
    }
}

/// `aplan timesheet validate`
pub fn timesheet_validate(api_url: &str, json: bool, date: Option<&str>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let date = date.map(String::from).unwrap_or_else(today);
    match client.run::<ValidateTimesheet>(validate_timesheet::Variables { date: date.clone() }) {
        Ok(r) => {
            if json { let _ = print_json(&r.raw); return ExitCode::Success; }
            println!("✓ {} validated — copy into Gryzzly", date);
            ExitCode::Success
        }
        Err(e) => { eprintln!("error: {e}"); ExitCode::Generic }
    }
}

/// `aplan timesheet set <project> <hours>` — pin one project to an exact number of hours.
/// Loads the current lines from the PERSISTED draft (preserving prior pins), sets/pins the
/// target project, carries the other lines forward, and saves.
///
/// IMPORTANT (bug avoided): do NOT load lines by calling `runTimesheetReconstruction` — for a
/// non-validated day that upserts a FRESH draft and wipes any previously saved pins, so two
/// consecutive `set` commands would lose the first pin. Read `timesheetDraft(date)` instead
/// (it preserves `isPinned`); only reconstruct when no draft exists yet.
pub fn timesheet_set(api_url: &str, json: bool, date: Option<&str>, project: &str, hours: f64) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let date = date.map(String::from).unwrap_or_else(today);
    // Prefer the persisted draft (keeps prior pins); reconstruct once only if it's null.
    let mut lines: Vec<save_timesheet_draft::TimesheetLineInput> =
        match client.run::<TimesheetDraft>(timesheet_draft::Variables { date: date.clone() }) {
            Ok(r) => match r.data.timesheet_draft {
                Some(d) => d.lines.iter().map(|l| save_timesheet_draft::TimesheetLineInput {
                    gryzzly_project_id: l.gryzzly_project_id.clone(),
                    hours: l.hours,
                    is_pinned: l.is_pinned,
                }).collect(),
                None => match client.run::<ReconstructTimesheet>(reconstruct_timesheet::Variables { date: date.clone() }) {
                    Ok(rr) => rr.data.run_timesheet_reconstruction.lines.iter().map(|l| save_timesheet_draft::TimesheetLineInput {
                        gryzzly_project_id: l.gryzzly_project_id.clone(),
                        hours: l.hours,
                        is_pinned: l.is_pinned,
                    }).collect(),
                    Err(e) => { eprintln!("error: {e}"); return ExitCode::Generic; }
                },
            },
            Err(e) => { eprintln!("error: {e}"); return ExitCode::Generic; }
        };
    match lines.iter_mut().find(|l| l.gryzzly_project_id.as_deref() == Some(project)) {
        Some(l) => { l.hours = hours; l.is_pinned = true; }
        None => lines.push(save_timesheet_draft::TimesheetLineInput {
            gryzzly_project_id: Some(project.to_string()), hours, is_pinned: true,
        }),
    }
    match client.run::<SaveTimesheetDraft>(save_timesheet_draft::Variables { date: date.clone(), lines }) {
        Ok(r) => {
            if json { let _ = print_json(&r.raw); return ExitCode::Success; }
            println!("✎ pinned {project} = {hours:.2}h; other lines rebalanced to target");
            ExitCode::Success
        }
        Err(e) => { eprintln!("error: {e}"); ExitCode::Generic }
    }
}

/// `aplan timesheet off [--am|--pm]`
pub fn timesheet_off(api_url: &str, json: bool, date: Option<&str>, am: bool, pm: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let date = date.map(String::from).unwrap_or_else(today);
    let scope = if am { mark_day_off::DayOffScopeGql::MORNING }
        else if pm { mark_day_off::DayOffScopeGql::AFTERNOON }
        else { mark_day_off::DayOffScopeGql::FULL };
    match client.run::<MarkDayOff>(mark_day_off::Variables { date: date.clone(), scope }) {
        Ok(r) => {
            if json { let _ = print_json(&r.raw); return ExitCode::Success; }
            println!("⏸ {} marked off", date);
            ExitCode::Success
        }
        Err(e) => { eprintln!("error: {e}"); ExitCode::Generic }
    }
}
```
> **Type-name caveat:** the exact codegen names (e.g. `reconstruct_timesheet::BlockKindGql::MEETING`, `ReconstructTimesheetRunTimesheetReconstruction`) are produced by graphql_client from the SDL. After Task 6/7 compile, adjust these identifiers to whatever the derive actually generates (the implementer confirms via `cargo build -p cli` errors, which name the exact types).

- [ ] **Step 3: Dispatch in `main.rs`**

Add `mod timesheet_cmd;` and in the `match args.command`:
```rust
        cli::Commands::Timesheet { date, action } => match action {
            None => timesheet_cmd::timesheet(&args.api_url, args.json, date.as_deref()),
            Some(cli::TimesheetAction::Validate) => timesheet_cmd::timesheet_validate(&args.api_url, args.json, date.as_deref()),
            Some(cli::TimesheetAction::Set { project, hours }) => timesheet_cmd::timesheet_set(&args.api_url, args.json, date.as_deref(), &project, hours),
            Some(cli::TimesheetAction::Off { am, pm }) => timesheet_cmd::timesheet_off(&args.api_url, args.json, date.as_deref(), am, pm),
        },
```

- [ ] **Step 4: Write CLI tests (assert_cmd + wiremock)**

Add an integration test under `backend/crates/cli/tests/` (or the crate's existing test location) mirroring the existing CLI test pattern: start a `wiremock` server that responds to the GraphQL POST with a canned `runTimesheetReconstruction` payload, run `aplan timesheet --json --api-url <mock>` via `assert_cmd`, and assert exit code 0 + the JSON contains `runTimesheetReconstruction`. Add a second test asserting the human render contains `hours × project` and the `unattributed` line when the mock returns `unattributedHours > 0`.
> Follow the existing CLI test file's wiremock setup verbatim (search `backend/crates/cli/tests` / `wiremock` usage). If no CLI integration test exists yet, add a minimal one using `wiremock::MockServer` + `assert_cmd::Command::cargo_bin("aplan")`.

- [ ] **Step 5: Run + commit**

Run: `cd backend && cargo test -p cli`
Expected: PASS.
```bash
git add backend/crates/cli/src/timesheet_cmd.rs backend/crates/cli/src/cli.rs backend/crates/cli/src/main.rs backend/crates/cli/tests
git commit -m "Add flag-driven aplan timesheet command (reconstruct/validate/set/off)"
```

---

### Task 9: `aplan map` command + tests

**Files:**
- Modify: `backend/crates/cli/src/cli.rs`, `backend/crates/cli/src/main.rs`, `backend/crates/cli/src/timesheet_cmd.rs` (add map fns)

**Interfaces:**
- Consumes: `LearnMapping`, `SignalMappings` ops.
- Produces: `aplan map add --repo <path> [--branch <glob>] --project <gid>`, `aplan map add --meeting-subject <kw> --project <gid>`, `aplan map add --meeting-organizer <email> --project <gid>`, `aplan map add --internal-project <id> --project <gid>`, `aplan map list`.

- [ ] **Step 1: Add the clap subcommand**

In `cli.rs`:
```rust
    /// Manage signal→Gryzzly-project mapping rules.
    Map {
        #[command(subcommand)]
        cmd: MapCmd,
    },
```
```rust
#[derive(Subcommand, Debug)]
pub enum MapCmd {
    /// Add/update a mapping rule (exactly one of --repo/--meeting-subject/--meeting-organizer/--internal-project).
    Add {
        #[arg(long)] repo: Option<String>,
        #[arg(long, requires = "repo")] branch: Option<String>,
        #[arg(long)] meeting_subject: Option<String>,
        #[arg(long)] meeting_organizer: Option<String>,
        #[arg(long)] internal_project: Option<String>,
        #[arg(long)] project: String,
    },
    /// List enabled mapping rules.
    List,
}
```

- [ ] **Step 2: Implement in `timesheet_cmd.rs`**

```rust
use crate::queries::{learn_mapping, signal_mappings, LearnMapping, SignalMappings};

pub fn map_add(
    api_url: &str, json: bool,
    repo: Option<&str>, branch: Option<&str>, meeting_subject: Option<&str>,
    meeting_organizer: Option<&str>, internal_project: Option<&str>, project: &str,
) -> ExitCode {
    // Determine kind + pattern from exactly one selector.
    let (kind, pattern, branch_pattern) = if let Some(r) = repo {
        if branch.is_some() {
            (learn_mapping::MappingKindGql::BRANCH, r.to_string(), branch.map(String::from))
        } else {
            (learn_mapping::MappingKindGql::REPO_PATH, r.to_string(), None)
        }
    } else if let Some(s) = meeting_subject {
        (learn_mapping::MappingKindGql::MEETING_SUBJECT, s.to_string(), None)
    } else if let Some(o) = meeting_organizer {
        (learn_mapping::MappingKindGql::MEETING_ORGANIZER, o.to_string(), None)
    } else if let Some(p) = internal_project {
        (learn_mapping::MappingKindGql::INTERNAL_PROJECT, p.to_string(), None)
    } else {
        eprintln!("error: provide one of --repo / --meeting-subject / --meeting-organizer / --internal-project");
        return ExitCode::PreconditionFailed;
    };
    let client = Client::new(api_url.to_string());
    let vars = learn_mapping::Variables {
        kind, pattern, branch_pattern, gryzzly_project_id: project.to_string(),
    };
    match client.run::<LearnMapping>(vars) {
        Ok(r) => {
            if json { let _ = print_json(&r.raw); return ExitCode::Success; }
            println!("✎ mapping saved → project {project}");
            ExitCode::Success
        }
        Err(e) => { eprintln!("error: {e}"); ExitCode::Generic }
    }
}

pub fn map_list(api_url: &str, json: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());
    match client.run::<SignalMappings>(signal_mappings::Variables {}) {
        Ok(r) => {
            if json { let _ = print_json(&r.raw); return ExitCode::Success; }
            for m in &r.data.signal_mappings {
                let br = m.branch_pattern.clone().map(|b| format!("@{b}")).unwrap_or_default();
                let name = m.gryzzly_project_name.clone().unwrap_or_default();
                println!("  [{:?}] {}{} → {} {}", m.kind, m.pattern, br, m.gryzzly_project_id, name);
            }
            ExitCode::Success
        }
        Err(e) => { eprintln!("error: {e}"); ExitCode::Generic }
    }
}
```

- [ ] **Step 3: Dispatch in `main.rs`**

```rust
        cli::Commands::Map { cmd } => match cmd {
            cli::MapCmd::Add { repo, branch, meeting_subject, meeting_organizer, internal_project, project } =>
                timesheet_cmd::map_add(&args.api_url, args.json, repo.as_deref(), branch.as_deref(),
                    meeting_subject.as_deref(), meeting_organizer.as_deref(), internal_project.as_deref(), &project),
            cli::MapCmd::List => timesheet_cmd::map_list(&args.api_url, args.json),
        },
```

- [ ] **Step 4: Test + commit**

Add a wiremock-backed `assert_cmd` test: mock `learnMapping` success, run `aplan map add --repo /x --project p1 --json`, assert exit 0. Mock `signalMappings` returning one rule, run `aplan map list`, assert the output contains the project id.
Run: `cd backend && cargo test -p cli`
```bash
git add backend/crates/cli/src/cli.rs backend/crates/cli/src/main.rs backend/crates/cli/src/timesheet_cmd.rs backend/crates/cli/tests
git commit -m "Add aplan map command (add/list mapping rules)"
```

---

### Task 10: Update specifications (French)

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md`, `SPEC_TECHNIQUE.md`

**Interfaces:** none (docs).

- [ ] **Step 1: Document the GraphQL surface**

In `SPEC_TECHNIQUE.md`, add the new GraphQL query/mutation list (`timesheetDraft`, `signalMappings`, `runTimesheetReconstruction`, `saveTimesheetDraft`, `validateTimesheet`, `markDayOff`, `learnMapping`) with their arguments and the `ReconstructedDay` shape, and note the SDL-regeneration step (`cargo run -p api -- export-schema`).

- [ ] **Step 2: Document the CLI + the REPL decision**

In `SPEC_FONCTIONNELLE.md`, document the `aplan timesheet` review flow and `aplan map` rules, and record the decision that the CLI review is **flag-driven (no interactive REPL)** — rich interactive editing is the frontend timesheet screen (Surface B). Write in French.

- [ ] **Step 3: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "Document timesheet GraphQL API + aplan CLI (flag-driven review)"
```

---

## Self-Review

**Spec coverage (§8 GraphQL contract, §9.1 CLI of the design):**
- `ReconstructedDay` type + `timesheetDraft`/`runTimesheetReconstruction`/`saveTimesheetDraft`/`validateTimesheet`/`markDayOff`/`learnMapping`/`signalMappings` → Tasks 1-4. ✅ (`reassignBlock` from §8 is intentionally omitted — block-level reassignment is a Surface-B/timeline concern; the CLI edits at the project-line level via `set`. Documented as a deliberate scope call.)
- Project picker → reuse existing `gryzzlyTasks` (Task 7 `gryzzly_projects.graphql`). ✅
- `aplan timesheet` reconstruct + table + ASCII timeline + unattributed/confidence → Task 8. ✅ Interactive REPL → **dropped** (documented; no stdin precedent, automation-facing CLI). Edits via `set`/`validate`/`off`. ✅
- `aplan map` → Task 9. ✅
- SDL regeneration hard-ordering → Task 6 between backend and CLI. ✅

**Placeholder scan:** The two "type-name caveat" notes (Tasks 7, 8) flag that graphql_client's generated identifiers must be matched against real codegen output — these are legitimate "confirm exact generated name" instructions, not unfinished work; the resolver/op logic is fully specified. The `build_test_schema_with` seeded variant (Task 5) is described precisely enough to implement.

**Type consistency:** GraphQL enum names (`MappingKindGql`, `DayOffScopeGql`, `ConfidenceGql`, `TimesheetStatusGql`, `BlockKindGql`) used consistently across enums.rs (Task 1), types (Task 2), resolvers (Task 4), and CLI ops (Task 7). `EditedLine`/`ReconstructedDay`/`TimesheetDraft` consumed with the exact Plan-1 shapes. `reconstruct_timesheet`'s 10-arg signature (incl. `draft_repo`) matches Plan 1 Task 13.

**Open verification notes for the implementer (confirm against real code):**
1. `types/enums.rs` import aliases (`types`, plus adding `DayOffScope`/`BlockKind`/`UnresolvedReason`) — confirm the existing `use` at the top and extend.
2. Exact graphql_client-generated type paths in the CLI (Tasks 7-9) — resolve from `cargo build -p cli` errors.
3. The GraphQL enum SDL names for input enums (`$kind`, `$scope`) — match the regenerated `schema.graphql` exactly in the `.graphql` op files.
4. `StubConfigRepository` in `tests.rs` returns `None` for all keys → reconstruction uses defaults (Paris, 7.5h, 0.25). Confirm; if it panics/errs instead, use a small in-memory config returning `None`.
5. Existing CLI integration-test harness (wiremock) location and helper — mirror it for Tasks 8-9.
6. The GraphQL type is named `ReconstructedDayGql` in both Rust and the SDL (async-graphql keeps the `Gql` suffix, like `TaskStatusGql`/`SourceGql`). Prose in this plan sometimes says "ReconstructedDay" for brevity — the actual type/SDL name has the suffix. CLI `.graphql` ops reference query/field names (not the type name) so this doesn't affect codegen.
