# Timesheet: concurrent evidence + quarter-day arbitration

**Date:** 2026-08-11
**Status:** Design — approved, ready for planning
**Author:** (brainstormed with Claude)
**Replaces:** the carry-forward half of `2026-07-02-gryzzly-timesheet-reconstruction-design.md`

## Problem

The reconstruction engine models a day as a **single track**: one project at a time, each
interval credited to the signal that *opens* it (`domain/src/rules/reconstruction.rs:304-338`).
Real days are not single-track. On 2026-08-10 three Claude sessions and one manual thread ran
concurrently, and the carry-forward rule produced a declaration nobody recognises:

| task | activity journal | timesheet | reality |
|---|---|---|---|
| SCB-364 eActions | 2 h 15 | **4.18 h** | ran all afternoon |
| SAFT GitHub Action | 2 h 32 | **3.54 h** | ran all afternoon |
| Gryzzly internal auth | 2 h 10 | **0.29 h** | ran all afternoon |
| Audit config | 1 min | **0** | one entry, 17:16 |

Three failures compound:

1. **Winner-takes-all by accident.** The 13:00–16:02 block — 3.03 h, the largest of the day —
   went entirely to SCB-364 because its 14:09 entry was the first signal after lunch. The two
   other sessions were working the whole time and logged nothing until 16:23.
2. **Interleaving starves the middle.** The Gryzzly task's entries fall between other tasks'
   entries, so it collects six slivers of 1–6 min: 0.29 h for a day's work.
3. **Silent truncation.** Twenty entries logged after 17:00 local, and one at 12:31, fall
   outside the configured windows and are dropped with no trace anywhere in the UI or CLI.

The user cannot correct any of this, because the only view offered is the single track that
already lost the information.

## Goal

Show every task that was alive at the same time, then let the user arbitrate the day in four
two-hour quarters. The declaration is a consequence of that arbitration, not of a heuristic.

## Model

```
evidence  →  lanes  →  quarters  →  shares  →  lines
```

| stage | layer | meaning |
|---|---|---|
| evidence | application | worklog entries, git commits, meetings, `manual` activity slots |
| lanes | domain (`presence.rs`) | one per task: the intervals it can be shown to have been alive; lanes overlap |
| quarters | domain (`quarters.rs`) | the two configured windows cut in half: 08–10, 10–12, 13–15, 15–17 |
| shares | domain (`quarters.rs`) | per quarter, hours per task, summing to the quarter's own length |
| lines | application | shares summed by Gryzzly project — what Gryzzly consumes |

### Presence

A worklog entry is a timestamp, not a duration, and it is written **after** the work. So each
point of evidence casts a **back-shadow**: the stretch it is evidence for.

- A point at `T` in lane `L` covers `[max(T − MAX_CONTINUATION_GAP_MINUTES, previous point of L), T]`.
  The clip at the lane's own previous point is what stops two entries counting the same minute
  twice; the clip at 45 minutes is what stops a lone entry claiming the morning.
- `MAX_CONTINUATION_GAP_MINUTES` (45) is reused from `domain/src/rules/worklog_time.rs:37`, not
  re-declared and not made configurable. That constant already carries the measured
  justification for 45 over 15, and a threshold that differed between the journal and the
  timesheet would make the two views disagree by construction.
- Meetings and `manual` activity slots contribute their **real span** instead of a shadow —
  those are measured, not inferred.
- Intervals are merged within a lane, then clipped to the configured windows.

Lanes overlap on purpose. On 2026-08-10 15:00–17:00 the three lanes hold 98 + 71 + 76 = 245 min
of presence inside 120 min of wall clock. **Presence is a weight, not a claim on the clock**: no
rule can recover how attention split between three concurrent sessions, so the quarter's hours
are apportioned in proportion to the weights.

### Quarter apportionment

