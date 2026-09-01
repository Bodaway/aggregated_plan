import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { TaskEditSheet } from './TaskEditSheet';
import type { FullTask } from '@/hooks/use-task-edit';

// ── Mock hooks & heavy sub-components ────────────────────────────────────────

// Every mutation now takes its target id explicitly: a debounced write must
// reach the task it was made for, not the one the panel is showing when it fires.
const mockUpdateTask = vi.fn(async (_id: string, _input: Record<string, unknown>) => {});
const mockUpdatePriority = vi.fn(async (_id: string, _urgency: string, _impact: string) => {});
const mockSkipOccurrence = vi.fn(async (_id: string) => {});
const mockUpdateRecurringTask = vi.fn(async (_id: string, _input: Record<string, unknown>) => {});
const mockRefetch = vi.fn();

let mockTask: FullTask | null = null;
/** Per-id tasks, for the tests that switch the panel from one task to another. */
const mockTasksById = new Map<string, FullTask>();


vi.mock('@/hooks/use-delegates', () => ({
  useDelegates: () => ({ delegates: ['Ahmed', 'Marie'] }),
}));

vi.mock('@/hooks/use-task-edit', () => ({
  useTaskEdit: (taskId: string | null) => ({
    task: taskId === null ? null : (mockTasksById.get(taskId) ?? mockTask),
    loading: false,
    error: null,
    updateTask: mockUpdateTask,
    updatePriority: mockUpdatePriority,
    skipOccurrence: mockSkipOccurrence,
    updateRecurringTask: mockUpdateRecurringTask,
    refetch: mockRefetch,
  }),
}));

vi.mock('@/components/markdown/MarkdownEditor', () => ({
  MarkdownEditor: ({ value, onChange, placeholder }: { value: string; onChange: (v: string) => void; placeholder?: string }) => (
    <textarea
      aria-label="Notes"
      value={value}
      onChange={e => onChange(e.target.value)}
      placeholder={placeholder}
    />
  ),
}));

vi.mock('@/components/worklog/WorklogSection', () => ({
  WorklogSection: ({ taskId }: { taskId: string }) => (
    <div data-testid="worklog-section" data-task-id={taskId} />
  ),
}));

vi.mock('@/components/gryzzly/GryzzlyTaskPicker', () => ({
  GryzzlyTaskPicker: () => <div data-testid="gryzzly-task-picker" />,
}));

// ── Fixtures ──────────────────────────────────────────────────────────────────

