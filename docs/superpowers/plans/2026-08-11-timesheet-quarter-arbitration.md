# Timesheet Quarter Arbitration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the single-track carry-forward timesheet with per-task presence lanes the user sees concurrently, arbitrated in four two-hour quarters.

**Architecture:** Two new pure domain modules (`presence.rs` builds overlapping per-task intervals from evidence; `quarters.rs` cuts the configured windows in four and apportions each quarter's hours by presence weight, reusing `apportion_to_target`). The application use case keeps its existing signal gathering, adds `manual` activity slots, and persists the resulting shares in a new table. GraphQL, CLI and React are rewired onto the new payload.

**Tech Stack:** Rust (domain/application/infrastructure/api crates), sqlx + SQLite, async-graphql 7, React 18 + urql + Tailwind, Vitest/RTL.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-08-11-timesheet-quarter-arbitration-design.md`. Every decision there is binding.
- DDD layering is strict: `domain` has zero I/O; `application` defines traits; `infrastructure` implements them; `api` depends on all.
- TDD: write the failing test, run it, see it fail, then implement. Tests are inline `#[cfg(test)] mod tests` in Rust.
- No `.unwrap()` in production code. `sqlx::Error` maps to `RepositoryError::Database(e.to_string())`.
- The gap threshold is `domain::rules::worklog_time::MAX_CONTINUATION_GAP_MINUTES` (45). Never re-declare it, never make it configurable.
- Rounding increment comes from `ReconstructionConfig::rounding_hours` (config `gryzzly.rounding_minutes`, default 15 min). Never hard-code 0.25 in production code.
- Build and test with the scoped command — the `mcp` crate does not compile at HEAD:
  `cd backend && cargo test -p domain -p application -p infrastructure -p api`
- Per `CLAUDE.md`, `SPEC_FONCTIONNELLE.md` / `SPEC_TECHNIQUE.md` (French) are updated in the same change that alters documented behaviour — that is Task 8, and it is not optional.
- Commit messages: imperative subject, no `Co-Authored-By`, no `Signed-off-by`. Stage only the files the task touched.

---

### Task 1: Presence lanes (domain, pure)

**Files:**
- Create: `backend/crates/domain/src/rules/presence.rs`
- Modify: `backend/crates/domain/src/rules/mod.rs` (add `pub mod presence;` after `pub mod reconstruction;`)

**Interfaces:**
- Consumes: `MAX_CONTINUATION_GAP_MINUTES` from `crate::rules::worklog_time`.
- Produces: `LaneKey`, `EvidenceKind`, `EvidencePoint`, `EvidenceSpan`, `Lane`, `build_lanes`, `minutes_in`, `covered_minutes`. Task 2 consumes `LaneKey` and `Lane`; Task 4 builds `EvidencePoint`/`EvidenceSpan`.

Intervals are **local minutes from midnight** (`i64`), the same unit `reconstruction.rs::mins` already uses. Callers rebuild timestamps from the day's date.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 8, 10).unwrap().and_hms_opt(h, m, 0).unwrap()
    }
    fn task(n: u128) -> LaneKey { LaneKey::Task(Uuid::from_u128(n)) }
    fn point(h: u32, m: u32, lane: LaneKey) -> EvidencePoint {
        EvidencePoint { at: at(h, m), lane, label: "t".into(), gryzzly_project_id: Some("p1".into()), kind: EvidenceKind::Log }
    }
    const DAY: [(i64, i64); 2] = [(480, 720), (780, 1020)];

    #[test]
    fn a_lone_point_casts_a_full_back_shadow() {
        let lanes = build_lanes(&[point(10, 0, task(1))], &[], &DAY);
        assert_eq!(lanes[0].intervals, vec![(600 - MAX_CONTINUATION_GAP_MINUTES, 600)]);
    }

    #[test]
    fn a_shadow_is_clipped_at_the_lanes_own_previous_point() {
        // 10:00 then 10:20 — the second reaches back 20 min, not 45.
        let lanes = build_lanes(&[point(10, 0, task(1)), point(10, 20, task(1))], &[], &DAY);
        assert_eq!(lanes[0].intervals, vec![(555, 620)], "merged 09:15-10:00 and 10:00-10:20");
        assert_eq!(minutes_in(&lanes[0], 480, 720), 65, "never 45 + 45");
    }

    #[test]
    fn another_lanes_point_never_clips_this_lanes_shadow() {
        // The carry-forward bug in one test: task 2 logging at 10:10 must not shorten
        // task 1's shadow, and task 1 must not shorten task 2's.
        let lanes = build_lanes(&[point(10, 0, task(1)), point(10, 10, task(2))], &[], &DAY);
        let l1 = lanes.iter().find(|l| l.key == task(1)).unwrap();
        let l2 = lanes.iter().find(|l| l.key == task(2)).unwrap();
        assert_eq!(l1.intervals, vec![(555, 600)]);
        assert_eq!(l2.intervals, vec![(565, 610)]);
    }

    #[test]
    fn overlapping_lanes_are_kept_whole_and_covered_minutes_is_a_union() {
        let lanes = build_lanes(&[point(10, 0, task(1)), point(10, 10, task(2))], &[], &DAY);
        let summed: i64 = lanes.iter().map(|l| minutes_in(l, 480, 720)).sum();
        assert_eq!(summed, 90, "presence is a weight: overlap counts twice");
        assert_eq!(covered_minutes(&lanes, 480, 720), 55, "wall clock 09:15-10:10 counts once");
    }

    #[test]
    fn a_span_is_taken_as_measured_not_shadowed() {
        let spans = vec![EvidenceSpan {
            start: at(9, 0), end: at(10, 0), lane: LaneKey::Meeting("mtg:1".into()),
            label: "Weekly".into(), gryzzly_project_id: None, kind: EvidenceKind::Meeting,
        }];
        let lanes = build_lanes(&[], &spans, &DAY);
        assert_eq!(lanes[0].intervals, vec![(540, 600)]);
    }

    #[test]
    fn evidence_outside_the_windows_is_reported_not_dropped() {
        // 18:30 is past the 17:00 window; its whole shadow lands outside.
        let lanes = build_lanes(&[point(18, 30), ][..].iter().cloned().collect::<Vec<_>>().as_slice(), &[], &DAY);
        assert!(lanes[0].intervals.is_empty());
        assert_eq!(lanes[0].outside_minutes, MAX_CONTINUATION_GAP_MINUTES);
    }

    #[test]
    fn a_shadow_straddling_a_window_edge_keeps_only_the_inside() {
        // 13:10 reaches back to 12:25 — 12:25-13:00 is lunch, 13:00-13:10 is inside.
        let lanes = build_lanes(&[point(13, 10, task(1))], &[], &DAY);
        assert_eq!(lanes[0].intervals, vec![(780, 790)]);
        assert_eq!(lanes[0].outside_minutes, 35);
    }

    #[test]
    fn duplicate_timestamps_do_not_double_count() {
        let lanes = build_lanes(
            &[point(10, 0, task(1)), point(10, 0, task(1)), point(10, 0, task(1))], &[], &DAY);
        assert_eq!(minutes_in(&lanes[0], 480, 720), MAX_CONTINUATION_GAP_MINUTES);
    }

    #[test]
    fn lane_key_round_trips_through_its_string_form() {
        for k in [task(7), LaneKey::Meeting("mtg:abc".into()), LaneKey::Unattributed] {
            assert_eq!(LaneKey::parse(&k.as_key()), Some(k));
        }
    }
}
```

Fix the sixth test's construction while writing it — it must read
`build_lanes(&[point(18, 30, task(1))], &[], &DAY)`.

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cd backend && cargo test -p domain presence`
Expected: FAIL — `unresolved module or unlinked crate 'presence'`.

