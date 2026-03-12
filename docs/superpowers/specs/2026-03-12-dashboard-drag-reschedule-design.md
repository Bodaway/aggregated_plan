# Dashboard Drag-to-Reschedule Design

**Date:** 2026-03-12
**Feature:** Drag-and-drop tasks between day columns in the dashboard week view to change their planned date.

---

## Problem

The dashboard week view displays tasks grouped by day. Users can edit a task's planned date via the `TaskEditSheet`, but there is no quick way to reschedule by dragging. Rescheduling should be as fast as moving a card from one column to another.

---

## Goals

- Drag any active (non-completed, non-cancelled) task card from one day column to another.
- Dropping on a different column updates the task's `planned_start` to that day at `08:00:00Z` (UTC — consistent with how the rest of the codebase stores and slices dates; safe for all negative UTC offsets and positive offsets up to +8; UTC+9 and beyond risk a one-day skew which is out of scope).
- Unplanned tasks (no `plannedStart`, no `deadline`) can be dragged to any day to set a planned date.
- No accidental drag on click (8px activation threshold).
- Optimistic UI: task moves immediately, mutation fires in background. Reverts on error.

---

## Approach: `@dnd-kit` with `DragOverlay`

Consistent with existing TriagePage and PriorityMatrixPage patterns.

---

## Helper Functions (inlined in `DashboardPage.tsx`)

Defined at module level, before the component:

```ts
/** Group a flat task list into a map keyed by planned date (YYYY-MM-DD). */
function buildTasksByDate(tasks: DashboardTask[]): Record<string, DashboardTask[]> {
  const map: Record<string, DashboardTask[]> = {};
  for (const t of tasks) {
    const d = getTaskDate(t); // uses plannedStart.slice(0,10) || deadline || today
    (map[d] ??= []).push(t);
  }
  return map;
}

/**
 * Move a task from one date bucket to another, updating its plannedStart field
 * so getTaskDate() routes it correctly in subsequent renders.
 */
function moveBetweenDays(
  prev: Record<string, DashboardTask[]>,
  task: DashboardTask,
  fromDate: string,  // "YYYY-MM-DD"
  toDate: string,    // "YYYY-MM-DD"
): Record<string, DashboardTask[]> {
  const updated: DashboardTask = { ...task, plannedStart: `${toDate}T08:00:00Z` };
  return {
    ...prev,
    [fromDate]: (prev[fromDate] ?? []).filter(t => t.id !== task.id),
    [toDate]: [...(prev[toDate] ?? []), updated],
  };
}
```

Key point: `moveBetweenDays` replaces `task.plannedStart` on the cloned task object. This ensures `getTaskDate()` returns the new date bucket on the next render cycle, preventing the task from disappearing and reappearing before the server confirms.

---

## Components

| Component | Change |
|-----------|--------|
| `DashboardPage` | `DndContext`, `DragOverlay`, drag handlers, `tasksByDate` state, `serverSnapshotRef`, `isMutatingRef` |
| `DayColumn` | Add `isDragging: boolean` prop; `useDroppable`; conditional ring/bg when `isOver && isDragging` |
| `DraggableTaskCard` | Thin wrapper inlined in `DashboardPage`: `useDraggable`; disabled for `DONE`/`CANCELLED` |

### Updated `DayColumnProps` Interface

```ts
interface DayColumnProps {
  readonly date: Date;
  readonly tasks: DashboardTask[];
  readonly meetings: DashboardMeeting[];
  readonly onTaskClick: (id: string) => void;
  readonly isDragging: boolean;  // NEW: true when any drag is in progress
}
```

---

## Sensor & Collision Detection

```ts
const sensors = useSensors(
  useSensor(PointerSensor, { activationConstraint: { distance: 8 } }),
);
// pointerWithin: only activates a column when the pointer is physically inside it.
// Safer than closestCenter for narrow adjacent columns.
<DndContext collisionDetection={pointerWithin} ...>
```

---

## State

```ts
// Drag state
const [activeTaskId, setActiveTaskId] = useState<string | null>(null);

// Frozen snapshot of the task that started dragging (sourced from tasksByDate,
// not data.tasks, so it is immune to mid-drag server refetches).
const draggingTaskRef = useRef<DashboardTask | null>(null);

// Optimistic display state
const [tasksByDate, setTasksByDate] = useState<Record<string, DashboardTask[]>>({});

// Last server-confirmed state — always reverts here on error
const serverSnapshotRef = useRef<Record<string, DashboardTask[]>>({});

// Guard: prevents seeding effect from overwriting optimistic state mid-mutation
const isMutatingRef = useRef(false);
```

### Prerequisite: `useDashboard` must use `cache-and-network`

The optimistic guard only has value when background refetches occur. Change `useDashboard` to pass `requestPolicy: 'cache-and-network'` to `useQuery`. Add to Files Affected.

### Seeding Effect

```ts
useEffect(() => {
  // Only reseed when no drag is active and no mutation is in flight.
  if (activeTaskId !== null || isMutatingRef.current) return;
  const fresh = buildTasksByDate(data?.tasks ?? []);
  setTasksByDate(fresh);
  serverSnapshotRef.current = fresh;
}, [data, activeTaskId]);
```

### Week Total Hours (from optimistic state)

```ts
const weekTotalHours = useMemo(
  () => Object.values(tasksByDate).flat().reduce((sum, t) => sum + getTaskHours(t), 0),
  [tasksByDate],
);
```

This replaces the current `data.tasks`-based computation so the overload indicator stays consistent with column content after an optimistic move.

---

## Drag Handlers

