import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';

// The grid now also hosts FocusBlock, which reads real data hooks (urql
// underneath) — mocked here so this page-level test stays about the boot
// sequence / grid handoff, not about GraphQL wiring (FocusBlock.test.tsx
// owns that).
vi.mock('@/hooks/use-activity', () => ({ useActivity: () => ({ currentActivity: null }) }));
vi.mock('@/hooks/use-timesheet', () => ({ useTimesheet: () => ({ day: null }) }));
const dashboardMock = vi.fn();
vi.mock('@/hooks/use-dashboard', () => ({ useDashboard: (...args: unknown[]) => dashboardMock(...args) }));
vi.mock('@/hooks/use-break-rules', () => ({
  useNextBreakDue: () => ({ nextBreakDue: null, refetch: vi.fn() }),
  useBreakRules: () => ({ stats: { perRule: [] } }),
}));

import { HudPage } from './HudPage';

// The grid now hosts HudNav, which reads router context (useLocation /
// useNavigate) — so every render needs a Router, same as it gets from
// BrowserRouter in the real app.
const renderHudPage = () => render(<HudPage />, { wrapper: MemoryRouter });

/** Renders and skips the boot sequence, which is what stands between a test
 *  and the grid the glow lives on. */
function renderGrid() {
  const view = renderHudPage();
  act(() => void vi.advanceTimersByTime(1600));
  return view;
}

const CALM_WEEK = { capacity: 10, totalPlanned: 20, totalMeetings: 0, overload: false };

function mockDashboard({
  tasks = [] as unknown[],
  meetings = [] as unknown[],
  weeklyWorkload = CALM_WEEK as unknown,
} = {}) {
  dashboardMock.mockReturnValue({ data: { tasks, meetings, weeklyWorkload, workingHoursPerDay: 8 } });
}

/** The three arbitration branches, plus the case where all three fire at
 *  once — the input space the "exactly one glow" invariant has to survive. */
const SCENARIOS = {
  calm: {},
  imminentMeeting: {
    meetings: [
      {
        id: 'm1',
        title: 'Point hebdo SAFT',
        startTime: '2026-08-28T09:35:00',
        endTime: '2026-08-28T10:00:00',
        durationHours: 0.5,
        showAs: 'busy',
      },
    ],
  },
  overloaded: {
    weeklyWorkload: { capacity: 10, totalPlanned: 40, totalMeetings: 5, overload: true },
  },
  everythingAtOnce: {
    tasks: [{ id: 't1', title: 'Revue eProject A3', status: 'TODO', deadline: '2026-08-28' }],
    meetings: [
      {
        id: 'm1',
        title: 'Point hebdo SAFT',
        startTime: '2026-08-28T09:35:00',
        endTime: '2026-08-28T10:00:00',
        durationHours: 0.5,
        showAs: 'busy',
      },
    ],
    weeklyWorkload: { capacity: 10, totalPlanned: 40, totalMeetings: 5, overload: true },
  },
};

const litPanels = (container: HTMLElement) => container.querySelectorAll('.hud-panel--lit');

describe('HudPage', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-08-28T09:30:00'));
    dashboardMock.mockReset();
    mockDashboard();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('shows the boot sequence first', () => {
    renderHudPage();
    expect(screen.getByTestId('boot-sequence')).toBeInTheDocument();
    expect(screen.queryByTestId('hud-grid')).not.toBeInTheDocument();
  });

  it('gives way to the grid after the sequence', () => {
    renderHudPage();
    act(() => void vi.advanceTimersByTime(1600));
    expect(screen.queryByTestId('boot-sequence')).not.toBeInTheDocument();
    expect(screen.getByTestId('hud-grid')).toBeInTheDocument();
  });

  it('paints a transparent background, the window being transparent', () => {
    const { container } = renderHudPage();
    const root = container.firstElementChild as HTMLElement;
    expect(root.className).toContain('bg-transparent');
  });

  // ─── the glow, and where it points ───

  it('lights exactly one panel, whatever the inputs', () => {
    // The invariant of the whole system, asserted where it can actually
    // break: the rendered grid. A hook returning one string can never return
    // two — a second panel left lit is a wiring bug, not a hook bug.
    for (const [name, scenario] of Object.entries(SCENARIOS)) {
      mockDashboard(scenario);
      const { container, unmount } = renderGrid();
      expect(litPanels(container), name).toHaveLength(1);
      unmount();
    }
  });

  it('rests the glow on Focus when nothing is pressing', () => {
    mockDashboard(SCENARIOS.calm);
    renderGrid();
    expect(screen.getByTestId('focus-block').className).toContain('hud-panel--lit');
    expect(screen.getByTestId('pressure-block').className).not.toContain('hud-panel--lit');
    expect(screen.getByTestId('agenda-block').className).not.toContain('hud-panel--lit');
  });

  it('moves the glow onto Pressure when the week is overloaded', () => {
    mockDashboard(SCENARIOS.overloaded);
    renderGrid();
    expect(screen.getByTestId('pressure-block').className).toContain('hud-panel--lit');
    expect(screen.getByTestId('focus-block').className).not.toContain('hud-panel--lit');
  });

  it('moves the glow onto Agenda when a meeting is minutes away', () => {
    mockDashboard(SCENARIOS.imminentMeeting);
    renderGrid();
    expect(screen.getByTestId('agenda-block').className).toContain('hud-panel--lit');
    expect(screen.getByTestId('focus-block').className).not.toContain('hud-panel--lit');
  });

  it('gives an imminent meeting the glow even while the week is overloaded', () => {
    mockDashboard(SCENARIOS.everythingAtOnce);
    renderGrid();
    expect(screen.getByTestId('agenda-block').className).toContain('hud-panel--lit');
    expect(screen.getByTestId('pressure-block').className).not.toContain('hud-panel--lit');
  });

  it('touches no data hook while the boot sequence plays', () => {
    // Regression guard: the glow arbiter reads the dashboard, so calling it
    // from `HudPage` itself made the page need a urql client before anything
    // was on screen — `App.test.tsx`, which renders without a Provider,
    // caught it. The grid, and everything that queries, mounts after boot.
    renderHudPage();
    expect(dashboardMock).not.toHaveBeenCalled();

    act(() => void vi.advanceTimersByTime(1600));
    expect(dashboardMock).toHaveBeenCalled();
  });
});