- [ ] **Step 3: Implement the module**

```rust
use chrono::{NaiveDateTime, Timelike};
use uuid::Uuid;

use crate::rules::worklog_time::MAX_CONTINUATION_GAP_MINUTES;

/// What produced a piece of evidence. Kept on the lane so the UI can say whether a
/// share rests on measured time or on an inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    Log,
    Commit,
    Meeting,
    ManualSlot,
}

/// What a lane is about: a task, a meeting that resolved to no task, or the residue
/// of a quarter nothing accounts for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LaneKey {
    Task(Uuid),
    Meeting(String),
    Unattributed,
}

impl LaneKey {
    /// The persisted / wire form. Stable: it is a database key and a GraphQL argument.
    pub fn as_key(&self) -> String {
        match self {
            LaneKey::Task(id) => format!("task:{id}"),
            LaneKey::Meeting(r) => format!("meeting:{r}"),
            LaneKey::Unattributed => "unattributed".to_string(),
        }
    }

    pub fn parse(s: &str) -> Option<LaneKey> {
        if s == "unattributed" {
            return Some(LaneKey::Unattributed);
        }
        if let Some(rest) = s.strip_prefix("task:") {
            return Uuid::parse_str(rest).ok().map(LaneKey::Task);
        }
        s.strip_prefix("meeting:").map(|rest| LaneKey::Meeting(rest.to_string()))
    }

    pub fn task_id(&self) -> Option<Uuid> {
        match self {
            LaneKey::Task(id) => Some(*id),
            _ => None,
        }
    }
}

/// A timestamped trace of work: a worklog entry or a commit. It is written AFTER the
/// work, so it is evidence for the stretch BEFORE it — see [`build_lanes`].
#[derive(Debug, Clone)]
pub struct EvidencePoint {
    pub at: NaiveDateTime,
    pub lane: LaneKey,
    pub label: String,
    pub gryzzly_project_id: Option<String>,
    pub kind: EvidenceKind,
}

/// Measured time: a meeting from the calendar, or a hand-run activity slot. Taken as
/// it stands — there is nothing to infer.
#[derive(Debug, Clone)]
pub struct EvidenceSpan {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub lane: LaneKey,
    pub label: String,
    pub gryzzly_project_id: Option<String>,
    pub kind: EvidenceKind,
}

/// One task's presence across the day. Lanes OVERLAP: two sessions running at once
/// produce two lanes covering the same minutes, and that is the point — presence is a
/// weight for apportionment, not a claim on the clock.
#[derive(Debug, Clone)]
pub struct Lane {
    pub key: LaneKey,
    pub label: String,
    pub gryzzly_project_id: Option<String>,
    /// Merged, window-clipped, sorted. Local minutes from midnight.
    pub intervals: Vec<(i64, i64)>,
    /// Minutes clipped away for falling outside the working windows. Reported, never
    /// silently dropped: a day's evening work is the user's to declare or ignore.
    pub outside_minutes: i64,
    pub kinds: Vec<EvidenceKind>,
}

fn mins(dt: NaiveDateTime) -> i64 {
    dt.time().hour() as i64 * 60 + dt.time().minute() as i64
}

/// Turn evidence into per-lane presence.
///
/// A point at `T` covers `[max(T - MAX_CONTINUATION_GAP_MINUTES, previous point of the
/// SAME lane), T]`. The clip at the lane's own previous point is what stops two entries
/// counting the same minute twice; the 45-minute cap is what stops a lone entry claiming
/// the morning. Clipping against OTHER lanes is exactly the carry-forward bug this
/// replaces, so it must never be reintroduced.
///
/// Spans are taken as measured. Everything is merged per lane, then clipped to
/// `windows`; what falls outside is accumulated into `outside_minutes` rather than lost.
pub fn build_lanes(
    points: &[EvidencePoint],
    spans: &[EvidenceSpan],
    windows: &[(i64, i64)],
) -> Vec<Lane> {
    use std::collections::BTreeMap;

    struct Acc {
        label: String,
        project: Option<String>,
        raw: Vec<(i64, i64)>,
        kinds: Vec<EvidenceKind>,
    }
    let mut acc: BTreeMap<LaneKey, Acc> = BTreeMap::new();

    let mut sorted: Vec<&EvidencePoint> = points.iter().collect();
    sorted.sort_by_key(|p| (p.lane.clone(), mins(p.at)));

    let mut prev: BTreeMap<LaneKey, i64> = BTreeMap::new();
    for p in sorted {
        let end = mins(p.at);
        let floor = prev.get(&p.lane).copied().unwrap_or(i64::MIN);
        let start = (end - MAX_CONTINUATION_GAP_MINUTES).max(floor);
        prev.insert(p.lane.clone(), end);
        let e = acc.entry(p.lane.clone()).or_insert_with(|| Acc {
            label: p.label.clone(),
            project: p.gryzzly_project_id.clone(),
            raw: vec![],
            kinds: vec![],
        });
        if !e.kinds.contains(&p.kind) {
            e.kinds.push(p.kind);
        }
        if end > start {
            e.raw.push((start, end));
        }
    }

    for s in spans {
        let e = acc.entry(s.lane.clone()).or_insert_with(|| Acc {
            label: s.label.clone(),
            project: s.gryzzly_project_id.clone(),
            raw: vec![],
            kinds: vec![],
        });
        if !e.kinds.contains(&s.kind) {
            e.kinds.push(s.kind);
        }
        let (a, b) = (mins(s.start), mins(s.end));
        if b > a {
            e.raw.push((a, b));
        }
    }

    acc.into_iter()
        .map(|(key, a)| {
            let merged = merge(a.raw);
            let total: i64 = merged.iter().map(|(s, e)| e - s).sum();
            let intervals = clip(&merged, windows);
            let inside: i64 = intervals.iter().map(|(s, e)| e - s).sum();
            Lane {
                key,
                label: a.label,
                gryzzly_project_id: a.project,
                intervals,
                outside_minutes: total - inside,
                kinds: a.kinds,
            }
        })
        .collect()
}

fn merge(mut v: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    v.sort();
    let mut out: Vec<(i64, i64)> = Vec::with_capacity(v.len());
    for (s, e) in v {
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

fn clip(v: &[(i64, i64)], windows: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    for (s, e) in v {
        for (ws, we) in windows {
            let a = (*s).max(*ws);
            let b = (*e).min(*we);
            if b > a {
                out.push((a, b));
            }
        }
    }
    out.sort();
    out
}

/// How many minutes of this lane fall inside `[start_min, end_min)`.
pub fn minutes_in(lane: &Lane, start_min: i64, end_min: i64) -> i64 {
    lane.intervals
        .iter()
        .map(|(s, e)| ((*e).min(end_min) - (*s).max(start_min)).max(0))
        .sum()
}

/// Distinct wall-clock minutes covered by ANY lane in the range — a union, never a sum.
/// This is what quarter confidence is measured on: three concurrent lanes do not make a
/// quarter three times better evidenced.
pub fn covered_minutes(lanes: &[Lane], start_min: i64, end_min: i64) -> i64 {
    let mut all: Vec<(i64, i64)> = Vec::new();
    for l in lanes {
        for (s, e) in &l.intervals {
            let a = (*s).max(start_min);
            let b = (*e).min(end_min);
            if b > a {
                all.push((a, b));
            }
        }
    }
    merge(all).iter().map(|(s, e)| e - s).sum()
}
```

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cd backend && cargo test -p domain presence`
Expected: PASS, 9 tests.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/domain/src/rules/presence.rs backend/crates/domain/src/rules/mod.rs
git commit -m "Build per-task presence lanes from evidence"
```

---

### Task 2: Quarters and apportionment (domain, pure)

**Files:**
- Create: `backend/crates/domain/src/rules/quarters.rs`
- Modify: `backend/crates/domain/src/rules/mod.rs` (add `pub mod quarters;`)

**Interfaces:**
- Consumes: `Lane`, `LaneKey`, `minutes_in`, `covered_minutes` (Task 1); `ReconstructionConfig`, `Bucket`, `apportion_to_target` from `crate::rules::reconstruction`; `Confidence` from `crate::types::common`.
- Produces: `Quarter`, `Share`, `QuarterAllocation`, `DayAllocation`, `Pin`, `quarters()`, `allocate_quarter()`, `allocate_day()`. Task 4 calls `allocate_day`; Tasks 3/5/6/7 carry `Share` fields on the wire.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::presence::{build_lanes, EvidenceKind, EvidencePoint};
    use chrono::NaiveDate;
    use uuid::Uuid;

    fn cfg() -> ReconstructionConfig { ReconstructionConfig::default() }
    fn lane(n: u128, intervals: Vec<(i64, i64)>) -> Lane {
        Lane {
            key: LaneKey::Task(Uuid::from_u128(n)),
            label: format!("task {n}"),
            gryzzly_project_id: Some(format!("p{n}")),
            intervals,
            outside_minutes: 0,
            kinds: vec![EvidenceKind::Log],
        }
    }

    #[test]
    fn the_day_is_cut_into_four_two_hour_quarters() {
        let q = quarters(&cfg());
        assert_eq!(q[0].start_min, 480);
        assert_eq!(q[0].end_min, 600);
        assert_eq!(q[1].end_min, 720);
        assert_eq!(q[2].start_min, 780);
        assert_eq!(q[3].end_min, 1020);
        assert!(q.iter().all(|x| (x.hours - 2.0).abs() < 1e-9));
    }

    #[test]
    fn a_quarter_is_shared_in_proportion_to_presence_and_sums_to_its_length() {
        // 2026-08-10 Q4: 98 / 76 / 71 minutes of presence in 15:00-17:00.
        let lanes = vec![
            lane(1, vec![(917, 1015)]),
            lane(2, vec![(938, 1014)]),
            lane(3, vec![(938, 1009)]),
        ];
        let a = allocate_quarter(&quarters(&cfg())[3], &lanes, &[], 0, 0.25);
        let hours: Vec<f64> = a.shares.iter().map(|s| s.hours).collect();
        assert!((hours.iter().sum::<f64>() - 2.0).abs() < 1e-9, "a quarter always sums to its length");
        assert!(hours.iter().all(|h| (h / 0.25).fract().abs() < 1e-9), "every share lands on the increment");
        let s1 = a.shares.iter().find(|s| s.lane == LaneKey::Task(Uuid::from_u128(1))).unwrap();
        assert!(s1.hours >= 0.75, "the most-present lane leads, never 0.25");
        assert!(a.shares.iter().all(|s| s.hours > 0.0), "a lane that ran gets hours");
    }

    #[test]
    fn a_quarter_with_no_presence_is_unattributed_whole() {
        let a = allocate_quarter(&quarters(&cfg())[0], &[], &[], 0, 0.25);
        assert_eq!(a.shares.len(), 1);
        assert_eq!(a.shares[0].lane, LaneKey::Unattributed);
        assert!((a.shares[0].hours - 2.0).abs() < 1e-9);
        assert!(matches!(a.confidence, Confidence::Low));
    }

    #[test]
    fn a_pinned_share_is_held_and_the_rest_rebalance() {
        let lanes = vec![lane(1, vec![(900, 1000)]), lane(2, vec![(900, 940)])];
        let pins = vec![Pin { lane: LaneKey::Task(Uuid::from_u128(2)), hours: 1.5 }];
        let a = allocate_quarter(&quarters(&cfg())[3], &lanes, &pins, 0, 0.25);
        let s2 = a.shares.iter().find(|s| s.lane == LaneKey::Task(Uuid::from_u128(2))).unwrap();
        assert!((s2.hours - 1.5).abs() < 1e-9);
        assert!(s2.is_pinned);
        assert!((a.shares.iter().map(|s| s.hours).sum::<f64>() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn out_of_office_shrinks_the_quarter() {
        let lanes = vec![lane(1, vec![(480, 540)])];
        let a = allocate_quarter(&quarters(&cfg())[0], &lanes, &[], 60, 0.25);
        assert!((a.declarable_hours - 1.0).abs() < 1e-9);
        assert!((a.shares.iter().map(|s| s.hours).sum::<f64>() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_fully_out_of_office_quarter_declares_nothing() {
        let a = allocate_quarter(&quarters(&cfg())[0], &[], &[], 120, 0.25);
        assert!(a.shares.is_empty());
        assert!((a.declarable_hours - 0.0).abs() < 1e-9);
        assert!(matches!(a.confidence, Confidence::High), "an off quarter is known, not doubtful");
    }

    #[test]
    fn confidence_reads_the_union_of_lanes_not_their_sum() {
        // Two lanes covering the SAME 60 of 120 minutes: 50% covered → Medium, not High.
        let lanes = vec![lane(1, vec![(900, 960)]), lane(2, vec![(900, 960)])];
        let a = allocate_quarter(&quarters(&cfg())[3], &lanes, &[], 0, 0.25);
        assert!(matches!(a.confidence, Confidence::Medium));
    }

    #[test]
    fn the_day_totals_the_quarters_and_takes_their_worst_confidence() {
        let lanes = vec![lane(1, vec![(480, 600), (600, 720), (780, 900)])];
        let day = allocate_day(&lanes, &[], &[], &cfg());
        assert_eq!(day.quarters.len(), 4);
        assert!((day.total_hours - 8.0).abs() < 1e-9);
        assert!(matches!(day.day_confidence, Confidence::Low), "the empty Q4 drags the day down");
    }

    /// The regression this whole design exists for. Three concurrent sessions on
    /// 2026-08-10: the old engine gave the third one 0.29 h for the entire day.
    #[test]
    fn concurrent_sessions_each_declare_real_hours() {
        let d = NaiveDate::from_ymd_opt(2026, 8, 10).unwrap();
        let p = |h: u32, m: u32, n: u128| EvidencePoint {
            at: d.and_hms_opt(h, m, 0).unwrap(),
            lane: LaneKey::Task(Uuid::from_u128(n)),
            label: format!("task {n}"),
            gryzzly_project_id: Some(format!("p{n}")),
            kind: EvidenceKind::Log,
        };
        let points = vec![
            p(14, 9, 1), p(16, 2, 1), p(16, 37, 1), p(16, 55, 1),
            p(16, 23, 2), p(16, 49, 2),
            p(16, 23, 3), p(16, 30, 3), p(16, 36, 3), p(16, 41, 3), p(16, 47, 3), p(16, 53, 3),
        ];
        let lanes = build_lanes(&points, &[], &[(480, 720), (780, 1020)]);
        let day = allocate_day(&lanes, &[], &[], &cfg());
        let per_task = |n: u128| -> f64 {
            day.quarters.iter().flat_map(|q| &q.shares)
                .filter(|s| s.lane == LaneKey::Task(Uuid::from_u128(n)))
                .map(|s| s.hours).sum()
        };
        assert!(per_task(2) >= 0.5, "a session that ran all afternoon declares real hours");
        assert!(per_task(3) >= 0.5, "not 0.29h for a day of work");
        assert!((day.total_hours - 8.0).abs() < 1e-9);
    }
}
```

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cd backend && cargo test -p domain quarters`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the module**

```rust
use crate::rules::presence::{covered_minutes, minutes_in, Lane, LaneKey};
use crate::rules::reconstruction::{apportion_to_target, Bucket, ReconstructionConfig};
use crate::types::common::Confidence;

/// A quarter-day: half of a configured window. Four per day with the defaults
/// (08-10, 10-12, 13-15, 15-17), each declaring its own length in hours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quarter {
    pub index: u8,
    pub start_min: i64,
    pub end_min: i64,
    pub hours: f64,
}

/// One lane's hours inside one quarter. `presence_minutes` is kept beside `hours` so a
/// reader can always see what the number rests on.
#[derive(Debug, Clone, PartialEq)]
pub struct Share {
    pub lane: LaneKey,
    pub label: String,
    pub gryzzly_project_id: Option<String>,
    pub presence_minutes: i64,
    pub hours: f64,
    pub is_pinned: bool,
}

/// A share the user set by hand. Held fixed while the rest of its quarter rebalances.
#[derive(Debug, Clone, PartialEq)]
pub struct Pin {
    pub lane: LaneKey,
    pub hours: f64,
}

#[derive(Debug, Clone)]
pub struct QuarterAllocation {
    pub quarter: Quarter,
    pub shares: Vec<Share>,
    pub ooo_hours: f64,
    /// The quarter's length minus out-of-office time: what the shares sum to.
    pub declarable_hours: f64,
    pub confidence: Confidence,
}

#[derive(Debug, Clone)]
pub struct DayAllocation {
    pub quarters: Vec<QuarterAllocation>,
    pub total_hours: f64,
    pub day_confidence: Confidence,
}

/// Where a pin belongs: quarter index plus lane.
#[derive(Debug, Clone, PartialEq)]
pub struct DayPin {
    pub quarter_index: u8,
    pub lane: LaneKey,
    pub hours: f64,
}

pub fn quarters(cfg: &ReconstructionConfig) -> [Quarter; 4] {
    let halve = |start: u32, end: u32, first: u8| {
        let (s, e) = (start as i64 * 60, end as i64 * 60);
        let mid = s + (e - s) / 2;
        [
            Quarter { index: first, start_min: s, end_min: mid, hours: (mid - s) as f64 / 60.0 },
            Quarter { index: first + 1, start_min: mid, end_min: e, hours: (e - mid) as f64 / 60.0 },
        ]
    };
    let m = halve(cfg.morning.0, cfg.morning.1, 0);
    let a = halve(cfg.afternoon.0, cfg.afternoon.1, 2);
    [m[0], m[1], a[0], a[1]]
}

fn rank(c: Confidence) -> u8 {
    match c {
        Confidence::Low => 0,
        Confidence::Medium => 1,
        Confidence::High => 2,
    }
}

/// Coverage-based confidence: how much of the quarter's wall clock ANY lane accounts for.
fn confidence_for(lanes: &[Lane], q: &Quarter) -> Confidence {
    let span = (q.end_min - q.start_min).max(1);
    let ratio = covered_minutes(lanes, q.start_min, q.end_min) as f64 / span as f64;
    if ratio >= 0.75 {
        Confidence::High
    } else if ratio >= 0.40 {
        Confidence::Medium
    } else {
        Confidence::Low
    }
}

/// Apportion one quarter's declarable hours across the lanes present in it, weighted by
/// presence minutes, rounded to the increment, summing exactly to the declarable hours.
///
/// Out-of-office minutes shrink the quarter first: an afternoon half spent off is not
/// billable time to be shared out. A quarter nothing accounts for becomes one
/// unattributed share rather than silently vanishing from the day.
pub fn allocate_quarter(
    q: &Quarter,
    lanes: &[Lane],
    pins: &[Pin],
    ooo_minutes: i64,
    rounding: f64,
) -> QuarterAllocation {
    let unit = rounding.max(f64::EPSILON);
    let round = |h: f64| (h / unit).round() * unit;
    let ooo_hours = round((ooo_minutes as f64 / 60.0).min(q.hours));
    let declarable = round((q.hours - ooo_hours).max(0.0));

    if declarable <= 0.0 {
        return QuarterAllocation {
            quarter: *q,
            shares: vec![],
            ooo_hours,
            declarable_hours: 0.0,
            confidence: Confidence::High,
        };
    }

    let present: Vec<(&Lane, i64)> = lanes
        .iter()
        .map(|l| (l, minutes_in(l, q.start_min, q.end_min)))
        .filter(|(l, m)| *m > 0 || pins.iter().any(|p| p.lane == l.key))
        .collect();

    if present.is_empty() {
        return QuarterAllocation {
            quarter: *q,
            shares: vec![Share {
                lane: LaneKey::Unattributed,
                label: "unattributed".to_string(),
                gryzzly_project_id: None,
                presence_minutes: 0,
                hours: declarable,
                is_pinned: false,
            }],
            ooo_hours,
            declarable_hours: declarable,
            confidence: confidence_for(lanes, q),
        };
    }

    let buckets: Vec<Bucket> = present
        .iter()
        .map(|(l, m)| {
            let pin = pins.iter().find(|p| p.lane == l.key);
            Bucket {
                key: Some(l.key.as_key()),
                hours: pin.map(|p| p.hours).unwrap_or(*m as f64),
                pinned: pin.is_some(),
            }
        })
        .collect();

    let apportioned = apportion_to_target(&buckets, declarable, unit);

    let mut shares: Vec<Share> = Vec::with_capacity(apportioned.len());
    for b in &apportioned {
        let Some(key) = b.key.as_ref().and_then(|k| LaneKey::parse(k)) else {
            // apportion_to_target parks any residual on the unattributed bucket.
            if b.hours > 0.0 {
                shares.push(Share {
                    lane: LaneKey::Unattributed,
                    label: "unattributed".to_string(),
                    gryzzly_project_id: None,
                    presence_minutes: 0,
                    hours: b.hours,
                    is_pinned: false,
                });
            }
            continue;
        };
        let Some((lane, minutes)) = present.iter().find(|(l, _)| l.key == key) else {
            continue;
        };
        shares.push(Share {
            lane: key,
            label: lane.label.clone(),
            gryzzly_project_id: lane.gryzzly_project_id.clone(),
            presence_minutes: *minutes,
            hours: b.hours,
            is_pinned: b.pinned,
        });
    }
    shares.retain(|s| s.hours > 0.0 || s.is_pinned);
    shares.sort_by(|a, b| {
        b.hours
            .partial_cmp(&a.hours)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.presence_minutes.cmp(&a.presence_minutes))
            .then(a.label.cmp(&b.label))
    });

    QuarterAllocation {
        quarter: *q,
        shares,
        ooo_hours,
        declarable_hours: declarable,
        confidence: confidence_for(lanes, q),
    }
}

/// Allocate the whole day. `ooo` holds out-of-office ranges in local minutes.
pub fn allocate_day(
    lanes: &[Lane],
    pins: &[DayPin],
    ooo: &[(i64, i64)],
    cfg: &ReconstructionConfig,
) -> DayAllocation {
    let qs = quarters(cfg);
    let mut out = Vec::with_capacity(4);
    for q in qs.iter() {
        let ooo_minutes: i64 = ooo
            .iter()
            .map(|(s, e)| ((*e).min(q.end_min) - (*s).max(q.start_min)).max(0))
            .sum();
        let local: Vec<Pin> = pins
            .iter()
            .filter(|p| p.quarter_index == q.index)
            .map(|p| Pin { lane: p.lane.clone(), hours: p.hours })
            .collect();
        out.push(allocate_quarter(q, lanes, &local, ooo_minutes, cfg.rounding_hours));
    }
    let total_hours = out.iter().flat_map(|q| &q.shares).map(|s| s.hours).sum();
    let day_confidence = out
        .iter()
        .map(|q| q.confidence)
        .min_by_key(|c| rank(*c))
        .unwrap_or(Confidence::Low);
    DayAllocation { quarters: out, total_hours, day_confidence }
}
```

If `Confidence` is not `Copy`, clone it in `confidence_for`/`allocate_day` rather than
changing the type.

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cd backend && cargo test -p domain quarters`
Expected: PASS, 9 tests.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/domain/src/rules/quarters.rs backend/crates/domain/src/rules/mod.rs
git commit -m "Apportion each quarter-day across concurrent lanes by presence"
```

---

### Task 3: Persist quarter shares (migration + repository)

**Files:**
- Create: `migrations/sqlite/018_create_timesheet_quarter_shares.sql`
- Create: `backend/crates/application/src/repositories/timesheet_quarter_share_repository.rs`
- Create: `backend/crates/infrastructure/src/database/timesheet_quarter_share_repo.rs`
- Modify: `backend/crates/application/src/repositories/mod.rs`, `backend/crates/infrastructure/src/database/mod.rs`
- Modify: `backend/crates/domain/src/types/timesheet.rs` (add `QuarterShareRow`, add `lanes_json` to `TimesheetDraft`)

**Interfaces:**
- Consumes: `LaneKey` (Task 1), `Share` (Task 2).
- Produces: `QuarterShareRow`, `TimesheetQuarterShareRepository { list_for_draft, replace_for_draft, set_pin, clear_pin, clear_quarter_pins }`. Task 4 calls all five.

- [ ] **Step 1: Write the migration**

```sql
-- A share is a billing decision, so it gets a table, not a JSON blob: `blocks_json`
-- is documented as opaque display JSON that readers tolerate missing, and hours that
-- reach an invoice must not live under that contract.
CREATE TABLE IF NOT EXISTS timesheet_quarter_shares (
    id                 TEXT PRIMARY KEY,
    draft_id           TEXT NOT NULL REFERENCES timesheet_drafts(id) ON DELETE CASCADE,
    quarter_index      INTEGER NOT NULL CHECK (quarter_index BETWEEN 0 AND 3),
    -- ON DELETE SET NULL, never CASCADE: deleting a task must not erase declared hours.
    -- lane_key and label survive the deletion so the row stays readable.
    task_id            TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    lane_key           TEXT NOT NULL,
    label              TEXT NOT NULL,
    gryzzly_project_id TEXT,
    presence_minutes   INTEGER NOT NULL DEFAULT 0,
    hours              REAL NOT NULL,
    is_pinned          INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL,
    UNIQUE (draft_id, quarter_index, lane_key)
);

CREATE INDEX IF NOT EXISTS idx_tqs_draft ON timesheet_quarter_shares(draft_id, quarter_index);

-- The evidence view: display only, tolerant parse, absence renders as "reconstruct to
-- see the evidence" — the same contract blocks_json and unresolved_json already follow.
ALTER TABLE timesheet_drafts ADD COLUMN lanes_json TEXT;
```

- [ ] **Step 2: Add the domain row type and widen the draft**

In `backend/crates/domain/src/types/timesheet.rs`:

```rust
/// One persisted quarter share. Mirrors `domain::rules::quarters::Share` plus the
/// identity needed to write it back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarterShareRow {
    pub id: Uuid,
    pub quarter_index: u8,
    pub task_id: Option<Uuid>,
    pub lane_key: String,
    pub label: String,
    pub gryzzly_project_id: Option<String>,
    pub presence_minutes: i64,
    pub hours: f64,
    pub is_pinned: bool,
}
```

Add `pub lanes_json: Option<String>,` to `TimesheetDraft` and fix every construction
site the compiler points at (`application/src/use_cases/timesheet.rs`,
`api/src/graphql/types/timesheet.rs` tests, `infrastructure/.../timesheet_draft_repo.rs`).

- [ ] **Step 3: Write the failing repository test**

In `backend/crates/infrastructure/src/database/timesheet_quarter_share_repo.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::connection::create_pool_with_migrations;

    async fn seed_draft(pool: &SqlitePool) -> (Uuid, Uuid) { /* insert a user + a draft row, return (user_id, draft_id) */ }

    #[tokio::test]
    async fn replace_then_list_round_trips_every_field() {
        let pool = create_pool_with_migrations("sqlite::memory:").await.unwrap();
        let (_u, draft_id) = seed_draft(&pool).await;
        let repo = SqliteTimesheetQuarterShareRepository::new(pool.clone());
        let row = QuarterShareRow {
            id: Uuid::new_v4(), quarter_index: 3, task_id: Some(Uuid::new_v4()),
            lane_key: "task:abc".into(), label: "SCB-364".into(),
            gryzzly_project_id: Some("p1".into()), presence_minutes: 98,
            hours: 0.75, is_pinned: true,
        };
        repo.replace_for_draft(draft_id, &[row.clone()]).await.unwrap();
        let back = repo.list_for_draft(draft_id).await.unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].presence_minutes, 98);
        assert!(back[0].is_pinned);
        assert!((back[0].hours - 0.75).abs() < 1e-9);
    }

    #[tokio::test]
    async fn set_pin_marks_the_row_and_clear_quarter_pins_unmarks_all_of_them() {
        let pool = create_pool_with_migrations("sqlite::memory:").await.unwrap();
        let (_u, draft_id) = seed_draft(&pool).await;
        let repo = SqliteTimesheetQuarterShareRepository::new(pool.clone());
        let mk = |lane: &str| QuarterShareRow {
            id: Uuid::new_v4(), quarter_index: 3, task_id: None, lane_key: lane.into(),
            label: lane.into(), gryzzly_project_id: None, presence_minutes: 10,
            hours: 1.0, is_pinned: false,
        };
        repo.replace_for_draft(draft_id, &[mk("task:a"), mk("task:b")]).await.unwrap();
        repo.set_pin(draft_id, 3, "task:a", 1.5).await.unwrap();
        let pinned: Vec<_> = repo.list_for_draft(draft_id).await.unwrap()
            .into_iter().filter(|r| r.is_pinned).collect();
        assert_eq!(pinned.len(), 1);
        assert!((pinned[0].hours - 1.5).abs() < 1e-9);
        repo.clear_quarter_pins(draft_id, 3).await.unwrap();
        assert!(repo.list_for_draft(draft_id).await.unwrap().iter().all(|r| !r.is_pinned));
    }
}
```

- [ ] **Step 4: Run it and watch it fail**

Run: `cd backend && cargo test -p infrastructure quarter_share`
Expected: FAIL — type not found.

- [ ] **Step 5: Implement the trait and the SQLite repository**

Trait (`application/src/repositories/timesheet_quarter_share_repository.rs`):

```rust
#[async_trait]
pub trait TimesheetQuarterShareRepository: Send + Sync {
    async fn list_for_draft(&self, draft_id: Uuid) -> Result<Vec<QuarterShareRow>, RepositoryError>;
    /// Delete-then-insert in one transaction: the caller has already merged the pins it
    /// wants kept, so a partial write would drop declared hours.
    async fn replace_for_draft(&self, draft_id: Uuid, rows: &[QuarterShareRow]) -> Result<(), RepositoryError>;
    async fn set_pin(&self, draft_id: Uuid, quarter_index: u8, lane_key: &str, hours: f64) -> Result<(), RepositoryError>;
    async fn clear_pin(&self, draft_id: Uuid, quarter_index: u8, lane_key: &str) -> Result<(), RepositoryError>;
    async fn clear_quarter_pins(&self, draft_id: Uuid, quarter_index: u8) -> Result<(), RepositoryError>;
}
```

Implement with `sqlx::query`, following `timesheet_draft_repo.rs` exactly: `begin()`,
`DELETE … WHERE draft_id = ?`, one `INSERT` per row, `commit()`, every error mapped to
`RepositoryError::Database(e.to_string())`. `set_pin` is
`UPDATE timesheet_quarter_shares SET hours = ?, is_pinned = 1 WHERE draft_id = ? AND quarter_index = ? AND lane_key = ?`;
when it affects zero rows, INSERT the row instead (pinning a lane that had no share).

- [ ] **Step 6: Run the tests and verify they pass**

Run: `cd backend && cargo test -p infrastructure quarter_share`
Expected: PASS, 2 tests.

- [ ] **Step 7: Commit**

```bash
git add migrations/sqlite/018_create_timesheet_quarter_shares.sql \
        backend/crates/domain/src/types/timesheet.rs \
        backend/crates/application/src/repositories/ \
        backend/crates/infrastructure/src/database/
