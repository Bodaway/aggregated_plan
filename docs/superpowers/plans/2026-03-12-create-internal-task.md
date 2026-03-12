# Create Internal Task Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `+` button to each dashboard day column that opens a slide-in panel to create a personal task pre-filled with that day's planned date.

**Architecture:** Four small changes — add `refetch` to `useDashboard`, create a `useCreateTask` hook, create a `TaskCreateSheet` component, and wire a `+` button into `DayColumn` inside `DashboardPage`. No backend changes needed.

**Tech Stack:** React 18, TypeScript (strict), urql (useMutation), Tailwind CSS, Vite/pnpm

---

## Chunk 1: Hook and Sheet

### Task 1: Export `refetch` from `useDashboard`

**Files:**
- Modify: `frontend/src/hooks/use-dashboard.ts`

- [ ] **Step 1: Update the hook**

Change line 162 from:
```ts
const [result] = useQuery<{ dailyDashboard: DailyDashboardData }>({
```
to:
```ts
const [result, reexecute] = useQuery<{ dailyDashboard: DailyDashboardData }>({
```

Add `refetch` to the return object:
```ts
return {
  data: result.data?.dailyDashboard ?? null,
  loading: result.fetching,
  error: result.error ?? null,
  refetch: () => reexecute({ requestPolicy: 'network-only' }),
};
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
cd frontend && pnpm tsc --noEmit
```
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/use-dashboard.ts
git commit -m "feat(frontend): export refetch from useDashboard"
```

---

### Task 2: Create `useCreateTask` hook

**Files:**
- Create: `frontend/src/hooks/use-create-task.ts`

- [ ] **Step 1: Create the hook**

```ts
// frontend/src/hooks/use-create-task.ts
import { useMutation } from 'urql';

const CREATE_TASK_MUTATION = `
  mutation CreateInternalTask($input: CreateTaskInput!) {
    createTask(input: $input) {
      id
      title
      plannedStart
      status
      urgency
      impact
      quadrant
    }
  }
`;

export interface NewTaskInput {
  title: string;
  plannedStart?: string;   // ISO 8601 e.g. "2026-03-12T08:00:00Z"
  estimatedHours?: number;
  urgency?: string;        // "LOW" | "MEDIUM" | "HIGH" | "CRITICAL"
  impact?: string;
  description?: string;
}