const BASE_TASK: FullTask = {
  id: 'task-id-xyz',
  title: 'Test task',
  description: null,
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

const RECURRING_TASK: FullTask = {
  ...BASE_TASK,
  recurrenceId: 'xyz',
  isRecurring: true,
};

const PERSONAL_TASK: FullTask = { ...BASE_TASK, source: 'PERSONAL' };

/** Distinct urgency/impact so each priority select is addressable on its own. */
const PRIORITY_TASK: FullTask = { ...PERSONAL_TASK, urgency: 'HIGH', impact: 'LOW' };

/** Jira task with both time fields set, so the override inputs get distinct placeholders. */
const JIRA_TASK: FullTask = {
  ...BASE_TASK,
  source: 'JIRA',
  jiraRemainingSeconds: 7200,
  jiraOriginalEstimateSeconds: 3600,
};

/** The autosave debounce for free-text fields. */
const DEBOUNCE_MS = 700;

// ── Helpers ───────────────────────────────────────────────────────────────────

function renderSheet(taskId: string | null = 'task-id-xyz') {
  const onClose = vi.fn();
  const onUpdated = vi.fn();
  const view = render(<TaskEditSheet taskId={taskId} onClose={onClose} onUpdated={onUpdated} />);
  /**
   * Re-render the panel, optionally on another task. With `mockTask` reassigned
   * beforehand this stands in for the `network-only` refetch `use-task-edit`
   * runs after every mutation.
   */
  const rerender = (nextTaskId: string | null = taskId) =>
    view.rerender(<TaskEditSheet taskId={nextTaskId} onClose={onClose} onUpdated={onUpdated} />);
  return { onClose, onUpdated, rerender };
}

/** Move the fake clock, letting the promises each timer starts settle. */
async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

/** Drain queued microtasks (mutation promises) without moving the clock. */
async function settle() {
  await act(async () => {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
  });
}

interface Deferred {
  readonly promise: Promise<void>;
  readonly resolve: () => void;
  readonly reject: (reason: unknown) => void;
}

function deferred(): Deferred {
  let resolve!: () => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<void>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/**
 * The shape urql hands back in `result.error`, which `use-task-edit` now throws
 * (see use-task-edit.test.ts for that seam). The panel only ever sees the
 * rejection, so that is what the mock produces here.
 */
function combinedError(message: string): Error {
  const err = new Error(`[Network] ${message}`);
  err.name = 'CombinedError';
  Object.assign(err, { graphQLErrors: [], networkError: new Error(message) });
  return err;
}

type Payload = Record<string, unknown>;

function updateTaskCalls(): [string, Payload][] {
  return mockUpdateTask.mock.calls as unknown as [string, Payload][];
}

/**
 * The single `updateTask` payload of the save. One edit is one write: a job
 * re-reads the baseline when it starts, so a field the in-flight write already
 * landed is never planned again.
 */
function savedPayload(): Payload {
  expect(mockUpdateTask).toHaveBeenCalledOnce();
  return updateTaskCalls()[0][1];
}

function updateTaskPayloads(): Payload[] {
  return updateTaskCalls().map(c => c[1]);
}

/** Every `updateRecurringTask` payload, in call order. */
function recurringPayloads(): Payload[] {
  return (mockUpdateRecurringTask.mock.calls as unknown as [string, Payload][]).map(c => c[1]);
}

/**
 * Whether any write at all carried this key. Field-presence claims are per-call
 * and stay strict: a key that must not be sent must be absent from every call,
 * however many the round trip produced.
 */
function anyUpdateTaskCarries(key: string): boolean {
  return updateTaskPayloads().some(payload => key in payload);
}

/** The task each `updateTask` write was addressed to, in call order. */
function updateTaskTargets(): string[] {
  return updateTaskCalls().map(c => c[0]);
}

/**
 * Flush whatever edit is pending and close the panel. This is the replacement
 * for the Save button the autosave conversion removes: the footer button both
 * flushes and closes, so the payload assertions it used to anchor still hold.
 */
function flushEdits() {
  fireEvent.click(screen.getByTestId('task-sheet-cancel'));
}

function autosaveText(): string {
  return screen.queryByTestId('task-sheet-autosave-status')?.textContent ?? '';
}

function deadlineInput(): HTMLInputElement {
  return screen.getByLabelText('Échéance') as HTMLInputElement;
}

function descriptionInput(): HTMLTextAreaElement {
  return screen.getByPlaceholderText('Add a description...') as HTMLTextAreaElement;
}

function notesInput(): HTMLTextAreaElement {
  return screen.getByLabelText('Notes') as HTMLTextAreaElement;
}

function headerCloseButton(): HTMLElement {
  return screen.getByTestId('task-sheet-header-close');
}

function backdrop(): HTMLElement {
  return screen.getByTestId('task-sheet-backdrop');
}

beforeEach(() => {
  mockTasksById.clear();
  mockTask = null;
  for (const m of [mockUpdateTask, mockUpdatePriority, mockSkipOccurrence, mockUpdateRecurringTask]) {
    m.mockReset();
    m.mockResolvedValue(undefined);
  }
  mockRefetch.mockReset();
});

// ── Tests: footer chrome after the autosave conversion ────────────────────────

describe('TaskEditSheet — autosave chrome', () => {
  beforeEach(() => {
    mockTask = PERSONAL_TASK;
  });

  it('offers no Save button at all', () => {
    renderSheet();
    expect(screen.queryByTestId('task-sheet-save')).toBeNull();
    expect(screen.queryByRole('button', { name: /^save$/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /^enregistrer$/i })).toBeNull();
  });

  it('labels the remaining footer button "Fermer"', () => {
    renderSheet();
    const closeBtn = screen.getByTestId('task-sheet-cancel');
    expect(closeBtn.textContent?.trim()).toBe('Fermer');
    expect(screen.queryByRole('button', { name: /^cancel$/i })).toBeNull();
  });

  it('shows an autosave status indicator', () => {
    renderSheet();
    expect(screen.getByTestId('task-sheet-autosave-status')).toBeInTheDocument();
  });

  it('offers no retry button while nothing has failed', () => {
    renderSheet();
    expect(screen.queryByTestId('task-sheet-autosave-retry')).toBeNull();
  });
});

// ── Tests: recurring task ─────────────────────────────────────────────────────

describe('TaskEditSheet — recurring task', () => {
  beforeEach(() => {
    mockTask = RECURRING_TASK;
  });

  it('renders the violet recurring-task banner', () => {
    renderSheet();
    expect(
      screen.getByText(
        /Cette tâche fait partie d'une série\. Le statut et les dates s'appliquent à cette occurrence/i
      )
    ).toBeTruthy();
  });

  it('renders the "Ignorer cette occurrence" button', () => {
    renderSheet();
    expect(screen.getByRole('button', { name: /ignorer cette occurrence/i })).toBeTruthy();
  });

  it('renders no Save button (autosave replaces it)', () => {
    renderSheet();
    expect(screen.queryByTestId('task-sheet-save')).toBeNull();
  });

  it('clicking skip calls skipOccurrence and then onClose', async () => {
    const { onClose } = renderSheet();

    const skipBtn = screen.getByRole('button', { name: /ignorer cette occurrence/i });
    fireEvent.click(skipBtn);

    await waitFor(() => expect(mockSkipOccurrence).toHaveBeenCalledOnce());
    await waitFor(() => expect(onClose).toHaveBeenCalledOnce());
  });

  it('a failed skip keeps the panel open and says so', async () => {
    // The occurrence is still on the calendar — closing would claim otherwise.
    mockSkipOccurrence.mockRejectedValueOnce(combinedError('skip refusé'));
    const { onClose, onUpdated } = renderSheet();

    fireEvent.click(screen.getByTestId('task-sheet-skip'));

    await waitFor(() => expect(screen.getByTestId('task-sheet-skip-error')).toBeInTheDocument());
    expect(onClose).not.toHaveBeenCalled();
    expect(onUpdated).not.toHaveBeenCalled();
    expect(screen.getByTestId('task-sheet-skip')).toBeInTheDocument();
  });

  it('clears the skip error once a retried skip lands', async () => {
    mockSkipOccurrence.mockRejectedValueOnce(combinedError('skip refusé'));
    const { onClose } = renderSheet();

    fireEvent.click(screen.getByTestId('task-sheet-skip'));
    await waitFor(() => expect(screen.getByTestId('task-sheet-skip-error')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('task-sheet-skip'));

    await waitFor(() => expect(onClose).toHaveBeenCalledOnce());
    expect(mockSkipOccurrence).toHaveBeenCalledTimes(2);
    expect(screen.queryByTestId('task-sheet-skip-error')).toBeNull();
  });

  it('a per-instance change (status) goes through updateTask, not updateRecurringTask', async () => {
    renderSheet();

    const statusSelect = screen.getByDisplayValue('To Do');
    fireEvent.change(statusSelect, { target: { value: 'IN_PROGRESS' } });

    await settle();
    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(mockUpdateRecurringTask).not.toHaveBeenCalled();
  });

  it('a template change (description) goes through updateRecurringTask, not updateTask', async () => {
    renderSheet();

    fireEvent.change(descriptionInput(), { target: { value: 'New description for the series' } });
    flushEdits();

    await settle();
    expect(mockUpdateRecurringTask).toHaveBeenCalledOnce();
    expect(mockUpdateTask).not.toHaveBeenCalled();
  });

  it('a mixed change fires both mutations', async () => {
    renderSheet();

    fireEvent.change(screen.getByDisplayValue('To Do'), { target: { value: 'DONE' } });
    fireEvent.change(descriptionInput(), { target: { value: 'Updated description' } });
    flushEdits();

    await settle();

    // One edit, one write, each through the mutation that owns it: the occurrence
    // field via updateTask, the series field via updateRecurringTask.
    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(savedPayload().status).toBe('DONE');
    expect(recurringPayloads()).toEqual([{ description: 'Updated description' }]);
  });
});

// ── Tests: one-shot task ──────────────────────────────────────────────────────

describe('TaskEditSheet — one-shot task', () => {
  beforeEach(() => {
    mockTask = BASE_TASK;
  });

  it('does NOT render the recurring-task banner', () => {
    renderSheet();
    expect(screen.queryByText(/cette tâche fait partie d'une série récurrente/i)).toBeNull();
  });

  it('does NOT render the skip button', () => {
    renderSheet();
    expect(screen.queryByRole('button', { name: /ignorer cette occurrence/i })).toBeNull();
  });

  it('renders no Save button', () => {
    renderSheet();
    expect(screen.queryByTestId('task-sheet-save')).toBeNull();
  });

  it('closing with nothing edited closes without writing', async () => {
    const { onClose } = renderSheet();

    flushEdits();

    await settle();
    expect(onClose).toHaveBeenCalledOnce();
    expect(mockUpdateTask).not.toHaveBeenCalled();
    expect(mockUpdatePriority).not.toHaveBeenCalled();
  });
});

// ── Tests: delegation ─────────────────────────────────────────────────────────

describe('delegation', () => {
  beforeEach(() => {
    mockTask = { ...BASE_TASK };
  });

  it('renders the delegated-to input with learned suggestions', () => {
    renderSheet();
    const input = screen.getByLabelText(/delegated to/i);
    expect(input).toHaveAttribute('list', 'delegate-suggestions');
    const datalist = document.getElementById('delegate-suggestions');
    expect(datalist).not.toBeNull();
    const options = Array.from(datalist!.querySelectorAll('option')).map(o => o.getAttribute('value'));
    expect(options).toEqual(['Ahmed', 'Marie']);
  });

  it('sends delegatedTo when a name is entered', async () => {
    renderSheet();
    fireEvent.change(screen.getByLabelText(/delegated to/i), { target: { value: 'Marie' } });
    flushEdits();
    await settle();
    expect(mockUpdateTask).toHaveBeenCalledWith(
      'task-id-xyz',
      expect.objectContaining({ delegatedTo: 'Marie' })
    );
  });

  it('sends delegatedTo: null when the field is emptied', async () => {
    mockTask = { ...BASE_TASK, delegatedTo: 'Marie' };
    renderSheet();
    fireEvent.change(screen.getByLabelText(/delegated to/i), { target: { value: '' } });
    flushEdits();
    await settle();
    expect(mockUpdateTask).toHaveBeenCalledWith(
      'task-id-xyz',
      expect.objectContaining({ delegatedTo: null })
    );
  });

  it('does not send delegatedTo when unchanged', async () => {
    mockTask = { ...BASE_TASK, delegatedTo: 'Marie', notes: 'x' };
    renderSheet();
    // change something else so the flush has an edit to write
    fireEvent.change(notesInput(), { target: { value: 'y' } });
    flushEdits();
    await settle();
    expect(mockUpdateTask).toHaveBeenCalled();
    expect(mockUpdateTask).toHaveBeenCalledWith(
      'task-id-xyz',
      expect.not.objectContaining({ delegatedTo: expect.anything() })
    );
  });
});

// ── Tests: copy title ─────────────────────────────────────────────────────────

describe('copy title', () => {
  const writeText = vi.fn(async () => {});

  beforeEach(() => {
    mockTask = { ...BASE_TASK, title: 'Refactor the sync engine' };
    writeText.mockReset();
    writeText.mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
      writable: true,
    });
  });

  it('renders a copy-title button in the header', () => {
    renderSheet();
    expect(screen.getByRole('button', { name: /copy title/i })).toBeTruthy();
  });

  it('writes the task title to the clipboard on click', async () => {
    renderSheet();
    fireEvent.click(screen.getByTestId('task-sheet-copy-title'));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('Refactor the sync engine'));
  });

  it('confirms the copy to the user', async () => {
    renderSheet();
    fireEvent.click(screen.getByTestId('task-sheet-copy-title'));
    await waitFor(() => expect(screen.getByRole('button', { name: /copied/i })).toBeTruthy());
  });

  it('shows no confirmation when the clipboard write fails', async () => {
    writeText.mockRejectedValueOnce(new Error('denied'));
    renderSheet();
    fireEvent.click(screen.getByTestId('task-sheet-copy-title'));
    await waitFor(() => expect(writeText).toHaveBeenCalled());
    expect(screen.queryByRole('button', { name: /copied/i })).toBeNull();
  });
});

// ── Tests: deadline editing (R76) ─────────────────────────────────────────────
//
// `UpdateTaskInput.deadline` is a `MaybeUndefined` (SPEC_TECHNIQUE §23.4):
// omitted means "leave it alone", `null` means "clear it", a value means "set
// it". The three are distinct on the wire, and conflating the first two would
// silently wipe deadlines on every save — hence the key-presence assertions
// rather than value comparisons.

describe('TaskEditSheet — deadline field visibility (R76)', () => {
  it('offers the date input on a personal task', () => {
    mockTask = PERSONAL_TASK;
    renderSheet();

    expect(deadlineInput()).toHaveAttribute('type', 'date');
  });

  it('prefills the input with the current deadline', () => {
    mockTask = { ...PERSONAL_TASK, deadline: '2026-09-30' };
    renderSheet();

    expect(deadlineInput().value).toBe('2026-09-30');
  });

  it.each([
    ['JIRA', 'Jira'],
    ['EXCEL', 'Excel'],
    ['OUTLOOK', 'Outlook'],
  ])('offers no date input on a %s task, and says who owns the deadline', (source, label) => {
    mockTask = { ...BASE_TASK, source, deadline: '2026-09-30' };
    renderSheet();

    expect(screen.queryByLabelText('Échéance')).toBeNull();
    expect(document.getElementById('task-deadline')).toBeNull();
    // The value is still shown, annotated with the system that rewrites it.
    expect(screen.getByText('2026-09-30')).toBeInTheDocument();
    expect(
      screen.getByText(new RegExp(`définie par ${label}, réécrite à chaque synchronisation`)),
    ).toBeInTheDocument();
  });

  it('shows no deadline row at all on a synced task that has none', () => {
    mockTask = { ...BASE_TASK, source: 'JIRA', deadline: null };
    renderSheet();

    expect(screen.queryByLabelText('Échéance')).toBeNull();
    expect(screen.queryByText(/définie par/)).toBeNull();
  });

  it('offers the clear button only once a deadline is set', async () => {
    mockTask = PERSONAL_TASK;
    renderSheet();
    expect(screen.queryByRole('button', { name: /effacer l’échéance|effacer l'échéance/i })).toBeNull();

    fireEvent.change(deadlineInput(), { target: { value: '2026-10-15' } });

    expect(screen.getByRole('button', { name: /effacer l’échéance|effacer l'échéance/i })).toBeInTheDocument();
    // The change also fires an immediate write; let it land inside act().
    await settle();
  });
});

describe('TaskEditSheet — deadline on the wire (R76)', () => {
  it('omits the key entirely when the field was never touched', async () => {
    // The dangerous case: `deadline: null` here would clear a real deadline on
    // every unrelated save. Absence and null are NOT interchangeable.
    mockTask = { ...PERSONAL_TASK, deadline: '2026-09-30', notes: 'x' };
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'y' } });
    flushEdits();

    await settle();
    // Non-vacuous: one write did go out, it just must not mention the deadline.
    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(anyUpdateTaskCarries('deadline')).toBe(false);
  });

  it('omits the key when the field is re-typed to the value it already had', async () => {
    mockTask = { ...PERSONAL_TASK, deadline: '2026-09-30', notes: 'x' };
    renderSheet();

    // A no-op edit must not write at all — hence the single-call assertion in
    // `savedPayload()` on top of the missing key.
    fireEvent.change(deadlineInput(), { target: { value: '2026-09-30' } });
    fireEvent.change(notesInput(), { target: { value: 'y' } });
    flushEdits();

    await settle();
    // Non-vacuous: one write did go out, it just must not mention the deadline.
    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(anyUpdateTaskCarries('deadline')).toBe(false);
  });

  it('sends an explicit null when the user clears the deadline', async () => {
    mockTask = { ...PERSONAL_TASK, deadline: '2026-09-30' };
    renderSheet();

    fireEvent.click(screen.getByRole('button', { name: /effacer l’échéance|effacer l'échéance/i }));
    flushEdits();

    await settle();
    const payload = savedPayload();
    expect('deadline' in payload).toBe(true);
    expect(payload.deadline).toBeNull();
  });

  it('sends an explicit null when the input itself is emptied', async () => {
    mockTask = { ...PERSONAL_TASK, deadline: '2026-09-30' };
    renderSheet();

    fireEvent.change(deadlineInput(), { target: { value: '' } });
    flushEdits();

    await settle();
    expect(savedPayload().deadline).toBeNull();
  });

  it('sends the plain date when the user sets a deadline', async () => {
    mockTask = PERSONAL_TASK;
    renderSheet();

    fireEvent.change(deadlineInput(), { target: { value: '2026-10-15' } });
    flushEdits();

    await settle();
    expect(savedPayload().deadline).toBe('2026-10-15');
  });

  it('sends the plain date when the user moves an existing deadline', async () => {
    mockTask = { ...PERSONAL_TASK, deadline: '2026-09-30' };
    renderSheet();

    fireEvent.change(deadlineInput(), { target: { value: '2026-10-15' } });
    flushEdits();

    await settle();
    expect(savedPayload().deadline).toBe('2026-10-15');
  });

  it('never sends a deadline for a synced task, whatever else changes', async () => {
    // The guard is `canEditDeadline`, not the mere absence of the input.
    mockTask = { ...BASE_TASK, source: 'JIRA', deadline: '2026-09-30', notes: 'x' };
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'y' } });
    flushEdits();

    await settle();
    // Non-vacuous: one write did go out, it just must not mention the deadline.
    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(anyUpdateTaskCarries('deadline')).toBe(false);
  });

  it('keeps the deadline out of the recurring-template mutation', async () => {
    // The deadline is per-instance, like the planned date: a series has none.
    mockTask = { ...RECURRING_TASK, source: 'PERSONAL' };
    renderSheet();

    fireEvent.change(deadlineInput(), { target: { value: '2026-10-15' } });
    fireEvent.change(descriptionInput(), { target: { value: 'Série mise à jour' } });
    flushEdits();

    await settle();

    // The series has no deadline, so no template payload may carry one.
    expect(recurringPayloads()).toEqual([{ description: 'Série mise à jour' }]);
    for (const payload of recurringPayloads()) expect('deadline' in payload).toBe(false);
    expect(savedPayload().deadline).toBe('2026-10-15');
  });
});

// ── Tests: autosave — immediate fields ────────────────────────────────────────
//
// Selects and dates are single-gesture edits: there is nothing to debounce, so
// they must reach the server without any timer having to fire.

describe('autosave — selects and dates write immediately', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('writes a status change with no timer advance', async () => {
    mockTask = PERSONAL_TASK;
    renderSheet();

    fireEvent.change(screen.getByDisplayValue('To Do'), { target: { value: 'IN_PROGRESS' } });
    await settle();

    expect(savedPayload().status).toBe('IN_PROGRESS');
  });

  it('writes an urgency change through updatePriority with no timer advance', async () => {
    mockTask = PRIORITY_TASK;
    renderSheet();

    fireEvent.change(screen.getByDisplayValue('High'), { target: { value: 'CRITICAL' } });
    await settle();

    expect(mockUpdatePriority).toHaveBeenCalledOnce();
    expect(mockUpdatePriority).toHaveBeenCalledWith('task-id-xyz', 'CRITICAL', 'LOW');
  });

  it('writes an impact change through updatePriority with no timer advance', async () => {
    mockTask = PRIORITY_TASK;
    renderSheet();

    fireEvent.change(screen.getByDisplayValue('Low'), { target: { value: 'CRITICAL' } });
    await settle();

    expect(mockUpdatePriority).toHaveBeenCalledOnce();
    expect(mockUpdatePriority).toHaveBeenCalledWith('task-id-xyz', 'HIGH', 'CRITICAL');
  });

  it('writes a planned-date change with no timer advance', async () => {
    mockTask = PERSONAL_TASK;
    renderSheet();

    fireEvent.change(screen.getByDisplayValue('2026-04-27'), { target: { value: '2026-05-04' } });
    await settle();

    expect(savedPayload().plannedStart).toBe('2026-05-04T08:00:00Z');
  });

  it('writes a deadline change with no timer advance', async () => {
    mockTask = PERSONAL_TASK;
    renderSheet();

    fireEvent.change(deadlineInput(), { target: { value: '2026-10-15' } });
    await settle();

    expect(savedPayload().deadline).toBe('2026-10-15');
  });

  it('does not lose a second choice made while the first write is in flight', async () => {
    const gate = deferred();
    mockUpdateTask.mockImplementationOnce(() => gate.promise);
    mockTask = PERSONAL_TASK;
    renderSheet();

    fireEvent.change(screen.getByDisplayValue('To Do'), { target: { value: 'IN_PROGRESS' } });
    await settle();
    expect(mockUpdateTask).toHaveBeenCalledOnce();

    // Second choice, made before the first write has answered.
    fireEvent.change(screen.getByDisplayValue('2026-04-27'), { target: { value: '2026-05-04' } });
    await settle();

    gate.resolve();
    await settle();

    expect(mockUpdateTask).toHaveBeenCalledTimes(2);
    expect(updateTaskPayloads()[1].plannedStart).toBe('2026-05-04T08:00:00Z');
  });

  it('writes a select change exactly once, not again when the debounce would have fired', async () => {
    mockTask = PERSONAL_TASK;
    renderSheet();

    fireEvent.change(screen.getByDisplayValue('To Do'), { target: { value: 'DONE' } });
    await settle();
    expect(mockUpdateTask).toHaveBeenCalledOnce();

    await advance(3 * DEBOUNCE_MS);
    expect(mockUpdateTask).toHaveBeenCalledOnce();
  });
});

// ── Tests: autosave — debounced fields ────────────────────────────────────────

describe('autosave — free text debounces on 700 ms', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockTask = PERSONAL_TASK;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('writes nothing before the debounce elapses', async () => {
    renderSheet();

    fireEvent.change(descriptionInput(), { target: { value: 'un brouillon' } });
    await advance(DEBOUNCE_MS - 1);

    expect(mockUpdateTask).not.toHaveBeenCalled();
  });

  it('writes the final description once the debounce elapses', async () => {
    renderSheet();

    fireEvent.change(descriptionInput(), { target: { value: 'un brouillon' } });
    await advance(DEBOUNCE_MS);

    expect(savedPayload().description).toBe('un brouillon');
  });

  it('collapses successive edits into a single mutation carrying the last value', async () => {
    renderSheet();
    const desc = descriptionInput();

    fireEvent.change(desc, { target: { value: 'a' } });
    await advance(300);
    fireEvent.change(desc, { target: { value: 'ab' } });
    await advance(300);
    fireEvent.change(desc, { target: { value: 'abc' } });
    await advance(DEBOUNCE_MS);

    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(savedPayload().description).toBe('abc');
  });

  it('debounces the notes field', async () => {
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'décision du jour' } });
    await advance(DEBOUNCE_MS - 1);
    expect(mockUpdateTask).not.toHaveBeenCalled();

    await advance(1);
    expect(savedPayload().notes).toBe('décision du jour');
  });

  it('debounces the delegate field', async () => {
    renderSheet();

    fireEvent.change(screen.getByLabelText(/delegated to/i), { target: { value: 'Marie' } });
    await advance(DEBOUNCE_MS - 1);
    expect(mockUpdateTask).not.toHaveBeenCalled();

    await advance(1);
    expect(savedPayload().delegatedTo).toBe('Marie');
  });

  it('turns a typed delegate name into one mutation, not one per character', async () => {
    renderSheet();
    const input = screen.getByLabelText(/delegated to/i);

    for (const value of ['M', 'Ma', 'Mar', 'Mari', 'Marie']) {
      fireEvent.change(input, { target: { value } });
      await advance(120);
    }
    expect(mockUpdateTask).not.toHaveBeenCalled();

    await advance(DEBOUNCE_MS);

    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(savedPayload().delegatedTo).toBe('Marie');
  });

  it('debounces the estimated-hours field', async () => {
    renderSheet();

    fireEvent.change(screen.getByPlaceholderText('e.g. 4'), { target: { value: '3.5' } });
    await advance(DEBOUNCE_MS - 1);
    expect(mockUpdateTask).not.toHaveBeenCalled();

    await advance(1);
    expect(savedPayload().estimatedHours).toBe(3.5);
  });

  it('debounces the Jira remaining override', async () => {
    mockTask = JIRA_TASK;
    renderSheet();

    fireEvent.change(screen.getByPlaceholderText('2.0h'), { target: { value: '3' } });
    await advance(DEBOUNCE_MS - 1);
    expect(mockUpdateTask).not.toHaveBeenCalled();

    await advance(1);
    expect(savedPayload().remainingHoursOverride).toBe(3);
  });

  it('debounces the Jira estimate override', async () => {
    mockTask = JIRA_TASK;
    renderSheet();

    fireEvent.change(screen.getByPlaceholderText('1.0h'), { target: { value: '5' } });
    await advance(DEBOUNCE_MS);

    expect(savedPayload().estimatedHoursOverride).toBe(5);
  });
});