git commit -m "Persist quarter shares as decisions, not display JSON"
```

---

### Task 4: Rewire the reconstruction use case

**Files:**
- Modify: `backend/crates/application/src/use_cases/timesheet.rs`

**Interfaces:**
- Consumes: everything from Tasks 1-3.
- Produces: `reconstruct_timesheet(... , share_repo, activity_repo, ...) -> ReconstructedDay` with `quarters`, `lanes`, `outside_workday`; `set_quarter_share`, `clear_quarter_share`, `reset_quarter`. Task 5 calls all four.

- [ ] **Step 1: Extend `ReconstructedDay` in `domain/src/rules/reconstruction.rs`**

Replace `blocks: Vec<AttributedBlock>` with:

```rust
pub lanes: Vec<Lane>,
pub quarters: Vec<QuarterAllocation>,
pub outside_workday: Vec<OutsideWork>,
```

```rust
/// Evidence that fell outside the working windows. Reported so a day's evening work is
/// the user's to declare or ignore — the old engine dropped it with no trace.
#[derive(Debug, Clone)]
pub struct OutsideWork {
    pub lane: LaneKey,
    pub label: String,
    pub minutes: i64,
}
```

Delete the carry-forward block builder (`reconstruction.rs:283-341`), `finalize_day`,
`is_low_signal`, `AttributedBlock`, `BlockKind`, `ProjectAllocation`, `reconstruct_day`
and their tests. Keep `Signal`, `SignalKind`, `MeetingBlock`, `MeetingKind`, `DayInputs`,
`ReconstructionConfig`, `Bucket`, `apportion_to_target`, `EditedLine`,
`renormalize_lines`, `UnresolvedSignal`.

- [ ] **Step 2: Write the failing use-case tests**

Add to the existing `#[cfg(test)] mod tests` in `use_cases/timesheet.rs`:

```rust
#[tokio::test]
async fn reconstruction_gives_every_concurrent_task_hours() {
    // Two tasks logging in the same afternoon, the second only near the end —
    // the shape that used to hand the whole afternoon to the first.
    // Assert: both appear in Q3/Q4 shares; the day sums to the window length.
}

#[tokio::test]
async fn a_pinned_share_survives_a_reconstruct() {
    // reconstruct → set_quarter_share(q=3, lane, 1.5) → reconstruct again
    // Assert: the share is still 1.5 and still pinned; the quarter still sums to 2.0.
}

#[tokio::test]
async fn manual_slots_weigh_but_worklog_slots_do_not() {
    // One manual slot 13:00-14:00 on task A, one worklog slot 13:00-14:00 on task B,
    // and one worklog entry on task B.
    // Assert: A's presence includes the measured hour; B's presence comes only from its
    // entry's shadow — counting its projected slot too would double-weight the lane.
}

#[tokio::test]
async fn a_validated_day_is_never_clobbered() {
    // Existing guard, re-asserted against the new write path (shares included).
}
```

Write these as real tests using the in-memory pool and the existing fixture helpers in
this file; do not leave them as comments.

- [ ] **Step 3: Run them and watch them fail**

Run: `cd backend && cargo test -p application timesheet`
Expected: FAIL — signature mismatch / functions not found.

