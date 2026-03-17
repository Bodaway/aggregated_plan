# Weekly Activity Report — Design Spec

**Date:** 2026-03-17
**Location:** Activity Journal page (below existing daily log)
**Goal:** Show a weekly time tracking summary with a stacked bar chart by day and a task-level breakdown table.

---

## 1. Data Model

New types defined in the use case file `application/src/use_cases/activity_reporting.rs` (following the pattern of `DailyDashboard` in `dashboard.rs`). No new domain types — we aggregate from existing `ActivitySlot`.

### WeeklyActivitySummary

| Field          | Type                        | Description                        |
|----------------|-----------------------------|------------------------------------|
| week_start     | NaiveDate                   | Monday of the week                 |
| week_end       | NaiveDate                   | Sunday of the week                 |
| total_hours    | f64                         | Sum of all tracked hours           |
| daily_totals   | Vec\<DailyActivityTotal\>   | Always 7 entries (Mon–Sun), zero-filled for days without activity |
| task_breakdown | Vec\<TaskActivitySummary\>  | One entry per distinct task + one for unassigned |

### DailyActivityTotal

| Field       | Type      | Description               |
|-------------|-----------|---------------------------|
| date        | NaiveDate | The day                   |
| total_hours | f64       | Total tracked hours       |

### TaskActivitySummary

| Field       | Type             | Description                                  |
|-------------|------------------|----------------------------------------------|
| task_id     | Option\<TaskId\> | None for unassigned activity                 |
| task_title  | Option\<String\> | Task title (None for unassigned)             |
| source_id   | Option\<String\> | Jira key or external ID if available         |
| total_hours | f64              | Total hours for this task across the week    |
| daily_hours | Vec\<f64\>       | Hours per day, indexed 0=Mon through 6=Sun   |

---

## 2. Backend Changes

### 2.1 Repository

Add to `ActivitySlotRepository` trait (application layer):

```rust
async fn find_by_user_and_date_range(
    &self,
    user_id: UserId,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Vec<ActivitySlot>, RepositoryError>;
```

SQLite implementation: `SELECT * FROM activity_slots WHERE user_id = ? AND date >= ? AND date <= ? AND end_time IS NOT NULL ORDER BY date, start_time`.

Only completed slots (with `end_time`) are included in the report.

### 2.2 Use Case

New file: `application/src/use_cases/activity_reporting.rs`

Function: `get_weekly_activity_summary(activity_repo, task_repo, user_id, week_start) -> Result<WeeklyActivitySummary>`

Logic:
1. Compute `week_end = week_start + 6 days`. Validate `week_start` is a Monday.
2. Fetch completed slots for the range via `find_by_user_and_date_range`.
3. Group slots by `task_id` (None key for unassigned).
4. For each group, compute `total_hours` and `daily_hours[0..7]` by mapping `slot.date` to day index.
5. Look up task details (title, source_id) via `task_repo.find_by_id()` for each distinct task_id.
6. Compute `daily_totals` — always 7 entries (Mon–Sun), zero-filled for days without activity.
7. Sort `task_breakdown` by `total_hours` descending.
8. Return assembled `WeeklyActivitySummary`.

**Empty week:** When no slots exist for the range, return `total_hours: 0.0`, 7 zero-valued `daily_totals`, and empty `task_breakdown`. Frontend shows an empty state message ("No activity tracked this week").

Duration computation: `(end_time - start_time).num_minutes() as f64 / 60.0` (consistent with existing `duration_hours` field on `ActivitySlotGql`).

### 2.3 GraphQL

New query on `QueryRoot`:

```graphql
weeklyActivitySummary(weekStart: NaiveDate!): WeeklyActivitySummaryGql!
```

New types:

```graphql
type WeeklyActivitySummaryGql {
  weekStart: NaiveDate!
  weekEnd: NaiveDate!
  totalHours: Float!
  dailyTotals: [DailyActivityTotalGql!]!
  taskBreakdown: [TaskActivitySummaryGql!]!
}

type DailyActivityTotalGql {
  date: NaiveDate!
  totalHours: Float!
}

type TaskActivitySummaryGql {
  taskId: ID
  taskTitle: String
  sourceId: String
  totalHours: Float!
  dailyHours: [Float!]!
}
```

---

## 3. Frontend Changes

### 3.1 Hook

New file: `frontend/src/hooks/use-weekly-activity.ts`

Hook: `useWeeklyActivity(weekStart: string)`

- Calls `weeklyActivitySummary` query
- Returns `{ summary, loading, error }`

### 3.2 Components

**WeeklyActivityReport** (container):
- Week navigation: `◀ Week 12 ▶` with prev/next buttons
- Auto-derives `weekStart` from the current `ActivityJournalPage` date using existing `getWeekStart()` from `date-utils.ts`
- Uses existing `getNextWeek()`/`getPrevWeek()`/`formatWeekRange()` utilities from `date-utils.ts`
- Contains the chart and table sub-components

**Stacked bar chart** (Recharts `BarChart`):
- X-axis: days of the week (Mon–Sun)
- Y-axis: hours
- Stacked segments: one `<Bar>` per task, colored from a palette
- Tooltip showing task name + hours on hover

**TaskTimeTable**:
- Columns: Task (sourceId + title), Mon, Tue, Wed, Thu, Fri, Sat, Sun, Total
- Rows: one per task, sorted by total hours descending
- Last row: "Unassigned" if any unassigned time exists
- Footer row: daily totals + week total
- Empty cells show `-` instead of `0`

### 3.3 Layout

Added as a collapsible section below the "Activity Log" section in `ActivityJournalPage`, with a section header "Weekly Summary".

---

## 4. Scope Exclusions

- No arbitrary date range picker (week-by-week navigation is in scope)
- No export/download
- No project-level grouping (task-level only)
- No comparison with estimated hours (future enhancement)
- No persistent chart color assignments (use index-based palette)