// ── Tests: autosave — real keystrokes ─────────────────────────────────────────
//
// The one test driven by a real keyboard rather than synthetic change events.
// Keep it on the real clock: userEvent deadlocks against faked timers here even
// with `advanceTimers` and `delay: null`, and the point of the test is the
// keystroke sequence, not the exact delay. Converting it back hangs the suite.

describe('autosave — real keystrokes', () => {
  beforeEach(() => {
    mockTask = PERSONAL_TASK;
  });

  it('turns three keystrokes into one mutation', async () => {
    const user = userEvent.setup();
    renderSheet();

    await user.type(descriptionInput(), 'abc');
    expect(mockUpdateTask).not.toHaveBeenCalled();

    await waitFor(() => expect(mockUpdateTask).toHaveBeenCalled(), { timeout: 4 * DEBOUNCE_MS });

    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(savedPayload().description).toBe('abc');
  });
});

// ── Tests: autosave — closing flushes ─────────────────────────────────────────

describe('autosave — every close path flushes first', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockTask = PERSONAL_TASK;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('the footer button flushes the pending edit, then closes', async () => {
    const { onClose } = renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'à ne pas perdre' } });
    fireEvent.click(screen.getByTestId('task-sheet-cancel'));
    await settle();

    expect(savedPayload().notes).toBe('à ne pas perdre');
    expect(onClose).toHaveBeenCalledOnce();
    expect(mockUpdateTask.mock.invocationCallOrder[0]).toBeLessThan(
      onClose.mock.invocationCallOrder[0],
    );
  });

  it('Escape flushes the pending edit, then closes', async () => {
    const { onClose } = renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'à ne pas perdre' } });
    fireEvent.keyDown(document, { key: 'Escape' });
    await settle();

    expect(savedPayload().notes).toBe('à ne pas perdre');
    expect(onClose).toHaveBeenCalledOnce();
    expect(mockUpdateTask.mock.invocationCallOrder[0]).toBeLessThan(
      onClose.mock.invocationCallOrder[0],
    );
  });

  it('the backdrop flushes the pending edit, then closes', async () => {
    const { onClose } = renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'à ne pas perdre' } });
    fireEvent.click(backdrop());
    await settle();

    expect(savedPayload().notes).toBe('à ne pas perdre');
    expect(onClose).toHaveBeenCalledOnce();
    expect(mockUpdateTask.mock.invocationCallOrder[0]).toBeLessThan(
      onClose.mock.invocationCallOrder[0],
    );
  });

  it('the header X flushes the pending edit, then closes', async () => {
    const { onClose } = renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'à ne pas perdre' } });
    fireEvent.click(headerCloseButton());
    await settle();

    expect(savedPayload().notes).toBe('à ne pas perdre');
    expect(onClose).toHaveBeenCalledOnce();
    expect(mockUpdateTask.mock.invocationCallOrder[0]).toBeLessThan(
      onClose.mock.invocationCallOrder[0],
    );
  });

  it('the flush disarms the debounce instead of racing it', async () => {
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'à ne pas perdre' } });
    fireEvent.click(screen.getByTestId('task-sheet-cancel'));
    await settle();
    expect(mockUpdateTask).toHaveBeenCalledOnce();

    await advance(3 * DEBOUNCE_MS);
    expect(mockUpdateTask).toHaveBeenCalledOnce();
  });
});

