import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
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

import { HudPage, resetBootSequenceForTests } from './HudPage';

// jsdom applies none of this stylesheet, so the source text is the only way to
// assert the reduced-motion contract — same technique the block tests use.
const HUD_CSS = readFileSync(resolve(__dirname, 'hud.css'), 'utf8');

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
    // The "played" flag lives at module scope so it survives HudPage
    // unmounting when the overlay leaves the HUD route — which means it also
    // survives between tests unless it is cleared here.
    resetBootSequenceForTests();
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

  // ─── the boot sequence, and who gets to see it ───

  /** jsdom reports `visible` by default and offers no way to toggle it, so the
   *  property is redefined and the event fired by hand — the same pair
   *  `useSurfaceVisibility` listens for in the real window. */
  function setSurface(state: 'visible' | 'hidden') {
    Object.defineProperty(document, 'visibilityState', { value: state, configurable: true });
    act(() => void document.dispatchEvent(new Event('visibilitychange')));
  }

  afterEach(() => setSurface('visible'));

  it('holds the boot sequence back while the surface is hidden', () => {
    // The window is born on a hidden special workspace. A sequence that starts
    // at mount plays out behind the curtain and is never seen by anyone.
    setSurface('hidden');
    renderHudPage();

    act(() => void vi.advanceTimersByTime(5000));
    expect(screen.getByTestId('boot-sequence')).toBeInTheDocument();
    expect(screen.queryByTestId('hud-grid')).not.toBeInTheDocument();
  });

  it('runs it on the first opening', () => {
    setSurface('hidden');
    renderHudPage();
    act(() => void vi.advanceTimersByTime(5000));

    setSurface('visible');
    expect(screen.getByTestId('boot-sequence')).toBeInTheDocument();

    act(() => void vi.advanceTimersByTime(1600));
    expect(screen.getByTestId('hud-grid')).toBeInTheDocument();
  });

  // ─── the opening animation ───

  it('plays on arrival, without holding a single value back', () => {
    // The distinction that matters: the grid is present and readable the whole
    // time. The boot sequence was cut back for standing between the keystroke
    // and the numbers; this animates them in instead.
    renderGrid();

    const grid = screen.getByTestId('hud-grid');
    expect(grid).toHaveAttribute('data-opening');
    expect(screen.getByTestId('hud-sweep')).toBeInTheDocument();
    expect(screen.getByTestId('station-block')).toBeInTheDocument();
  });

  it('takes the animation back off, which is what arms the next opening', () => {
    // Removing the attribute is load-bearing, not tidiness: the rules stop
    // matching, so applying them again later runs them from the top instead of
    // finding an animation already spent.
    renderGrid();
    act(() => void vi.advanceTimersByTime(1000));

    expect(screen.getByTestId('hud-grid')).not.toHaveAttribute('data-opening');
    expect(screen.queryByTestId('hud-sweep')).not.toBeInTheDocument();
  });

  it('plays again on every reopening', () => {
    renderGrid();
    act(() => void vi.advanceTimersByTime(1000));
    expect(screen.getByTestId('hud-grid')).not.toHaveAttribute('data-opening');

    setSurface('hidden');
    setSurface('visible');

    expect(screen.getByTestId('hud-grid')).toHaveAttribute('data-opening');
    expect(screen.getByTestId('hud-sweep')).toBeInTheDocument();
  });

  it('gives someone who asked for less motion the grid, still', () => {
    // Asserted against the stylesheet, jsdom applying none of it: a shorter
    // animation would not honour the request, so the rule must cancel it.
    const reduced = HUD_CSS.match(/@media \(prefers-reduced-motion:\s*reduce\)\s*\{[^}]*\{[^}]*\}[^}]*\}/)?.[0] ?? '';
    expect(reduced).toMatch(/animation:\s*none/);
    expect(reduced).toMatch(/\.hud-sweep\s*\{\s*display:\s*none/);
  });

  it('never runs it a second time', () => {
    // Once per process. It ran on every opening for a while; the verdict was
    // that 1.5s between the keystroke and the data is not worth it.
    renderGrid();
    expect(screen.queryByTestId('boot-sequence')).not.toBeInTheDocument();

    setSurface('hidden');
    setSurface('visible');

    expect(screen.queryByTestId('boot-sequence')).not.toBeInTheDocument();
    expect(screen.getByTestId('hud-grid')).toBeInTheDocument();
  });

  it('does not replay when the HUD is remounted by a return from another tab', () => {
    // `HudPage` unmounts on the way to Timesheet and mounts again on the way
    // back, so a flag held in component state would reset and the sequence
    // would replay on every return — which is exactly what module scope is
    // for here.
    const first = renderGrid();
    first.unmount();

    renderHudPage();
    expect(screen.queryByTestId('boot-sequence')).not.toBeInTheDocument();
    expect(screen.getByTestId('hud-grid')).toBeInTheDocument();
  });
});
