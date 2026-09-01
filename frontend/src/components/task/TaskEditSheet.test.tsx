import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { TaskEditSheet } from './TaskEditSheet';
import type { FullTask } from '@/hooks/use-task-edit';

// ── Mock hooks & heavy sub-components ────────────────────────────────────────

const mockUpdateTask = vi.fn(async () => {});
const mockUpdatePriority = vi.fn(async () => {});
const mockSkipOccurrence = vi.fn(async () => {});
const mockUpdateRecurringTask = vi.fn(async () => {});

let mockTask: FullTask | null = null;

vi.mock('@/hooks/use-delegates', () => ({
  useDelegates: () => ({ delegates: ['Ahmed', 'Marie'] }),
}));

vi.mock('@/hooks/use-task-edit', () => ({
  useTaskEdit: () => ({
    task: mockTask,
    loading: false,
    error: null,
    updateTask: mockUpdateTask,
    updatePriority: mockUpdatePriority,
    skipOccurrence: mockSkipOccurrence,
    updateRecurringTask: mockUpdateRecurringTask,
    refetch: vi.fn(),
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

// ── Helpers ───────────────────────────────────────────────────────────────────

function renderSheet(taskId: string | null = 'task-id-xyz') {
  const onClose = vi.fn();
  const onUpdated = vi.fn();
  render(<TaskEditSheet taskId={taskId} onClose={onClose} onUpdated={onUpdated} />);
  return { onClose, onUpdated };
}

// ── Tests: recurring task ─────────────────────────────────────────────────────

describe('TaskEditSheet — recurring task', () => {
  beforeEach(() => {
    mockTask = RECURRING_TASK;
    mockSkipOccurrence.mockClear();
    mockUpdateTask.mockClear();
    mockUpdateRecurringTask.mockClear();
  });

  it('renders the violet recurring-task banner', () => {
    renderSheet();
    // Wave 12 banner copy — read from TaskEditSheet.tsx
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

  it('renders the Save button (Wave 12: Save is unconditionally visible)', () => {
    renderSheet();
    // Wave 12 restores Save for recurring instances — must be present.
    expect(screen.getByRole('button', { name: /^save$/i })).toBeTruthy();
  });

  it('clicking skip calls skipOccurrence and then onClose', async () => {
    const { onClose } = renderSheet();

    const skipBtn = screen.getByRole('button', { name: /ignorer cette occurrence/i });
    fireEvent.click(skipBtn);

    await waitFor(() => expect(mockSkipOccurrence).toHaveBeenCalledOnce());
    await waitFor(() => expect(onClose).toHaveBeenCalledOnce());
  });

  it('Save with only per-instance change (status) calls updateTask, not updateRecurringTask', async () => {
    const { onClose } = renderSheet();

    // Change status (per-instance field)
    const statusSelect = screen.getByDisplayValue('To Do');
    fireEvent.change(statusSelect, { target: { value: 'IN_PROGRESS' } });

    const saveBtn = screen.getByRole('button', { name: /^save$/i });
    fireEvent.click(saveBtn);

    await waitFor(() => expect(onClose).toHaveBeenCalledOnce());
    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(mockUpdateRecurringTask).not.toHaveBeenCalled();
  });

  it('Save with only a template field change (description) calls updateRecurringTask, not updateTask', async () => {
    const { onClose } = renderSheet();

    // Change description (template field for recurring tasks)
    const descTextarea = screen.getByPlaceholderText('Add a description...');
    fireEvent.change(descTextarea, { target: { value: 'New description for the series' } });

    const saveBtn = screen.getByRole('button', { name: /^save$/i });
    fireEvent.click(saveBtn);

    await waitFor(() => expect(onClose).toHaveBeenCalledOnce());
    expect(mockUpdateRecurringTask).toHaveBeenCalledOnce();
    expect(mockUpdateTask).not.toHaveBeenCalled();
  });

  it('Save with both per-instance and template changes calls both mutations', async () => {
    const { onClose } = renderSheet();

    // Change status (per-instance)
    const statusSelect = screen.getByDisplayValue('To Do');
    fireEvent.change(statusSelect, { target: { value: 'DONE' } });

    // Change description (template)
    const descTextarea = screen.getByPlaceholderText('Add a description...');
    fireEvent.change(descTextarea, { target: { value: 'Updated description' } });

    const saveBtn = screen.getByRole('button', { name: /^save$/i });
    fireEvent.click(saveBtn);

    await waitFor(() => expect(onClose).toHaveBeenCalledOnce());
    expect(mockUpdateTask).toHaveBeenCalledOnce();
    expect(mockUpdateRecurringTask).toHaveBeenCalledOnce();
  });
});

// ── Tests: one-shot task ──────────────────────────────────────────────────────

describe('TaskEditSheet — one-shot task', () => {
  beforeEach(() => {
    mockTask = BASE_TASK;
    mockUpdateTask.mockClear();
    mockUpdatePriority.mockClear();
  });

  it('does NOT render the recurring-task banner', () => {
    renderSheet();
    expect(screen.queryByText(/cette tâche fait partie d'une série récurrente/i)).toBeNull();
  });

  it('does NOT render the skip button', () => {
    renderSheet();
    expect(screen.queryByRole('button', { name: /ignorer cette occurrence/i })).toBeNull();
  });

  it('renders the Save button', () => {
    renderSheet();
    expect(screen.getByRole('button', { name: /^save$/i })).toBeTruthy();
  });

  it('clicking Save calls onClose', async () => {
    const { onClose } = renderSheet();

    const saveBtn = screen.getByRole('button', { name: /^save$/i });
    fireEvent.click(saveBtn);

    await waitFor(() => expect(onClose).toHaveBeenCalledOnce());
  });
});

// ── Tests: delegation ─────────────────────────────────────────────────────────

describe('delegation', () => {
  beforeEach(() => {
    mockTask = { ...BASE_TASK };
    mockUpdateTask.mockClear();
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

  it('sends delegatedTo on save when a name is entered', async () => {
    renderSheet();
    fireEvent.change(screen.getByLabelText(/delegated to/i), { target: { value: 'Marie' } });
    fireEvent.click(screen.getByTestId('task-sheet-save'));
    await waitFor(() => {
      expect(mockUpdateTask).toHaveBeenCalledWith(
        expect.objectContaining({ delegatedTo: 'Marie' })
      );
    });
  });

  it('sends delegatedTo: null on save when the field is emptied', async () => {
    mockTask = { ...BASE_TASK, delegatedTo: 'Marie' };
    renderSheet();
    fireEvent.change(screen.getByLabelText(/delegated to/i), { target: { value: '' } });
    fireEvent.click(screen.getByTestId('task-sheet-save'));
    await waitFor(() => {
      expect(mockUpdateTask).toHaveBeenCalledWith(
        expect.objectContaining({ delegatedTo: null })
      );
    });
  });

  it('does not send delegatedTo when unchanged', async () => {
    mockTask = { ...BASE_TASK, delegatedTo: 'Marie', notes: 'x' };
    renderSheet();
    // change something else so save fires an update
    fireEvent.change(screen.getByLabelText('Notes'), { target: { value: 'y' } });
    fireEvent.click(screen.getByTestId('task-sheet-save'));
    await waitFor(() => expect(mockUpdateTask).toHaveBeenCalled());
    expect(mockUpdateTask).toHaveBeenCalledWith(
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

const PERSONAL_TASK: FullTask = { ...BASE_TASK, source: 'PERSONAL' };

type Payload = Record<string, unknown>;

/** The single `updateTask` payload of the last save. */
function savedPayload(): Payload {
  expect(mockUpdateTask).toHaveBeenCalledOnce();
  const calls = mockUpdateTask.mock.calls as unknown as Payload[][];
  return calls[0][0];
}

function deadlineInput(): HTMLInputElement {
  return screen.getByLabelText('Échéance') as HTMLInputElement;
}

describe('TaskEditSheet — deadline field visibility (R76)', () => {
  beforeEach(() => {
    mockUpdateTask.mockClear();
  });

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

  it('offers the clear button only once a deadline is set', () => {
    mockTask = PERSONAL_TASK;
    renderSheet();
    expect(screen.queryByRole('button', { name: /effacer l’échéance|effacer l'échéance/i })).toBeNull();

    fireEvent.change(deadlineInput(), { target: { value: '2026-10-15' } });

    expect(screen.getByRole('button', { name: /effacer l’échéance|effacer l'échéance/i })).toBeInTheDocument();
  });
});

describe('TaskEditSheet — deadline on the wire (R76)', () => {
  beforeEach(() => {
    mockUpdateTask.mockClear();
  });

  it('omits the key entirely when the field was never touched', async () => {
    // The dangerous case: `deadline: null` here would clear a real deadline on
    // every unrelated save. Absence and null are NOT interchangeable.
    mockTask = { ...PERSONAL_TASK, deadline: '2026-09-30', notes: 'x' };
    renderSheet();

    fireEvent.change(screen.getByLabelText('Notes'), { target: { value: 'y' } });
    fireEvent.click(screen.getByTestId('task-sheet-save'));

    await waitFor(() => expect(mockUpdateTask).toHaveBeenCalled());
    expect('deadline' in savedPayload()).toBe(false);
  });

  it('omits the key when the field is re-typed to the value it already had', async () => {
    mockTask = { ...PERSONAL_TASK, deadline: '2026-09-30', notes: 'x' };
    renderSheet();

    fireEvent.change(deadlineInput(), { target: { value: '2026-09-30' } });
    fireEvent.change(screen.getByLabelText('Notes'), { target: { value: 'y' } });
    fireEvent.click(screen.getByTestId('task-sheet-save'));

    await waitFor(() => expect(mockUpdateTask).toHaveBeenCalled());
    expect('deadline' in savedPayload()).toBe(false);
  });

  it('sends an explicit null when the user clears the deadline', async () => {
    mockTask = { ...PERSONAL_TASK, deadline: '2026-09-30' };
    renderSheet();

    fireEvent.click(screen.getByRole('button', { name: /effacer l’échéance|effacer l'échéance/i }));
    fireEvent.click(screen.getByTestId('task-sheet-save'));

    await waitFor(() => expect(mockUpdateTask).toHaveBeenCalled());
    const payload = savedPayload();
    expect('deadline' in payload).toBe(true);
    expect(payload.deadline).toBeNull();
  });

  it('sends an explicit null when the input itself is emptied', async () => {
    mockTask = { ...PERSONAL_TASK, deadline: '2026-09-30' };
    renderSheet();

    fireEvent.change(deadlineInput(), { target: { value: '' } });
    fireEvent.click(screen.getByTestId('task-sheet-save'));

    await waitFor(() => expect(mockUpdateTask).toHaveBeenCalled());
    expect(savedPayload().deadline).toBeNull();
  });

  it('sends the plain date when the user sets a deadline', async () => {
    mockTask = PERSONAL_TASK;
    renderSheet();

    fireEvent.change(deadlineInput(), { target: { value: '2026-10-15' } });
    fireEvent.click(screen.getByTestId('task-sheet-save'));

    await waitFor(() => expect(mockUpdateTask).toHaveBeenCalled());
    expect(savedPayload().deadline).toBe('2026-10-15');
  });

  it('sends the plain date when the user moves an existing deadline', async () => {
    mockTask = { ...PERSONAL_TASK, deadline: '2026-09-30' };
    renderSheet();

    fireEvent.change(deadlineInput(), { target: { value: '2026-10-15' } });
    fireEvent.click(screen.getByTestId('task-sheet-save'));

    await waitFor(() => expect(mockUpdateTask).toHaveBeenCalled());
    expect(savedPayload().deadline).toBe('2026-10-15');
  });

  it('never sends a deadline for a synced task, whatever else changes', async () => {
    // The guard is `canEditDeadline`, not the mere absence of the input.
    mockTask = { ...BASE_TASK, source: 'JIRA', deadline: '2026-09-30', notes: 'x' };
    renderSheet();

    fireEvent.change(screen.getByLabelText('Notes'), { target: { value: 'y' } });
    fireEvent.click(screen.getByTestId('task-sheet-save'));

    await waitFor(() => expect(mockUpdateTask).toHaveBeenCalled());
    expect('deadline' in savedPayload()).toBe(false);
  });

  it('keeps the deadline out of the recurring-template mutation', async () => {
    // The deadline is per-instance, like the planned date: a series has none.
    mockTask = { ...RECURRING_TASK, source: 'PERSONAL' };
    mockUpdateRecurringTask.mockClear();
    renderSheet();

    fireEvent.change(deadlineInput(), { target: { value: '2026-10-15' } });
    fireEvent.change(screen.getByPlaceholderText('Add a description...'), {
      target: { value: 'Série mise à jour' },
    });
    fireEvent.click(screen.getByTestId('task-sheet-save'));

    await waitFor(() => expect(mockUpdateRecurringTask).toHaveBeenCalledOnce());
    const recurringCalls = mockUpdateRecurringTask.mock.calls as unknown as [string, Payload][];
    expect('deadline' in recurringCalls[0][1]).toBe(false);
    expect(savedPayload().deadline).toBe('2026-10-15');
  });
});
