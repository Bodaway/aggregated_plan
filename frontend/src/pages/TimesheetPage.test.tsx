import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

const day = {
  date: '2026-06-08', status: 'DRAFT', targetHours: 7.5, roundingIncrement: 0.25,
  totalHours: 7.5, dayConfidence: 'HIGH', unattributedHours: 0, unresolved: [], blocks: [],
  lines: [{ gryzzlyProjectId: 'p1', projectName: 'Proj One', hours: 7.5, isPinned: false, confidence: 'HIGH', sourceRefs: [] }],
};
vi.mock('@/hooks/use-timesheet', () => ({
  useTimesheet: () => ({
    day, loading: false, error: null,
    reconstruct: vi.fn(), saveLines: vi.fn(), validate: vi.fn(), markOff: vi.fn(), refetch: vi.fn(),
  }),
}));

import { TimesheetPage } from './TimesheetPage';

describe('TimesheetPage', () => {
  it('renders the day summary and timeline heading', () => {
    render(<TimesheetPage />);
    expect(screen.getByText('Proj One')).toBeInTheDocument();
    expect(screen.getByText(/hours × project/i)).toBeInTheDocument();
  });
});