For each quarter, buckets are the lanes with presence > 0, weighted by presence minutes, and the
quarter's own length is apportioned across them with the **existing**
`apportion_to_target(&buckets, quarter_hours, rounding_hours)` (`reconstruction.rs:141`). That
function already does largest-remainder rounding to the increment with pinned buckets held
fixed — which is exactly a quarter with some shares set by hand.

2026-08-10, Q4 (15:00–17:00):

| lane | presence | weight | share |
|---|---|---|---|
| SCB-364 | 98 min | 40 % | 0.75 h |
| Gryzzly internal auth | 76 min | 31 % | 0.75 h |
| SAFT GitHub Action | 71 min | 29 % | 0.50 h |
| | | | **2.00 h** |

### Edge cases

| situation | behaviour |
|---|---|
| quarter with no presence at all | one unattributed share (project `None`) for the full quarter, confidence `Low` |
| quarter partly covered by an out-of-office meeting | the quarter's declarable hours drop by the OOO minutes, rounded to the increment; at zero the quarter declares nothing |
| work meeting | an ordinary lane, weighted by its measured span, labelled with its subject |
| task with no `gryzzly_project_id` | keeps its share; the hours land on the unattributed line and the lane is flagged "no Gryzzly project" |
| two tasks on the same project | both keep their own lane; the lines sum them (this is normal — SCB-364 and the SAFT tasks all live in `SAFT - 2026 - S2`) |
| evidence entirely outside the windows | never silently dropped: reported per task in `outside_workday` with minutes and first/last timestamp |
| pinned share | held fixed; the rest of that quarter re-apportions around it |

### Confidence

Per quarter, from the **distinct covered minutes** (union of all lanes, not the sum):

- `High` ≥ 75 % of the quarter covered
- `Medium` ≥ 40 %
- `Low` otherwise

Day confidence is the lowest of the four quarters. This retires `is_low_signal`
(`reconstruction.rs:363`), whose span-based heuristic exists only to decide between the two
`finalize_day` branches, both of which disappear.

## Behaviour changes (deliberate)