// ── Tests: autosave — the refetch must not clobber typing ─────────────────────

describe('autosave — a landing refetch never clobbers what is being typed', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('keeps the typed text when the server task comes back with an older value', async () => {
    mockTask = { ...PERSONAL_TASK, description: 'valeur serveur initiale' };
    const { rerender } = renderSheet();

    fireEvent.change(descriptionInput(), { target: { value: 'ce que je tape' } });

    // `use-task-edit` refetches network-only after every mutation; the answer
    // arrives with values that predate the keystroke.
    mockTask = { ...PERSONAL_TASK, description: 'valeur serveur plus ancienne' };
    rerender();

    expect(descriptionInput().value).toBe('ce que je tape');
  });

  it('keeps the typed text across a refetch that lands mid-flight', async () => {
    const gate = deferred();
    mockUpdateTask.mockImplementationOnce(() => gate.promise);
    mockTask = { ...PERSONAL_TASK, notes: 'note serveur' };
    const { rerender } = renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'ma note' } });
    await advance(DEBOUNCE_MS);
    expect(mockUpdateTask).toHaveBeenCalledOnce();

    mockTask = { ...PERSONAL_TASK, notes: 'note serveur' };
    rerender();
    gate.resolve();
    await settle();

    expect(notesInput().value).toBe('ma note');
  });

  // Only the touched field is a contract: an untouched field may legitimately
  // take the fresher server value, so nothing is asserted about it.
  it('judges the clobber only on the field the user touched', async () => {
    mockTask = { ...PERSONAL_TASK, description: 'd0', notes: 'n0' };
    const { rerender } = renderSheet();

    fireEvent.change(descriptionInput(), { target: { value: 'ma frappe' } });

    mockTask = { ...PERSONAL_TASK, description: 'd-serveur', notes: 'n-serveur' };
    rerender();

    expect(descriptionInput().value).toBe('ma frappe');
  });
});

