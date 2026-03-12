import { useState, useCallback, useMemo, useRef, useEffect } from 'react';
import {
  DndContext,
  DragOverlay,
  pointerWithin,
  PointerSensor,
  useSensor,
  useSensors,
  type DragStartEvent,
  type DragEndEvent,
} from '@dnd-kit/core';
import { useDroppable, useDraggable } from '@dnd-kit/core';
import { useMutation } from 'urql';
import { useDashboard, type DashboardTask, type DashboardMeeting } from '@/hooks/use-dashboard';
import { TaskCard } from '@/components/task/TaskCard';
import { AlertPanel } from '@/components/alert/AlertPanel';
import { SyncStatusBar } from '@/components/sync/SyncStatusBar';
import {
  formatDate,
  formatWeekRange,
  formatDayShort,
  getWeekStart,
  getNextWeek,
  getPrevWeek,
  getWeekDays,
  isToday,
} from '@/lib/date-utils';
import { TaskEditSheet } from '@/components/task/TaskEditSheet';
import { TaskCreateSheet } from '@/components/task/TaskCreateSheet';

// ─── Constants ────────────────────────────────────────────────────────────────

const DAILY_CAPACITY_HOURS_FALLBACK = 8;

const UPDATE_TASK_MUTATION = `
  mutation RescheduleDashboardTask($id: ID!, $input: UpdateTaskInput!) {
    updateTask(id: $id, input: $input) {
      id
      plannedStart
    }
  }
`;

// ─── Helpers (module-level, no deps) ─────────────────────────────────────────


function getTaskHours(t: DashboardTask): number {
  return t.effectiveRemainingHours ?? t.effectiveEstimatedHours ?? 0;
}

/** Returns the "planned date" key (YYYY-MM-DD) for routing a task to a day column. */
function getTaskDate(t: DashboardTask): string {
  if (t.plannedStart) return t.plannedStart.slice(0, 10);
  if (t.deadline) return t.deadline;
  return formatDate(new Date());
}

function isUnplanned(t: DashboardTask): boolean {
  return !t.plannedStart && !t.deadline;
}

function getMeetingDate(m: DashboardMeeting): string {
  return m.startTime.slice(0, 10);
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  } catch {
    return iso;
  }
}

function countsMeetingAgainstCapacity(m: DashboardMeeting): boolean {
  if (m.title.toLowerCase() === 'pause midi') return false;
  if (m.showAs === 'free' || m.showAs === 'oof' || m.showAs === 'workingElsewhere') return false;
  return true;
}

function formatHoursCompact(hours: number): string {
  if (hours === 0) return '0h';
  if (hours < 1) return `${Math.round(hours * 60)}m`;
  return `${hours % 1 === 0 ? hours : hours.toFixed(1)}h`;
}

/** Build a date-keyed map from a flat task list. */
function buildTasksByDate(tasks: readonly DashboardTask[]): Record<string, DashboardTask[]> {
  const map: Record<string, DashboardTask[]> = {};
  for (const t of tasks) {
    if (isUnplanned(t)) continue;
    const d = getTaskDate(t);
    (map[d] ??= []).push(t);
  }
  return map;
}

/**
 * Return a new map with `task` moved from `fromDate` to `toDate`.
 * The task's `plannedStart` is updated in the cloned object so that
 * `getTaskDate()` keeps routing it to the correct bucket.
 */
function moveBetweenDays(
  prev: Record<string, DashboardTask[]>,
  task: DashboardTask,
  fromDate: string,
  toDate: string,
): Record<string, DashboardTask[]> {
  const updated: DashboardTask = { ...task, plannedStart: `${toDate}T08:00:00Z` };
  return {
    ...prev,
    [fromDate]: (prev[fromDate] ?? []).filter(t => t.id !== task.id),
    [toDate]: [...(prev[toDate] ?? []), updated],
  };
}

// ─── DraggableTaskCard ───────────────────────────────────────────────────────

