# Unplanned Tasks Sidebar Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a left sidebar to the Dashboard that surfaces unplanned tasks (no `plannedStart`, no `deadline`) and supports bidirectional drag-and-drop to schedule/unschedule them.

**Architecture:** Backend adds `MaybeUndefined<DateTime<Utc>>` to `UpdateTaskInput.planned_start` so clients can send `null` to clear the field. Frontend splits the flat task list into planned (day columns) and unplanned (sidebar), adds an `UnplannedSidebar` drop zone, and rewires `onDragEnd` for the three resulting DnD cases.

**Tech Stack:** Rust / async-graphql 7 (MaybeUndefined), React 18 / TypeScript / @dnd-kit / urql

**Spec:** `docs/superpowers/specs/2026-03-12-unplanned-sidebar-design.md`

---

## Chunk 1: Backend — MaybeUndefined planned_start

### Task 1: UpdateTaskInput — use MaybeUndefined for planned_start

**Files:**
- Modify: `backend/crates/api/src/graphql/types/task.rs:174`
- Modify: `backend/crates/api/src/graphql/mutation.rs:615`

- [ ] **Step 1: Open `backend/crates/api/src/graphql/types/task.rs` and update `UpdateTaskInput`**

  Find the struct `UpdateTaskInput` (around line 168). Change line 174 from:
  ```rust
  pub planned_start: Option<DateTime<Utc>>,
  ```
  to:
  ```rust
  #[graphql(default)]
  pub planned_start: MaybeUndefined<DateTime<Utc>>,
  ```

  Ensure `MaybeUndefined` is imported. The file already imports `async_graphql::*` (wildcard import) so `MaybeUndefined` is available without any new use statement.

- [ ] **Step 2: Update `convert_update_input` in `backend/crates/api/src/graphql/mutation.rs`**

  Find line 615:
  ```rust
  planned_start: input.planned_start.map(Some),
  ```
  Replace with:
  ```rust
  planned_start: match input.planned_start {
      MaybeUndefined::Value(dt) => Some(Some(dt)),
      MaybeUndefined::Null      => Some(None),
      MaybeUndefined::Undefined => None,
  },
  ```

  `MaybeUndefined` is from `async_graphql` which is already in scope via the existing `use async_graphql::*` import.

- [ ] **Step 3: Compile-check the backend**

  ```bash
  cd /home/mbt/appfactory/aggregated_plan/backend && cargo check -p api
  ```
  Expected: no errors.

- [ ] **Step 4: Run backend tests**

  ```bash
  cd /home/mbt/appfactory/aggregated_plan/backend && cargo test
  ```
  Expected: all tests pass (no regressions).

- [ ] **Step 5: Commit**

  ```bash
  cd /home/mbt/appfactory/aggregated_plan
  git add backend/crates/api/src/graphql/types/task.rs backend/crates/api/src/graphql/mutation.rs
  git commit -m "feat(api): support clearing plannedStart via MaybeUndefined in UpdateTaskInput"
  ```

---

## Chunk 2: Frontend — Unplanned Sidebar

### Task 2: Update DashboardPage.tsx with sidebar, state, and DnD logic

**Files:**
- Modify: `frontend/src/pages/DashboardPage.tsx`

The full file is at `frontend/src/pages/DashboardPage.tsx` (~520 lines). The changes touch multiple sections; apply them in order.

#### 2a — Add `isUnplanned` helper and update `buildTasksByDate`

- [ ] **Step 1: Add `isUnplanned` helper after line 56 (`getTaskDate` function)**

  After the closing `}` of `getTaskDate`, insert:
  ```ts
  function isUnplanned(t: DashboardTask): boolean {
    return !t.plannedStart && !t.deadline;
  }
  ```

- [ ] **Step 2: Update `buildTasksByDate` to skip unplanned tasks**

  Current `buildTasksByDate` (lines 77-84):
  ```ts
  function buildTasksByDate(tasks: readonly DashboardTask[]): Record<string, DashboardTask[]> {
    const map: Record<string, DashboardTask[]> = {};
    for (const t of tasks) {
      const d = getTaskDate(t);
      (map[d] ??= []).push(t);
    }
    return map;
  }
  ```
  Replace the `for` loop body to skip unplanned:
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

