import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

const day = {
  date: '2026-06-08', status: 'DRAFT', targetHours: 7.5, roundingIncrement: 0.25,
  totalHours: 7.5, dayConfidence: 'HIGH', unattributedHours: 0, unresolved: [], blocks: [],
  lines: [{ gryzzlyProjectId: 'p1', projectName: 'Proj One', hours: 7.5, isPinned: false, confidence: 'HIGH', sourceRefs: [] }],
};

// Shared mock so the tests can assert on / re-stub the reconstruct call.
const mocks = vi.hoisted(() => ({ reconstruct: vi.fn() }));

vi.mock('@/hooks/use-timesheet', () => ({
  useTimesheet: () => ({
    day, loading: false, error: null,
    reconstruct: mocks.reconstruct, saveLines: vi.fn(), validate: vi.fn(), markOff: vi.fn(), refetch: vi.fn(),
  }),
  useGryzzlyProjects: () => ({ projects: [], loading: false, error: undefined }),
}));

import { TimesheetPage } from './TimesheetPage';

describe('TimesheetPage', () => {
  beforeEach(() => {
    mocks.reconstruct.mockReset();
    mocks.reconstruct.mockResolvedValue({ message: 'Reconstruit : ...', isError: false });
  });

  it('renders the day summary and timeline heading', () => {
    render(<TimesheetPage />);
    expect(screen.getByText('Proj One')).toBeInTheDocument();
    expect(screen.getByText(/heures × projet/i)).toBeInTheDocument();
  });

  it('reveals the confirmation banner without reconstructing on "Reconstruire depuis les signaux"', () => {
    render(<TimesheetPage />);
    fireEvent.click(screen.getByRole('button', { name: 'Reconstruire depuis les signaux' }));
    expect(screen.getByText(/Reconstruire depuis les signaux \?/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /confirmer/i })).toBeInTheDocument();
    expect(mocks.reconstruct).not.toHaveBeenCalled();
  });

  it('hides the banner and does not reconstruct when "Annuler" is clicked', () => {
    render(<TimesheetPage />);
    fireEvent.click(screen.getByRole('button', { name: 'Reconstruire depuis les signaux' }));
    fireEvent.click(screen.getByRole('button', { name: /annuler/i }));
    expect(screen.queryByText(/Reconstruire depuis les signaux \?/i)).not.toBeInTheDocument();
    expect(mocks.reconstruct).not.toHaveBeenCalled();
  });

  it('reconstructs once and shows the returned message when "Confirmer" is clicked', async () => {
    render(<TimesheetPage />);
    fireEvent.click(screen.getByRole('button', { name: 'Reconstruire depuis les signaux' }));
    fireEvent.click(screen.getByRole('button', { name: /confirmer/i }));
    expect(await screen.findByText(/Reconstruit :/i)).toBeInTheDocument();
    expect(mocks.reconstruct).toHaveBeenCalledOnce();
  });

  it('shows an error message (styled red) when reconstruct reports isError', async () => {
    mocks.reconstruct.mockResolvedValue({ message: 'Validation error: ...', isError: true });
    render(<TimesheetPage />);
    fireEvent.click(screen.getByRole('button', { name: 'Reconstruire depuis les signaux' }));
    fireEvent.click(screen.getByRole('button', { name: /confirmer/i }));
    const msg = await screen.findByText(/Validation error:/i);
    expect(msg).toBeInTheDocument();
    expect(msg).toHaveClass('text-red-600');
  });
});
