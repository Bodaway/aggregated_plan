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

describe('ProjectSummarySidebar', () => {
  it('renders each line, the unattributed row, and the total vs target', () => {
    render(<ProjectSummarySidebar day={day} onSaveLines={vi.fn()} onValidate={vi.fn()} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    expect(screen.getByText('Proj One')).toBeInTheDocument();
    expect(screen.getByText(/unattributed/i)).toBeInTheDocument();
    expect(screen.getByText(/7\.5.*\/.*7\.5/)).toBeInTheDocument(); // total / target
  });

  it('validates via the callback', () => {
    const onValidate = vi.fn();
    render(<ProjectSummarySidebar day={day} onSaveLines={vi.fn()} onValidate={onValidate} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    fireEvent.click(screen.getByRole('button', { name: /validate/i }));
    expect(onValidate).toHaveBeenCalledOnce();
  });

  it('saves edited hours (pinning the edited line) via onSaveLines', () => {
    const onSaveLines = vi.fn();
    render(<ProjectSummarySidebar day={day} onSaveLines={onSaveLines} onValidate={vi.fn()} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    const inputs = screen.getAllByRole('spinbutton');
    fireEvent.change(inputs[0], { target: { value: '5' } });
    fireEvent.click(screen.getByRole('button', { name: /save/i }));
    expect(onSaveLines).toHaveBeenCalledWith(
      expect.arrayContaining([
        expect.objectContaining({ gryzzlyProjectId: 'p1', hours: 5, isPinned: true }),
      ]),
    );
  });
});
