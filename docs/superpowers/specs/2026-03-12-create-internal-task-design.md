# Create Internal Task Design

**Date:** 2026-03-12
**Feature:** Allow creating personal tasks directly from the dashboard week view.

---

## Problem

Tasks from Jira/Excel are synced automatically, but there is no way to create a personal (internal) task from the UI. Users need a quick way to add a task to a specific day in the week view.

---

## Entry Point

Each `DayColumn` header gets a small `+` icon button (right side, next to the day label). Clicking it opens `TaskCreateSheet` with that day's date pre-filled as `plannedStart`.

---

## `TaskCreateSheet` Component

**File:** `frontend/src/components/task/TaskCreateSheet.tsx`

Same visual pattern as `TaskEditSheet` (slide-in panel from the right, backdrop overlay, Escape closes). Not shared with `TaskEditSheet` — create and edit have different shapes.

**Fields:**

| Field | Required | Default |
|-------|----------|---------|
| Title | yes | — |
| Estimated hours | no | — |
| Urgency | no | MEDIUM |
| Impact | no | MEDIUM |
| Description | no | — |

`plannedStart` is pre-filled from the column date (`<date>T08:00:00Z`) and not shown in the form.

**Behaviour:**
- "Save" button disabled while title is empty or while mutation is in flight (`loading`)
- On submit: calls `createTask` mutation, then closes sheet and triggers dashboard refetch
- On error: show inline error message, keep sheet open

**Props:**
```ts
interface TaskCreateSheetProps {
  plannedDate: string | null; // "YYYY-MM-DD", null = closed
  onClose: () => void;
  onCreated: () => void;      // triggers dashboard refetch
}
```

---

## `useCreateTask` Hook

**File:** `frontend/src/hooks/use-create-task.ts`

Wraps the `createTask` GraphQL mutation.

```ts
const CREATE_TASK_MUTATION = `
  mutation CreateTask($input: CreateTaskInput!) {
    createTask(input: $input) {
      id title plannedStart
    }
  }
`;

export function useCreateTask() {
  const [result, execute] = useMutation(CREATE_TASK_MUTATION);
  return {
    createTask: (input: CreateTaskInput) => execute({ input }),
    loading: result.fetching,
    error: result.error ?? null,
  };
}

interface CreateTaskInput {
  title: string;
  plannedStart?: string;
  estimatedHours?: number;
  urgency?: string;
  impact?: string;
  description?: string;
}
```

---

## `DashboardPage` Changes

Note: `DayColumn` is defined inline within `DashboardPage.tsx`, not in a separate file.

1. Add `creatingForDate: string | null` state (null = closed).
2. Add `onAddTask: () => void` to the inline `DayColumnProps` interface; render `+` icon button in the day header.
3. Clicking `+` sets `creatingForDate` to that column's `dayStr`.
4. Render `<TaskCreateSheet plannedDate={creatingForDate} onClose={...} onCreated={refetchDashboard} />`.
5. `onCreated` calls `refetchDashboard()` — from the new `refetch` export of `useDashboard`.

---

## `useDashboard` Change

The hook currently destructures only the first element: `const [result] = useQuery(...)`. Change to `const [result, reexecute] = useQuery(...)` to capture the reexecute function, then export `refetch`:

```ts
const [result, reexecute] = useQuery(...);
return {
  ...
  refetch: () => reexecute({ requestPolicy: 'network-only' }),
};
```

---

## GraphQL Mutation Input

The `createTask` backend mutation already accepts `CreateTaskInput` with all needed fields. Source defaults to `PERSONAL` server-side when not specified. `trackingState` is set to `FOLLOWED` for all personal tasks created via `createTask` (confirmed in `task_management.rs`). Tasks will therefore appear in the dashboard immediately after creation without any additional step.

The `plannedStart` field maps to `DateTime<Utc>` on the backend. `async-graphql` deserialises this from an ISO 8601 string (e.g. `"2026-03-12T08:00:00Z"`), which is exactly the format constructed in the sheet.

---

## Files Affected

| File | Change |
|------|--------|
| `frontend/src/components/task/TaskCreateSheet.tsx` | New |
| `frontend/src/hooks/use-create-task.ts` | New |
| `frontend/src/pages/DashboardPage.tsx` | Add `+` button, `creatingForDate` state, `TaskCreateSheet` |
| `frontend/src/hooks/use-dashboard.ts` | Export `refetch` function |