// ── Tests: autosave — switching task ──────────────────────────────────────────

describe('autosave — switching task re-hydrates the form', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockTasksById.set('task-a', { ...PERSONAL_TASK, id: 'task-a', description: 'desc A' });
    mockTasksById.set('task-b', { ...PERSONAL_TASK, id: 'task-b', description: 'desc B' });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('issues the outgoing edit against the outgoing task before showing the new one', async () => {
    // Cmd/Ctrl+K switches task without closing the panel. A's queued write must
    // go out, bound to A, and only then may the form belong to B.
    const { rerender } = renderSheet('task-a');
    expect(descriptionInput().value).toBe('desc A');

    fireEvent.change(descriptionInput(), { target: { value: 'frappe sur A' } });
    rerender('task-b');
    await settle();

    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(savedPayload().description).toBe('frappe sur A');
    expect(updateTaskTargets()).toEqual(['task-a']);
    expect(descriptionInput().value).toBe('desc B');
  });

  it('still shows the outgoing task at the moment its write goes out', async () => {
    // Ordering, observed from inside the mutation: if the form had already been
    // re-hydrated, the write would be carrying B's context.
    let shownWhenIssued: string | null = null;
    mockUpdateTask.mockImplementationOnce(async (id: string) => {
      shownWhenIssued = descriptionInput().value;
      expect(id).toBe('task-a');
    });
    const { rerender } = renderSheet('task-a');

    fireEvent.change(descriptionInput(), { target: { value: 'frappe sur A' } });
    rerender('task-b');
    await settle();

    expect(shownWhenIssued).toBe('frappe sur A');
    expect(descriptionInput().value).toBe('desc B');
  });

  it('drops nothing when the outgoing task had no pending edit', async () => {
    const { rerender } = renderSheet('task-a');

    rerender('task-b');
    await settle();

    expect(mockUpdateTask).not.toHaveBeenCalled();
    expect(descriptionInput().value).toBe('desc B');
  });

  it('resets the autosave indicator', async () => {
    const { rerender } = renderSheet('task-a');

    fireEvent.change(descriptionInput(), { target: { value: 'frappe sur A' } });
    await advance(DEBOUNCE_MS);
    expect(autosaveText()).toMatch(/Enregistré/);

    rerender('task-b');

    expect(autosaveText()).not.toMatch(/Enregistré/);
    expect(autosaveText()).not.toMatch(/Modification/);
  });
});