#### 2b — Add `UnplannedSidebar` component (new function, before `DayColumn`)

- [ ] **Step 3: Add `UnplannedSidebar` component before the `DayColumn` definition (around line 147)**

  Insert this new function between the `DraggableTaskCard` function and the `DayColumn` interface:
  ```tsx
  // ─── UnplannedSidebar ─────────────────────────────────────────────────────

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

#### 2c — Add unplanned state to `DashboardPage`

- [ ] **Step 4: Add `unplannedTasks` state and `serverUnplannedRef` ref**

  In `DashboardPage`, after:
  ```ts
  const serverSnapshotRef = useRef<Record<string, DashboardTask[]>>({});
  const isMutatingRef = useRef(false);
  ```
  Add:
  ```ts
  const [unplannedTasks, setUnplannedTasks] = useState<DashboardTask[]>([]);
  const serverUnplannedRef = useRef<DashboardTask[]>([]);
  ```

#### 2d — Update `useEffect` to split planned/unplanned

- [ ] **Step 5: Update the `useEffect` that seeds `tasksByDate`**

  Current (around line 299-304):
  ```ts
  useEffect(() => {
    if (activeTaskId !== null || isMutatingRef.current) return;
    const fresh = buildTasksByDate(data?.tasks ?? []);
    setTasksByDate(fresh);
    serverSnapshotRef.current = fresh;
  }, [data, activeTaskId]);
  ```
  Replace with:
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

#### 2e — Update `onDragStart` to search unplanned tasks

- [ ] **Step 6: Update `onDragStart` to also search `unplannedTasks`**

  Current (around line 329-338):
  ```ts
  const onDragStart = useCallback(({ active }: DragStartEvent) => {
    const id = active.id as string;
    setActiveTaskId(id);
    setEditingTaskId(null);   // close edit sheet on drag start
    setCreatingForDate(null); // close create sheet on drag start

    // Freeze the task from current optimistic state — immune to refetches mid-drag
    const allTasks = Object.values(tasksByDate).flat();
    draggingTaskRef.current = allTasks.find(t => t.id === id) ?? null;
  }, [tasksByDate]);
  ```
  Replace with:
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

#### 2f — Replace `onDragEnd` with three-case logic

- [ ] **Step 7: Replace `onDragEnd` with the three-case implementation**

  Current `onDragEnd` (lines 345-372) — replace entirely:
  ```ts
  const onDragEnd = useCallback(({ over }: DragEndEvent) => {
    const draggedTask = draggingTaskRef.current;
    setActiveTaskId(null);
    draggingTaskRef.current = null;
    if (!draggedTask || !over) return;

    const overId = over.id as string;

    // ── Case 1: dropped on unplanned sidebar ──
    if (overId === 'unplanned') {
      if (draggedTask.deadline) return; // deadline tasks cannot be unscheduled
      isMutatingRef.current = true;
      const fromDate = getTaskDate(draggedTask);
      setTasksByDate(prev => ({
        ...prev,
        [fromDate]: (prev[fromDate] ?? []).filter(t => t.id !== draggedTask.id),
      }));
      setUnplannedTasks(prev => [...prev, { ...draggedTask, plannedStart: null }]);
      executeUpdate({ id: draggedTask.id, input: { plannedStart: null } })
        .then(r => { if (r.error || !r.data) restore(); })
        .catch(restore)
        .finally(() => {
          isMutatingRef.current = false;
          serverSnapshotRef.current = {
            ...serverSnapshotRef.current,
            [fromDate]: (serverSnapshotRef.current[fromDate] ?? []).filter(t => t.id !== draggedTask.id),
          };
          serverUnplannedRef.current = [...serverUnplannedRef.current, { ...draggedTask, plannedStart: null }];
        });
      return;
    }

    // ── Case 2 / 3: dropped on a day column ──
    if (!overId.startsWith('day-')) return;
    const newDate = overId.replace('day-', '');
    const fromUnplanned = serverUnplannedRef.current.some(t => t.id === draggedTask.id);

    if (fromUnplanned) {
      // Case 2: unplanned → day
      isMutatingRef.current = true;
      setUnplannedTasks(prev => prev.filter(t => t.id !== draggedTask.id));
      const scheduled = { ...draggedTask, plannedStart: `${newDate}T08:00:00Z` };
      setTasksByDate(prev => ({
        ...prev,
        [newDate]: [...(prev[newDate] ?? []), scheduled],
      }));
      executeUpdate({ id: draggedTask.id, input: { plannedStart: `${newDate}T08:00:00Z` } })
        .then(r => { if (r.error || !r.data) restore(); })
        .catch(restore)
        .finally(() => {
          isMutatingRef.current = false;
          serverUnplannedRef.current = serverUnplannedRef.current.filter(t => t.id !== draggedTask.id);
          serverSnapshotRef.current = {
            ...serverSnapshotRef.current,
            [newDate]: [...(serverSnapshotRef.current[newDate] ?? []), scheduled],
          };
        });
    } else {
      // Case 3: day → day (existing logic)
      const currentDate = getTaskDate(draggedTask);
      if (newDate === currentDate) return;
      isMutatingRef.current = true;
      setTasksByDate(prev => moveBetweenDays(prev, draggedTask, currentDate, newDate));
      executeUpdate({ id: draggedTask.id, input: { plannedStart: `${newDate}T08:00:00Z` } })
        .then(result => { if (result.error) setTasksByDate(serverSnapshotRef.current); })
        .catch(() => { setTasksByDate(serverSnapshotRef.current); })
        .finally(() => { isMutatingRef.current = false; });
    }

    function restore() {
      setTasksByDate(serverSnapshotRef.current);
      setUnplannedTasks(serverUnplannedRef.current);
    }
  }, [executeUpdate]);
  ```

#### 2g — Update JSX layout

- [ ] **Step 8: Add sidebar to the `DndContext` layout**

  In the JSX, find the `<DndContext ...>` block. Inside it, find:
  ```tsx
  <div className="grid grid-cols-5 gap-2">
  ```
  Replace the entire `<DndContext>` inner content (from `<div className="grid grid-cols-5 gap-2">` through `</DragOverlay>`) so the grid is wrapped in a flex layout with the sidebar:

  Before (the content inside `<DndContext>`):
  ```tsx
  <div className="grid grid-cols-5 gap-2">
    {weekDays.map(day => {
      const dayStr = formatDate(day);
      return (
        <DayColumn
          key={dayStr}
          date={day}
          tasks={tasksByDate[dayStr] ?? []}
          meetings={meetingsByDate[dayStr] ?? []}
          onTaskClick={setEditingTaskId}
          isDragging={activeTaskId !== null}
          onAddTask={() => setCreatingForDate(dayStr)}
        />
      );
    })}
  </div>
  ```

  Replace with:
  ```tsx
  <div className="flex gap-3">
    <UnplannedSidebar tasks={unplannedTasks} onTaskClick={setEditingTaskId} />
    <div className="flex-1 min-w-0">
      <div className="grid grid-cols-5 gap-2">
        {weekDays.map(day => {
          const dayStr = formatDate(day);
          return (
            <DayColumn
              key={dayStr}
              date={day}
              tasks={tasksByDate[dayStr] ?? []}
              meetings={meetingsByDate[dayStr] ?? []}
              onTaskClick={setEditingTaskId}
              isDragging={activeTaskId !== null}
              onAddTask={() => setCreatingForDate(dayStr)}
            />
          );
        })}
      </div>
    </div>
  </div>
  ```

- [ ] **Step 9: TypeScript check**

  ```bash
  cd /home/mbt/appfactory/aggregated_plan/frontend && pnpm exec tsc --noEmit
  ```
  Expected: no errors.

- [ ] **Step 10: Commit**

  ```bash
  cd /home/mbt/appfactory/aggregated_plan
  git add frontend/src/pages/DashboardPage.tsx
  git commit -m "feat(dashboard): add unplanned tasks sidebar with drag-to-schedule support"
  ```