- [ ] **Step 4: Implement**

In `reconstruct_timesheet`, keep lines 87-167 (worklog, commit and meeting gathering)
unchanged, then replace everything from `let day = reconstruct_day(...)`:

1. Convert signals to `EvidencePoint` (`lane: signal.task_id.map(LaneKey::Task)`, dropping
   signals with no task into a `Meeting`-less bucket only when they carry a project).
2. Convert `MeetingKind::Work` meetings to `EvidenceSpan` with
   `LaneKey::Meeting(source_ref)`; collect `MeetingKind::OutOfOffice` into `ooo` ranges.
3. Load the date's activity slots via `ActivitySlotRepository::find_by_user_and_date`,
   keep `source == SlotSource::Manual` and `end_time.is_some()`, convert to
   `EvidenceSpan` with `EvidenceKind::ManualSlot`.
4. `let lanes = build_lanes(&points, &spans, &windows_of(&cfg));`
5. Load the existing draft; read its pins with `list_for_draft(...).filter(is_pinned)`
   into `Vec<DayPin>`.
6. `let day = allocate_day(&lanes, &pins, &ooo, &cfg);`
7. Derive lines: group every share by `gryzzly_project_id`, sum hours, `source_refs` =
   the distinct lane keys contributing, confidence = the lowest of the contributing
   quarters. `None` project → the unattributed line.
