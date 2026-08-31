import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { LaneGryzzlyPicker } from './LaneGryzzlyPicker';

const mockOptions = vi.fn();
vi.mock('@/hooks/use-gryzzly-tasks', () => ({
  useGryzzlyTasks: () => ({ options: mockOptions(), fetching: false, error: null }),
}));

const onAssign = vi.fn();

function renderPicker(props: Partial<Parameters<typeof LaneGryzzlyPicker>[0]> = {}) {
  return render(
    <LaneGryzzlyPicker
      laneLabel="HUD overlay"
      projectLabel="sans projet Gryzzly"
      hasProject={false}
      onAssign={onAssign}
      {...props}
    />,
  );
}

describe('LaneGryzzlyPicker', () => {
  beforeEach(() => {
    onAssign.mockClear();
    mockOptions.mockReturnValue([
      { gryzzlyTaskId: 't1', name: 'Pilotage', projectName: 'Canal Plus', projectStatus: 'active' },
      { gryzzlyTaskId: 't2', name: 'Recette', projectName: 'Saft', projectStatus: 'active' },
    ]);
  });

  it('labels the trigger with the project the lane declares today', () => {
    renderPicker({ projectLabel: 'eProject A3 — Saft', hasProject: true });
    expect(
      screen.getByRole('button', { name: /projet gryzzly de HUD overlay/i }),
    ).toHaveTextContent('eProject A3 — Saft');
  });

  it('keeps the list closed until the trigger is clicked', () => {
    renderPicker();
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /projet gryzzly/i }));
    expect(screen.getByRole('listbox')).toBeInTheDocument();
  });

  /// The whole point: one click on the row, one click on a task, done.
  it('assigns the picked Gryzzly task and closes', () => {
    renderPicker();
    fireEvent.click(screen.getByRole('button', { name: /projet gryzzly/i }));
    fireEvent.click(screen.getByRole('option', { name: /Pilotage/ }));

    expect(onAssign).toHaveBeenCalledWith('t1');
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  it('offers to detach the project, but only on a lane that has one', () => {
    renderPicker({ projectLabel: 'Canal Plus', hasProject: true });
    fireEvent.click(screen.getByRole('button', { name: /projet gryzzly/i }));
    fireEvent.click(screen.getByRole('button', { name: /retirer le projet gryzzly/i }));

    expect(onAssign).toHaveBeenCalledWith(null);
  });

  it('hides the detach action when the lane declares no project', () => {
    renderPicker();
    fireEvent.click(screen.getByRole('button', { name: /projet gryzzly/i }));
    expect(screen.queryByRole('button', { name: /retirer le projet gryzzly/i })).not.toBeInTheDocument();
  });

  it('closes on Escape', () => {
    renderPicker();
    fireEvent.click(screen.getByRole('button', { name: /projet gryzzly/i }));
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  it('closes on a click outside, without assigning anything', () => {
    renderPicker();
    fireEvent.click(screen.getByRole('button', { name: /projet gryzzly/i }));
    fireEvent.mouseDown(document.body);

    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
    expect(onAssign).not.toHaveBeenCalled();
  });

  // Typing in the search box scrolls its own container, which used to shut the
  // dashboard chip's menu — the same trap applies here.
  it('stays open while the search box is used', () => {
    renderPicker();
    fireEvent.click(screen.getByRole('button', { name: /projet gryzzly/i }));

    const input = screen.getByPlaceholderText('Search tasks…');
    fireEvent.mouseDown(input);
    fireEvent.change(input, { target: { value: 'pil' } });

    expect(screen.getByRole('listbox')).toBeInTheDocument();
  });
});
