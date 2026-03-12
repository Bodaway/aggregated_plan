# Unplanned Tasks Sidebar Design

**Date:** 2026-03-12
**Feature:** Left sidebar on the Dashboard showing tasks with no planned date, with full drag-and-drop support.

---

## Problem

Unplanned tasks (no `plannedStart`, no `deadline`) are silently routed to "today" in the current frontend via a fallback in `getTaskDate()`. They blend in with genuinely scheduled tasks and are hard to identify. There is no dedicated place to see the backlog of unscheduled work and assign it to a day.

---

## Goals

- Surface unplanned tasks in a dedicated left sidebar on the Dashboard.
- Allow dragging an unplanned task → day column to schedule it (`plannedStart` set).
- Allow dragging a day-column task → sidebar to unschedule it (`plannedStart` cleared; only if it has no `deadline`).
- Day → day drag is unchanged.

---

## No New Queries

The backend already returns "active unplanned" tasks in `get_daily_dashboard`: tasks with `status IN (TODO, IN_PROGRESS)`, `tracking_state = FOLLOWED`, no `plannedStart`, and no `deadline`. The frontend just needs to separate them from planned tasks instead of routing them to today.

---

## Backend Change: `UpdateTaskInput.planned_start`

### Current state

`UpdateTaskInput` in `api/src/graphql/types/task.rs`:
```rust
pub planned_start: Option<DateTime<Utc>>,
```

Conversion in `mutation.rs`:
```rust
planned_start: input.planned_start.map(Some),
```

This can produce `None` (skip) or `Some(Some(dt))` (set), but **cannot produce `Some(None)` (clear)**.

The application-layer `task_management::UpdateTaskInput` already has:
```rust
pub planned_start: Option<Option<DateTime<Utc>>>,
```
where `None` = skip, `Some(None)` = clear, `Some(Some(dt))` = set. So the application and repository layers need **no changes**.

### Fix

Change the GraphQL input field to `MaybeUndefined<DateTime<Utc>>`:

**`api/src/graphql/types/task.rs`** — `UpdateTaskInput`:
```rust
use async_graphql::MaybeUndefined;

pub planned_start: MaybeUndefined<DateTime<Utc>>,
```

**`api/src/graphql/mutation.rs`** — `convert_update_input`:
```rust
planned_start: match input.planned_start {
    MaybeUndefined::Value(dt) => Some(Some(dt)),
    MaybeUndefined::Null      => Some(None),
    MaybeUndefined::Undefined => None,
},
```

GraphQL clients send `plannedStart: null` to clear, `plannedStart: "2026-03-12T08:00:00Z"` to set, and omit the field to skip.

---

## Frontend Changes

All changes are in `frontend/src/pages/DashboardPage.tsx`.

### Layout

Replace the current full-width week grid with a two-column layout:

```
┌───────────────────────────────────────────────────────┐
│ [Unplanned sidebar ~220px] │ [5-day week grid flex-1] │
└───────────────────────────────────────────────────────┘
```

The outer wrapper becomes:
```tsx
<div className="flex gap-3">
  <UnplannedSidebar tasks={unplannedTasks} onTaskClick={setEditingTaskId} />
  <div className="flex-1 min-w-0">
    <div className="grid grid-cols-5 gap-2">...</div>
  </div>
</div>
```

### `isUnplanned` helper

```ts
function isUnplanned(t: DashboardTask): boolean {
  return !t.plannedStart && !t.deadline;
}
```

### Updated `buildTasksByDate`

Skip unplanned tasks so they no longer appear in any day column:
```ts
function buildTasksByDate(tasks: readonly DashboardTask[]): Record<string, DashboardTask[]> {
  const map: Record<string, DashboardTask[]> = {};
  for (const t of tasks) {
    if (isUnplanned(t)) continue;
    const d = getTaskDate(t);
    (map[d] ??= []).push(t);
  }
  return map;
}
```

### State additions

```ts
const [unplannedTasks, setUnplannedTasks] = useState<DashboardTask[]>([]);
const serverUnplannedRef = useRef<DashboardTask[]>([]);
```

### Updated `useEffect` (data seeding)

```ts
useEffect(() => {
  if (activeTaskId !== null || isMutatingRef.current) return;
  const allTasks = data?.tasks ?? [];
  const fresh = buildTasksByDate(allTasks);
  const freshUnplanned = allTasks.filter(isUnplanned);
  setTasksByDate(fresh);
  setUnplannedTasks(freshUnplanned);
  serverSnapshotRef.current = fresh;
  serverUnplannedRef.current = freshUnplanned;
}, [data, activeTaskId]);
```

### Updated `onDragStart`

Also search `unplannedTasks` when capturing the dragged task:
```ts
const onDragStart = useCallback(({ active }: DragStartEvent) => {
  const id = active.id as string;
  setActiveTaskId(id);
  setEditingTaskId(null);
  setCreatingForDate(null);
  const allDayTasks = Object.values(tasksByDate).flat();
  draggingTaskRef.current =
    allDayTasks.find(t => t.id === id) ??
    unplannedTasks.find(t => t.id === id) ??
    null;
}, [tasksByDate, unplannedTasks]);
```

### Updated `onDragEnd`

Three cases based on drop target and task origin:

```ts
const onDragEnd = useCallback(({ over }: DragEndEvent) => {
  const draggedTask = draggingTaskRef.current;
  setActiveTaskId(null);
  draggingTaskRef.current = null;
  if (!draggedTask || !over) return;

  const overId = over.id as string;

  // ── Case 1: dropped on unplanned sidebar ──
  if (overId === 'unplanned') {
    // Reject if task has a deadline (it belongs on that date)
    if (draggedTask.deadline) return;
    isMutatingRef.current = true;
    setTasksByDate(prev => {
      const fromDate = getTaskDate(draggedTask);
      return {
        ...prev,
        [fromDate]: (prev[fromDate] ?? []).filter(t => t.id !== draggedTask.id),
      };
    });
    setUnplannedTasks(prev => [...prev, { ...draggedTask, plannedStart: null }]);
    executeUpdate({ id: draggedTask.id, input: { plannedStart: null } })
      .then(r => { if (r.error) { restore(); } })
      .catch(restore)
      .finally(() => { isMutatingRef.current = false; });
    return;
  }

  // ── Case 2: dropped on a day column ──
  if (!overId.startsWith('day-')) return;
  const newDate = overId.replace('day-', '');
  const fromUnplanned = serverUnplannedRef.current.some(t => t.id === draggedTask.id);

  if (fromUnplanned) {
    isMutatingRef.current = true;
    setUnplannedTasks(prev => prev.filter(t => t.id !== draggedTask.id));
    const scheduled = { ...draggedTask, plannedStart: `${newDate}T08:00:00Z` };
    setTasksByDate(prev => ({
      ...prev,
      [newDate]: [...(prev[newDate] ?? []), scheduled],
    }));
    executeUpdate({ id: draggedTask.id, input: { plannedStart: `${newDate}T08:00:00Z` } })
      .then(r => { if (r.error) { restore(); } })
      .catch(restore)
      .finally(() => { isMutatingRef.current = false; });
  } else {
    // ── Case 3: day-to-day move (existing logic) ──
    const currentDate = getTaskDate(draggedTask);
    if (newDate === currentDate) return;
    isMutatingRef.current = true;
    setTasksByDate(prev => moveBetweenDays(prev, draggedTask, currentDate, newDate));
    executeUpdate({ id: draggedTask.id, input: { plannedStart: `${newDate}T08:00:00Z` } })
      .then(r => { if (r.error) setTasksByDate(serverSnapshotRef.current); })
      .catch(() => setTasksByDate(serverSnapshotRef.current))
      .finally(() => { isMutatingRef.current = false; });
  }

  function restore() {
    setTasksByDate(serverSnapshotRef.current);
    setUnplannedTasks(serverUnplannedRef.current);
  }
}, [executeUpdate]);
```

### Updated `UPDATE_TASK_MUTATION`

The existing mutation already accepts `UpdateTaskInput`. The `plannedStart` field now supports `null` (clear) in addition to a date string (set) or omission (skip). No mutation string change needed — the schema change is transparent to the client.

### `UnplannedSidebar` component

```tsx
function UnplannedSidebar({
  tasks,
  onTaskClick,
}: {
  readonly tasks: DashboardTask[];
  readonly onTaskClick: (id: string) => void;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: 'unplanned' });
  const sortedTasks = [...tasks].sort((a, b) => {
    if (b.urgency !== a.urgency) return b.urgency - a.urgency;
    return b.impact - a.impact;
  });

  return (
    <div className="flex flex-col w-52 flex-shrink-0">
      {/* Header */}
      <div className="flex items-center gap-2 px-1 mb-2">
        <span className="text-xs font-semibold text-gray-600 uppercase tracking-wider">
          Unplanned
        </span>
        <span className="text-xs font-medium text-gray-500 bg-gray-100 rounded-full px-1.5 py-0.5">
          {tasks.length}
        </span>
      </div>

      {/* Drop zone */}
      <div
        ref={setNodeRef}
        className={`flex-1 rounded-lg border-2 border-dashed transition-colors p-2 space-y-1.5 overflow-y-auto
          ${isOver ? 'border-blue-400 bg-blue-50/40' : 'border-gray-200 bg-gray-50/50'}`}
        style={{ minHeight: 120, maxHeight: 'calc(100vh - 200px)' }}
      >
        {sortedTasks.length === 0 ? (
          <p className="text-xs text-gray-400 text-center py-6">No unplanned tasks</p>
        ) : (
          sortedTasks.map(t => (
            <DraggableTaskCard key={t.id} task={t} onTaskClick={onTaskClick} />
          ))
        )}
      </div>

      {/* Hint */}
      <p className="text-xs text-gray-400 text-center mt-1.5">
        Drag to a day to schedule
      </p>
    </div>
  );
}
```

---

## Files Affected

| File | Change |
|------|--------|
| `backend/crates/api/src/graphql/types/task.rs` | `UpdateTaskInput.planned_start: MaybeUndefined<DateTime<Utc>>` |
| `backend/crates/api/src/graphql/mutation.rs` | `convert_update_input` handles `MaybeUndefined` three-way match |
| `frontend/src/pages/DashboardPage.tsx` | Layout, state, DnD logic, `UnplannedSidebar` component |

---

## Out of Scope

- Tasks with a `deadline` cannot be moved to the unplanned sidebar (they are rejected on drop — the deadline date is their anchor).
- Unplanned tasks from previous weeks are not shown (backend only returns `FOLLOWED` active tasks; others stay hidden until given a date).
- No new GraphQL fields, no migrations.
