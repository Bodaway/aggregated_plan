import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act, within } from '@testing-library/react';
import type { ReactNode } from 'react';
import type { DashboardTask } from '@/hooks/use-dashboard';
import type { OverdueKind } from '@/lib/overdue';

// The board is frozen on a Wednesday so "today" and the rendered week are
// stable: week = Mon 2026-08-31 … Fri 2026-09-04, today = 2026-09-02.
const TODAY = '2026-09-02';
const LATER_THIS_WEEK = '2026-09-04';

// ─── Mocks ───────────────────────────────────────────────────────────────────

const harness = vi.hoisted(() => ({
  /** Captured from the mocked DndContext so a drop can be replayed directly. */
  drag: {} as {
    onDragStart?: (e: unknown) => void;
    onDragEnd?: (e: unknown) => void;
    onDragCancel?: () => void;
  },
  /** The `useDashboard` return value. Held as ONE stable object per test: the
   *  page re-seeds its optimistic state whenever `data` changes identity, so a
   *  fresh literal per render would spin the component forever. */
  dashboard: { data: null as unknown, loading: false, error: null, refetch: () => undefined },
  updateResult: { data: { updateTask: { id: 'x', plannedStart: null as string | null } } } as unknown,
  executeUpdate: vi.fn(),
}));

// dnd-kit drags cannot be simulated meaningfully in jsdom (no layout, no
// pointer geometry). The context is replaced by a pass-through that hands the
// handlers back to the test, and droppables tag their node with their id so the
// column a card ended up in can be asserted.
vi.mock('@dnd-kit/core', () => ({
  DndContext: ({
    children,
    onDragStart,
    onDragEnd,
    onDragCancel,
  }: {
    children: ReactNode;
    onDragStart?: (e: unknown) => void;
    onDragEnd?: (e: unknown) => void;
    onDragCancel?: () => void;
  }) => {
    harness.drag.onDragStart = onDragStart;
    harness.drag.onDragEnd = onDragEnd;
    harness.drag.onDragCancel = onDragCancel;
    return <>{children}</>;
  },
  DragOverlay: ({ children }: { children?: ReactNode }) => <>{children}</>,
  useDroppable: ({ id }: { id: string }) => ({
    setNodeRef: (node: HTMLElement | null) => node?.setAttribute('data-droppable-id', id),
    isOver: false,
  }),
  useDraggable: () => ({
    attributes: {},
    listeners: {},
    setNodeRef: () => undefined,
    transform: null,
    isDragging: false,
  }),
  pointerWithin: () => [],
  PointerSensor: function PointerSensor() {},
  useSensor: () => ({}),
  useSensors: () => [],
}));

vi.mock('urql', () => ({
  useMutation: () => [{ fetching: false, data: null, error: null }, harness.executeUpdate],
  useQuery: () => [{ fetching: false, data: null, error: null }, vi.fn()],
}));

vi.mock('@/hooks/use-dashboard', () => ({
  useDashboard: () => harness.dashboard,
}));

vi.mock('@/lib/search/SearchProvider', () => ({
  useSearch: () => ({
    matchedIds: new Set<string>(),
    highlightActive: false,
    openTaskInSheet: vi.fn(),
  }),
}));

vi.mock('@/hooks/use-memory', () => ({
  useMemoryCapture: () => ({ remember: vi.fn(), saving: false, error: null }),
}));

vi.mock('@/components/memory/SelectionToMemory', () => ({ SelectionToMemory: () => null }));
vi.mock('@/components/task/TaskCreateSheet', () => ({ TaskCreateSheet: () => null }));
vi.mock('@/components/sync/SyncStatusBar', () => ({ SyncStatusBar: () => null }));
vi.mock('@/components/alert/AlertPanel', () => ({ AlertPanel: () => null }));
vi.mock('@/components/gryzzly/GryzzlyTaskMenu', () => ({ GryzzlyTaskMenu: () => null }));

// Imported after the mocks so the module graph resolves against them.
import { DashboardPage, getTaskDate, compareDayTasks } from './DashboardPage';