export function useCreateTask() {
  const [result, execute] = useMutation(CREATE_TASK_MUTATION);

  const createTask = (input: NewTaskInput) =>
    execute({ input });

  return {
    createTask,
    loading: result.fetching,
    error: result.error ?? null,
  };
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
cd frontend && pnpm tsc --noEmit
```
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/use-create-task.ts
git commit -m "feat(frontend): add useCreateTask hook"
```

---

### Task 3: Create `TaskCreateSheet` component

**Files:**
- Create: `frontend/src/components/task/TaskCreateSheet.tsx`

- [ ] **Step 1: Create the component**

```tsx
// frontend/src/components/task/TaskCreateSheet.tsx
import { useState, useEffect, useCallback } from 'react';
import { useCreateTask } from '@/hooks/use-create-task';

export interface TaskCreateSheetProps {
  plannedDate: string | null; // "YYYY-MM-DD"; null = closed
  onClose: () => void;
  onCreated: () => void;
}

const URGENCY_OPTIONS = [
  { value: 'LOW', label: 'Low' },
  { value: 'MEDIUM', label: 'Medium' },
  { value: 'HIGH', label: 'High' },
  { value: 'CRITICAL', label: 'Critical' },
] as const;

const IMPACT_OPTIONS = [
  { value: 'LOW', label: 'Low' },
  { value: 'MEDIUM', label: 'Medium' },
  { value: 'HIGH', label: 'High' },
  { value: 'CRITICAL', label: 'Critical' },
] as const;

export function TaskCreateSheet({ plannedDate, onClose, onCreated }: TaskCreateSheetProps) {
  const isOpen = plannedDate !== null;
  const { createTask, loading, error } = useCreateTask();

  const [title, setTitle] = useState('');
  const [estimatedHours, setEstimatedHours] = useState('');
  const [urgency, setUrgency] = useState('MEDIUM');
  const [impact, setImpact] = useState('MEDIUM');
  const [description, setDescription] = useState('');

  // Reset form when sheet opens
  useEffect(() => {
    if (isOpen) {
      setTitle('');
      setEstimatedHours('');
      setUrgency('MEDIUM');
      setImpact('MEDIUM');
      setDescription('');
    }
  }, [isOpen]);

  // Close on Escape
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    if (isOpen) {
      document.addEventListener('keydown', handleKeyDown);
      return () => document.removeEventListener('keydown', handleKeyDown);
    }
  }, [isOpen, onClose]);

  const handleSave = useCallback(async () => {
    if (!title.trim() || loading || !plannedDate) return;

    const result = await createTask({
      title: title.trim(),
      plannedStart: `${plannedDate}T08:00:00Z`,
      estimatedHours: estimatedHours ? parseFloat(estimatedHours) : undefined,
      urgency,
      impact,
      description: description.trim() || undefined,
    });

    if (!result.error) {
      onCreated();
      onClose();
    }
  }, [title, estimatedHours, urgency, impact, description, plannedDate, loading, createTask, onCreated, onClose]);

  return (
    <>
      {/* Backdrop */}
      {isOpen && (
        <div
          className="fixed inset-0 bg-black/20 z-40 transition-opacity"
          onClick={onClose}
        />
      )}

      {/* Sheet panel */}
      <div
        className={`fixed top-0 right-0 h-full w-full max-w-md bg-white shadow-xl z-50 transform transition-transform duration-200 ease-in-out ${
          isOpen ? 'translate-x-0' : 'translate-x-full'
        }`}
      >
        {isOpen && (
          <div className="flex flex-col h-full">
            {/* Header */}
            <div className="flex items-center justify-between px-5 py-4 border-b border-gray-200">
              <h2 className="text-base font-semibold text-gray-900">New Task</h2>
              <button
                onClick={onClose}
                className="p-1.5 text-gray-400 hover:text-gray-600 rounded-md hover:bg-gray-100 transition-colors"
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>

            {/* Content */}
            <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
              {/* Title */}
              <div>
                <label className="block text-xs font-medium text-gray-700 mb-1">
                  Title <span className="text-red-500">*</span>
                </label>
                <input
                  type="text"
                  value={title}
                  onChange={e => setTitle(e.target.value)}
                  autoFocus
                  placeholder="Task title..."
                  className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                />
              </div>

              {/* Planned date (read-only display) */}
              {plannedDate && (
                <div className="flex items-center gap-2 text-sm text-gray-500">
                  <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M6.75 3v2.25M17.25 3v2.25M3 18.75V7.5a2.25 2.25 0 012.25-2.25h13.5A2.25 2.25 0 0121 7.5v11.25m-18 0A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75m-18 0v-7.5A2.25 2.25 0 015.25 9h13.5A2.25 2.25 0 0121 11.25v7.5" />
                  </svg>
                  <span>Planned for <strong>{plannedDate}</strong></span>
                </div>
              )}

              {/* Priority */}
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs font-medium text-gray-700 mb-1">Urgency</label>
                  <select
                    value={urgency}
                    onChange={e => setUrgency(e.target.value)}
                    className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                  >
                    {URGENCY_OPTIONS.map(o => (
                      <option key={o.value} value={o.value}>{o.label}</option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="block text-xs font-medium text-gray-700 mb-1">Impact</label>
                  <select
                    value={impact}
                    onChange={e => setImpact(e.target.value)}
                    className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                  >
                    {IMPACT_OPTIONS.map(o => (
                      <option key={o.value} value={o.value}>{o.label}</option>
                    ))}
                  </select>
                </div>
              </div>

              {/* Estimated hours */}
              <div>
                <label className="block text-xs font-medium text-gray-700 mb-1">Estimated hours</label>
                <input
                  type="number"
                  step="0.5"
                  min="0"
                  value={estimatedHours}
                  onChange={e => setEstimatedHours(e.target.value)}
                  placeholder="e.g. 2"
                  className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                />
              </div>

              {/* Description */}
              <div>
                <label className="block text-xs font-medium text-gray-700 mb-1">Description</label>
                <textarea
                  value={description}
                  onChange={e => setDescription(e.target.value)}
                  rows={3}
                  placeholder="Optional description..."
                  className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500 resize-none"
                />
              </div>

              {/* Error */}
              {error && (
                <p className="text-sm text-red-600 bg-red-50 rounded-md px-3 py-2">
                  Failed to create task: {error.message}
                </p>
              )}
            </div>

            {/* Footer */}
            <div className="px-5 py-3 border-t border-gray-200 flex items-center justify-end gap-2">
              <button
                onClick={onClose}
                className="px-3 py-1.5 text-sm font-medium text-gray-700 border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleSave}
                disabled={!title.trim() || loading}
                className="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {loading ? 'Creating...' : 'Create Task'}
              </button>
            </div>
          </div>
        )}
      </div>
    </>
  );
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
cd frontend && pnpm tsc --noEmit
```
Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/task/TaskCreateSheet.tsx
git commit -m "feat(frontend): add TaskCreateSheet component"
```

---

### Task 4: Wire `+` button and `TaskCreateSheet` into `DashboardPage`

**Files:**
- Modify: `frontend/src/pages/DashboardPage.tsx`

- [ ] **Step 1: Add import and update `useDashboard` destructuring**

Add to the existing import block at the top of the file:
```ts
import { TaskCreateSheet } from '@/components/task/TaskCreateSheet';
```

Find the existing line (around line 274):
```ts
const { data, loading, error } = useDashboard(dateStr);
```
Replace it with:
```ts
const { data, loading, error, refetch } = useDashboard(dateStr);
```

- [ ] **Step 2: Add `creatingForDate` state and `onAddTask` to `DayColumnProps`**

Add to the `DayColumnProps` interface (inline in the file):
```ts
readonly onAddTask: () => void;
```

Add to the `DashboardPage` component body (near `editingTaskId`):
```ts
const [creatingForDate, setCreatingForDate] = useState<string | null>(null);
```

- [ ] **Step 3: Add `+` button to `DayColumn` header**

In the `DayColumn` function signature, add `onAddTask`:
```ts
function DayColumn({ date, tasks, meetings, onTaskClick, isDragging, onAddTask }: DayColumnProps)
```

In the day header `div`, replace the existing inner flex row:
```tsx
{/* OLD — replace this block: */}
<div className="flex items-center justify-between">
  <span className={`text-xs font-semibold uppercase tracking-wider ${today ? 'text-blue-700' : 'text-gray-600'}`}>
    {formatDayShort(date)}
  </span>
  <span className={`text-xs font-medium ${overloaded ? 'text-red-600' : 'text-gray-500'}`}>
    {formatHoursCompact(totalHours)}/{DAILY_CAPACITY_HOURS}h
  </span>
</div>
```
With:
```tsx
<div className="flex items-center justify-between">
  <span className={`text-xs font-semibold uppercase tracking-wider ${today ? 'text-blue-700' : 'text-gray-600'}`}>
    {formatDayShort(date)}
  </span>
  <div className="flex items-center gap-1.5">
    <span className={`text-xs font-medium ${overloaded ? 'text-red-600' : 'text-gray-500'}`}>
      {formatHoursCompact(totalHours)}/{DAILY_CAPACITY_HOURS}h
    </span>
    <button
      onClick={e => { e.stopPropagation(); onAddTask(); }}
      className="p-0.5 rounded hover:bg-gray-200 text-gray-400 hover:text-gray-700 transition-colors"
      title="Add task"
    >
      <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
        <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
      </svg>
    </button>
  </div>
</div>
```

- [ ] **Step 4: Pass `onAddTask` from `DashboardPage` to each `DayColumn`**

In the week grid map, update the `DayColumn` usage:
```tsx
<DayColumn
  key={dayStr}
  date={day}
  tasks={tasksByDate[dayStr] ?? []}
  meetings={meetingsByDate[dayStr] ?? []}
  onTaskClick={setEditingTaskId}
  isDragging={activeTaskId !== null}
  onAddTask={() => setCreatingForDate(dayStr)}
/>
```

- [ ] **Step 5: Render `TaskCreateSheet` below `TaskEditSheet`**

Add after `<TaskEditSheet ...>`:
```tsx
<TaskCreateSheet
  plannedDate={creatingForDate}
  onClose={() => setCreatingForDate(null)}
  onCreated={() => refetch()}
/>
```

- [ ] **Step 6: Build and verify**

```bash
cd frontend && pnpm build
```
Expected: no TypeScript errors, build succeeds

- [ ] **Step 7: Commit**

```bash
git add frontend/src/pages/DashboardPage.tsx
git commit -m "feat(frontend): add create task button to day columns in dashboard"
```
