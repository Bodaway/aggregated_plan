import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { TaskCreateSheet } from './TaskCreateSheet';

// ── Mock hooks ────────────────────────────────────────────────────────────────

let capturedCreateTaskInput: unknown = null;
let capturedCreateRecurringTaskInput: unknown = null;
const mockCreateTask = vi.fn(async (input: unknown) => {
  capturedCreateTaskInput = input;
  return { error: null };
});
const mockCreateRecurringTask = vi.fn(async (input: unknown) => {
  capturedCreateRecurringTaskInput = input;
  return { error: null };
});

vi.mock('@/hooks/use-create-task', () => ({
  useCreateTask: () => ({
    createTask: mockCreateTask,
    loading: false,
    error: null,
  }),
}));

vi.mock('@/hooks/use-create-recurring-task', () => ({
  useCreateRecurringTask: () => ({
    createRecurringTask: mockCreateRecurringTask,
    loading: false,
    error: null,
  }),
}));

// Stub MarkdownEditor to a plain textarea so we can interact with it
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

// RecurrencePicker: use a minimal stub that exposes a way to inject a value
// We need the real one for the weekly-Friday test, so we use the real implementation.
// For null-recurrence tests the picker is already rendered with null value.

// ── Helpers ───────────────────────────────────────────────────────────────────

function renderSheet(plannedDate: string | null = '2026-04-27') {
  const onClose = vi.fn();
  const onCreated = vi.fn();
  render(
    <TaskCreateSheet
      plannedDate={plannedDate}
      onClose={onClose}
      onCreated={onCreated}
    />
  );
  return { onClose, onCreated };
}

function fillTitle(title: string) {
  fireEvent.change(screen.getByPlaceholderText('Task title...'), { target: { value: title } });
}

function clickSave() {
  // The save button label changes depending on recurrence state
  const btn = screen.getByRole('button', { name: /create/i });
  fireEvent.click(btn);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

beforeEach(() => {
  capturedCreateTaskInput = null;
  capturedCreateRecurringTaskInput = null;
  mockCreateTask.mockClear();
  mockCreateRecurringTask.mockClear();
});

describe('TaskCreateSheet — no recurrence', () => {
  it('calls createTask (not createRecurringTask) when recurrence is null', async () => {
    renderSheet('2026-04-27');
    fillTitle('My task');
    clickSave();

    await waitFor(() => expect(mockCreateTask).toHaveBeenCalledOnce());
    expect(mockCreateRecurringTask).not.toHaveBeenCalled();
  });

  it('createTask input has no recurrence-related fields', async () => {
    renderSheet('2026-04-27');
    fillTitle('My task');
    clickSave();

    await waitFor(() => expect(mockCreateTask).toHaveBeenCalledOnce());
    const input = capturedCreateTaskInput as Record<string, unknown>;
    expect(input).not.toHaveProperty('rule');
    expect(input).not.toHaveProperty('startsOn');
    expect(input).not.toHaveProperty('endsOn');
    expect(input).not.toHaveProperty('maxOccurrences');
  });

  it('save button reads "Create Task" when recurrence is null', () => {
    renderSheet('2026-04-27');
    expect(screen.getByRole('button', { name: 'Create Task' })).toBeTruthy();
  });
});

describe('TaskCreateSheet — weekly/Friday recurrence', () => {
  it('calls createRecurringTask (not createTask) with weekly Friday rule', async () => {
    renderSheet('2026-04-27');

    // Switch frequency to weekly
    const frequencySelect = screen.getByRole('combobox', { name: /récurrence/i });
    fireEvent.change(frequencySelect, { target: { value: 'weekly' } });

    // Toggle Friday
    const friButton = screen.getByRole('button', { name: 'Ven' });
    fireEvent.click(friButton);

    fillTitle('Weekly Friday task');

    // Save button label should now reflect recurring branch
    expect(screen.getByRole('button', { name: 'Create Recurring Task' })).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Create Recurring Task' }));

    await waitFor(() => expect(mockCreateRecurringTask).toHaveBeenCalledOnce());
    expect(mockCreateTask).not.toHaveBeenCalled();

    const input = capturedCreateRecurringTaskInput as Record<string, unknown>;
    expect(input).toMatchObject({
      rule: { kind: 'WEEKLY', interval: 1, weekdays: ['FRIDAY'] },
      startsOn: '2026-04-27',
    });
  });

  it('save button reads "Create Recurring Task" when recurrence is set', () => {
    renderSheet('2026-04-27');

    const frequencySelect = screen.getByRole('combobox', { name: /récurrence/i });
    fireEvent.change(frequencySelect, { target: { value: 'weekly' } });

    expect(screen.getByRole('button', { name: 'Create Recurring Task' })).toBeTruthy();
  });
});