// ─── Fixtures ────────────────────────────────────────────────────────────────

function makeTask(overrides: Partial<DashboardTask> & { id: string }): DashboardTask {
  return {
    title: `Task ${overrides.id}`,
    source: 'PERSONAL',
    sourceId: null,
    trackingState: 'IDLE',
    status: 'TODO',
    jiraStatus: null,
    urgency: 2,
    impact: 2,
    quadrant: 'Important',
    deadline: null,
    plannedStart: null,
    assignee: null,
    project: null,
    tags: [],
    effectiveRemainingHours: null,
    effectiveEstimatedHours: null,
    jiraTimeSpentSeconds: null,
    gryzzlyTask: null,
    overdueKind: 'NONE' as OverdueKind,
    overdueDays: null,
    ...overrides,
  };
}

/** Seed the board. Called before `render`, never during. */
function givenTasks(tasks: DashboardTask[]) {
  harness.dashboard.data = {
    tasks,
    meetings: [],
    alerts: [],
    syncStatuses: [],
    workingDays: [1, 2, 3, 4, 5],
    workingHoursPerDay: 8,
  };
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date(`${TODAY}T12:00:00Z`));
  givenTasks([]);
  harness.drag = {};
  harness.executeUpdate = vi.fn(() => Promise.resolve(harness.updateResult));
  localStorage.clear();
});

afterEach(() => {
  vi.useRealTimers();
});

// ─── getTaskDate (R74) ───────────────────────────────────────────────────────

describe('getTaskDate — a delay routes the card to today', () => {
  it('sends an overdue-by-deadline task to today, whatever its own dates say', () => {
    const task = makeTask({
      id: 'a',
      plannedStart: '2026-07-01T08:00:00Z',
      deadline: '2026-07-10',
      overdueKind: 'DEADLINE',
      overdueDays: 54,
    });

    expect(getTaskDate(task)).toBe(TODAY);
  });

  it('sends an overdue-by-planning task to today', () => {
    const task = makeTask({
      id: 'b',
      plannedStart: '2026-08-20T08:00:00Z',
      overdueKind: 'PLANNED',
      overdueDays: 13,
    });

    expect(getTaskDate(task)).toBe(TODAY);
  });

  it('trusts the server qualification over the dates it can see', () => {
    // The delay is derived server-side (R73); the client never re-derives it.
    // Even dates that look future-dated must not pull the card out of today.
    const task = makeTask({
      id: 'c',
      plannedStart: '2026-12-01T08:00:00Z',
      deadline: '2026-12-31',
      overdueKind: 'DEADLINE',
      overdueDays: 1,
    });

    expect(getTaskDate(task)).toBe(TODAY);
  });

  it('falls back to plannedStart for an on-time task', () => {
    const task = makeTask({ id: 'd', plannedStart: '2026-09-03T08:00:00Z', deadline: '2026-09-30' });

    expect(getTaskDate(task)).toBe('2026-09-03');
  });

  it('prefers plannedStart over deadline when both are present', () => {
    const task = makeTask({ id: 'e', plannedStart: '2026-09-03T08:00:00Z', deadline: '2026-09-04' });

    expect(getTaskDate(task)).toBe('2026-09-03');
  });

  it('falls back to the deadline when there is no plannedStart', () => {
    const task = makeTask({ id: 'f', deadline: '2026-09-04' });

    expect(getTaskDate(task)).toBe('2026-09-04');
  });

  it('falls back to today when the task has neither date', () => {
    const task = makeTask({ id: 'g' });

    expect(getTaskDate(task)).toBe(TODAY);
  });

  it('reads the plannedStart date in isolation from its time component', () => {
    const task = makeTask({ id: 'h', plannedStart: '2026-09-03T23:45:00Z' });

    expect(getTaskDate(task)).toBe('2026-09-03');
  });
});

// ─── compareDayTasks (R74) ───────────────────────────────────────────────────