// ── Tests: autosave — a late answer must not patch the new task ───────────────
//
// Review finding #4: A's mutation can resolve after B's data has landed. If the
// write-back patched the baseline in place, B's untouched fields would look
// dirty and the next flush would rewrite them.

describe('autosave — a late answer never patches the incoming task baseline', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockTasksById.set('task-a', { ...PERSONAL_TASK, id: 'task-a', description: 'desc A' });
    mockTasksById.set('task-b', { ...PERSONAL_TASK, id: 'task-b', description: 'desc B', notes: 'notes B' });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('leaves the incoming task with nothing to write', async () => {
    const gate = deferred();
    mockUpdateTask.mockImplementationOnce(() => gate.promise);
    const { rerender } = renderSheet('task-a');

    fireEvent.change(descriptionInput(), { target: { value: 'frappe sur A' } });
    rerender('task-b');
    await settle();
    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(descriptionInput().value).toBe('desc B');

    // A's answer lands while B is on screen.
    gate.resolve();
    await settle();

    // B was never touched, so closing it must write nothing at all.
    fireEvent.click(screen.getByTestId('task-sheet-cancel'));
    await settle();

    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(updateTaskTargets()).toEqual(['task-a']);
  });

  it('does not rewrite the incoming series template', async () => {
    // The same bug on a recurring B would push A's description onto B's whole
    // series, not just one occurrence.
    mockTasksById.set('task-b', {
      ...RECURRING_TASK,
      id: 'task-b',
      recurrenceId: 'rec-b',
      description: 'desc B',
    });
    const gate = deferred();
    mockUpdateTask.mockImplementationOnce(() => gate.promise);
    const { rerender } = renderSheet('task-a');

    fireEvent.change(descriptionInput(), { target: { value: 'frappe sur A' } });
    rerender('task-b');
    await settle();

    gate.resolve();
    await settle();

    fireEvent.click(screen.getByTestId('task-sheet-cancel'));
    await settle();

    expect(mockUpdateRecurringTask).not.toHaveBeenCalled();
  });

  it('keeps the incoming task edits bound to the incoming task', async () => {
    const gate = deferred();
    mockUpdateTask.mockImplementationOnce(() => gate.promise);
    const { rerender } = renderSheet('task-a');

    fireEvent.change(descriptionInput(), { target: { value: 'frappe sur A' } });
    rerender('task-b');
    await settle();

    fireEvent.change(notesInput(), { target: { value: 'frappe sur B' } });
    gate.resolve();
    await advance(DEBOUNCE_MS);

    expect(mockUpdateTask).toHaveBeenCalledTimes(2);
    expect(updateTaskTargets()).toEqual(['task-a', 'task-b']);
    expect(updateTaskPayloads()[1]).toEqual({ notes: 'frappe sur B' });
  });
});