1. **The day total is the sum of the quarters (8 h with the default windows), not
   `workday.daily_target_hours` (7.5 by default; this user's value is 8).** A quarter that sums
   to 2.00 h by construction cannot also sum to 1.875 h. `daily_target_hours` becomes a *check*:
   when the declared total differs, the UI and CLI say so. OOO still reduces the declarable day.
2. **Line-level pinning is removed.** Lines are derived from shares, so a pinned line would be a
   second source of truth that the quarters could not explain. Editing moves to the quarter.
   `saveTimesheetLines` / `TimesheetLineInput.isPinned` disappear.
3. **`aplan timesheet set <project> <hours>` becomes `aplan timesheet set --quarter <1-4>
   <task> <hours>`** — the same verb, retargeted at the new unit.
4. **The single-track timeline is replaced by lanes.** `blocks` / `blocks_json` stop being
   written; days persisted before this change render with no evidence view until reconstructed,
   which the UI states explicitly rather than showing a blank strip.

## Components

### Domain (pure, no I/O)

`backend/crates/domain/src/rules/presence.rs` — new

```rust
pub enum EvidenceKind { Log, Commit, Meeting, ManualSlot }

pub struct EvidencePoint { pub at: NaiveDateTime, pub lane: LaneKey, pub kind: EvidenceKind }
pub struct EvidenceSpan  { pub start: NaiveDateTime, pub end: NaiveDateTime,
                           pub lane: LaneKey, pub kind: EvidenceKind }

/// Task id, or a meeting that resolved to no task.
pub enum LaneKey { Task(Uuid), Meeting(String) }

pub struct Lane {
    pub key: LaneKey,
    pub label: String,                      // task title, or meeting subject
    pub gryzzly_project_id: Option<String>,
    pub intervals: Vec<(NaiveDateTime, NaiveDateTime)>,   // merged, window-clipped
    pub outside_minutes: i64,               // clipped away, reported not discarded
}

pub fn build_lanes(points: &[EvidencePoint], spans: &[EvidenceSpan],
                   labels: &LaneLabels, windows: &[(i64, i64)]) -> Vec<Lane>;
pub fn minutes_in(lane: &Lane, start_min: i64, end_min: i64) -> i64;
pub fn covered_minutes(lanes: &[Lane], start_min: i64, end_min: i64) -> i64;  // union
```

`backend/crates/domain/src/rules/quarters.rs` — new

```rust
pub struct Quarter { pub index: u8, pub start_min: i64, pub end_min: i64, pub hours: f64 }

pub struct Share {
    pub lane: LaneKey,
    pub label: String,
    pub gryzzly_project_id: Option<String>,
    pub presence_minutes: i64,
    pub hours: f64,
    pub is_pinned: bool,
}

pub struct QuarterAllocation {
    pub quarter: Quarter,
    pub shares: Vec<Share>,
    pub ooo_hours: f64,
    pub confidence: Confidence,
}

pub fn quarters(cfg: &ReconstructionConfig) -> [Quarter; 4];
pub fn allocate_quarter(q: &Quarter, lanes: &[Lane], pinned: &[(LaneKey, f64)],
                        ooo_minutes: i64, rounding: f64) -> QuarterAllocation;
pub fn allocate_day(lanes: &[Lane], pins: &Pins, ooo: &[(i64, i64)],
                    cfg: &ReconstructionConfig) -> DayAllocation;
```

`reconstruction.rs` keeps `ReconstructionConfig`, `apportion_to_target`, `Bucket`, the window
helpers and the meeting/OOO anchoring. The carry-forward builder (`:283-341`) and `finalize_day`
(`:377-456`) are deleted with their tests.

### Application

`backend/crates/application/src/use_cases/timesheet.rs` — the gathering half is unchanged
(worklog entries, git commits, meetings, project resolution through
`project_mapping::resolve_signal_project`). Added: `manual` activity slots for the date via
`ActivitySlotRepository::find_by_user_and_date`, filtered on `source == SlotSource::Manual` —
a hand-run timer is measured time the worklog projection cannot derive. `worklog`-sourced slots
are ignored: they are a projection of the same entries already feeding the points, and counting
both would double-weight the lane.

`reconstruct_timesheet` becomes: gather → `build_lanes` → load pins → `allocate_day` → derive
lines by project → persist. It keeps the existing guard that never clobbers a
`Validated | Submitted | DayOff` day.

New use cases: `set_quarter_share`, `clear_quarter_share`, `reset_quarter` — each re-apportions
the touched quarter and rewrites the derived lines, nothing else.

### Persistence — migration `018`

```sql
CREATE TABLE timesheet_quarter_shares (
    id                 TEXT PRIMARY KEY,
    draft_id           TEXT NOT NULL REFERENCES timesheet_drafts(id) ON DELETE CASCADE,
    quarter_index      INTEGER NOT NULL CHECK (quarter_index BETWEEN 0 AND 3),
    task_id            TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    lane_key           TEXT NOT NULL,      -- 'task:<uuid>' | 'meeting:<source_ref>'
    label              TEXT NOT NULL,
    gryzzly_project_id TEXT,
    presence_minutes   INTEGER NOT NULL,
    hours              REAL NOT NULL,
    is_pinned          INTEGER NOT NULL DEFAULT 0,
    UNIQUE (draft_id, quarter_index, lane_key)
);
CREATE INDEX idx_tqs_draft ON timesheet_quarter_shares(draft_id, quarter_index);

ALTER TABLE timesheet_drafts ADD COLUMN lanes_json TEXT;
```

A share row is a **billing decision**, so it gets a table — not a JSON blob. `lanes_json` is the
evidence view (display only, tolerant parse, absence renders as "reconstruct to see the
evidence"), matching the contract `blocks_json` / `unresolved_json` already follow.

`task_id` uses `ON DELETE SET NULL`, never `CASCADE`: deleting a task must not silently erase
declared hours. `lane_key` and `label` survive the deletion so the row stays readable.

A re-reconstruct **preserves rows with `is_pinned = 1`** and rewrites the rest.

### GraphQL

`ReconstructedDayGql` gains:

```graphql
lanes:          [LaneGql!]!        # label, gryzzlyProjectId, intervals, outsideMinutes
quarters:       [QuarterGql!]!     # index, startTime, endTime, hours, oooHours,
                                   # confidence, shares { laneKey, taskId, label,
                                   #   gryzzlyProjectId, presenceMinutes, hours, isPinned }
outsideWorkday: [OutsideWorkGql!]! # taskId, label, minutes, firstAt, lastAt
```

and loses `blocks`. Mutations: `setQuarterShare(date, quarterIndex, laneKey, hours)`,
`clearQuarterShare(date, quarterIndex, laneKey)`, `resetQuarter(date, quarterIndex)`;
`saveTimesheetLines` is removed. `validateTimesheet` / `markDayOff` are untouched.

### CLI (`aplan timesheet`)

Read-only by default, one editing verb. Plain output gains a quarter block; `--json` returns the
new payload verbatim, so `quarters` and `lanes` are machine-readable.

```
== timesheet 2026-08-10 ==  [DRAFT]  8.00h / 8.0h target

Q3  13:00-15:00                                   confidence: HIGH
    SCB-364 eActions          ████████  82 min    1.25h
    SAFT GitHub Action        ████      31 min    0.50h
    Gryzzly internal auth     ███       23 min    0.25h

hours × project:
   7.25 h  SAFT - 2026 - S2
   0.75 h  Interne
  ── total 8.00h  (✓ matches target)

⚠ 1 h 34 of evidence outside 08:00-17:00 (SAFT GitHub Action, Gryzzly internal auth)
```

### Frontend

`TimesheetTimeline.tsx` is replaced by `TimesheetLanes.tsx`: one row per lane across the full
day, quarter boundaries drawn as verticals, so concurrency is visible at a glance.
`QuarterEditor.tsx` is new — per quarter, the lanes present with their presence minutes and an
hours stepper in 15-min increments; the quarter shows `x.xx / 2.00 h` and refuses to leave the
sum wrong. Editing a share pins it; a "reset quarter" control drops the pins.
`ProjectSummarySidebar.tsx` keeps the project totals but they become **read-only** — derived,
with a link back to the quarter that produced each contribution.

## Testing

Domain (pure, table-driven, inline `#[cfg(test)]`):

- back-shadow: lone point → 45 min; two points 20 min apart → 45 + 20, never 90; clip at the
  lane's own previous point, not at another lane's
- merge, then clip to windows; `outside_minutes` accounts for exactly what was clipped
- `covered_minutes` is a union, never a sum (the 245-vs-120 case)
- apportionment: every quarter sums to its own length at the increment; pinned shares survive;
  pinned > quarter hours is rejected
- OOO reduces the quarter; a fully-OOO quarter declares nothing
- empty quarter → one unattributed share
- **regression fixture: 2026-08-10.** The real 66 entries. Asserted as properties, not as
  hand-computed hours: every one of the three tasks that ran that afternoon declares ≥ 1.00 h
  (the engine gave Gryzzly 0.29 h for the whole day); no task exceeds 4.00 h; the four quarters
  sum to 8.00 h; and the ordering by presence is preserved within each quarter.

Application (in-memory SQLite): pinned shares survive a re-reconstruct; a `Validated` day is not
clobbered; `manual` slots weigh, `worklog` slots do not; lines equal the sum of shares by
project.

API/CLI: the new payload round-trips; a draft persisted without `lanes_json` degrades to an
empty evidence view instead of failing the query.

Frontend (Vitest + RTL): lanes render one row per task; a quarter refuses a sum ≠ its length;
editing a share marks it pinned; reset clears pins.

## Out of scope

Pushing to Gryzzly, multi-day or weekly arbitration, auto-validation, changing how worklog
entries or activity slots are produced, and the `signal_project_mappings` rules (unchanged —
they still resolve commits and meetings, and worklog signals still read
`task.gryzzly_project_id`).
