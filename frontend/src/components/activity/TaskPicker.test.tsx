import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, within } from '@testing-library/react';
import { TaskPicker } from './TaskPicker';
import type { TaskPickerItem } from '@/hooks/use-activity';

// ── Mock the search hook so the component's "typing >= 2 chars" branch is
//    fully controllable without urql / timers. ────────────────────────────────
interface SearchState {
  results: readonly TaskPickerItem[];
  loading: boolean;
  active: boolean;
}

let searchState: SearchState;
const useTaskSearchSpy = vi.fn((_term: string) => searchState);

vi.mock('@/hooks/use-task-search', () => ({
  MIN_SEARCH_LENGTH: 2,
  useTaskSearch: (term: string) => useTaskSearchSpy(term),
}));

// ── Helpers ───────────────────────────────────────────────────────────────────
function item(id: string, title: string, extras: Partial<TaskPickerItem> = {}): TaskPickerItem {
  return { id, title, plannedStart: null, deadline: null, urgency: 1, impact: 1, ...extras };
}

const WORKING_SET: TaskPickerItem[] = [
  item('w1', 'Finish quarterly report'),
  item('w2', 'Review PR #42'),
];

function renderPicker(props: Partial<React.ComponentProps<typeof TaskPicker>> = {}) {
  const onChange = vi.fn();
  render(
    <TaskPicker
      tasks={WORKING_SET}
      value=""
      onChange={onChange}
      {...props}
    />
  );
  return { onChange };
}

function getCombobox() {
  return screen.getByRole('combobox') as HTMLInputElement;
}

function type(value: string) {
  fireEvent.change(getCombobox(), { target: { value } });
}

beforeEach(() => {
  searchState = { results: [], loading: false, active: false };
  useTaskSearchSpy.mockClear();
});

// ── Closed label ────────────────────────────────────────────────────────────
describe('TaskPicker — closed label', () => {
  it('shows the "No task" placeholder when value is empty', () => {
    renderPicker({ value: '' });
    expect(getCombobox().placeholder).toContain('No task');
  });

  it('shows the selected task title resolved from the working set when value is set', () => {
    renderPicker({ value: 'w2' });
    expect(getCombobox().value).toBe('Review PR #42');
  });

  it('falls back to selectedTask for an arbitrary id not in the working set (edit mode)', () => {
    renderPicker({ value: 'x9', selectedTask: { id: 'x9', title: 'Legacy task' } });
    expect(getCombobox().value).toBe('Legacy task');
  });
});

// ── Open: working set + "No task" ─────────────────────────────────────────────
describe('TaskPicker — open shows working set', () => {
  it('renders a "No task" option plus every working-set task on focus', () => {
    renderPicker();
    fireEvent.focus(getCombobox());
    const listbox = screen.getByRole('listbox');
    const options = within(listbox).getAllByRole('option');
    // "No task" + 2 working-set tasks
    expect(options).toHaveLength(WORKING_SET.length + 1);
    expect(within(listbox).getByText('No task')).toBeInTheDocument();
    expect(within(listbox).getByText('Finish quarterly report')).toBeInTheDocument();
    expect(within(listbox).getByText('Review PR #42')).toBeInTheDocument();
  });

  it('keeps showing the working set for a 1-char query (no search yet)', () => {
    renderPicker();
    fireEvent.focus(getCombobox());
    type('a');
    const listbox = screen.getByRole('listbox');
    expect(within(listbox).getByText('Finish quarterly report')).toBeInTheDocument();
  });
});