// ── Tests: autosave — a partly failed flush ───────────────────────────────────
//
// A flush can issue up to three mutations. Each one that landed must be written
// back on its own, so the retry resends only what actually failed.

describe('autosave — a partly failed flush retries only what failed', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockTask = PRIORITY_TASK;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  /** Queue a text edit, then trip an immediate one — both go in a single flush. */
  function editBothHalves() {
    fireEvent.change(notesInput(), { target: { value: 'note à sauver' } });
    fireEvent.change(screen.getByDisplayValue('High'), { target: { value: 'CRITICAL' } });
  }

  it('does not resend the priority when only the task write failed', async () => {
    mockUpdateTask.mockRejectedValueOnce(combinedError('write refused'));
    renderSheet();

    editBothHalves();
    await settle();

    expect(mockUpdatePriority).toHaveBeenCalledOnce();
    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(autosaveText()).toMatch(/échec|erreur|non enregistré/i);

    fireEvent.click(screen.getByTestId('task-sheet-autosave-retry'));
    await settle();

    // The priority landed the first time; resending it would be a second write
    // of a value the server already holds.
    expect(mockUpdatePriority).toHaveBeenCalledOnce();
    expect(mockUpdateTask).toHaveBeenCalledTimes(2);
    expect(updateTaskPayloads()[1].notes).toBe('note à sauver');
    expect(autosaveText()).toMatch(/Enregistré/);
  });

  it('does not resend the task fields when only the priority write failed', async () => {
    mockUpdatePriority.mockRejectedValueOnce(combinedError('priority refused'));
    renderSheet();

    editBothHalves();
    await settle();

    expect(mockUpdatePriority).toHaveBeenCalledOnce();
    expect(autosaveText()).toMatch(/échec|erreur|non enregistré/i);

    fireEvent.click(screen.getByTestId('task-sheet-autosave-retry'));
    await settle();

    // Whether the per-instance write went out before the priority failure or
    // only on the retry is the implementation's business — that it lands exactly
    // once, and that the failed half is the one resent, is not.
    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(savedPayload().notes).toBe('note à sauver');
    expect(mockUpdatePriority).toHaveBeenCalledTimes(2);
    expect(mockUpdatePriority).toHaveBeenLastCalledWith('task-id-xyz', 'CRITICAL', 'LOW');
    expect(autosaveText()).toMatch(/Enregistré/);
  });
});

// ── Tests: autosave — indicator states ────────────────────────────────────────

describe('autosave — the status indicator', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockTask = PERSONAL_TASK;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('walks pending → saving → saved', async () => {
    const gate = deferred();
    mockUpdateTask.mockImplementationOnce(() => gate.promise);
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'x' } });
    expect(autosaveText()).toMatch(/Modification/);

    await advance(DEBOUNCE_MS);
    expect(autosaveText()).toMatch(/Enregistrement/);

    gate.resolve();
    await settle();
    expect(autosaveText()).toMatch(/Enregistré/);
  });

  it('claims no save when the flush had nothing to write', async () => {
    renderSheet();

    fireEvent.click(screen.getByTestId('task-sheet-cancel'));
    await settle();

    expect(mockUpdateTask).not.toHaveBeenCalled();
    expect(autosaveText()).not.toMatch(/Enregistr/);
  });

  it('drops the ✓ when a later edit turns out to write nothing', async () => {
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'a' } });
    await advance(DEBOUNCE_MS);
    expect(autosaveText()).toMatch(/Enregistré/);

    // Typed away and back: nothing left to write, so nothing left to claim.
    fireEvent.change(notesInput(), { target: { value: 'ab' } });
    fireEvent.change(notesInput(), { target: { value: 'a' } });
    await advance(DEBOUNCE_MS);

    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(autosaveText()).not.toMatch(/Enregistré/);
  });

  it('claims nothing before the first edit', () => {
    renderSheet();

    expect(autosaveText()).not.toMatch(/Modification/);
    expect(autosaveText()).not.toMatch(/Enregistr/);
  });
});

// ── Tests: autosave — failure and retry ───────────────────────────────────────

