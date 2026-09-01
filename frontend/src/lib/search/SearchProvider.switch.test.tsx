import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { SearchProvider, useSearch } from './SearchProvider';
import type { FullTask } from '@/hooks/use-task-edit';
import type { SearchableTask } from './types';

// ── Why this file exists ──────────────────────────────────────────────────────
//
// `openTaskInSheet` is what Cmd/Ctrl+K calls, and it can move the panel from one
// task to another without ever closing it. The panel is rendered by the provider,
// so the switch — and the refusal to switch on a failed write — is only
// observable here. Everything else about the panel lives in
// TaskEditSheet.test.tsx, which mocks the panel's own hooks.

const mockUpdateTask = vi.fn(async (_id: string, _input: Record<string, unknown>) => {});
const mockUpdatePriority = vi.fn(async (_id: string, _urgency: string, _impact: string) => {});
const mockSkipOccurrence = vi.fn(async (_id: string) => {});
const mockUpdateRecurringTask = vi.fn(async (_id: string, _input: Record<string, unknown>) => {});

vi.mock('@/hooks/use-searchable-tasks', () => ({
  useSearchableTasks: () => ({
    tasks: SEARCHABLE,
    loading: false,
    error: null,
    refetch: () => {},
  }),
}));

vi.mock('@/hooks/use-task-edit', () => ({
  useTaskEdit: (taskId: string | null) => ({
    task: taskId === null ? null : (TASKS.get(taskId) ?? null),
    loading: false,
    error: null,
    updateTask: mockUpdateTask,
    updatePriority: mockUpdatePriority,
    skipOccurrence: mockSkipOccurrence,
    updateRecurringTask: mockUpdateRecurringTask,
    refetch: vi.fn(),
  }),
}));

vi.mock('@/hooks/use-delegates', () => ({
  useDelegates: () => ({ delegates: [] }),
}));

vi.mock('@/components/markdown/MarkdownEditor', () => ({
  MarkdownEditor: ({ value, onChange }: { value: string; onChange: (v: string) => void }) => (
    <textarea aria-label="Notes" value={value} onChange={e => onChange(e.target.value)} />
  ),
}));

vi.mock('@/components/worklog/WorklogSection', () => ({
  WorklogSection: () => <div data-testid="worklog-section" />,
}));

vi.mock('@/components/gryzzly/GryzzlyTaskPicker', () => ({
  GryzzlyTaskPicker: () => <div data-testid="gryzzly-task-picker" />,
}));

// ── Fixtures ──────────────────────────────────────────────────────────────────

const SEARCHABLE: SearchableTask[] = [
  { id: 'task-a', title: 'Task A', sourceId: null, source: 'PERSONAL', assignee: null,
    projectName: null, tags: [], description: null, status: 'TODO' },
  { id: 'task-b', title: 'Task B', sourceId: null, source: 'PERSONAL', assignee: null,
    projectName: null, tags: [], description: null, status: 'TODO' },
];

const BASE_TASK: FullTask = {
  id: 'task-a',
  title: 'Task A',
  description: 'desc A',
  notes: null,
  source: 'PERSONAL',
  sourceId: null,
  status: 'TODO',
  jiraStatus: null,
  urgency: 'MEDIUM',
  impact: 'MEDIUM',
  quadrant: 'Important',
  deadline: null,
  plannedStart: '2026-04-27T08:00:00Z',
  assignee: null,
  delegatedTo: null,
  estimatedHours: null,
  trackingState: 'IDLE',
  jiraRemainingSeconds: null,
  jiraOriginalEstimateSeconds: null,
  jiraTimeSpentSeconds: null,
  remainingHoursOverride: null,
  estimatedHoursOverride: null,
  effectiveRemainingHours: null,
  effectiveEstimatedHours: null,
  project: null,
  tags: [],
  recurrenceId: null,
  occurrenceDate: null,
  isRecurring: false,
  gryzzlyTask: null,
};