// ── Typing >= 2 chars renders search results ──────────────────────────────────
describe('TaskPicker — search results', () => {
  it('renders the mocked search results when query length >= 2', () => {
    searchState = {
      results: [item('s1', 'Audit auth flow'), item('s2', 'Authorize API keys')],
      loading: false,
      active: true,
    };
    renderPicker();
    fireEvent.focus(getCombobox());
    type('au');
    const listbox = screen.getByRole('listbox');
    expect(within(listbox).getByText('Audit auth flow')).toBeInTheDocument();
    expect(within(listbox).getByText('Authorize API keys')).toBeInTheDocument();
    // The working-set items must NOT be shown in search mode.
    expect(within(listbox).queryByText('Finish quarterly report')).toBeNull();
  });

  it('forwards the typed query to useTaskSearch', () => {
    renderPicker();
    fireEvent.focus(getCombobox());
    type('auth');
    expect(useTaskSearchSpy).toHaveBeenCalledWith('auth');
  });

  it('shows a loading row while the search is loading', () => {
    searchState = { results: [], loading: true, active: true };
    renderPicker();
    fireEvent.focus(getCombobox());
    type('au');
    expect(screen.getByText(/loading/i)).toBeInTheDocument();
  });

  it('shows a "No tasks match" empty row when results are empty and not loading', () => {
    searchState = { results: [], loading: false, active: true };
    renderPicker();
    fireEvent.focus(getCombobox());
    type('zz');
    expect(screen.getByText(/no tasks match/i)).toBeInTheDocument();
  });

  // Regression: the debounce-aware `active` flag must gate the empty row so the
  // "No tasks match" message never flashes while the debounced search is pending.
  it('does NOT show "No tasks match" while the search is still pending (active=false)', () => {
    searchState = { results: [], loading: false, active: false };
    renderPicker();
    fireEvent.focus(getCombobox());
    type('zz');
    expect(screen.queryByText(/no tasks match/i)).toBeNull();
    // A loading/searching row is shown instead.
    expect(screen.getByText(/loading/i)).toBeInTheDocument();
  });

  it('shows "No tasks match" once the debounced search has run with no results (active=true)', () => {
    searchState = { results: [], loading: false, active: true };
    renderPicker();
    fireEvent.focus(getCombobox());
    type('zz');
    expect(screen.getByText(/no tasks match/i)).toBeInTheDocument();
  });
});

// ── Picking ───────────────────────────────────────────────────────────────────
describe('TaskPicker — selection', () => {
  it('clicking a working-set task calls onChange(id) and closes the dropdown', () => {
    const { onChange } = renderPicker();
    fireEvent.focus(getCombobox());
    fireEvent.mouseDown(screen.getByText('Review PR #42'));
    expect(onChange).toHaveBeenCalledWith('w2');
    expect(screen.queryByRole('listbox')).toBeNull();
  });

  it('clicking "No task" calls onChange("") and closes', () => {
    const { onChange } = renderPicker({ value: 'w1' });
    fireEvent.focus(getCombobox());
    fireEvent.mouseDown(screen.getByText('No task'));
    expect(onChange).toHaveBeenCalledWith('');
    expect(screen.queryByRole('listbox')).toBeNull();
  });

  it('clicking a search result calls onChange(id)', () => {
    searchState = { results: [item('s1', 'Audit auth flow')], loading: false, active: true };
    const { onChange } = renderPicker();
    fireEvent.focus(getCombobox());
    type('au');
    fireEvent.mouseDown(screen.getByText('Audit auth flow'));
    expect(onChange).toHaveBeenCalledWith('s1');
  });
});

// ── Keyboard ──────────────────────────────────────────────────────────────────
describe('TaskPicker — keyboard', () => {
  it('ArrowDown then Enter selects the working-set option after "No task"', () => {
    const { onChange } = renderPicker();
    const input = getCombobox();
    fireEvent.focus(input);
    // index 0 = "No task", index 1 = first working-set task
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onChange).toHaveBeenCalledWith('w1');
  });

  it('Escape closes the dropdown without changing the value', () => {
    const { onChange } = renderPicker({ value: 'w2' });
    const input = getCombobox();
    fireEvent.focus(input);
    expect(screen.getByRole('listbox')).toBeInTheDocument();
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(screen.queryByRole('listbox')).toBeNull();
    expect(onChange).not.toHaveBeenCalled();
  });
});
