import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

import type { UnresolvedSignal } from '@/hooks/use-timesheet';

const share = (label: string, hours: number, presenceMinutes: number, isPinned = false) => ({
  laneKey: `task:${label}`, taskId: null, label, gryzzlyProjectId: 'p1',
  presenceMinutes, hours, isPinned,
});

const quarter = (index: number, shares: ReturnType<typeof share>[]) => ({
  index, startMin: 480 + index * 120, endMin: 600 + index * 120,
  hours: 2, oooHours: 0, declarableHours: 2, confidence: 'HIGH' as const, shares,
});

const day = {
  date: '2026-06-08', status: 'DRAFT', targetHours: 8, roundingIncrement: 0.25,
  totalHours: 8, dayConfidence: 'HIGH', unattributedHours: 0, unresolved: [],
  lanes: [
    { laneKey: 'task:A', taskId: 'A', label: 'Tâche A', gryzzlyProjectId: 'p1', outsideMinutes: 0,
      intervals: [{ startMin: 540, endMin: 620 }] },
    { laneKey: 'task:B', taskId: 'B', label: 'Tâche B', gryzzlyProjectId: 'p1', outsideMinutes: 94,
      intervals: [{ startMin: 560, endMin: 700 }] },
  ],
  quarters: [
    quarter(0, [share('Tâche A', 1.5, 80), share('Tâche B', 0.5, 40)]),
    quarter(1, [share('Tâche B', 2, 100)]),
    quarter(2, []),
    quarter(3, []),
  ],
  outsideWorkday: [{ laneKey: 'task:B', label: 'Tâche B', minutes: 94 }],
  lines: [{ gryzzlyProjectId: 'p1', projectName: 'Proj One', hours: 8, isPinned: false, confidence: 'HIGH', sourceRefs: [] }],
};

// Shared mock so the tests can assert on / re-stub the reconstruct call.
const mocks = vi.hoisted(() => ({
  reconstruct: vi.fn(),
  assignLaneGryzzlyTask: vi.fn(),
  setShare: vi.fn(),
  resetQuarter: vi.fn(),
}));

// Read at render time (not at mock-factory time), so a test can hand the page a day
// carrying unresolved signals without a second mock factory.
let unresolvedSignals: UnresolvedSignal[] = [];

vi.mock('@/hooks/use-timesheet', () => ({
  useTimesheet: () => ({
    day: { ...day, unresolved: unresolvedSignals }, loading: false, error: null,
    reconstruct: mocks.reconstruct, assignLaneGryzzlyTask: mocks.assignLaneGryzzlyTask,
    setShare: mocks.setShare, clearShare: vi.fn(),
    resetQuarter: mocks.resetQuarter, validate: vi.fn(), markOff: vi.fn(), refetch: vi.fn(),
  }),
  useGryzzlyProjects: () => ({ projects: [], loading: false, error: undefined }),
}));

// The lane picker's list is the shared Gryzzly one; stub its catalog query so the page
// test needs no urql provider.
vi.mock('@/hooks/use-gryzzly-tasks', () => ({
  useGryzzlyTasks: () => ({
    options: [
      { gryzzlyTaskId: 't1', name: 'Pilotage', projectName: 'Canal Plus', projectStatus: 'active' },
    ],
    fetching: false,
    error: null,
  }),
}));

import { TimesheetPage } from './TimesheetPage';

describe('TimesheetPage', () => {
  beforeEach(() => {
    mocks.reconstruct.mockReset();
    mocks.reconstruct.mockResolvedValue({ message: 'Reconstruit : ...', isError: false });
    mocks.setShare.mockReset();
    mocks.setShare.mockResolvedValue(null);
    mocks.resetQuarter.mockReset();
    mocks.resetQuarter.mockResolvedValue(null);
    mocks.assignLaneGryzzlyTask.mockReset();
    mocks.assignLaneGryzzlyTask.mockResolvedValue({ message: 'Projet Gryzzly mis à jour.', isError: false });
    unresolvedSignals = [];
  });

  it('renders the day summary and the concurrent-work heading', () => {
    render(<TimesheetPage />);
    expect(screen.getByText('Proj One')).toBeInTheDocument();
    expect(screen.getByText(/heures × projet/i)).toBeInTheDocument();
    expect(screen.getByText(/travail concurrent/i)).toBeInTheDocument();
  });

  /// The point of the whole screen: two tasks that ran at the same time must both be
  /// visible, which the single-track timeline could not show.
  it('renders one lane row per concurrent task', () => {
    render(<TimesheetPage />);
    // Each lane label appears once in the lanes view; the quarters repeat them as shares.
    expect(screen.getAllByTitle('Tâche A').length).toBeGreaterThan(0);
    expect(screen.getAllByTitle('Tâche B').length).toBeGreaterThan(0);
  });

  it('shows the four quarters with their hours against the declarable total', () => {
    render(<TimesheetPage />);
    expect(screen.getByText(/Q1 ·/)).toBeInTheDocument();
    expect(screen.getByText(/Q4 ·/)).toBeInTheDocument();
    expect(screen.getAllByText(/2\.00 \/ 2\.00 h/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/Rien de déclaré sur ce quart/).length).toBe(2);
  });

  it('pins a share through the hook when an hours field is edited', () => {
    render(<TimesheetPage />);
    fireEvent.change(screen.getByLabelText('heures pour Tâche A'), { target: { value: '1.75' } });
    expect(mocks.setShare).toHaveBeenCalledWith(0, 'task:Tâche A', 1.75);
  });

  /// The fast path this screen exists to offer: spot the wrong project on a lane row,
  /// fix it there, and see the day rebuilt without a confirmation detour.
  it('reassigns a lane\'s Gryzzly task from the row and reports the rebuild', async () => {
    render(<TimesheetPage />);
    fireEvent.click(screen.getByRole('button', { name: /projet gryzzly de Tâche A/i }));
    fireEvent.click(screen.getByRole('option', { name: /Pilotage/ }));

    expect(mocks.assignLaneGryzzlyTask).toHaveBeenCalledWith('A', 't1');
    expect(await screen.findByText(/Projet Gryzzly mis à jour/)).toBeInTheDocument();
    expect(mocks.reconstruct).not.toHaveBeenCalled();
  });

  it('reports the evidence that fell outside the working day', () => {
    render(<TimesheetPage />);
    expect(screen.getByText(/1 h 34 de traces hors plage horaire/)).toBeInTheDocument();
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

  it('lists one time + note row per unresolved signal, not just their count', () => {
    // Bare local NaiveDateTime, like the backend wire format → stable HH:MM in any timezone.
    unresolvedSignals = [
      { sourceRef: 'wl:11111111-1111-1111-1111-111111111111', label: 'Refactor du parseur de signaux', at: '2026-06-08T09:15:00' },
      { sourceRef: 'wl:22222222-2222-2222-2222-222222222222', label: 'Revue de la migration 016', at: '2026-06-08T14:05:00' },
    ];
    render(<TimesheetPage />);

    expect(screen.getByText(/2 signal\(aux\) non résolu\(s\)/)).toBeInTheDocument();
    expect(screen.getByText('09:15')).toBeInTheDocument();
    expect(screen.getByText(/Refactor du parseur de signaux/)).toBeInTheDocument();
    expect(screen.getByText('14:05')).toBeInTheDocument();
    expect(screen.getByText(/Revue de la migration 016/)).toBeInTheDocument();
  });

  it('renders no unresolved-signal list when the day has none', () => {
    render(<TimesheetPage />);
    expect(screen.queryByText(/non résolu/i)).not.toBeInTheDocument();
  });
});