describe('compareDayTasks — the gravest delay leads the day column', () => {
  const sortIds = (tasks: DashboardTask[]) => [...tasks].sort(compareDayTasks).map(t => t.id);

  it('orders DEADLINE, then PLANNED, then on-time', () => {
    const tasks = [
      makeTask({ id: 'ontime' }),
      makeTask({ id: 'planned', overdueKind: 'PLANNED', overdueDays: 2 }),
      makeTask({ id: 'deadline', overdueKind: 'DEADLINE', overdueDays: 1 }),
    ];

    expect(sortIds(tasks)).toEqual(['deadline', 'planned', 'ontime']);
  });

  it('puts the delay tier above urgency — a low-urgency delay still leads', () => {
    const tasks = [
      makeTask({ id: 'critical-ontime', urgency: 4 }),
      makeTask({ id: 'low-deadline', urgency: 1, overdueKind: 'DEADLINE', overdueDays: 9 }),
    ];

    expect(sortIds(tasks)).toEqual(['low-deadline', 'critical-ontime']);
  });

  it('puts a low-urgency deadline miss above a critical planning slip', () => {
    const tasks = [
      makeTask({ id: 'critical-planned', urgency: 4, overdueKind: 'PLANNED', overdueDays: 1 }),
      makeTask({ id: 'low-deadline', urgency: 1, overdueKind: 'DEADLINE', overdueDays: 1 }),
    ];

    expect(sortIds(tasks)).toEqual(['low-deadline', 'critical-planned']);
  });

  it('sorts by urgency descending inside a tier', () => {
    const tasks = [
      makeTask({ id: 'u2', urgency: 2, overdueKind: 'DEADLINE', overdueDays: 1 }),
      makeTask({ id: 'u4', urgency: 4, overdueKind: 'DEADLINE', overdueDays: 1 }),
      makeTask({ id: 'u3', urgency: 3, overdueKind: 'DEADLINE', overdueDays: 1 }),
    ];

    expect(sortIds(tasks)).toEqual(['u4', 'u3', 'u2']);
  });

  it('sorts by urgency descending among on-time tasks too', () => {
    const tasks = [
      makeTask({ id: 'u1', urgency: 1 }),
      makeTask({ id: 'u3', urgency: 3 }),
      makeTask({ id: 'u2', urgency: 2 }),
    ];

    expect(sortIds(tasks)).toEqual(['u3', 'u2', 'u1']);
  });

  it('breaks an urgency tie on impact descending', () => {
    const tasks = [
      makeTask({ id: 'i1', urgency: 3, impact: 1 }),
      makeTask({ id: 'i4', urgency: 3, impact: 4 }),
      makeTask({ id: 'i2', urgency: 3, impact: 2 }),
    ];

    expect(sortIds(tasks)).toEqual(['i4', 'i2', 'i1']);
  });

  it('mixes delayed and same-day work in one list rather than boxing them apart', () => {
    const tasks = [
      makeTask({ id: 'today-high', urgency: 4 }),
      makeTask({ id: 'late-deadline-low', urgency: 1, overdueKind: 'DEADLINE', overdueDays: 20 }),
      makeTask({ id: 'today-low', urgency: 1 }),
      makeTask({ id: 'late-planned-high', urgency: 4, overdueKind: 'PLANNED', overdueDays: 3 }),
    ];

    expect(sortIds(tasks)).toEqual([
      'late-deadline-low',
      'late-planned-high',
      'today-high',
      'today-low',
    ]);
  });

  it('is symmetric — reversing the input does not change the result', () => {
    const tasks = [
      makeTask({ id: 'a', urgency: 3, overdueKind: 'DEADLINE', overdueDays: 1 }),
      makeTask({ id: 'b', urgency: 4 }),
      makeTask({ id: 'c', urgency: 2, overdueKind: 'PLANNED', overdueDays: 1 }),
    ];

    expect(sortIds(tasks)).toEqual(sortIds([...tasks].reverse()));
  });
});

// ─── Drag: the optimistic clone drops the delay ──────────────────────────────
//
// `replanned()` is module-private, so it is exercised through the behaviour it
// exists for: a delayed card dragged to another day must not snap back to
// today's column. Keeping a stale `overdueKind` on the clone would send
// `getTaskDate` — and the card with it — straight back to today.