function DraggableTaskCard({
  task,
  onTaskClick,
}: {
  readonly task: DashboardTask;
  readonly onTaskClick: (id: string) => void;
}) {
  const disabled = task.status === 'DONE' || task.status === 'CANCELLED';
  const { attributes, listeners, setNodeRef, transform, isDragging } = useDraggable({
    id: task.id,
    disabled,
  });

  const style = transform
    ? { transform: `translate(${transform.x}px, ${transform.y}px)`, opacity: isDragging ? 0.4 : 1 }
    : undefined;

  return (
    <div ref={setNodeRef} style={style} {...listeners} {...attributes}>
      <TaskCard
        id={task.id}
        title={task.title}
        source={task.source ?? 'PERSONAL'}
        sourceId={task.sourceId}
        status={task.status}
        jiraStatus={task.jiraStatus}
        urgency={task.urgency}
        impact={task.impact}
        quadrant={task.quadrant}
        deadline={task.deadline}
        effectiveRemainingHours={task.effectiveRemainingHours}
        effectiveEstimatedHours={task.effectiveEstimatedHours}
        jiraTimeSpentSeconds={task.jiraTimeSpentSeconds}
        compact
        onClick={disabled ? undefined : () => onTaskClick(task.id)}
      />
    </div>
  );
}

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

// ─── DayColumn ───────────────────────────────────────────────────────────────

interface DayColumnProps {
  readonly date: Date;
  readonly tasks: DashboardTask[];
  readonly meetings: DashboardMeeting[];
  readonly onTaskClick: (id: string) => void;
  readonly isDragging: boolean;
  readonly onAddTask: () => void;
  readonly workingHoursPerDay: number;
}

function DayColumn({ date, tasks, meetings, onTaskClick, isDragging, onAddTask, workingHoursPerDay }: DayColumnProps) {
  const dayStr = formatDate(date);
  const { setNodeRef, isOver } = useDroppable({ id: `day-${dayStr}` });

  const today = isToday(date);

  const sortedTasks = [...tasks].sort((a, b) => {
    if (b.urgency !== a.urgency) return b.urgency - a.urgency; // urgency desc
    return b.impact - a.impact;                                 // impact desc
  });

  const sortedMeetings = [...meetings].sort(
    (a, b) => new Date(a.startTime).getTime() - new Date(b.startTime).getTime(),
  );

  const meetingHours = sortedMeetings
    .filter(countsMeetingAgainstCapacity)
    .reduce((sum, m) => sum + m.durationHours, 0);
  const availableHours = Math.max(workingHoursPerDay - meetingHours, 0);
  const totalHours = tasks.reduce((sum, t) => sum + getTaskHours(t), 0);
  const fillPct = availableHours > 0 ? Math.min((totalHours / availableHours) * 100, 100) : 100;
  const overloaded = totalHours > availableHours;

  const dropHighlight = isDragging && isOver;

  return (
    <div
      ref={setNodeRef}
      className={`flex flex-col rounded-lg border overflow-hidden transition-colors ${
        dropHighlight
          ? 'border-blue-400 bg-blue-50/60 ring-2 ring-blue-300'
          : today
            ? 'border-blue-300 bg-blue-50/30'
            : 'border-gray-200 bg-white'
      }`}
    >
      {/* Day header */}
      <div className={`px-3 py-2 border-b ${today ? 'border-blue-200 bg-blue-50' : 'border-gray-100 bg-gray-50'}`}>
        <div className="flex items-center justify-between">
          <span className={`text-xs font-semibold uppercase tracking-wider ${today ? 'text-blue-700' : 'text-gray-600'}`}>
            {formatDayShort(date)}
          </span>
          <div className="flex items-center gap-1.5">
            <span className={`text-xs font-medium ${overloaded ? 'text-red-600' : 'text-gray-500'}`}>
              {formatHoursCompact(totalHours)}/{formatHoursCompact(availableHours)}
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
        {/* Workload bar */}
        <div className="mt-1.5 h-1.5 bg-gray-200 rounded-full overflow-hidden">
          <div
            className={`h-full rounded-full transition-all ${
              overloaded ? 'bg-red-500' : fillPct > 75 ? 'bg-yellow-500' : 'bg-blue-500'
            }`}
            style={{ width: `${fillPct}%` }}
          />
        </div>
      </div>

      {/* Content */}
      <div
        className={`flex-1 p-2 space-y-1.5 overflow-y-auto ${dropHighlight ? 'bg-blue-50/40' : ''}`}
        style={{ minHeight: 80, maxHeight: 'calc(100vh - 240px)' }}
      >
        {/* Meetings */}
        {sortedMeetings.map(m => (
          <div key={m.id} className="flex items-center gap-1.5 px-2 py-1 rounded bg-indigo-50 border border-indigo-100">
            <svg className="w-3 h-3 text-indigo-500 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <div className="min-w-0 flex-1">
              <p className="text-xs font-medium text-indigo-900 truncate">{m.title}</p>
              <p className="text-xs text-indigo-600">
                {formatTime(m.startTime)} - {formatTime(m.endTime)}
              </p>
            </div>
          </div>
        ))}

        {/* Tasks */}
        {sortedTasks.map(t => (
          <DraggableTaskCard key={t.id} task={t} onTaskClick={onTaskClick} />
        ))}

        {sortedTasks.length === 0 && sortedMeetings.length === 0 && (
          <p className={`text-xs text-center py-4 ${dropHighlight ? 'text-blue-400' : 'text-gray-400'}`}>
            {dropHighlight ? 'Drop here' : 'No items'}
          </p>
        )}
      </div>

      {/* Footer */}
      {meetingHours > 0 && (
        <div className="px-3 py-1 border-t border-gray-100 bg-gray-50/50">
          <span className="text-xs text-gray-400">
            {sortedMeetings.length} mtg{sortedMeetings.length !== 1 ? 's' : ''} ({formatHoursCompact(meetingHours)})
          </span>
        </div>
      )}
    </div>
  );
}