8. Persist: `to_draft` writes `lanes_json` (shape
   `[{laneKey,label,gryzzlyProjectId,intervals:[[s,e]],outsideMinutes}]`) and no
   `blocks_json`; then `replace_for_draft` with one `QuarterShareRow` per share.
9. Keep the `Validated | Submitted | DayOff` early return before any write.

Then add the three editing use cases, each: load draft → `set_pin` / `clear_pin` /
`clear_quarter_pins` → re-run steps 5-8 → return the fresh `ReconstructedDay`.
`set_quarter_share` rejects `hours < 0` or `hours > quarter.declarable_hours` with
`AppError::Validation`.

- [ ] **Step 5: Run the whole backend suite**

Run: `cd backend && cargo test -p domain -p application -p infrastructure -p api`
Expected: PASS. Fix every call site the deletions broke.

- [ ] **Step 6: Commit**

```bash
git add backend/crates/domain/src/rules/reconstruction.rs backend/crates/application/src/use_cases/timesheet.rs
git commit -m "Reconstruct the day from presence lanes instead of carry-forward"
```

---

### Task 5: GraphQL contract

**Files:**
- Modify: `backend/crates/api/src/graphql/types/timesheet.rs`, `mutation.rs`, `query.rs`, `backend/crates/api/graphql/schema.graphql`

