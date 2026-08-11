import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

import { ProjectSummarySidebar } from './ProjectSummarySidebar';
import type { Quarter, ReconstructedDay } from '@/hooks/use-timesheet';

const quarter = (index: number, projectId: string | null, hours: number): Quarter => ({
  index,
  startMin: 480 + index * 120,
  endMin: 600 + index * 120,
  hours: 2,
  oooHours: 0,
  declarableHours: 2,
  confidence: 'HIGH',
  shares: [
    {
      laneKey: `task:${index}`,
      taskId: null,
      label: `Tâche ${index}`,
      gryzzlyProjectId: projectId,
      presenceMinutes: 90,
      hours,
      isPinned: false,
    },
  ],
});

const day: ReconstructedDay = {
  date: '2026-06-08',
  status: 'DRAFT',
  targetHours: 8,
  roundingIncrement: 0.25,
  totalHours: 8,
  dayConfidence: 'HIGH',
  unattributedHours: 1.5,
  unresolved: [],
  lanes: [],
  outsideWorkday: [],
  quarters: [quarter(0, 'p1', 2), quarter(1, 'p1', 2), quarter(2, 'p1', 2), quarter(3, null, 1.5)],
  lines: [
    { gryzzlyProjectId: 'p1', projectName: 'Proj One', hours: 6, isPinned: false, confidence: 'HIGH', sourceRefs: [] },
    { gryzzlyProjectId: null, projectName: null, hours: 1.5, isPinned: false, confidence: 'LOW', sourceRefs: [] },
  ],
};

const projects = [
  { id: 'p1', label: 'SAFT' },
  { id: 'p2', label: 'Canal+' },
];

describe('ProjectSummarySidebar', () => {
  it('renders each line, the unattributed row, and the total vs target', () => {
    render(
      <ProjectSummarySidebar
        day={day}
        onValidate={vi.fn()}
        onMarkOff={vi.fn()}
        onRefresh={vi.fn()}
        busy={false}
      />,
    );
    expect(screen.getByText('Proj One')).toBeInTheDocument();
    expect(screen.getByText('Non attribué')).toBeInTheDocument();
    expect(screen.getByText(/7\.50.*\/.*8\.0/)).toBeInTheDocument();
  });

  it('resolves a project id to its catalog label', () => {
    render(
      <ProjectSummarySidebar
        day={day}
        projects={projects}
        onValidate={vi.fn()}
        onMarkOff={vi.fn()}
        onRefresh={vi.fn()}
        busy={false}
      />,
    );
    expect(screen.getByText('SAFT')).toBeInTheDocument();
  });

  /// Hours are derived from the quarters, so the sidebar must expose no way to edit
  /// them — a second source of truth is exactly what this design removed.
  it('offers no hours input: the totals are derived from the quarters', () => {
    const { container } = render(
      <ProjectSummarySidebar
        day={day}
        projects={projects}
        onValidate={vi.fn()}
        onMarkOff={vi.fn()}
        onRefresh={vi.fn()}
        busy={false}
      />,
    );
    expect(container.querySelectorAll('input')).toHaveLength(0);
    expect(container.querySelectorAll('select')).toHaveLength(0);
  });

  it('traces each project total back to the quarters that produced it', () => {
    render(
      <ProjectSummarySidebar
        day={day}
        projects={projects}
        onValidate={vi.fn()}
        onMarkOff={vi.fn()}
        onRefresh={vi.fn()}
        busy={false}
      />,
    );
    expect(screen.getByText('depuis Q1, Q2, Q3')).toBeInTheDocument();
  });

  it('validates, reconstructs and marks the day off via callbacks', () => {
    const onValidate = vi.fn();
    const onRefresh = vi.fn();
    const onMarkOff = vi.fn();
    render(
      <ProjectSummarySidebar
        day={day}
        onValidate={onValidate}
        onMarkOff={onMarkOff}
        onRefresh={onRefresh}
        busy={false}
      />,
    );
    fireEvent.click(screen.getByText('Valider et verrouiller'));
    fireEvent.click(screen.getByText('Reconstruire depuis les signaux'));
    fireEvent.click(screen.getByText('Jour off'));
    expect(onValidate).toHaveBeenCalled();
    expect(onRefresh).toHaveBeenCalled();
    expect(onMarkOff).toHaveBeenCalledWith('FULL');
  });

  it('hides the actions on a validated day', () => {
    render(
      <ProjectSummarySidebar
        day={{ ...day, status: 'VALIDATED' }}
        onValidate={vi.fn()}
        onMarkOff={vi.fn()}
        onRefresh={vi.fn()}
        busy={false}
      />,
    );
    expect(screen.queryByText('Valider et verrouiller')).not.toBeInTheDocument();
    expect(screen.getByText('Validé')).toBeInTheDocument();
  });
});