```ts
const onDragStart = useCallback(({ active }: DragStartEvent) => {
  const id = active.id as string;
  setActiveTaskId(id);
  setEditingTaskId(null); // close TaskEditSheet (matches TriagePage/PriorityMatrixPage)

  // Freeze the task object at drag-start from the current optimistic state.
  // This makes draggingTaskRef immune to data refetches mid-drag.
  const allTasks = Object.values(tasksByDate).flat();
  draggingTaskRef.current = allTasks.find(t => t.id === id) ?? null;
}, [tasksByDate]);

const onDragCancel = useCallback(() => {
  setActiveTaskId(null);
  draggingTaskRef.current = null;
}, []);

const onDragEnd = useCallback(({ over }: DragEndEvent) => {
  // Capture before clearing state
  const draggedTask = draggingTaskRef.current;
  setActiveTaskId(null);
  draggingTaskRef.current = null;

  if (!draggedTask || !over) return;

  const newDate = (over.id as string).replace('day-', ''); // "YYYY-MM-DD"
  const currentDate = getTaskDate(draggedTask);            // "YYYY-MM-DD"
  if (newDate === currentDate) return; // dropped on same column → no-op

  // Optimistic move
  isMutatingRef.current = true;
  setTasksByDate(prev => moveBetweenDays(prev, draggedTask, currentDate, newDate));

  // Mutation — use try/finally to guarantee isMutatingRef is always cleared
  executeUpdate({ id: draggedTask.id, input: { plannedStart: `${newDate}T08:00:00Z` } })
    .then(result => {
      if (result.error) {
        setTasksByDate(serverSnapshotRef.current); // revert on GraphQL error
      }
      // On success: next data refetch re-seeds state via the guarded effect
    })
    .catch(() => {
      setTasksByDate(serverSnapshotRef.current); // revert on network/unexpected error
    })
    .finally(() => {
      isMutatingRef.current = false;
    });
}, [executeUpdate]);
```

Note: `isMutatingRef.current = false` is set in `finally` (after both `.then` and `.catch`), so it is always cleared regardless of how the promise settles.

---

## `DraggableTaskCard` (inlined)

```tsx
function DraggableTaskCard({ task, onTaskClick }: { task: DashboardTask; onTaskClick: (id: string) => void }) {
  const disabled = task.status === 'DONE' || task.status === 'CANCELLED';
  const { attributes, listeners, setNodeRef, transform } = useDraggable({
    id: task.id,
    disabled,
  });
  const style = transform ? { transform: `translate(${transform.x}px, ${transform.y}px)` } : undefined;

  return (
    <div ref={setNodeRef} style={style} {...listeners} {...attributes}>
      <TaskCard
        id={task.id} title={task.title} source={task.source ?? 'PERSONAL'}
        sourceId={task.sourceId} status={task.status} jiraStatus={task.jiraStatus}
        urgency={task.urgency} impact={task.impact} quadrant={task.quadrant}
        deadline={task.deadline} effectiveRemainingHours={task.effectiveRemainingHours}
        effectiveEstimatedHours={task.effectiveEstimatedHours}
        jiraTimeSpentSeconds={task.jiraTimeSpentSeconds}
        compact onClick={disabled ? undefined : () => onTaskClick(task.id)}
      />
    </div>
  );
}
```

---

## `DragOverlay`

`activeTask` for the overlay is sourced from `draggingTaskRef.current` (frozen at drag start), not from `data?.tasks`. This prevents the ghost from vanishing if `data` updates during the drag.

```tsx
<DragOverlay dropAnimation={null}>
  {activeTaskId && draggingTaskRef.current && (
    <div style={{ opacity: 0.9 }}>
      <TaskCard
        id={draggingTaskRef.current.id}
        title={draggingTaskRef.current.title}
        source={draggingTaskRef.current.source ?? 'PERSONAL'}
        sourceId={draggingTaskRef.current.sourceId}
        status={draggingTaskRef.current.status}
        jiraStatus={draggingTaskRef.current.jiraStatus}
        urgency={draggingTaskRef.current.urgency}
        impact={draggingTaskRef.current.impact}
        quadrant={draggingTaskRef.current.quadrant}
        deadline={draggingTaskRef.current.deadline}
        effectiveRemainingHours={draggingTaskRef.current.effectiveRemainingHours}
        effectiveEstimatedHours={draggingTaskRef.current.effectiveEstimatedHours}
        jiraTimeSpentSeconds={draggingTaskRef.current.jiraTimeSpentSeconds}
        compact
      />
    </div>
  )}
</DragOverlay>
```

---

## Data Flow

```
User starts drag
  → onDragStart: setActiveTaskId, freeze draggingTaskRef from tasksByDate, close sheet

User hovers DayColumn
  → isOver=true → blue ring + bg (when isDragging=true)

User drops on different column
  → onDragEnd: capture draggedTask from ref, clear activeTaskId and ref
  → newDate !== currentDate
  → isMutatingRef = true
  → optimistic: moveBetweenDays in tasksByDate (task.plannedStart updated in clone)
  → fire mutation
  → .then: on GraphQL error → restore serverSnapshotRef
  → .catch: on network error → restore serverSnapshotRef
  → .finally: isMutatingRef = false

User drops on same column or outside all columns
  → no-op (newDate === currentDate, or over is null)

User presses Escape mid-drag
  → onDragCancel: clear activeTaskId + draggingTaskRef
```

---

## Files Affected

| File | Change |
|------|--------|
| `frontend/src/pages/DashboardPage.tsx` | Full rewrite of drag wiring; `DraggableTaskCard`; helpers; optimistic state |
| `frontend/src/hooks/use-dashboard.ts` | Add `requestPolicy: 'cache-and-network'` to `useQuery` |
| `frontend/src/components/task/TaskCard.tsx` | No changes |

---

## Out of Scope

- Drag between weeks
- Reordering tasks within a day column
- Drag from external sources
