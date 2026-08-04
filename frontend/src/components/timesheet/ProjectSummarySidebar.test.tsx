import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

import { ProjectSummarySidebar } from './ProjectSummarySidebar';
import type { ReconstructedDay } from '@/hooks/use-timesheet';

const day: ReconstructedDay = {
  date: '2026-06-08', status: 'DRAFT', targetHours: 7.5, roundingIncrement: 0.25,
  totalHours: 7.5, dayConfidence: 'HIGH', unattributedHours: 1.5, unresolved: [], blocks: [],
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
    render(<ProjectSummarySidebar day={day} onSaveLines={vi.fn()} onValidate={vi.fn()} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    expect(screen.getByText('Proj One')).toBeInTheDocument();
    // Exact match hits only the row label span, not the "— Non attribué —" <option>.
    expect(screen.getByText('Non attribué')).toBeInTheDocument();
    expect(screen.getByText(/7\.5.*\/.*7\.5/)).toBeInTheDocument(); // total / target
  });

  it('validates via the callback', () => {
    const onValidate = vi.fn();
    render(<ProjectSummarySidebar day={day} onSaveLines={vi.fn()} onValidate={onValidate} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    fireEvent.click(screen.getByRole('button', { name: /valider et verrouiller/i }));
    expect(onValidate).toHaveBeenCalledOnce();
  });

  it('saves edited hours (pinning the edited line) via onSaveLines', () => {
    const onSaveLines = vi.fn();
    render(<ProjectSummarySidebar day={day} onSaveLines={onSaveLines} onValidate={vi.fn()} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    const inputs = screen.getAllByRole('spinbutton');
    fireEvent.change(inputs[0], { target: { value: '5' } });
    fireEvent.click(screen.getByRole('button', { name: /enregistrer/i }));
    expect(onSaveLines).toHaveBeenCalledWith(
      expect.arrayContaining([
        expect.objectContaining({ gryzzlyProjectId: 'p1', hours: 5, isPinned: true }),
      ]),
    );
  });

  it('renders one project select per line', () => {
    render(<ProjectSummarySidebar day={day} projects={projects} onSaveLines={vi.fn()} onValidate={vi.fn()} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    expect(screen.getAllByRole('combobox')).toHaveLength(day.lines.length);
  });

  it('assigns a project to the unattributed row and pins it on save', () => {
    const onSaveLines = vi.fn();
    render(<ProjectSummarySidebar day={day} projects={projects} onSaveLines={onSaveLines} onValidate={vi.fn()} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    const selects = screen.getAllByRole('combobox');
    // Second row is the null (Unattributed) line.
    fireEvent.change(selects[1], { target: { value: 'p1' } });
    fireEvent.click(screen.getByRole('button', { name: /enregistrer/i }));
    expect(onSaveLines).toHaveBeenCalledWith(
      expect.arrayContaining([
        expect.objectContaining({ gryzzlyProjectId: 'p1', isPinned: true }),
      ]),
    );
  });

  it('merges two rows reassigned to the same project into a single summed line', () => {
    const dayTwoLines: ReconstructedDay = {
      ...day,
      unattributedHours: 3,
      lines: [
        { gryzzlyProjectId: null, projectName: null, hours: 3, isPinned: false, confidence: 'LOW', sourceRefs: [] },
        { gryzzlyProjectId: 'p2', projectName: 'Canal+', hours: 4.5, isPinned: false, confidence: 'HIGH', sourceRefs: [] },
      ],
    };
    const onSaveLines = vi.fn();
    render(<ProjectSummarySidebar day={dayTwoLines} projects={projects} onSaveLines={onSaveLines} onValidate={vi.fn()} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    const selects = screen.getAllByRole('combobox');
    fireEvent.change(selects[0], { target: { value: 'p1' } });
    fireEvent.change(selects[1], { target: { value: 'p1' } });
    fireEvent.click(screen.getByRole('button', { name: /enregistrer/i }));
    const savedLines = onSaveLines.mock.calls[0][0] as Array<{ gryzzlyProjectId: string | null; hours: number }>;
    const p1Lines = savedLines.filter((l) => l.gryzzlyProjectId === 'p1');
    expect(p1Lines).toHaveLength(1);
    expect(p1Lines[0].hours).toBeCloseTo(7.5);
  });

  it('folds two rows reassigned to Unattributed into a single summed null line', () => {
    const dayTwoProjects: ReconstructedDay = {
      ...day,
      unattributedHours: 0,
      lines: [
        { gryzzlyProjectId: 'p1', projectName: 'Proj One', hours: 3, isPinned: false, confidence: 'HIGH', sourceRefs: [] },
        { gryzzlyProjectId: 'p2', projectName: 'Canal+', hours: 4.5, isPinned: false, confidence: 'HIGH', sourceRefs: [] },
      ],
    };
    const onSaveLines = vi.fn();
    render(<ProjectSummarySidebar day={dayTwoProjects} projects={projects} onSaveLines={onSaveLines} onValidate={vi.fn()} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    const selects = screen.getAllByRole('combobox');
    // Reassign both rows back to "— Non attribué —" (value "" → null).
    fireEvent.change(selects[0], { target: { value: '' } });
    fireEvent.change(selects[1], { target: { value: '' } });
    fireEvent.click(screen.getByRole('button', { name: /enregistrer/i }));
    const savedLines = onSaveLines.mock.calls[0][0] as Array<{ gryzzlyProjectId: string | null; hours: number }>;
    const nullLines = savedLines.filter((l) => l.gryzzlyProjectId === null);
    expect(nullLines).toHaveLength(1);
    expect(nullLines[0].hours).toBeCloseTo(7.5);
  });

  it('shows the error message returned by onSaveLines', async () => {
    const onSaveLines = vi.fn().mockResolvedValue({ message: 'pinned hours (11.5) exceed the daily target (7.5)' });
    render(<ProjectSummarySidebar day={day} projects={projects} onSaveLines={onSaveLines} onValidate={vi.fn()} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    fireEvent.click(screen.getByRole('button', { name: /enregistrer/i }));
    expect(
      await screen.findByText(/pinned hours \(11\.5\) exceed the daily target \(7\.5\)/i),
    ).toBeInTheDocument();
  });

  it('renders the action grid for an editable (DRAFT) day', () => {
    render(<ProjectSummarySidebar day={day} projects={projects} onSaveLines={vi.fn()} onValidate={vi.fn()} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    expect(screen.getByRole('button', { name: /reconstruire depuis les signaux/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /enregistrer/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /valider et verrouiller/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /jour off/i })).toBeInTheDocument();
  });

  it('keeps the action grid visible on a DAY_OFF day so the day can be recovered via Enregistrer', () => {
    const dayOff: ReconstructedDay = { ...day, status: 'DAY_OFF' };
    render(<ProjectSummarySidebar day={dayOff} projects={projects} onSaveLines={vi.fn()} onValidate={vi.fn()} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    expect(screen.getByRole('button', { name: /reconstruire depuis les signaux/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /enregistrer/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /valider et verrouiller/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /jour off/i })).toBeInTheDocument();
  });
});
