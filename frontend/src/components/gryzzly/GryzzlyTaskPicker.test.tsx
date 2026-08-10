import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { GryzzlyTaskPicker } from './GryzzlyTaskPicker';

const executeAssign = vi.fn().mockResolvedValue({});
vi.mock('urql', () => ({ useMutation: () => [{}, executeAssign] }));

const mockOptions = vi.fn();
vi.mock('@/hooks/use-gryzzly-tasks', () => ({
  useGryzzlyTasks: () => ({ options: mockOptions(), fetching: false, error: null }),
}));

/** Opens the dropdown. The trigger is the only button before the menu exists. */
function openMenu() {
  fireEvent.click(screen.getByRole('button', { name: /assign gryzzly task/i }));
}

describe('GryzzlyTaskPicker', () => {
  beforeEach(() => {
    executeAssign.mockClear();
    mockOptions.mockReturnValue([
      { gryzzlyTaskId: 't1', name: 'Pilotage', projectName: 'Canal Plus', projectStatus: 'active' },
      { gryzzlyTaskId: 't2', name: 'Recette', projectName: 'Saft', projectStatus: 'done' },
    ]);
  });

  it('badges only the group header of a terminated project', () => {
    render(<GryzzlyTaskPicker taskId="task-1" assigned={null} />);
    openMenu();

    // One badge for the done project's group, none for the active one.
    expect(screen.getAllByText('terminé')).toHaveLength(1);
  });

  // A project routinely closes with declarations still owed, so the row must
  // remain clickable.
  it('still assigns a task whose project is terminated', async () => {
    render(<GryzzlyTaskPicker taskId="task-1" assigned={null} />);
    openMenu();
    fireEvent.click(screen.getByRole('option', { name: /Recette/ }));

    await waitFor(() =>
      expect(executeAssign).toHaveBeenCalledWith({ taskId: 'task-1', gryzzlyTaskId: 't2' }),
    );
  });

  it('badges the trigger when the assigned task’s project is terminated', () => {
    render(
      <GryzzlyTaskPicker
        taskId="task-1"
        assigned={{
          gryzzlyTaskId: 't2',
          name: 'Recette',
          projectName: 'Saft',
          projectStatus: 'done',
          stale: false,
        }}
      />,
    );
    expect(screen.getByText('terminé')).toBeInTheDocument();
  });

  it('shows both markers when a task is stale and its project terminated', () => {
    render(
      <GryzzlyTaskPicker
        taskId="task-1"
        assigned={{
          gryzzlyTaskId: 't2',
          name: 'Recette',
          projectName: 'Saft',
          projectStatus: 'done',
          stale: true,
        }}
      />,
    );
    expect(screen.getByText('stale')).toBeInTheDocument();
    expect(screen.getByText('terminé')).toBeInTheDocument();
  });

  it('shows no badge when nothing is terminated', () => {
    mockOptions.mockReturnValue([
      { gryzzlyTaskId: 't1', name: 'Pilotage', projectName: 'Canal Plus', projectStatus: 'active' },
    ]);
    render(<GryzzlyTaskPicker taskId="task-1" assigned={null} />);
    openMenu();

    expect(screen.queryByText('terminé')).not.toBeInTheDocument();
  });
});
