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
