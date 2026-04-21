# Task Edit & Jira Time Tracking — Design Spec

## Goal

Enable task editing from any screen via a slide-in sheet panel, display Jira card ID and time tracking on all task cards, and allow local override of planning fields (remaining hours, estimated hours, urgency, impact).

## Architecture

Three changes layered on the existing DDD architecture:

1. **Jira time tracking data** — Fetch `timeestimate`, `timespent`, `timeoriginalestimate` from Jira REST API, store as new fields on Task domain type, expose via GraphQL.
2. **Unified task card** — Single reusable `TaskCard` component used across all screens (dashboard, triage, priority matrix), showing Jira key, time tracking, and click-to-edit affordance.
3. **Edit sheet** — shadcn/ui `Sheet` (right panel) for editing local override fields. Synced fields (title, status, assignee, deadline) shown read-only for Jira tasks. Editable fields: urgency, impact, remaining hours, estimated hours, description, tags.

## Data Model Changes

### New fields on `Task` (domain type in `domain/src/types/task.rs`)

```rust
pub struct Task {
    // ... existing fields ...
    pub jira_remaining_seconds: Option<i32>,        // From Jira timeestimate (seconds)
    pub jira_original_estimate_seconds: Option<i32>, // From Jira timeoriginalestimate (seconds)
    pub jira_time_spent_seconds: Option<i32>,       // From Jira timespent (seconds)
    pub remaining_hours_override: Option<f32>,       // Local override for remaining time
    pub estimated_hours_override: Option<f32>,       // Local override for estimated time
}
```

Note: `i32` is sufficient for Jira time values (max ~68 years in seconds) and maps directly to GraphQL `Int`.

### Computed methods on `impl Task`

Add to `domain/src/types/task.rs`:

```rust
impl Task {
    /// Effective remaining hours: local override > Jira remaining > None
    pub fn effective_remaining_hours(&self) -> Option<f32> {
        self.remaining_hours_override
            .or(self.jira_remaining_seconds.map(|s| s as f32 / 3600.0))
    }

    /// Effective estimated hours: local override > Jira estimate > estimated_hours (personal tasks)
    pub fn effective_estimated_hours(&self) -> Option<f32> {
        self.estimated_hours_override
            .or(self.jira_original_estimate_seconds.map(|s| s as f32 / 3600.0))
            .or(self.estimated_hours)
    }
}
```

### Field initialization

- **New Jira-synced tasks**: Jira time fields populated from API, override fields set to `None`.
- **Personal tasks** (`create_personal_task`): All 5 new fields set to `None`. Users set `estimated_hours` directly (existing field). Override fields are only meaningful for Jira tasks.
- **Existing tasks** (migration): All 5 columns default to `NULL`.

### Override logic for personal tasks

For personal tasks (`source == Personal`), the edit sheet writes directly to `estimated_hours` (the existing field). The `estimated_hours_override` and `remaining_hours_override` fields are only used for Jira/Excel tasks where the user wants to override synced data. The frontend determines which field to write based on `task.source`.

### Migration (`migrations/sqlite/003_add_time_tracking.sql`)

```sql
ALTER TABLE tasks ADD COLUMN jira_remaining_seconds INTEGER;
ALTER TABLE tasks ADD COLUMN jira_original_estimate_seconds INTEGER;
ALTER TABLE tasks ADD COLUMN jira_time_spent_seconds INTEGER;
ALTER TABLE tasks ADD COLUMN remaining_hours_override REAL;
ALTER TABLE tasks ADD COLUMN estimated_hours_override REAL;
```

## Jira Connector Changes

### Fields to fetch

Add to Jira REST API request fields list in `infrastructure/src/connectors/jira/client.rs`:

```
"timeestimate", "timespent", "timeoriginalestimate"
```

### JiraIssueFields deserialization struct update (`infrastructure/src/connectors/jira/types.rs`)

```rust
pub struct JiraIssueFields {
    // ... existing fields ...
    pub timeestimate: Option<i32>,
    pub timespent: Option<i32>,
    pub timeoriginalestimate: Option<i32>,
}
```

### JiraTask DTO update (`application/src/services/jira_client.rs`)

```rust
pub struct JiraTask {
    // ... existing fields ...
    pub time_estimate_seconds: Option<i32>,
    pub time_spent_seconds: Option<i32>,
    pub time_original_estimate_seconds: Option<i32>,
}
```

### Mapper update (`infrastructure/src/connectors/jira/mapper.rs`)

The `map_jira_issue` function must extract time fields from `JiraIssueFields` into `JiraTask`:

```rust
time_estimate_seconds: fields.timeestimate,
time_spent_seconds: fields.timespent,
time_original_estimate_seconds: fields.timeoriginalestimate,
```

### Sync mapping (`application/src/use_cases/sync.rs`)

During Jira sync, for both new and existing tasks:
```rust
task.jira_remaining_seconds = jira_task.time_estimate_seconds;
task.jira_original_estimate_seconds = jira_task.time_original_estimate_seconds;
task.jira_time_spent_seconds = jira_task.time_spent_seconds;
// remaining_hours_override and estimated_hours_override are NOT touched by sync
```

## GraphQL API Changes

### TaskGql additional fields

```graphql
type Task {
  # ... existing fields ...
  jiraRemainingSeconds: Int
  jiraOriginalEstimateSeconds: Int
  jiraTimeSpentSeconds: Int
  remainingHoursOverride: Float
  estimatedHoursOverride: Float
  effectiveRemainingHours: Float    # Computed via Task::effective_remaining_hours()
  effectiveEstimatedHours: Float    # Computed via Task::effective_estimated_hours()
}
```

