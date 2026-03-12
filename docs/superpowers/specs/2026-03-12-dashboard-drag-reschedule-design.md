# Dashboard Drag-to-Reschedule Design

**Date:** 2026-03-12
**Feature:** Drag-and-drop tasks between day columns in the dashboard week view to change their planned date.

---

## Problem

The dashboard week view displays tasks grouped by day. Users can edit a task's planned date via the `TaskEditSheet`, but there is no quick way to reschedule by dragging. Rescheduling should be as fast as moving a card from one column to another.

---

## Goals

- Drag any active (non-completed, non-cancelled) task card from one day column to another.
- Dropping on a different column updates the task's `planned_start` to that day at `08:00:00Z`.
- Unplanned tasks (no `plannedStart`, no `deadline`) can be dragged to any day to set a planned date.
- No accidental drag on click (8px activation threshold).
- Optimistic UI: task moves immediately, mutation fires in background. Reverts on error.

---

## Approach: `@dnd-kit` with `DragOverlay`

Consistent with the existing TriagePage and PriorityMatrixPage patterns.

### Components

| Component | Change |
|-----------|--------|
| `DashboardPage` | Wrap week grid in `DndContext`; manage `activeTaskId` state; render `DragOverlay`; handle `onDragEnd` |
| `DayColumn` | Wrap content in `useDroppable(id = "day-<YYYY-MM-DD>")`; apply blue ring when `isOver` |
| `DraggableTaskCard` | New thin wrapper: `useDraggable(id = task.id)`; applies `transform` style; disabled for `DONE`/`CANCELLED` tasks |

### Sensor

```ts
const sensors = useSensors(
  useSensor(PointerSensor, { activationConstraint: { distance: 8 } }),
);
```

### Drag State

```ts
const [activeTaskId, setActiveTaskId] = useState<string | null>(null);
// derived:
const activeTask = useMemo(
  () => data?.tasks.find(t => t.id === activeTaskId) ?? null,
  [data, activeTaskId],
);
```

### `onDragEnd` Logic

```
overId = "day-<YYYY-MM-DD>" or null
currentDay = getTaskDate(activeTask)   // plannedStart?.slice(0,10) || deadline || today

if overId is null OR overId === currentDay → no-op
else:
  newDate = overId.replace("day-", "")
  1. Optimistic: update tasksByDate local state (move task to new day key)
  2. Call updateTask mutation: { plannedStart: `${newDate}T08:00:00Z` }
  3. On error: revert tasksByDate to snapshot taken before optimistic update
```

### Optimistic Update

`tasksByDate` is currently derived from `data` via `useMemo`. To support optimistic updates, it will be managed as a `useState` that is seeded from `data` whenever the query result changes (using a `useEffect`). This allows instant local mutation before the server responds.

### `DragOverlay`

Renders a compact `TaskCard` (same as in day columns) at 90% opacity with a subtle drop shadow.

```tsx
<DragOverlay>
  {activeTask && (
    <TaskCard {...mapToCardProps(activeTask)} compact />
  )}
</DragOverlay>
```

### Drop Zone Visual

In `DayColumn`, when `isOver && dragging`:
- Column border: `border-blue-400`
- Column background: `bg-blue-50/60`

---

## Data Flow

```
User starts drag
  → setActiveTaskId(task.id)
  → DragOverlay renders ghost card

User hovers DayColumn
  → isOver = true → blue highlight on column

User drops
  → onDragEnd fires
  → overId !== currentDay?
      yes → snapshot tasksByDate
           → optimistic move in tasksByDate state
           → fire updateTask mutation
           → on error: restore snapshot + show toast (optional)
      no  → no-op
  → setActiveTaskId(null)

GraphQL mutation:
  updateTask(id, { plannedStart: "<newDate>T08:00:00Z" })
```

---

## Constraints

- Only tasks with `status` NOT in `[DONE, CANCELLED]` are draggable.
- Dragging a task does not change its `deadline`.
- Dragging an unplanned task (no `plannedStart`, no `deadline`) to a day sets `plannedStart`; it no longer appears in today's unplanned bucket.
- Week navigation (prev/next week) is not affected by drag state.
- The `TaskEditSheet` remains available for more precise editing.

---

## Files Affected

| File | Change |
|------|--------|
| `frontend/src/pages/DashboardPage.tsx` | `DndContext`, `DragOverlay`, `onDragEnd`, optimistic `tasksByDate` state |
| `frontend/src/components/task/TaskCard.tsx` | No changes (already supports `compact` + `onClick`) |
| `frontend/src/hooks/use-dashboard.ts` | No changes |

A `DraggableTaskCard` wrapper is inlined in `DashboardPage.tsx` (small, single-use).

---

## Out of Scope

- Drag between weeks (only within the visible week grid)
- Touch/mobile drag (PointerSensor handles pointer events including touch on most devices)
- Reordering tasks within a day column
- Drag from external sources
