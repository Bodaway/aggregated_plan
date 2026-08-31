import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { TimesheetLanes } from './TimesheetLanes';
import type { Lane } from '@/hooks/use-timesheet';

vi.mock('@/hooks/use-gryzzly-tasks', () => ({
  useGryzzlyTasks: () => ({
    options: [
      { gryzzlyTaskId: 't1', name: 'Pilotage', projectName: 'Canal Plus', projectStatus: 'active' },
    ],
    fetching: false,
    error: null,
  }),
}));

const taskLane: Lane = {
  laneKey: 'task:11111111-1111-1111-1111-111111111111',
  taskId: '11111111-1111-1111-1111-111111111111',
  label: 'HUD overlay',
  gryzzlyProjectId: null,
  outsideMinutes: 0,
  intervals: [{ startMin: 540, endMin: 620 }],
};

// A meeting: evidence with no plan task, so no Gryzzly snapshot to correct.
const sourceLane: Lane = {
  laneKey: 'src:mtg:42',
  taskId: null,
  label: 'Daily',
  gryzzlyProjectId: 'p1',
  outsideMinutes: 0,
  intervals: [{ startMin: 600, endMin: 630 }],
};

const projects = [{ id: 'p1', label: 'Canal Plus' }];
const onAssignLaneTask = vi.fn();

describe('TimesheetLanes', () => {
  beforeEach(() => onAssignLaneTask.mockClear());

  it('offers the Gryzzly picker on a lane that has a task', () => {
    render(
      <TimesheetLanes
        lanes={[taskLane]}
        quarters={[]}
        projects={projects}
        onAssignLaneTask={onAssignLaneTask}
      />,
    );
    expect(screen.getByRole('button', { name: /projet gryzzly de HUD overlay/i })).toBeInTheDocument();
  });

  it('leaves a task-less lane as plain text', () => {
    render(
      <TimesheetLanes
        lanes={[sourceLane]}
        quarters={[]}
        projects={projects}
        onAssignLaneTask={onAssignLaneTask}
      />,
    );
    expect(screen.queryByRole('button', { name: /projet gryzzly/i })).not.toBeInTheDocument();
    expect(screen.getByText('Canal Plus')).toBeInTheDocument();
  });

  it('shows no picker on a validated day', () => {
    render(
      <TimesheetLanes
        lanes={[taskLane]}
        quarters={[]}
        projects={projects}
        onAssignLaneTask={onAssignLaneTask}
        readOnly
      />,
    );
    expect(screen.queryByRole('button', { name: /projet gryzzly/i })).not.toBeInTheDocument();
    expect(screen.getByText('sans projet Gryzzly')).toBeInTheDocument();
  });

  it('hands the lane task id up with the picked Gryzzly task', () => {
    render(
      <TimesheetLanes
        lanes={[taskLane]}
        quarters={[]}
        projects={projects}
        onAssignLaneTask={onAssignLaneTask}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /projet gryzzly/i }));
    fireEvent.click(screen.getByRole('option', { name: /Pilotage/ }));

    expect(onAssignLaneTask).toHaveBeenCalledWith(
      '11111111-1111-1111-1111-111111111111',
      't1',
    );
  });

  it('stays read-only when no assign handler is wired', () => {
    render(<TimesheetLanes lanes={[taskLane]} quarters={[]} projects={projects} />);
    expect(screen.queryByRole('button', { name: /projet gryzzly/i })).not.toBeInTheDocument();
  });
});