The `effectiveRemainingHours` and `effectiveEstimatedHours` are computed resolvers that call the domain methods and cast `f32` to `f64` (matching existing `estimated_hours` resolver pattern).

### UpdateTaskInput additions

```graphql
input UpdateTaskInput {
  # ... existing fields ...
  remainingHoursOverride: Float
  estimatedHoursOverride: Float
}
```

In the application layer `UpdateTaskInput`, these use `Option<Option<f32>>` to distinguish "don't change" (`None`) from "clear override" (`Some(None)`) from "set value" (`Some(Some(12.0))`). This matches the existing pattern used for `description`, `deadline`, etc.

## Frontend Components

### Unified TaskCard (`components/task/TaskCard.tsx`)

One `TaskCard` component used everywhere. Two display densities controlled by a `compact` prop:

**Full card** (dashboard, triage):
- Jira key badge (top-left), status badge (top-right)
- Title
- Time tracking row: Remaining / Logged / Estimate with progress bar
- Assignee, deadline (bottom row)

**Compact card** (priority matrix):
- Jira key badge + remaining hours (top row)
- Title
- Status badge + assignee (bottom row)

Both variants call `onEdit(taskId)` on click.

### TaskEditSheet (`components/task/TaskEditSheet.tsx`)

Uses shadcn/ui `Sheet` component (side="right").

**Synced fields** (read-only for Jira/Excel tasks, editable for personal):
- Title
- Status (shows both jiraStatus and mapped status for Jira tasks)
- Assignee
- Deadline

**Local override fields** (always editable):
- Urgency (select: Low/Medium/High/Critical)
- Impact (select: Low/Medium/High/Critical)
- Time tracking section:
  - Jira values displayed as read-only (estimate, logged, remaining)
  - Override inputs: remaining hours, estimated hours
  - For personal tasks: single "Estimated hours" input that writes to `estimated_hours`
- Description (textarea)
- Tags (multi-select)

**Behavior:**
- Opens when any TaskCard is clicked
- For personal tasks (`source === 'PERSONAL'`), all fields are editable
- Uses existing `updateTask` mutation for most fields (description, estimated_hours, override fields, tags)
- Uses `updatePriority` mutation for urgency/impact changes (preserves existing override flag logic)
- Sheet does NOT block drag-and-drop — clicking the card opens the sheet, dragging still works (drag requires >8px movement via PointerSensor, click doesn't)

### Integration points

Each page wraps tasks with click-to-edit and optionally drag-and-drop:
- **DashboardPage**: `TaskCard` components + shared `TaskEditSheet`
- **TriagePage**: `TaskCard` wrapped in `useDraggable`. Click opens sheet, drag moves between columns.
- **PriorityMatrixPage**: `TaskCard compact` wrapped in `useDraggable`. Click opens sheet, drag moves between quadrants.

### Drag vs Click disambiguation

`PointerSensor` with `activationConstraint: { distance: 8 }` (already in place for priority matrix, to be added to triage page). A click (no/minimal movement) opens the sheet. A drag (>8px movement) initiates drag-and-drop. The sheet closes automatically when a drag starts.

### Frontend GraphQL queries to update

The following hooks use inline GraphQL queries that need the new fields added:
- `hooks/use-priority-matrix.ts` — Add time fields to MatrixTask in PRIORITY_MATRIX_QUERY
- `hooks/use-triage.ts` — Add time fields to TRIAGE_TASKS_QUERY
- `hooks/use-dashboard.ts` — Add time fields to DASHBOARD_QUERY

## File Changes Summary

### Backend
- `domain/src/types/task.rs` — Add 5 new fields + `impl Task` with `effective_*` methods
- `application/src/use_cases/sync.rs` — Map Jira time fields during sync (new + existing tasks)
- `application/src/use_cases/task_management.rs` — Handle override fields in update; init new fields to `None` in `create_personal_task`
- `application/src/services/jira_client.rs` — Add time fields to `JiraTask` DTO
- `infrastructure/src/connectors/jira/client.rs` — Add time fields to API request
- `infrastructure/src/connectors/jira/types.rs` — Add time fields to `JiraIssueFields` deserialization struct
- `infrastructure/src/connectors/jira/mapper.rs` — Extract time fields in `map_jira_issue`
- `infrastructure/src/database/task_repo.rs` — Read/write 5 new columns
- `api/src/graphql/types/task.rs` — Add new fields + computed resolvers (cast `f32` to `f64`)
- `api/src/graphql/mutation.rs` — Handle override fields in `convert_update_input`
- `migrations/sqlite/003_add_time_tracking.sql` — New migration (auto-applied by `sqlx::migrate!`)

### Frontend
- `components/task/TaskCard.tsx` — Rewrite as unified component with `compact` prop
- `components/task/TaskEditSheet.tsx` — New: edit sheet using shadcn/ui Sheet
- `hooks/use-task-edit.ts` — New: hook for task edit mutations
- `hooks/use-priority-matrix.ts` — Update MatrixTask interface + query with new fields
- `hooks/use-triage.ts` — Update TriageTask interface + query with new fields
- `hooks/use-dashboard.ts` — Update DashboardTask interface + query with new fields
- `pages/DashboardPage.tsx` — Use unified TaskCard + TaskEditSheet
- `pages/TriagePage.tsx` — Use unified TaskCard inside draggable + TaskEditSheet
- `pages/PriorityMatrixPage.tsx` — Use unified TaskCard (compact) + TaskEditSheet
- `components/priority/QuadrantColumn.tsx` — Use unified TaskCard (compact) in draggable wrapper

## Out of Scope

- Writing back to Jira (remains read-only)
- Override flags for title/assignee/deadline (always synced from Jira)
- Activity tracking integration with time tracking
- Bulk editing
