# Activity Edit & Create — Design Spec

## Goal

Add the ability to **edit** existing activity slots (task, start time, end time) and **create** manual activity slots (without using the start/stop timer) in the Activity Journal tab.

## UI Approach

**Sheet / Dialog** pattern (consistent with existing `TaskEditSheet`).

### ActivitySlotSheet Component

A shadcn `Sheet` component used for both create and edit modes.

**Fields:**
- **Date** — date picker. Create mode: defaults to today. Edit mode: displayed read-only (disabled) showing the slot's date.
- **Start Time** — time input (HH:mm). Required.
- **End Time** — time input (HH:mm). Required.
- **Task** — dropdown task picker (same data source as timer: TODO/IN_PROGRESS tasks). Optional (can be cleared to "no task").

**Buttons:** Save, Cancel.

**Validation:**
- `endTime` must be after `startTime`.
- Both start and end times are required.
- Client-side validation with error messages on fields.

### Entry Points

1. **Edit button on SlotCard** — pencil icon added next to the existing delete button. Opens sheet pre-filled with slot data.
2. **"+ Add Activity" button** — placed above the activity log list section in `ActivityJournalPage`. Opens sheet with empty fields (date defaults to today).

## Backend Changes

### 1. Extend `UpdateActivitySlotInput` (GraphQL)

Current input only has `taskId`. Add optional fields:

```graphql
input UpdateActivitySlotInput {
  taskId: MaybeUndefined<ID>   # None = no change, null = clear task, Some = set task
  startTime: DateTime           # Optional — only update if provided
  endTime: DateTime             # Optional — only update if provided
}
```

Note: `taskId` changes from `Option<ID>` to `MaybeUndefined<ID>` (async-graphql's `MaybeUndefined`) to distinguish "don't change" from "clear the task".

**Mutation resolver changes required:**
- The current resolver hardcodes `None, None` for `start_time` and `end_time` when calling the use case. Must be updated to pass through the new input fields.

**Use case changes required (`update_activity_slot`):**
- Add `end_time > start_time` validation when either time is updated (check against the other existing or new value).
- When `start_time` is updated, recompute `half_day` from the new start time (not currently done).

### 2. Add `createActivitySlot` mutation

```graphql
input CreateActivitySlotInput {
  startTime: DateTime!
  endTime: DateTime!
  taskId: ID
}
```

The `date` field is **derived from `startTime`** (not a separate input) to avoid contradictions.

```graphql
type Mutation {
  createActivitySlot(input: CreateActivitySlotInput!): ActivitySlotGql!
}
```

**Use case logic:**
- Creates a new `ActivitySlot` with the provided fields.
- `date` extracted from `startTime` as `NaiveDate`.
- `half_day` auto-computed from `startTime` hour.
- `end_time` is set (slot is immediately "completed" — not an active timer).
- Validates `endTime > startTime`.

### 3. New use case function: `create_manual_activity_slot`

In `application/src/use_cases/activity_tracking.rs`:

```rust
pub async fn create_manual_activity_slot(
    repo: &dyn ActivitySlotRepository,
    user_id: UserId,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    task_id: Option<TaskId>,
) -> Result<ActivitySlot, AppError>
```

Validates `end_time > start_time`, derives `date` and `half_day` from `start_time`, creates and saves the slot.

## Frontend Changes

### 1. New component: `ActivitySlotSheet`

Location: `frontend/src/components/activity/ActivitySlotSheet.tsx`

Props:
- `open: boolean`
- `onOpenChange: (open: boolean) => void`
- `mode: 'create' | 'edit'`
- `slot?: ActivitySlot` (for edit mode, pre-fills fields)
- `onSave: (data) => void`

Uses shadcn `Sheet`, `SheetContent`, `SheetHeader`, `SheetTitle`, plus `Input`, `Button`, `Select` components.

### 2. Update `SlotCard`

Add an `onEdit` callback prop. Add a pencil icon button next to the delete button.

### 3. Update `ActivityJournalPage`

- Add "+ Add Activity" button in the activity log section header.
- Render `ActivitySlotSheet` with state for open/mode/selected slot.
- Wire create and edit save handlers.

### 4. Update `use-activity.ts` hook

Add new GraphQL mutations (neither exists in the hook currently):
- `UpdateActivitySlot` mutation — with `taskId`, `startTime`, `endTime` fields.
- `CreateActivitySlot` mutation — new.

Expose from hook:
- `updateActivitySlot(id, { taskId, startTime, endTime })`
- `createActivitySlot({ startTime, endTime, taskId })`

## Data Flow

```
User clicks Edit on SlotCard
  → ActivitySlotSheet opens (edit mode, pre-filled, date read-only)
  → User modifies fields, clicks Save
  → updateActivitySlot mutation fires
  → Backend validates, updates slot, recomputes half_day if startTime changed
  → urql cache invalidates → journal refetches
  → Sheet closes

User clicks "+ Add Activity"
  → ActivitySlotSheet opens (create mode, date=today displayed)
  → User fills start/end time and optionally selects task, clicks Save
  → createActivitySlot mutation fires (startTime/endTime include today's date)
  → Backend creates completed slot, derives date and half_day from startTime
  → urql cache invalidates → journal refetches
  → Sheet closes
```

## Out of Scope

- Editing the date of an existing slot (date is read-only in edit mode; delete + recreate if needed).
- Overlap detection between activity slots.
- Bulk create/edit operations.
- Ownership check on update (existing limitation, tracked separately).