const TASKS = new Map<string, FullTask>([
  ['task-a', BASE_TASK],
  ['task-b', { ...BASE_TASK, id: 'task-b', title: 'Task B', description: 'desc B' }],
]);

/** The shape urql hands back in `result.error`, which use-task-edit throws. */
function combinedError(message: string): Error {
  const err = new Error(`[Network] ${message}`);
  err.name = 'CombinedError';
  Object.assign(err, { graphQLErrors: [], networkError: new Error(message) });
  return err;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function Probe({ spy }: { spy: (ctx: ReturnType<typeof useSearch>) => void }) {
  spy(useSearch());
  return null;
}

function renderWithProvider() {
  let ctx!: ReturnType<typeof useSearch>;
  render(
    <SearchProvider>
      <Probe spy={c => { ctx = c; }} />
    </SearchProvider>,
  );
  return { getCtx: () => ctx };
}

async function openTask(getCtx: () => ReturnType<typeof useSearch>, id: string) {
  // openTaskInSheet awaits the outgoing flush, so drive it inside an async act.
  await act(async () => {
    getCtx().openTaskInSheet(id);
  });
}

function descriptionInput(): HTMLTextAreaElement {
  return screen.getByPlaceholderText('Add a description...') as HTMLTextAreaElement;
}

beforeEach(() => {
  for (const m of [mockUpdateTask, mockUpdatePriority, mockSkipOccurrence, mockUpdateRecurringTask]) {
    m.mockReset();
    m.mockResolvedValue(undefined);
  }
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('SearchProvider — switching task with an unsaved edit', () => {
  it('flushes the outgoing edit and completes the switch', async () => {
    const { getCtx } = renderWithProvider();
    await openTask(getCtx, 'task-a');

    fireEvent.change(descriptionInput(), { target: { value: 'frappe sur A' } });
    await openTask(getCtx, 'task-b');

    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(mockUpdateTask).toHaveBeenCalledWith(
      'task-a',
      expect.objectContaining({ description: 'frappe sur A' }),
    );
    expect(getCtx().openTaskId).toBe('task-b');
    expect(descriptionInput().value).toBe('desc B');
  });

  it('aborts the switch when the outgoing write fails', async () => {
    // Switching would leave the edit behind with no screen to fix it on.
    mockUpdateTask.mockRejectedValueOnce(combinedError('réseau coupé'));
    const { getCtx } = renderWithProvider();
    await openTask(getCtx, 'task-a');

    fireEvent.change(descriptionInput(), { target: { value: 'frappe sur A' } });
    await openTask(getCtx, 'task-b');

    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(getCtx().openTaskId).toBe('task-a');
    expect(descriptionInput().value).toBe('frappe sur A');
    expect(screen.getByTestId('task-sheet-autosave-retry')).toBeInTheDocument();
  });

  it('lets the switch through once the retry has written the edit', async () => {
    mockUpdateTask.mockRejectedValueOnce(combinedError('réseau coupé'));
    const { getCtx } = renderWithProvider();
    await openTask(getCtx, 'task-a');

    fireEvent.change(descriptionInput(), { target: { value: 'frappe sur A' } });
    await openTask(getCtx, 'task-b');
    expect(getCtx().openTaskId).toBe('task-a');

    await act(async () => {
      fireEvent.click(screen.getByTestId('task-sheet-autosave-retry'));
    });
    await openTask(getCtx, 'task-b');

    expect(mockUpdateTask).toHaveBeenCalledTimes(2);
    expect(getCtx().openTaskId).toBe('task-b');
    expect(descriptionInput().value).toBe('desc B');
  });

  it('switches straight through when nothing is pending', async () => {
    const { getCtx } = renderWithProvider();
    await openTask(getCtx, 'task-a');

    await openTask(getCtx, 'task-b');

    expect(mockUpdateTask).not.toHaveBeenCalled();
    expect(getCtx().openTaskId).toBe('task-b');
    expect(descriptionInput().value).toBe('desc B');
  });
});
