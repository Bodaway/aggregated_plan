# Activity Edit & Create Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the ability to edit existing activity slots (task, start/end time) and create manual activity slots via a Sheet UI.

**Architecture:** Extend the existing `update_activity_slot` use case with validation and `half_day` recomputation. Add a new `create_manual_activity_slot` use case. Wire both through GraphQL mutations. Build an `ActivitySlotSheet` frontend component (matching the existing `TaskEditSheet` slide-over pattern) for both create and edit modes.

**Tech Stack:** Rust (async-graphql, chrono, uuid), React 18 (TypeScript), urql, Tailwind CSS

**Spec:** `docs/superpowers/specs/2026-03-24-activity-edit-create-design.md`

---

## File Structure

**Backend — Modify:**
- `backend/crates/domain/src/errors.rs` — add `ValidationError` variant to `DomainError`
- `backend/crates/application/src/use_cases/activity_tracking.rs` — add `create_manual_activity_slot`, add validation + `half_day` recompute to `update_activity_slot`
- `backend/crates/api/src/graphql/types/activity.rs` — extend `UpdateActivitySlotInput`, add `CreateActivitySlotInput`
- `backend/crates/api/src/graphql/mutation.rs` — update `update_activity_slot` resolver, add `create_activity_slot` resolver

**Frontend — Modify:**
- `frontend/src/hooks/use-activity.ts` — add `updateActivitySlot` and `createActivitySlot` mutations
- `frontend/src/components/activity/SlotCard.tsx` — add edit button + `onEdit` prop
- `frontend/src/pages/ActivityJournalPage.tsx` — add "+ Add Activity" button, wire sheet state

**Frontend — Create:**
- `frontend/src/components/activity/ActivitySlotSheet.tsx` — new sheet component for edit/create

---

### Task 1: Backend — Add `ValidationError` variant to `DomainError`

**Files:**
- Modify: `backend/crates/domain/src/errors.rs`

- [ ] **Step 1: Add the `ValidationError` variant**

Add a new variant to the `DomainError` enum:

```rust
#[error("Validation error: {0}")]
ValidationError(String),
```

- [ ] **Step 2: Verify it compiles**

Run: `cd backend && cargo check -p domain`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add backend/crates/domain/src/errors.rs
git commit -m "feat(domain): add ValidationError variant to DomainError"
```

---

### Task 2: Backend — Add validation and half_day recompute to `update_activity_slot`

**Files:**
- Modify: `backend/crates/application/src/use_cases/activity_tracking.rs:79-104`

- [ ] **Step 1: Write failing tests for validation and half_day recompute**

Add these tests inside the existing `#[cfg(test)] mod tests` block at the bottom of the file:

```rust
#[tokio::test]
async fn update_activity_slot_recomputes_half_day_on_start_time_change() {
    let repo = InMemoryActivitySlotRepository::new();
    // Create a morning slot at 09:00
    let morning = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
    let slot = start_activity(&repo, test_user_id(), None, morning).await.unwrap();
    assert_eq!(slot.half_day, HalfDay::Morning);

    // Update start_time to afternoon (15:00)
    let afternoon = Utc.with_ymd_and_hms(2026, 3, 9, 15, 0, 0).unwrap();
    let updated = update_activity_slot(&repo, slot.id, None, Some(afternoon), None)
        .await
        .unwrap();

    assert_eq!(updated.half_day, HalfDay::Afternoon);
    assert_eq!(updated.start_time, afternoon);
}

#[tokio::test]
async fn update_activity_slot_rejects_end_before_start() {
    let repo = InMemoryActivitySlotRepository::new();
    let start = Utc.with_ymd_and_hms(2026, 3, 9, 14, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 3, 9, 16, 0, 0).unwrap();

    let slot = start_activity(&repo, test_user_id(), None, start).await.unwrap();
    // Stop the slot so it has an end_time
    stop_activity(&repo, test_user_id(), end).await.unwrap();

    // Try to set end_time before start_time
    let bad_end = Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap();
    let result = update_activity_slot(&repo, slot.id, None, None, Some(bad_end)).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn update_activity_slot_rejects_start_after_end() {
    let repo = InMemoryActivitySlotRepository::new();
    let start = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap();

    let slot = start_activity(&repo, test_user_id(), None, start).await.unwrap();
    stop_activity(&repo, test_user_id(), end).await.unwrap();

    // Try to set start_time after the existing end_time
    let bad_start = Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap();
    let result = update_activity_slot(&repo, slot.id, None, Some(bad_start), None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn update_activity_slot_clears_task_id() {
    let repo = InMemoryActivitySlotRepository::new();
    let now = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
    let task_id = Some(Uuid::new_v4());

    let slot = start_activity(&repo, test_user_id(), task_id, now).await.unwrap();
    assert!(slot.task_id.is_some());

    // Clear task_id with Some(None)
    let updated = update_activity_slot(&repo, slot.id, Some(None), None, None)
        .await
        .unwrap();

    assert!(updated.task_id.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test -p application update_activity_slot_recomputes -- --nocapture`
