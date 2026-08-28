import { render, screen, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// The hooks the block reads from — mocked at the module boundary so the
// tests exercise the block's own logic (chrono tick, quarter/overload
// detection, empty state), not urql or the GraphQL wire format.
// `useSurfaceVisibility` is deliberately NOT mocked — real hook, toggled via
// `setVisibility()` below, same technique `useSurfaceVisibility.test.ts` uses.
const activityMock = vi.fn();
const timesheetMock = vi.fn();
const dashboardMock = vi.fn();
const nextBreakDueMock = vi.fn();

vi.mock('@/hooks/use-activity', () => ({ useActivity: (...args: unknown[]) => activityMock(...args) }));
vi.mock('@/hooks/use-timesheet', () => ({ useTimesheet: (...args: unknown[]) => timesheetMock(...args) }));
vi.mock('@/hooks/use-dashboard', () => ({ useDashboard: (...args: unknown[]) => dashboardMock(...args) }));
vi.mock('@/hooks/use-break-rules', () => ({ useNextBreakDue: (...args: unknown[]) => nextBreakDueMock(...args) }));

import { FocusBlock } from './FocusBlock';

function setVisibility(state: DocumentVisibilityState) {
  Object.defineProperty(document, 'visibilityState', {
    configurable: true,
    get: () => state,
  });
  document.dispatchEvent(new Event('visibilitychange'));
}

// Real quarter boundaries (08–10, 10–12, 13–15, 15–17), in minutes since
// midnight — matches the configured workday windows documented in CLAUDE.md.
const QUARTER_BOUNDS: readonly [number, number][] = [
  [480, 600],
  [600, 720],
  [780, 900],
  [900, 1020],
];

function makeQuarters(confidences: readonly ('HIGH' | 'MEDIUM' | 'LOW')[] = ['LOW', 'LOW', 'LOW', 'LOW']) {
  return QUARTER_BOUNDS.map(([startMin, endMin], index) => ({
    index,
    startMin,
    endMin,
    hours: (endMin - startMin) / 60,
    oooHours: 0,
    declarableHours: (endMin - startMin) / 60,
    confidence: confidences[index],
    shares: [],
  }));
}

function mockHooks({
  currentActivity = null as null | { id: string; task: { id: string; title: string } | null; startTime: string; halfDay: string },
  quarters = makeQuarters(),
  dashboardData = null as null | Record<string, unknown>,
  nextBreakDue = null as string | null,
  refetchNextBreakDue = vi.fn(),
} = {}) {
  activityMock.mockReturnValue({ currentActivity });
  timesheetMock.mockReturnValue({ day: quarters.length ? { quarters } : null });
  dashboardMock.mockReturnValue({ data: dashboardData });
  nextBreakDueMock.mockReturnValue({ nextBreakDue, refetch: refetchNextBreakDue });
  return { refetchNextBreakDue };
}

describe('FocusBlock', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-08-28T09:30:00'));
    activityMock.mockReset();
    timesheetMock.mockReset();
    dashboardMock.mockReset();
    nextBreakDueMock.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
    setVisibility('visible');
  });

  it('shows the active task and its running chronometer', () => {
    // Started 1h01m01s before "now" (09:30:00) — an arbitrary, non-round
    // duration so the assertion can't pass by accident on a rounded value.
    mockHooks({
      currentActivity: {
        id: 'act-1',
        task: { id: 'task-1', title: 'Revue du dossier de cadrage' },
        startTime: '2026-08-28T08:28:59',
        halfDay: 'MORNING',
      },
    });

    render(<FocusBlock lit />);

    expect(screen.getByText('Revue du dossier de cadrage')).toBeInTheDocument();
    expect(screen.getByText('01:01:01')).toBeInTheDocument();

    // It ticks: a second later the display advances by one second too.
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.getByText('01:01:02')).toBeInTheDocument();
  });

  it('marks the current quarter of the day', () => {
    // System time is 09:30 → falls inside Q1 (08–10), index 0.
    mockHooks({ quarters: makeQuarters() });

    render(<FocusBlock lit />);

    expect(screen.getByTestId('quarter-0').className).toContain('hud-quarters__segment--current');
    expect(screen.getByTestId('quarter-1').className).not.toContain('hud-quarters__segment--current');
    expect(screen.getByTestId('quarter-2').className).not.toContain('hud-quarters__segment--current');
    expect(screen.getByTestId('quarter-3').className).not.toContain('hud-quarters__segment--current');
  });

  it('warns when the day load exceeds capacity', () => {
    mockHooks({
      dashboardData: {
        tasks: [
          {
            id: 't1',
            status: 'IN_PROGRESS',
            effectiveRemainingHours: 9,
            effectiveEstimatedHours: null,
            plannedStart: '2026-08-28T09:00:00Z',
            deadline: null,
          },
        ],
        meetings: [],
        workingHoursPerDay: 8,
      },
    });

    render(<FocusBlock lit />);

    expect(screen.getByText(/over capacity/i)).toBeInTheDocument();
  });

  it('falls back to a readable empty state when no task is active', () => {
    mockHooks({ currentActivity: null });

    render(<FocusBlock lit />);

    expect(screen.getByText('No active task')).toBeInTheDocument();
    expect(screen.getByText('--:--:--')).toBeInTheDocument();
  });

  it('carries the lit class only when told to', () => {
    mockHooks();

    const { rerender } = render(<FocusBlock lit />);
    expect(screen.getByTestId('focus-block').className).toContain('hud-panel--lit');

    rerender(<FocusBlock lit={false} />);
    expect(screen.getByTestId('focus-block').className).not.toContain('hud-panel--lit');
  });

  it('shows a countdown to the next break', () => {
    // 09:30:00 "now" → due at 09:42:00 is exactly 12 minutes out.
    mockHooks({ nextBreakDue: '2026-08-28T09:42:00' });

    render(<FocusBlock lit />);

    expect(screen.getByText('12 min')).toBeInTheDocument();
  });

  it('reads "None today" when no break is due', () => {
    // A normal outcome (e.g. an all-daily routine) — not an error, not a
    // loading state. `nextBreakDue: null` is the resolver's own contract.
    mockHooks({ nextBreakDue: null });

    render(<FocusBlock lit />);

    expect(screen.getByText('None today')).toBeInTheDocument();
  });

  it('reads "Overdue" instead of a negative countdown, and asks for a fresh value', () => {
    // Due a minute ago — the fetched instant is stale the moment it passes.
    const { refetchNextBreakDue } = mockHooks({ nextBreakDue: '2026-08-28T09:29:00' });

    render(<FocusBlock lit />);

    expect(screen.getByText('Overdue')).toBeInTheDocument();
    expect(refetchNextBreakDue).toHaveBeenCalledTimes(1);
  });

  it('stops ticking the countdown while the surface is hidden, and catches up when it returns', () => {
    setVisibility('hidden');
    mockHooks({ nextBreakDue: '2026-08-28T09:42:00' });

    render(<FocusBlock lit />);
    expect(screen.getByText('12 min')).toBeInTheDocument();

    // Five minutes pass with the HUD hidden — the display must not move.
    act(() => {
      vi.advanceTimersByTime(5 * 60 * 1000);
    });
    expect(screen.getByText('12 min')).toBeInTheDocument();

    // The surface comes back: it catches up to the real remaining time.
    act(() => {
      setVisibility('visible');
    });
    expect(screen.getByText('7 min')).toBeInTheDocument();
  });
});