// ─── DashboardPage ────────────────────────────────────────────────────────────

export function DashboardPage() {
  // ── Edit sheet ──
  const [editingTaskId, setEditingTaskId] = useState<string | null>(null);
  const [creatingForDate, setCreatingForDate] = useState<string | null>(null);
  const handleSheetClose = useCallback(() => setEditingTaskId(null), []);

  // ── Week navigation ──
  const [currentDate, setCurrentDate] = useState(() => new Date());
  const weekStart = getWeekStart(currentDate);
  const dateStr = formatDate(weekStart);
  const { data, loading, error, refetch } = useDashboard(dateStr);
  const workingDays = data?.workingDays ?? [1, 2, 3, 4, 5];
  const weekDays = useMemo(
    () => getWeekDays(weekStart, workingDays),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [weekStart.getTime(), JSON.stringify(workingDays)],
  );

  // ── Mutation ──
  const [, executeUpdate] = useMutation(UPDATE_TASK_MUTATION);

  // ── Optimistic task state ──
  const [tasksByDate, setTasksByDate] = useState<Record<string, DashboardTask[]>>({});
  const serverSnapshotRef = useRef<Record<string, DashboardTask[]>>({});
  const isMutatingRef = useRef(false);
  const [unplannedTasks, setUnplannedTasks] = useState<DashboardTask[]>([]);
  const serverUnplannedRef = useRef<DashboardTask[]>([]);

  // ── Drag state ──
  const [activeTaskId, setActiveTaskId] = useState<string | null>(null);
  const draggingTaskRef = useRef<DashboardTask | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 8 } }),
  );

  // ── Seed tasksByDate from server data (guarded) ──
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

  // ── Meetings by date (server data — no optimistic needed) ──
  const meetingsByDate = useMemo(() => {
    if (!data) return {};
    const map: Record<string, DashboardMeeting[]> = {};
    for (const m of data.meetings) {
      const d = getMeetingDate(m);
      (map[d] ??= []).push(m);
    }
    return map;
  }, [data]);

  // ── Week totals from optimistic state ──
  const weekTotalHours = useMemo(
    () => Object.values(tasksByDate).flat().reduce((sum, t) => sum + getTaskHours(t), 0),
    [tasksByDate],
  );

  const weekTotalMeetingHours = useMemo(() => {
    if (!data) return 0;
    return data.meetings
      .filter(countsMeetingAgainstCapacity)
      .reduce((sum, m) => sum + m.durationHours, 0);
  }, [data]);

  // ── Drag handlers ──
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

  const onDragCancel = useCallback(() => {
    setActiveTaskId(null);
    draggingTaskRef.current = null;
  }, []);

  const onDragEnd = useCallback(({ over }: DragEndEvent) => {
    const draggedTask = draggingTaskRef.current;
    setActiveTaskId(null);
    draggingTaskRef.current = null;
    if (!draggedTask || !over) return;

    const overId = over.id as string;

    // ── Case 1: dropped on unplanned sidebar ──
    if (overId === 'unplanned') {
      if (draggedTask.deadline) return;
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
      // Case 3: day → day
      const currentDate = getTaskDate(draggedTask);
      if (newDate === currentDate) return;
      isMutatingRef.current = true;
      setTasksByDate(prev => moveBetweenDays(prev, draggedTask, currentDate, newDate));
      executeUpdate({ id: draggedTask.id, input: { plannedStart: `${newDate}T08:00:00Z` } })
        .then(result => { if (result.error || !result.data) setTasksByDate(serverSnapshotRef.current); })
        .catch(() => { setTasksByDate(serverSnapshotRef.current); })
        .finally(() => { isMutatingRef.current = false; });
    }

    function restore() {
      setTasksByDate(serverSnapshotRef.current);
      setUnplannedTasks(serverUnplannedRef.current);
    }
  }, [executeUpdate]);

  // ─────────────────────────────────────────────────────────────────────────

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <p className="text-red-500 text-sm font-medium">Failed to load dashboard</p>
          <p className="text-gray-400 text-xs mt-1">{error.message}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {/* Week navigation */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <button
            onClick={() => setCurrentDate(prev => getPrevWeek(prev))}
            className="p-1.5 rounded-md border border-gray-300 hover:bg-gray-50 transition-colors"
            aria-label="Previous week"
          >
            <svg className="w-4 h-4 text-gray-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 19.5L8.25 12l7.5-7.5" />
            </svg>
          </button>
          <h2 className="text-lg font-semibold text-gray-800">{formatWeekRange(weekStart, workingDays)}</h2>
          <button
            onClick={() => setCurrentDate(prev => getNextWeek(prev))}
            className="p-1.5 rounded-md border border-gray-300 hover:bg-gray-50 transition-colors"
            aria-label="Next week"
          >
            <svg className="w-4 h-4 text-gray-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
            </svg>
          </button>
          <button
            onClick={() => setCurrentDate(new Date())}
            className="px-3 py-1 text-xs font-medium text-blue-600 border border-blue-300 rounded-md hover:bg-blue-50 transition-colors"
          >
            This Week
          </button>
        </div>

        {/* Week summary */}
        {data && (
          <div className="flex items-center gap-4 text-xs text-gray-500">
            <span>
              <span className="font-medium text-gray-700">{formatHoursCompact(weekTotalHours)}</span> planned
            </span>
            <span>
              <span className="font-medium text-gray-700">{formatHoursCompact(weekTotalMeetingHours)}</span> meetings
            </span>
            {weekTotalHours + weekTotalMeetingHours > (data.workingHoursPerDay ?? DAILY_CAPACITY_HOURS_FALLBACK) * workingDays.length && (
              <span className="flex items-center gap-1 text-red-600 font-medium">
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z" />
                </svg>
                Overloaded
              </span>
            )}
          </div>
        )}
      </div>

      {/* Sync status */}
      {data && <SyncStatusBar statuses={data.syncStatuses} />}

      {loading && !data ? (
        <div className="flex items-center justify-center h-64">
          <div className="text-center">
            <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin mx-auto mb-2" />
            <p className="text-gray-500 text-sm">Loading dashboard...</p>
          </div>
        </div>
      ) : (
        <>
          {/* Week grid */}
          <DndContext
            sensors={sensors}
            collisionDetection={pointerWithin}
            onDragStart={onDragStart}
            onDragEnd={onDragEnd}
            onDragCancel={onDragCancel}
          >
            <div className="flex gap-3">
              <UnplannedSidebar tasks={unplannedTasks} onTaskClick={setEditingTaskId} />
              <div className="flex-1 min-w-0">
                <div style={{ display: 'grid', gridTemplateColumns: `repeat(${workingDays.length}, minmax(0, 1fr))`, gap: '0.5rem' }}>
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
                        workingHoursPerDay={data?.workingHoursPerDay ?? DAILY_CAPACITY_HOURS_FALLBACK}
                      />
                    );
                  })}
                </div>
              </div>
            </div>

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
          </DndContext>

          {/* Alerts */}
          {data && data.alerts.length > 0 && (
            <div className="bg-white rounded-lg border border-gray-200 p-4">
              <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider mb-3">Alerts</h3>
              <AlertPanel alerts={data.alerts} />
            </div>
          )}
        </>
      )}

      <TaskEditSheet taskId={editingTaskId} onClose={handleSheetClose} />
      <TaskCreateSheet
        plannedDate={creatingForDate}
        onClose={() => setCreatingForDate(null)}
        onCreated={() => refetch()}
      />
    </div>
  );
}
