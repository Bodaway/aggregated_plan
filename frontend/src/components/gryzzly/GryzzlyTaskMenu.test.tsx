import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { GryzzlyTaskMenu } from './GryzzlyTaskMenu';

const executeAssign = vi.fn().mockResolvedValue({});
vi.mock('urql', () => ({ useMutation: () => [{}, executeAssign] }));

const mockOptions = vi.fn();
vi.mock('@/hooks/use-gryzzly-tasks', () => ({
  useGryzzlyTasks: () => ({ options: mockOptions(), fetching: false, error: null }),
}));

/** The chip's accessible name is the assigned task, or "Gryzzly" when free. */
function openMenu(name: RegExp = /gryzzly/i) {
  fireEvent.click(screen.getByRole('button', { name }));
}

const ASSIGNED = {
  gryzzlyTaskId: 't1',
  name: 'Pilotage',
  projectName: 'Canal Plus',
  projectStatus: 'active',
  stale: false,
};

describe('GryzzlyTaskMenu', () => {
  beforeEach(() => {
    executeAssign.mockClear();
    mockOptions.mockReturnValue([
      { gryzzlyTaskId: 't1', name: 'Pilotage', projectName: 'Canal Plus', projectStatus: 'active' },
      { gryzzlyTaskId: 't2', name: 'Recette', projectName: 'Saft', projectStatus: 'done' },
    ]);
  });

  it('labels the chip with the assigned task name', () => {
    render(<GryzzlyTaskMenu taskId="task-1" assigned={ASSIGNED} />);
    expect(screen.getByRole('button', { name: /Pilotage/ })).toBeInTheDocument();
  });

  it('shows a placeholder chip when nothing is assigned', () => {
    render(<GryzzlyTaskMenu taskId="task-1" assigned={null} />);
    expect(screen.getByRole('button', { name: /gryzzly/i })).toBeInTheDocument();
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  it('assigns the picked task', async () => {
    render(<GryzzlyTaskMenu taskId="task-1" assigned={null} />);
    openMenu();
    fireEvent.click(screen.getByRole('option', { name: /Pilotage/ }));

    await waitFor(() =>
      expect(executeAssign).toHaveBeenCalledWith({ taskId: 'task-1', gryzzlyTaskId: 't1' }),
    );
  });

  it('clears the assignment', async () => {
    render(<GryzzlyTaskMenu taskId="task-1" assigned={ASSIGNED} />);
    openMenu(/Pilotage/);
    fireEvent.click(screen.getByRole('option', { name: /clear assignment/i }));

    await waitFor(() =>
      expect(executeAssign).toHaveBeenCalledWith({ taskId: 'task-1', gryzzlyTaskId: null }),
    );
  });

  it('badges the chip when the assigned project is terminated', () => {
    render(
      <GryzzlyTaskMenu taskId="task-1" assigned={{ ...ASSIGNED, projectStatus: 'done' }} />,
    );
    expect(screen.getByText('terminé')).toBeInTheDocument();
  });

  // The chip lives inside a dnd-kit draggable card whose own onClick opens the
  // edit sheet: every pointer event it handles must stop there, portal included.
  it('does not leak clicks to the surrounding card', () => {
    const onCardClick = vi.fn();
    render(
      <div onClick={onCardClick}>
        <GryzzlyTaskMenu taskId="task-1" assigned={null} />
      </div>,
    );

    openMenu();
    expect(onCardClick).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('option', { name: /Pilotage/ }));
    expect(onCardClick).not.toHaveBeenCalled();
  });

  // Regression: the menu used to close on any scroll, and focusing the search
  // box scrolls its own container — so it shut the instant it was opened.
  it('stays open when a scroll fires under it', () => {
    render(<GryzzlyTaskMenu taskId="task-1" assigned={null} />);
    openMenu();

    fireEvent.scroll(document.body);
    expect(screen.getByRole('listbox')).toBeInTheDocument();
  });

  it('keeps the dropdown open when typing in its search box', () => {
    render(<GryzzlyTaskMenu taskId="task-1" assigned={null} />);
    openMenu();

    const input = screen.getByPlaceholderText('Search tasks…');
    fireEvent.mouseDown(input);
    fireEvent.change(input, { target: { value: 'pil' } });

    expect(screen.getByRole('listbox')).toBeInTheDocument();
  });
});