function columnOf(container: HTMLElement, dayStr: string): HTMLElement {
  return container.querySelector(`[data-droppable-id="day-${dayStr}"]`) as HTMLElement;
}

function dropOn(taskId: string, droppableId: string) {
  act(() => {
    harness.drag.onDragStart?.({ active: { id: taskId } });
  });
  act(() => {
    harness.drag.onDragEnd?.({ active: { id: taskId }, over: { id: droppableId } });
  });
}

describe('DashboardPage — dragging a delayed card away from today', () => {
  it('lands it on the target day instead of snapping back to today', async () => {
    givenTasks([
      makeTask({
        id: 'late',
        title: 'Tâche en retard',
        plannedStart: '2026-08-20T08:00:00Z',
        overdueKind: 'PLANNED',
        overdueDays: 13,
      }),
    ]);

    const { container } = render(<DashboardPage />);
    // It starts in today's column, brought up by the delay (R74).
    expect(within(columnOf(container, TODAY)).getByText('Tâche en retard')).toBeInTheDocument();

    dropOn('late', `day-${LATER_THIS_WEEK}`);

    expect(within(columnOf(container, LATER_THIS_WEEK)).getByText('Tâche en retard')).toBeInTheDocument();
    expect(within(columnOf(container, TODAY)).queryByText('Tâche en retard')).not.toBeInTheDocument();
  });

  it('drops the delay marker from the moved card', () => {
    givenTasks([
      makeTask({
        id: 'late',
        title: 'Tâche en retard',
        plannedStart: '2026-08-20T08:00:00Z',
        overdueKind: 'DEADLINE',
        overdueDays: 13,
      }),
    ]);

    const { container } = render(<DashboardPage />);
    expect(screen.getByTestId('overdue-badge')).toHaveTextContent('⚠ -13j');

    dropOn('late', `day-${LATER_THIS_WEEK}`);

    // Re-planned by the user: the delay is gone until the server says otherwise.
    expect(screen.queryByTestId('overdue-badge')).not.toBeInTheDocument();
    expect(within(columnOf(container, LATER_THIS_WEEK)).getByText('Tâche en retard')).toBeInTheDocument();
  });

  it('sends the new plannedStart to the server, and nothing else', () => {
    givenTasks([
      makeTask({
        id: 'late',
        plannedStart: '2026-08-20T08:00:00Z',
        overdueKind: 'PLANNED',
        overdueDays: 13,
      }),
    ]);

    render(<DashboardPage />);
    dropOn('late', `day-${LATER_THIS_WEEK}`);

    // R72: the delay is never written back, only the date the user just set.
    expect(harness.executeUpdate).toHaveBeenCalledWith({
      id: 'late',
      input: { plannedStart: `${LATER_THIS_WEEK}T08:00:00Z` },
    });
  });

  it('drops the delay when the card goes back to the unplanned sidebar', () => {
    givenTasks([
      makeTask({
        id: 'late',
        title: 'Tâche en retard',
        plannedStart: '2026-08-20T08:00:00Z',
        overdueKind: 'PLANNED',
        overdueDays: 13,
      }),
    ]);

    const { container } = render(<DashboardPage />);

    dropOn('late', 'unplanned');

    const sidebar = container.querySelector('[data-droppable-id="unplanned"]') as HTMLElement;
    expect(within(sidebar).getByText('Tâche en retard')).toBeInTheDocument();
    expect(screen.queryByTestId('overdue-badge')).not.toBeInTheDocument();
  });

  it('leaves an on-time card where the user drops it', () => {
    givenTasks([
      makeTask({ id: 'ok', title: 'Tâche à l’heure', plannedStart: `${TODAY}T08:00:00Z` }),
    ]);

    const { container } = render(<DashboardPage />);

    dropOn('ok', `day-${LATER_THIS_WEEK}`);

    expect(within(columnOf(container, LATER_THIS_WEEK)).getByText('Tâche à l’heure')).toBeInTheDocument();
  });
});