Expected: FAIL — `half_day` is not recomputed (still `Morning`).

Run: `cd backend && cargo test -p application update_activity_slot_rejects -- --nocapture`
Expected: FAIL — no validation, `result.is_err()` assertions fail.

- [ ] **Step 3: Implement validation and half_day recompute**

Replace the `update_activity_slot` function in `backend/crates/application/src/use_cases/activity_tracking.rs`:

```rust
/// Update an existing activity slot.
pub async fn update_activity_slot(
    activity_repo: &dyn ActivitySlotRepository,
    slot_id: ActivitySlotId,
    task_id: Option<Option<TaskId>>,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
) -> Result<ActivitySlot, AppError> {
    let mut slot = activity_repo
        .find_by_id(slot_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ActivitySlot {}", slot_id)))?;

    if let Some(tid) = task_id {
        slot.task_id = tid;
    }
    if let Some(st) = start_time {
        slot.start_time = st;
        // Recompute half_day from new start time
        slot.half_day = half_day_of(st.hour());
    }
    if let Some(et) = end_time {
        slot.end_time = Some(et);
    }

    // Validate: end_time must be after start_time (if both are set)
    if let Some(et) = slot.end_time {
        if et <= slot.start_time {
            return Err(AppError::Domain(
                domain::errors::DomainError::ValidationError(
                    "End time must be after start time".to_string(),
                ),
            ));
        }
    }

    activity_repo.update(&slot).await?;
    Ok(slot)
}
```