describe('autosave — a failed write is visible and retryable', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockTask = PERSONAL_TASK;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('reports the failure in the status indicator', async () => {
    mockUpdateTask.mockRejectedValueOnce(combinedError('réseau coupé'));
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'x' } });
    await advance(DEBOUNCE_MS);

    expect(mockUpdateTask).toHaveBeenCalledOnce();
    // Exact copy is the implementation's call; it must read as a failure and
    // must not read as a success.
    expect(autosaveText()).toMatch(/échec|erreur|non enregistré/i);
    expect(autosaveText()).not.toMatch(/✓/);
  });

  it('offers a "Réessayer" button on failure', async () => {
    mockUpdateTask.mockRejectedValueOnce(combinedError('réseau coupé'));
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'x' } });
    await advance(DEBOUNCE_MS);

    const retry = screen.getByTestId('task-sheet-autosave-retry');
    expect(retry.textContent?.trim()).toMatch(/Réessayer/i);
  });

  it('re-issues the very same edit when the retry is clicked', async () => {
    mockUpdateTask.mockRejectedValueOnce(combinedError('réseau coupé'));
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'à sauver quand même' } });
    await advance(DEBOUNCE_MS);

    fireEvent.click(screen.getByTestId('task-sheet-autosave-retry'));
    await settle();

    expect(mockUpdateTask).toHaveBeenCalledTimes(2);
    expect(updateTaskPayloads()[1].notes).toBe('à sauver quand même');
    expect(autosaveText()).toMatch(/Enregistré/);
  });

  it('drops the retry button once the retry succeeds', async () => {
    mockUpdateTask.mockRejectedValueOnce(combinedError('réseau coupé'));
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'x' } });
    await advance(DEBOUNCE_MS);
    fireEvent.click(screen.getByTestId('task-sheet-autosave-retry'));
    await settle();

    expect(screen.queryByTestId('task-sheet-autosave-retry')).toBeNull();
  });

  it('refuses to close while the pending write is failing', async () => {
    // Closing on a failed flush would drop the edit with no way back — the
    // panel stays open with the retry in reach.
    mockUpdateTask.mockRejectedValueOnce(combinedError('réseau coupé'));
    const { onClose } = renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'à sauver quand même' } });
    fireEvent.click(screen.getByTestId('task-sheet-cancel'));
    await settle();

    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByTestId('task-sheet-autosave-retry')).toBeInTheDocument();
    expect(notesInput().value).toBe('à sauver quand même');
  });

  it('refuses to close while an immediate write is still failing', async () => {
    // A select or date writes at once, so at the moment of the click there is
    // nothing queued — only a cycle in flight. Closing on its failure loses the
    // edit exactly as closing on a failed debounced write would.
    mockUpdateTask.mockRejectedValueOnce(combinedError('réseau coupé'));
    const { onClose } = renderSheet();

    fireEvent.change(deadlineInput(), { target: { value: '2026-10-15' } });
    fireEvent.click(screen.getByTestId('task-sheet-cancel'));
    await settle();

    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByTestId('task-sheet-autosave-retry')).toBeInTheDocument();
  });

  it('Escape does not close the panel while the write is failing', async () => {
    mockUpdateTask.mockRejectedValueOnce(combinedError('réseau coupé'));
    const { onClose } = renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'à sauver quand même' } });
    fireEvent.keyDown(document, { key: 'Escape' });
    await settle();

    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(onClose).not.toHaveBeenCalled();
    expect(autosaveText()).toMatch(/échec|erreur|non enregistré/i);
    expect(screen.getByTestId('task-sheet-autosave-retry')).toBeInTheDocument();
  });

  it('Réessayer saves without closing, and the panel closes on the next ask', async () => {
    mockUpdateTask.mockRejectedValueOnce(combinedError('réseau coupé'));
    const { onClose } = renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'à sauver quand même' } });
    fireEvent.click(screen.getByTestId('task-sheet-cancel'));
    await settle();
    expect(onClose).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('task-sheet-autosave-retry'));
    await settle();

    expect(mockUpdateTask).toHaveBeenCalledTimes(2);
    expect(autosaveText()).toMatch(/Enregistré/);
    // Retrying writes; it is not a close.
    expect(onClose).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('task-sheet-cancel'));
    await settle();

    expect(onClose).toHaveBeenCalledOnce();
    // Nothing left to write, so the closing flush must add no third mutation.
    expect(mockUpdateTask).toHaveBeenCalledTimes(2);
  });

  it('closes on the second attempt once the write goes through', async () => {
    mockUpdateTask.mockRejectedValueOnce(combinedError('réseau coupé'));
    const { onClose } = renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'à sauver quand même' } });
    fireEvent.click(screen.getByTestId('task-sheet-cancel'));
    await settle();
    expect(onClose).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('task-sheet-cancel'));
    await settle();

    expect(mockUpdateTask).toHaveBeenCalledTimes(2);
    expect(updateTaskPayloads()[1].notes).toBe('à sauver quand même');
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('keeps the failed edit in the field instead of reverting it', async () => {
    mockUpdateTask.mockRejectedValueOnce(combinedError('réseau coupé'));
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'à sauver quand même' } });
    await advance(DEBOUNCE_MS);

    expect(notesInput().value).toBe('à sauver quand même');
  });
});

// ── Tests: autosave — skipping an occurrence ──────────────────────────────────

describe('autosave — the skip flushes the pending edit first', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockTask = RECURRING_TASK;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('writes the pending edit, then skips', async () => {
    const { onClose } = renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'à ne pas perdre' } });
    fireEvent.click(screen.getByTestId('task-sheet-skip'));
    await settle();

    expect(savedPayload().notes).toBe('à ne pas perdre');
    expect(mockSkipOccurrence).toHaveBeenCalledOnce();
    expect(mockUpdateTask.mock.invocationCallOrder[0]).toBeLessThan(
      mockSkipOccurrence.mock.invocationCallOrder[0],
    );
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('does not skip when the flush failed', async () => {
    // Skipping would close the panel and take the unwritten edit with it.
    mockUpdateTask.mockRejectedValueOnce(combinedError('réseau coupé'));
    const { onClose } = renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'à ne pas perdre' } });
    fireEvent.click(screen.getByTestId('task-sheet-skip'));
    await settle();

    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(mockSkipOccurrence).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByTestId('task-sheet-autosave-retry')).toBeInTheDocument();
    expect(notesInput().value).toBe('à ne pas perdre');
  });
});

// ── Tests: autosave — no duplicate writes ─────────────────────────────────────
//
// The component must diff against what it last sent, not against `task`: the
// refetch that follows a mutation may not have landed, so `task` still holds
// the pre-edit value and a naive diff resends it forever.

describe('autosave — no duplicate writes', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockTask = PERSONAL_TASK;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('a second flush with nothing changed in between writes nothing', async () => {
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'valeur finale' } });
    await advance(DEBOUNCE_MS);
    expect(mockUpdateTask).toHaveBeenCalledOnce();

    // `mockTask.notes` is deliberately left stale — the refetch has not landed.
    fireEvent.click(screen.getByTestId('task-sheet-cancel'));
    await settle();

    expect(mockUpdateTask).toHaveBeenCalledOnce();
  });

  it('a field returned to the value just saved is not written again', async () => {
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'a' } });
    await advance(DEBOUNCE_MS);
    expect(mockUpdateTask).toHaveBeenCalledOnce();

    fireEvent.change(notesInput(), { target: { value: 'ab' } });
    fireEvent.change(notesInput(), { target: { value: 'a' } });
    await advance(DEBOUNCE_MS);

    expect(mockUpdateTask).toHaveBeenCalledOnce();
  });

  it('a second edit after a successful save carries only the new value', async () => {
    renderSheet();

    fireEvent.change(notesInput(), { target: { value: 'première' } });
    await advance(DEBOUNCE_MS);

    fireEvent.change(descriptionInput(), { target: { value: 'seconde' } });
    await advance(DEBOUNCE_MS);

    expect(mockUpdateTask).toHaveBeenCalledTimes(2);
    const second = updateTaskPayloads()[1];
    expect(second.description).toBe('seconde');
    expect('notes' in second).toBe(false);
  });
});
