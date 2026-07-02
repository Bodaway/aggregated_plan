import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ActivityTimer } from './ActivityTimer';
import type { TaskPickerItem } from '@/hooks/use-activity';

// ActivityTimer now renders TaskPicker, which calls useTaskSearch. Mock it the
// same way TaskPicker.test.tsx does so the empty-state working-set branch is
// fully controllable without urql / timers.
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

function item(id: string, title: string): TaskPickerItem {
  return { id, title, plannedStart: null, deadline: null, urgency: 1, impact: 1 };
}

const WORKING_SET: TaskPickerItem[] = [
  item('w1', 'Finish quarterly report'),
  item('w2', 'Review PR #42'),
];

function renderTimer(props: Partial<React.ComponentProps<typeof ActivityTimer>> = {}) {
  const onStart = vi.fn();
  const onStop = vi.fn();
  render(
    <ActivityTimer
      currentActivity={null}
      tasks={WORKING_SET}
      onStart={onStart}
      onStop={onStop}
      {...props}
    />
  );
  return { onStart, onStop };
}

beforeEach(() => {
  searchState = { results: [], loading: false, active: false };
  useTaskSearchSpy.mockClear();
});

describe('ActivityTimer — task selector', () => {
  it('renders the searchable combobox (not a native select) in the no-activity state', () => {
    renderTimer();
    expect(screen.getByRole('combobox')).toBeInTheDocument();
    // The old native <select> exposed the "listbox" role at rest; the combobox
    // only opens its listbox on focus, so none should be present initially.
    expect(screen.queryByRole('listbox')).toBeNull();
  });

  it('disables the Start button initially', () => {
    renderTimer();
    expect(screen.getByRole('button', { name: /start/i })).toBeDisabled();
  });

  it('enables Start after picking a working-set task and calls onStart with that id', () => {
    const { onStart } = renderTimer();
    fireEvent.focus(screen.getByRole('combobox'));
    fireEvent.mouseDown(screen.getByText('Review PR #42'));

    const start = screen.getByRole('button', { name: /start/i });
    expect(start).toBeEnabled();
    fireEvent.click(start);
    expect(onStart).toHaveBeenCalledWith('w2');
  });
});