Note: Requires `DomainError::ValidationError` added in Task 1. Also add `use chrono::Timelike;` to the imports at the top of this file if not already present (needed for `.hour()`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test -p application activity_tracking -- --nocapture`
Expected: All tests PASS including the 3 new ones.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/application/src/use_cases/activity_tracking.rs
git commit -m "feat(activity): add validation and half_day recompute to update_activity_slot"
```

---

### Task 3: Backend — Add `create_manual_activity_slot` use case

**Files:**
- Modify: `backend/crates/application/src/use_cases/activity_tracking.rs`

- [ ] **Step 1: Write failing tests for manual slot creation**

Add to the test module:

```rust
#[tokio::test]
async fn create_manual_activity_slot_success() {
    let repo = InMemoryActivitySlotRepository::new();
    let start = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 3, 9, 11, 30, 0).unwrap();
    let task_id = Some(Uuid::new_v4());

    let slot = create_manual_activity_slot(&repo, test_user_id(), start, end, task_id)
        .await
        .unwrap();

    assert_eq!(slot.user_id, test_user_id());
    assert_eq!(slot.task_id, task_id);
    assert_eq!(slot.start_time, start);
    assert_eq!(slot.end_time, Some(end));
    assert_eq!(slot.half_day, HalfDay::Morning);
    assert_eq!(slot.date, start.date_naive());
}

#[tokio::test]
async fn create_manual_activity_slot_afternoon() {
    let repo = InMemoryActivitySlotRepository::new();
    let start = Utc.with_ymd_and_hms(2026, 3, 9, 14, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 3, 9, 16, 0, 0).unwrap();

    let slot = create_manual_activity_slot(&repo, test_user_id(), start, end, None)
        .await
        .unwrap();

    assert_eq!(slot.half_day, HalfDay::Afternoon);
    assert!(slot.task_id.is_none());
}

#[tokio::test]
async fn create_manual_activity_slot_rejects_end_before_start() {
    let repo = InMemoryActivitySlotRepository::new();
    let start = Utc.with_ymd_and_hms(2026, 3, 9, 14, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap();

    let result = create_manual_activity_slot(&repo, test_user_id(), start, end, None).await;
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test -p application create_manual_activity_slot -- --nocapture`
Expected: FAIL — function does not exist.

- [ ] **Step 3: Implement `create_manual_activity_slot`**

Add this function above the `delete_activity_slot` function in `activity_tracking.rs`:

```rust
/// Create a manual (completed) activity slot with explicit start and end times.
pub async fn create_manual_activity_slot(
    activity_repo: &dyn ActivitySlotRepository,
    user_id: UserId,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    task_id: Option<TaskId>,
) -> Result<ActivitySlot, AppError> {
    // Validate: end_time must be after start_time
    if end_time <= start_time {
        return Err(AppError::Domain(
            domain::errors::DomainError::ValidationError(
                "End time must be after start time".to_string(),
            ),
        ));
    }

    let half_day = half_day_of(start_time.hour());
    let date = start_time.date_naive();

    let slot = ActivitySlot {
        id: Uuid::new_v4(),
        user_id,
        task_id,
        start_time,
        end_time: Some(end_time),
        half_day,
        date,
        created_at: Utc::now(),
    };

    activity_repo.save(&slot).await?;
    Ok(slot)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test -p application create_manual_activity_slot -- --nocapture`
Expected: All 3 new tests PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/application/src/use_cases/activity_tracking.rs
git commit -m "feat(activity): add create_manual_activity_slot use case"
```

---

### Task 4: Backend — Extend GraphQL inputs and mutations

**Files:**
- Modify: `backend/crates/api/src/graphql/types/activity.rs:76-80`
- Modify: `backend/crates/api/src/graphql/mutation.rs:383-417`

- [ ] **Step 1: Update `UpdateActivitySlotInput` and add `CreateActivitySlotInput`**

Replace the `UpdateActivitySlotInput` at the bottom of `backend/crates/api/src/graphql/types/activity.rs` and add the new input:

```rust
use async_graphql::MaybeUndefined;
```

Add this import at the top of the file (alongside the existing imports).

Then replace the `UpdateActivitySlotInput`:

```rust
/// Input for updating an existing activity slot.
#[derive(InputObject, Debug)]
pub struct UpdateActivitySlotInput {
    /// Change the associated task. Null clears it, undefined leaves unchanged.
    pub task_id: MaybeUndefined<ID>,
    /// Update the start time.
    pub start_time: Option<DateTime<Utc>>,
    /// Update the end time.
    pub end_time: Option<DateTime<Utc>>,
}

/// Input for creating a manual (completed) activity slot.
#[derive(InputObject, Debug)]
pub struct CreateActivitySlotInput {
    /// Start time (also determines date and half-day).
    pub start_time: DateTime<Utc>,
    /// End time (must be after start_time).
    pub end_time: DateTime<Utc>,
    /// Optional task to associate with.
    pub task_id: Option<ID>,
}
```

- [ ] **Step 2: Update the `update_activity_slot` mutation resolver**

In `backend/crates/api/src/graphql/mutation.rs`, replace the `update_activity_slot` method (lines 383-417):

```rust
    /// Update an existing activity slot.
    async fn update_activity_slot(
        &self,
        ctx: &Context<'_>,
        id: ID,
        input: UpdateActivitySlotInput,
    ) -> Result<ActivitySlotGql> {
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;
        let slot_id = Uuid::parse_str(&id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid slot ID: {}", e)))?;

        // Convert MaybeUndefined task_id:
        // Undefined => None (don't change), Null => Some(None) (clear), Value => Some(Some(id))
        let task_id = match input.task_id {
            MaybeUndefined::Value(tid) => {
                let parsed = Uuid::parse_str(&tid)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;
                Some(Some(parsed))
            }
            MaybeUndefined::Null => Some(None),
            MaybeUndefined::Undefined => None,
        };

        let slot = activity_tracking::update_activity_slot(
            activity_repo.as_ref(),
            slot_id,
            task_id,
            input.start_time,
            input.end_time,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(ActivitySlotGql(slot))
    }
```

- [ ] **Step 3: Add the `create_activity_slot` mutation**

Add this method inside the `impl MutationRoot` block, after the `delete_activity_slot` method:

```rust
    /// Create a manual activity slot with explicit start and end times.
    async fn create_activity_slot(
        &self,
        ctx: &Context<'_>,
        input: CreateActivitySlotInput,
    ) -> Result<ActivitySlotGql> {
        let user_id = ctx.data::<UserId>()?;
        let activity_repo = ctx.data::<Arc<dyn ActivitySlotRepository>>()?;

        let task_id = match input.task_id {
            Some(id) => Some(
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?,
            ),
            None => None,
        };

        let slot = activity_tracking::create_manual_activity_slot(
            activity_repo.as_ref(),
            *user_id,
            input.start_time,
            input.end_time,
            task_id,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(ActivitySlotGql(slot))
    }
```

- [ ] **Step 4: Verify it compiles**

Run: `cd backend && cargo check`
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/api/src/graphql/types/activity.rs backend/crates/api/src/graphql/mutation.rs
git commit -m "feat(activity): extend update mutation and add create_activity_slot mutation"
```

---

### Task 5: Frontend — Add mutations to `use-activity.ts` hook

**Files:**
- Modify: `frontend/src/hooks/use-activity.ts`

- [ ] **Step 1: Add the GraphQL mutation strings**

Add after the `DELETE_ACTIVITY_SLOT_MUTATION` constant (line 66):

```typescript
const UPDATE_ACTIVITY_SLOT_MUTATION = `
  mutation UpdateActivitySlot($id: ID!, $input: UpdateActivitySlotInput!) {
    updateActivitySlot(id: $id, input: $input) {
      id startTime endTime halfDay date durationMinutes task { id title }
    }
  }
`;

const CREATE_ACTIVITY_SLOT_MUTATION = `
  mutation CreateActivitySlot($input: CreateActivitySlotInput!) {
    createActivitySlot(input: $input) {
      id startTime endTime halfDay date durationMinutes task { id title }
    }
  }
`;
```

- [ ] **Step 2: Add mutation hooks and callback functions**

In the `useActivity` function body, add after the `executeDelete` line (line 109):

```typescript
const [, executeUpdate] = useMutation(UPDATE_ACTIVITY_SLOT_MUTATION);
const [, executeCreate] = useMutation(CREATE_ACTIVITY_SLOT_MUTATION);
```

Add these callback functions after the `deleteSlot` callback (before `const availableTasks`):

```typescript
const updateSlot = useCallback(
  async (id: string, input: { taskId?: string | null; startTime?: string; endTime?: string }) => {
    const res = await executeUpdate({ id, input });
    if (!res.error) {
      reexecute({ requestPolicy: 'network-only' });
    }
    return res;
  },
  [executeUpdate, reexecute]
);

const createSlot = useCallback(
  async (input: { startTime: string; endTime: string; taskId?: string | null }) => {
    const res = await executeCreate({ input });
    if (!res.error) {
      reexecute({ requestPolicy: 'network-only' });
    }
    return res;
  },
  [executeCreate, reexecute]
);
```

- [ ] **Step 3: Add to the return object**

Add `updateSlot` and `createSlot` to the return object:

```typescript
return {
  slots: result.data?.activityJournal ?? [],
  currentActivity: result.data?.currentActivity ?? null,
  availableTasks,
  loading: result.fetching,
  error: result.error ?? null,
  startActivity,
  stopActivity,
  deleteSlot,
  updateSlot,
  createSlot,
  refetch: () => reexecute({ requestPolicy: 'network-only' }),
};
```

- [ ] **Step 4: Verify frontend compiles**

Run: `cd frontend && pnpm build`
Expected: No TypeScript errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/hooks/use-activity.ts
git commit -m "feat(activity): add updateSlot and createSlot mutations to use-activity hook"
```

---

### Task 6: Frontend — Add edit button to `SlotCard`

**Files:**
- Modify: `frontend/src/components/activity/SlotCard.tsx`

- [ ] **Step 1: Add `onEdit` prop**

Update the `SlotCardProps` interface to add the `onEdit` callback:

```typescript
interface SlotCardProps {
  readonly id: string;
  readonly taskTitle: string | null;
  readonly startTime: string;
  readonly endTime: string | null;
  readonly halfDay: string;
  readonly durationMinutes: number | null;
  readonly onDelete: (id: string) => void;
  readonly onEdit?: (id: string) => void;
}
```

Update the destructured props in the component function to include `onEdit`:

```typescript
export function SlotCard({
  id,
  taskTitle,
  startTime,
  endTime,
  halfDay,
  durationMinutes,
  onDelete,
  onEdit,
}: SlotCardProps) {
```

- [ ] **Step 2: Add edit button next to delete button**

Add a pencil/edit button before the delete button (inside the button container div). Replace the delete button section (the last child div containing the button) with:

```tsx
{/* Action buttons */}
<div className="flex items-center gap-1 flex-shrink-0">
  <button
    type="button"
    onClick={() => onEdit?.(id)}
    className="p-1.5 text-gray-400 hover:text-blue-500 hover:bg-blue-50 rounded-md transition-colors"
    aria-label="Edit activity slot"
    title="Edit this activity slot"
  >
    <svg
      className="w-4 h-4"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeWidth={1.5}
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0115.75 21H5.25A2.25 2.25 0 013 18.75V8.25A2.25 2.25 0 015.25 6H10"
      />
    </svg>
  </button>
  <button
    type="button"
    onClick={() => onDelete(id)}
    className="p-1.5 text-gray-400 hover:text-red-500 hover:bg-red-50 rounded-md transition-colors"
    aria-label="Delete activity slot"
    title="Delete this activity slot"
  >
    <svg
      className="w-4 h-4"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeWidth={1.5}
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0"
      />
    </svg>
  </button>
</div>
```

- [ ] **Step 3: Verify frontend compiles**

Run: `cd frontend && pnpm build`
Expected: No errors (onEdit is optional so existing usage is unaffected).

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/activity/SlotCard.tsx
git commit -m "feat(activity): add edit button to SlotCard"
```

---

### Task 7: Frontend — Create `ActivitySlotSheet` component

**Files:**
- Create: `frontend/src/components/activity/ActivitySlotSheet.tsx`

- [ ] **Step 1: Create the sheet component**

Create `frontend/src/components/activity/ActivitySlotSheet.tsx`:

```tsx
import { useState, useEffect, useCallback } from 'react';
import type { ActivitySlot, TaskPickerItem } from '@/hooks/use-activity';

interface ActivitySlotSheetProps {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
  readonly mode: 'create' | 'edit';
  readonly slot?: ActivitySlot;
  readonly tasks: readonly TaskPickerItem[];
  readonly onSave: (data: {
    taskId?: string | null;
    startTime: string;
    endTime: string;
  }) => void;
}

function todayStr(): string {
  const d = new Date();
  return d.toISOString().slice(0, 10);
}

/** Extract HH:mm from an ISO datetime string (UTC). */
function extractTime(isoStr: string): string {
  try {
    const d = new Date(isoStr);
    if (!isNaN(d.getTime())) {
      return d.getUTCHours().toString().padStart(2, '0') + ':' + d.getUTCMinutes().toString().padStart(2, '0');
    }
  } catch {
    // fall through
  }
  return '';
}

/** Extract YYYY-MM-DD from an ISO datetime or date string. */
function extractDate(str: string): string {
  return str.slice(0, 10);
}

export function ActivitySlotSheet({
  open,
  onOpenChange,
  mode,
  slot,
  tasks,
  onSave,
}: ActivitySlotSheetProps) {
  const [date, setDate] = useState(todayStr);
  const [startTime, setStartTime] = useState('');
  const [endTime, setEndTime] = useState('');
  const [taskId, setTaskId] = useState<string>('');
  const [error, setError] = useState<string | null>(null);

  // Sync form state when slot changes (edit mode) or mode changes
  useEffect(() => {
    if (open) {
      if (mode === 'edit' && slot) {
        setDate(extractDate(slot.date));
        setStartTime(extractTime(slot.startTime));
        setEndTime(slot.endTime ? extractTime(slot.endTime) : '');
        setTaskId(slot.task?.id ?? '');
      } else {
        // Create mode
        setDate(todayStr());
        setStartTime('');
        setEndTime('');
        setTaskId('');
      }
      setError(null);
    }
  }, [open, mode, slot]);

  const handleClose = useCallback(() => {
    onOpenChange(false);
  }, [onOpenChange]);

  const handleSave = useCallback(() => {
    // Validation
    if (!startTime) {
      setError('Start time is required');
      return;
    }
    if (!endTime) {
      setError('End time is required');
      return;
    }
    if (endTime <= startTime) {
      setError('End time must be after start time');
      return;
    }

    setError(null);

    // Build ISO datetime strings from date + time
    const startIso = `${date}T${startTime}:00Z`;
    const endIso = `${date}T${endTime}:00Z`;

    onSave({
      taskId: taskId || null,
      startTime: startIso,
      endTime: endIso,
    });

    onOpenChange(false);
  }, [date, startTime, endTime, taskId, onSave, onOpenChange]);

  // Close on Escape
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') handleClose();
    };
    if (open) {
      document.addEventListener('keydown', handleKeyDown);
      return () => document.removeEventListener('keydown', handleKeyDown);
    }
  }, [open, handleClose]);

  return (
    <>
      {/* Backdrop */}
      {open && (
        <div
          className="fixed inset-0 bg-black/20 z-40 transition-opacity"
          onClick={handleClose}
        />
      )}

      {/* Sheet panel */}
      <div
        className={`fixed top-0 right-0 h-full w-full max-w-md bg-white shadow-xl z-50 transform transition-transform duration-200 ease-in-out ${
          open ? 'translate-x-0' : 'translate-x-full'
        }`}
      >
        {open && (
          <div className="flex flex-col h-full">
            {/* Header */}
            <div className="flex items-center justify-between px-5 py-4 border-b border-gray-200">
              <h2 className="text-base font-semibold text-gray-900">
                {mode === 'create' ? 'Add Activity' : 'Edit Activity'}
              </h2>
              <button
                onClick={handleClose}
                className="p-1.5 text-gray-400 hover:text-gray-600 rounded-md hover:bg-gray-100 transition-colors"
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>

            {/* Content */}
            <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
              {/* Error message */}
              {error && (
                <div className="p-2.5 bg-red-50 border border-red-200 rounded-md">
                  <p className="text-sm text-red-600">{error}</p>
                </div>
              )}

              {/* Date */}
              <div>
                <label className="block text-xs font-medium text-gray-700 mb-1">Date</label>
                <input
                  type="date"
                  value={date}
                  onChange={(e) => setDate(e.target.value)}
                  disabled={mode === 'edit'}
                  className={`w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500 ${
                    mode === 'edit' ? 'bg-gray-50 text-gray-500 cursor-not-allowed' : ''
                  }`}
                />
              </div>

              {/* Start / End time */}
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs font-medium text-gray-700 mb-1">Start Time</label>
                  <input
                    type="time"
                    value={startTime}
                    onChange={(e) => setStartTime(e.target.value)}
                    className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                  />
                </div>
                <div>
                  <label className="block text-xs font-medium text-gray-700 mb-1">End Time</label>
                  <input
                    type="time"
                    value={endTime}
                    onChange={(e) => setEndTime(e.target.value)}
                    className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                  />
                </div>
              </div>

              {/* Task selector */}
              <div>
                <label className="block text-xs font-medium text-gray-700 mb-1">Task (optional)</label>
                <select
                  value={taskId}
                  onChange={(e) => setTaskId(e.target.value)}
                  className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                >
                  <option value="">No task</option>
                  {tasks.map(t => (
                    <option key={t.id} value={t.id}>{t.title}</option>
                  ))}
                </select>
              </div>
            </div>

            {/* Footer */}
            <div className="px-5 py-3 border-t border-gray-200 flex items-center justify-end gap-2">
              <button
                onClick={handleClose}
                className="px-3 py-1.5 text-sm font-medium text-gray-700 border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleSave}
                className="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 transition-colors"
              >
                Save
              </button>
            </div>
          </div>
        )}
      </div>
    </>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/activity/ActivitySlotSheet.tsx
git commit -m "feat(activity): add ActivitySlotSheet component for edit and create"
```

---

### Task 8: Frontend — Wire everything in `ActivityJournalPage`

**Files:**
- Modify: `frontend/src/pages/ActivityJournalPage.tsx`

- [ ] **Step 1: Add state and imports**

Add import for the new sheet component at the top:

```typescript
import { ActivitySlotSheet } from '@/components/activity/ActivitySlotSheet';
```

Update the `useActivity` destructuring to include the new functions:

```typescript
const { slots, currentActivity, availableTasks, loading, error, startActivity, stopActivity, deleteSlot, updateSlot, createSlot } =
  useActivity(dateStr);
```

Add state for the sheet after the existing `useActivity` call:

```typescript
const [sheetOpen, setSheetOpen] = useState(false);
const [sheetMode, setSheetMode] = useState<'create' | 'edit'>('create');
const [editingSlot, setEditingSlot] = useState<typeof slots[number] | undefined>(undefined);
```

- [ ] **Step 2: Add handler functions**

Add after the existing `handleDelete` callback:

```typescript
const handleEdit = useCallback(
  (id: string) => {
    const slot = slots.find(s => s.id === id);
    if (slot) {
      setEditingSlot(slot);
      setSheetMode('edit');
      setSheetOpen(true);
    }
  },
  [slots]
);

const handleCreate = useCallback(() => {
  setEditingSlot(undefined);
  setSheetMode('create');
  setSheetOpen(true);
}, []);

const handleSheetSave = useCallback(
  async (data: { taskId?: string | null; startTime: string; endTime: string }) => {
    if (sheetMode === 'edit' && editingSlot) {
      await updateSlot(editingSlot.id, {
        taskId: data.taskId,
        startTime: data.startTime,
        endTime: data.endTime,
      });
    } else {
      await createSlot({
        startTime: data.startTime,
        endTime: data.endTime,
        taskId: data.taskId,
      });
    }
  },
  [sheetMode, editingSlot, updateSlot, createSlot]
);
```

- [ ] **Step 3: Add "+ Add Activity" button**

In the Activity Log section header (the `div` with "Activity Log" heading), add the button. Replace the existing header div:

```tsx
<div className="flex items-center justify-between mb-3">
  <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider">
    Activity Log
  </h3>
  <div className="flex items-center gap-2">
    <span className="text-xs text-gray-400">
      {completedSlots.length} entr{completedSlots.length !== 1 ? 'ies' : 'y'}
    </span>
    <button
      onClick={handleCreate}
      className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium text-blue-600 border border-blue-300 rounded-md hover:bg-blue-50 transition-colors"
    >
      <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
        <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
      </svg>
      Add Activity
    </button>
  </div>
</div>
```

- [ ] **Step 4: Pass `onEdit` to `SlotCard` and render `ActivitySlotSheet`**

Update the `SlotCard` rendering to pass `onEdit`:

```tsx
<SlotCard
  key={slot.id}
  id={slot.id}
  taskTitle={slot.task?.title ?? null}
  startTime={slot.startTime}
  endTime={slot.endTime}
  halfDay={slot.halfDay}
  durationMinutes={slot.durationMinutes}
  onDelete={handleDelete}
  onEdit={handleEdit}
/>
```

Add the `ActivitySlotSheet` at the end of the returned JSX (just before the closing `</div>`):

```tsx
<ActivitySlotSheet
  open={sheetOpen}
  onOpenChange={setSheetOpen}
  mode={sheetMode}
  slot={editingSlot}
  tasks={availableTasks}
  onSave={handleSheetSave}
/>
```

- [ ] **Step 5: Verify frontend compiles**

Run: `cd frontend && pnpm build`
Expected: No TypeScript errors, build succeeds.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/pages/ActivityJournalPage.tsx
git commit -m "feat(activity): wire ActivitySlotSheet for edit and create in journal page"
```

---

### Task 9: End-to-end verification

- [ ] **Step 1: Run all backend tests**

Run: `cd backend && cargo test`
Expected: All tests pass (including the new activity tracking tests).

- [ ] **Step 2: Run backend with debug logging and verify new mutations appear in schema**

Run: `cd backend && cargo run -p api` (if not already running)
Then test with curl:

```bash
curl -s http://localhost:3001/graphql -H 'Content-Type: application/json' \
  -d '{"query":"{ __schema { mutationType { fields { name } } } }"}' | grep -E 'createActivitySlot|updateActivitySlot'
```

Expected: Both `createActivitySlot` and `updateActivitySlot` appear in the schema.

- [ ] **Step 3: Test create mutation manually**

```bash
curl -s http://localhost:3001/graphql -H 'Content-Type: application/json' \
  -d '{"query":"mutation { createActivitySlot(input: { startTime: \"2026-03-24T09:00:00Z\", endTime: \"2026-03-24T11:30:00Z\" }) { id date halfDay startTime endTime durationMinutes } }"}'
```

Expected: Returns a slot with `halfDay: "AM"`, `durationMinutes: 150`.

- [ ] **Step 4: Test update mutation manually**

Use the `id` from step 3:

```bash
curl -s http://localhost:3001/graphql -H 'Content-Type: application/json' \
  -d '{"query":"mutation { updateActivitySlot(id: \"<ID>\", input: { startTime: \"2026-03-24T14:00:00Z\", endTime: \"2026-03-24T16:00:00Z\" }) { id halfDay startTime endTime durationMinutes } }"}'
```

Expected: Returns updated slot with `halfDay: "PM"`, `durationMinutes: 120`.

- [ ] **Step 5: Verify frontend UI**

Open http://localhost:3000 in browser, navigate to Activity tab:
1. Click "+ Add Activity" button → sheet opens in create mode with today's date
2. Fill in start/end time, optionally pick a task, click Save → new slot appears in the list
3. Click pencil icon on a slot card → sheet opens in edit mode with pre-filled data, date is disabled
4. Change the times, click Save → slot updates in the list

- [ ] **Step 6: Final commit (if any fixes needed)**

```bash
git add -A
git commit -m "feat(activity): complete edit and create activity feature"
```

---

### Task 10: Update specifications

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md` — document the new "create manual activity" and "edit activity" features
- Modify: `SPEC_TECHNIQUE.md` — document the new `createActivitySlot` mutation and extended `UpdateActivitySlotInput`

Per CLAUDE.md: "Whenever code changes affect documented behaviour, update SPEC_FONCTIONNELLE.md and/or SPEC_TECHNIQUE.md in the same commit."

- [ ] **Step 1: Update SPEC_FONCTIONNELLE.md**

Add entries describing:
- Users can manually create activity slots with start/end time, date, and optional task
- Users can edit existing activity slots (change task, start time, end time)
- Date is read-only when editing an existing slot

- [ ] **Step 2: Update SPEC_TECHNIQUE.md**

Add entries describing:
- New GraphQL mutation: `createActivitySlot(input: CreateActivitySlotInput!): ActivitySlotGql!`
- `CreateActivitySlotInput`: `startTime: DateTime!`, `endTime: DateTime!`, `taskId: ID`
- Extended `UpdateActivitySlotInput`: `taskId: MaybeUndefined<ID>`, `startTime: DateTime`, `endTime: DateTime`
- Validation: `endTime > startTime` enforced on both create and update

- [ ] **Step 3: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "docs: update specs for activity edit and create features"
```