**Interfaces:**
- Consumes: Task 4's use cases.
- Produces: `LaneGql`, `QuarterGql`, `ShareGql`, `OutsideWorkGql` on `ReconstructedDayGql`; mutations `setQuarterShare`, `clearQuarterShare`, `resetQuarter`. Tasks 6 and 7 query these.

- [ ] **Step 1: Write the failing type test**

```rust
#[test]
fn from_draft_restores_quarters_and_degrades_without_lanes_json() {
    // A draft with two share rows and lanes_json = None.
    // Assert: quarters are rebuilt from the rows (Q3 holds both shares, summing 2.0),
    // and lanes is empty rather than the query failing — a day persisted before this
    // change must still render its numbers.
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd backend && cargo test -p api timesheet`
Expected: FAIL.

- [ ] **Step 3: Implement**

Add the four `SimpleObject`s, drop `AttributedBlockGql` / `blocks` /
`TimesheetLineInput.is_pinned` / `saveTimesheetLines`, and add
`parse_lanes_json` following `parse_blocks_json`'s tolerant contract exactly (a missing
optional key yields `None` for that lane, never an empty list for the whole day).
`from_draft` groups `draft.shares` by `quarter_index` into `QuarterGql`s.

Regenerate the schema — **note this migrates the real database**, so back it up first:

```bash
cp backend/aggregated_plan.db backend/aggregated_plan.db.bak-$(date +%Y%m%d-%H%M)-pre-mig018
cd backend && cargo run -p api -- export-schema
```

- [ ] **Step 4: Run the tests**

Run: `cd backend && cargo test -p api`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/api/ backend/crates/cli/graphql/schema.graphql
git commit -m "Expose lanes, quarters and shares over GraphQL"
```

---

### Task 6: CLI output

**Files:**
- Modify: `backend/crates/cli/src/timesheet_cmd.rs`, `backend/crates/cli/src/cli.rs`, `backend/crates/cli/graphql/*.graphql`

**Interfaces:**
- Consumes: Task 5's schema.
- Produces: the quarter-block rendering and `aplan timesheet set --quarter <1-4> <task> <hours>`.

- [ ] **Step 1: Update the queries and the `set` argument shape**

`--quarter <1-4>` is required on `set`; it maps to `quarterIndex = n - 1`. The
positional project argument becomes a task/lane reference resolved through the day's
lanes (fuzzy on label, exit `3` on ambiguity, exit `2` when nothing matches) — reuse
`lookup.rs`'s existing candidate-listing behaviour.

- [ ] **Step 2: Render the quarters**

```
Q3  13:00-15:00                                   confidence: HIGH
    SCB-364 eActions          ████████  82 min    1.25h
    SAFT GitHub Action        ████      31 min    0.50h
```

Bar width is `presence_minutes` scaled to the quarter's span, capped at 8 characters.
Keep the existing `hours × project` summary and the overlap warning (plain path only),
and add the outside-workday warning:
`⚠ 1 h 34 of evidence outside 08:00-17:00 (SAFT GitHub Action, Gryzzly internal auth)`.

- [ ] **Step 3: Verify against a real day**

Run: `aplan timesheet --date 2026-08-10` then `aplan timesheet --date 2026-08-10 --json`
Expected: four quarter blocks; the three concurrent tasks each hold ≥ 0.5 h; the JSON
carries `quarters` and `lanes`.

- [ ] **Step 4: Commit**

```bash
git add backend/crates/cli/
git commit -m "Print the day as four arbitrated quarters"
```

---

### Task 7: React lanes and quarter editor

**Files:**
- Create: `frontend/src/components/timesheet/TimesheetLanes.tsx`, `frontend/src/components/timesheet/QuarterEditor.tsx`
- Delete: `frontend/src/components/timesheet/TimesheetTimeline.tsx`
- Modify: `frontend/src/pages/TimesheetPage.tsx`, `frontend/src/components/timesheet/ProjectSummarySidebar.tsx`, `frontend/src/hooks/use-timesheet.ts`
- Test: `frontend/src/pages/TimesheetPage.test.tsx`

**Interfaces:**
- Consumes: Task 5's schema (`lanes`, `quarters`, `outsideWorkday`, the three mutations).
- Produces: the editing surface. Nothing consumes it.

- [ ] **Step 1: Write the failing tests**

```tsx
it('renders one row per concurrent task', () => { /* 3 lanes → 3 rows */ });
it('refuses a quarter whose shares do not sum to its length', () => { /* 2.25/2.00 → disabled save + message */ });
it('marks an edited share as pinned and rebalances the others', () => { /* setQuarterShare called with the right args */ });
it('shows the outside-workday warning with its minutes', () => { /* 94 min → "1 h 34" */ });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cd frontend && pnpm test TimesheetPage`
Expected: FAIL.

- [ ] **Step 3: Implement**

`TimesheetLanes` draws one absolutely-positioned row per lane across `08:00–17:00`,
with the three quarter boundaries as verticals and the lunch gap hatched; each bar
carries `title="<label> — <project> — <n> min"`. `QuarterEditor` lists the quarter's
shares with presence minutes and a stepper in `roundingIncrement` steps, shows
`x.xx / 2.00 h`, disables save while the sum is wrong, and offers "reset quarter".
`ProjectSummarySidebar` drops its inputs and pin toggles and renders derived totals,
each expandable to the quarters that produced it. Keep French UI copy.

- [ ] **Step 4: Run the tests**

Run: `cd frontend && pnpm test` then `pnpm build`
Expected: PASS, clean build.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "Show the day as concurrent lanes with a per-quarter editor"
```

---

### Task 8: French specifications

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md`, `SPEC_TECHNIQUE.md`

- [ ] **Step 1: Update `SPEC_FONCTIONNELLE.md`**

Rewrite the timesheet section in French: les voies de présence concurrentes, les quatre
quarts de journée, l'arbitrage manuel, la règle de l'ombre portée de 45 minutes, le
signalement du travail hors plage horaire, et le fait que le total de la journée est
désormais la somme des quarts.

- [ ] **Step 2: Update `SPEC_TECHNIQUE.md`**

Document migration `018`, the `timesheet_quarter_shares` table, the `lanes_json` column,
the two new domain modules, the removal of `blocks`/`saveTimesheetLines`, and the new
GraphQL mutations.

- [ ] **Step 3: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "Documenter l'arbitrage par quart de journée"
```

---

## Self-review

- **Spec coverage:** presence → Task 1; quarters/apportionment/OOO/confidence → Task 2;
  persistence + migration → Task 3; evidence gathering, manual slots, pins, derived
  lines, editing use cases, the three deliberate behaviour changes → Task 4; GraphQL
  contract and the removals → Task 5; CLI → Task 6; frontend → Task 7; French specs →
  Task 8. The 2026-08-10 regression fixture is Task 2 Step 1's last test.
- **Type consistency:** `LaneKey::as_key()` is the single string form used by the table
  (`lane_key`), the GraphQL argument and the CLI; `Share` fields match `QuarterShareRow`
  field for field; `allocate_day(lanes, pins: &[DayPin], ooo, cfg)` is what Task 4 calls.
- **Known cost:** Task 4 deletes `reconstruct_day`, `finalize_day`, `is_low_signal` and
  `AttributedBlock`, which breaks their tests and every call site — that fallout is
  expected work inside Task 4 Step 5, not a surprise.
