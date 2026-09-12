import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { render, screen, act, cleanup } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Read directly, same technique hud.css.test.ts uses — jsdom does not apply
// this stylesheet, so a rendered element's computed style can't tell us
// whether an empty state is actually styled as deliberate rather than merely
// present in the DOM.
const HUD_CSS = readFileSync(resolve(__dirname, '../hud.css'), 'utf8');

// The hooks the block reads from, mocked at the module boundary so these
// tests exercise the block's own logic, not urql or the GraphQL wire format.
// `useSurfaceVisibility` is deliberately NOT mocked — real hook, toggled via
// `setVisibility()` below, same technique `useSurfaceVisibility.test.ts` uses.
const dashboardMock = vi.fn();
const timesheetMock = vi.fn();
const nextBreakDueMock = vi.fn();

vi.mock('@/hooks/use-dashboard', () => ({ useDashboard: (...args: unknown[]) => dashboardMock(...args) }));
vi.mock('@/hooks/use-timesheet', () => ({ useTimesheet: (...args: unknown[]) => timesheetMock(...args) }));
vi.mock('@/hooks/use-break-rules', () => ({ useNextBreakDue: (...args: unknown[]) => nextBreakDueMock(...args) }));

import { PressureBlock } from './PressureBlock';

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
  tasks = [] as Record<string, unknown>[],
  meetings = [] as Record<string, unknown>[],
  weeklyWorkload = { capacity: 10, totalPlanned: 30, totalMeetings: 6, overload: false },
  workingHoursPerDay = 8,
  quarters = makeQuarters(),
  nextBreakDue = null as string | null,
  refetchNextBreakDue = vi.fn(),
} = {}) {
  dashboardMock.mockReturnValue({ data: { tasks, meetings, weeklyWorkload, workingHoursPerDay } });
  timesheetMock.mockReturnValue({ day: quarters.length ? { quarters } : null });
  nextBreakDueMock.mockReturnValue({ nextBreakDue, refetch: refetchNextBreakDue });
  return { refetchNextBreakDue };
}

describe('PressureBlock', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-08-28T09:30:00'));
    dashboardMock.mockReset();
    timesheetMock.mockReset();
    nextBreakDueMock.mockReset();
  });

  afterEach(() => {
    // Unmount BEFORE touching timers/visibility: RTL's own auto-cleanup
    // afterEach is registered at import time (outside this describe), so it
    // would otherwise run AFTER this one — leaving the component mounted
    // while `vi.useRealTimers()` / `setVisibility()` fire, which is what was
    // producing an act() warning on the timer-driven tests.
    cleanup();
    vi.useRealTimers();
    setVisibility('visible');
  });

  // ─── deadlines and the week's capacity ───

  it('lists deadlines sorted by proximity, today in the sanctioned pink', () => {
    // Deliberately out of chronological order in the source data — the
    // component, not the fixture, must do the sorting.
    mockHooks({
      tasks: [
        { id: 't1', title: 'Cadrage Standards', deadline: '2026-08-30' },
        { id: 't2', title: 'Revue eProject A3', deadline: '2026-08-28' },
      ],
    });

    render(<PressureBlock lit={false} />);

    expect(screen.getByTestId('pressure-block')).toBeInTheDocument();
    expect(screen.getByText(/pressure/i)).toBeInTheDocument();
    expect(screen.getByText(/2 deadlines/i)).toBeInTheDocument();

    const rows = screen.getAllByTestId('pressure-deadline');
    expect(rows).toHaveLength(2);
    // Today's deadline (08-28) sorts before the later one (08-30).
    expect(rows[0]).toHaveTextContent('Revue eProject A3');
    expect(rows[1]).toHaveTextContent('Cadrage Standards');

    const whens = screen.getAllByTestId('deadline-when');
    expect(whens[0]).toHaveTextContent('Today');
    expect(whens[0].className).toContain('hud-pressure__when--hot');
    expect(whens[1]).toHaveTextContent('In 2d');
    expect(whens[1].className).not.toContain('hud-pressure__when--hot');

    // Capacity: (30 + 6) planned hours over 10 half-days * 4h = 40h → 90%.
    expect(screen.getByText('90%')).toBeInTheDocument();
    const gauge = screen.getByTestId('pressure-gauge');
    expect(gauge.className).not.toContain('hud-gauge--over');
    expect(gauge.querySelector('i')).toHaveStyle({ width: '90%' });
  });

  it('caps the rendered rows at five while the label keeps the true total', () => {
    // Controller ruling: a panel overflowing its grid cell is worse than one
    // that summarises, but the label must still tell the truth. Seven
    // deadlines exercises both halves of that ruling at once.
    const tasks = Array.from({ length: 7 }, (_, i) => ({
      id: `t${i}`,
      title: `Deadline ${i}`,
      deadline: `2026-09-${String(10 + i).padStart(2, '0')}`,
    }));
    mockHooks({ tasks });

    render(<PressureBlock lit={false} />);

    expect(screen.getByText(/7 deadlines/i)).toBeInTheDocument();
    expect(screen.getAllByTestId('pressure-deadline')).toHaveLength(5);
  });

  it('reads a deliberate empty state when there are no upcoming deadlines', () => {
    mockHooks({ tasks: [] });

    render(<PressureBlock lit={false} />);

    expect(screen.getByText(/no upcoming deadlines/i)).toBeInTheDocument();
    expect(screen.queryAllByTestId('pressure-deadline')).toHaveLength(0);

    // Presence in the DOM is not legibility — assert the CSS actually marks
    // this as a deliberate empty state (italic, muted), not a stray string.
    const emptyRule = HUD_CSS.match(/\.hud-pressure__empty\s*\{[^}]*\}/)?.[0] ?? '';
    expect(emptyRule).toMatch(/font-style:\s*italic/);
  });

  it('marks an overdue deadline as overdue, not as pink', () => {
    // Pink is reserved for a deadline falling TODAY — an overdue one (in the
    // past, not today) must not borrow it.
    mockHooks({
      tasks: [{ id: 't3', title: 'eActions — mort au démarrage si CSV corrompu', deadline: '2026-08-25' }],
    });

    render(<PressureBlock lit={false} />);

    const when = screen.getByTestId('deadline-when');
    expect(when).toHaveTextContent('Overdue');
    expect(when.className).not.toContain('hud-pressure__when--hot');
  });

  it('turns the capacity gauge orange once the domain marks the week overloaded', () => {
    // Per the task brief: "plus la capacité en jauge, en orange au-delà du
    // seuil" — the threshold itself is the domain's own `overload` verdict
    // (R16), not re-derived here.
    mockHooks({
      weeklyWorkload: { capacity: 10, totalPlanned: 40, totalMeetings: 5, overload: true },
    });

    render(<PressureBlock lit={false} />);

    // 45 planned hours over 40 capacity hours → 113%, gauge bar clamped at 100%.
    expect(screen.getByText('113%')).toBeInTheDocument();
    const gauge = screen.getByTestId('pressure-gauge');
    expect(gauge.className).toContain('hud-gauge--over');
    expect(gauge.querySelector('i')).toHaveStyle({ width: '100%' });

    const overRule = HUD_CSS.match(/\.hud-gauge--over[^{]*\{[^}]*\}/)?.[0] ?? '';
    expect(overRule).toMatch(/var\(--cn-orange\)/);
  });

  it('leaves a done or cancelled deadline out of the list entirely', () => {
    // The backend does not filter for us — `find_by_date_range` selects purely
    // on date range — so a finished task with today's deadline really does
    // reach this block. It is not pressure: nothing is left to do on it.
    mockHooks({
      tasks: [
        { id: 't1', title: 'Livrable signé', status: 'DONE', deadline: '2026-08-28' },
        { id: 't2', title: 'Occurrence sautée', status: 'CANCELLED', deadline: '2026-08-28' },
        { id: 't3', title: 'Revue eProject A3', status: 'BLOCKED', deadline: '2026-08-28' },
      ],
    });

    render(<PressureBlock lit={false} />);

    // Blocked still counts: stalled is not finished.
    expect(screen.getByText(/1 deadline\b/i)).toBeInTheDocument();
    const rows = screen.getAllByTestId('pressure-deadline');
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveTextContent('Revue eProject A3');
  });

  it('wears the HUD’s one glow only while it is the dominant block', () => {
    mockHooks();

    const { rerender } = render(<PressureBlock lit />);
    expect(screen.getByTestId('pressure-block').className).toContain('hud-panel--lit');

    rerender(<PressureBlock lit={false} />);
    expect(screen.getByTestId('pressure-block').className).not.toContain('hud-panel--lit');
  });

  // ─── the day: quarters, load, next break. Inherited from the Focus block
  //     this one replaced in the dominant cell — the chronometer that made
  //     Focus permanently empty is gone, everything below it was real and
  //     moved here rather than being dropped. ───

  it('marks the current quarter of the day', () => {
    // System time is 09:30 → falls inside Q1 (08–10), index 0.
    mockHooks({ quarters: makeQuarters() });

    render(<PressureBlock lit />);

    expect(screen.getByTestId('quarter-0').className).toContain('hud-quarters__segment--current');
    expect(screen.getByTestId('quarter-1').className).not.toContain('hud-quarters__segment--current');
    expect(screen.getByTestId('quarter-2').className).not.toContain('hud-quarters__segment--current');
    expect(screen.getByTestId('quarter-3').className).not.toContain('hud-quarters__segment--current');
  });

  it('fills a quarter the domain judges well-evidenced', () => {
    mockHooks({ quarters: makeQuarters(['HIGH', 'LOW', 'LOW', 'LOW']) });

    render(<PressureBlock lit />);

    expect(screen.getByTestId('quarter-0').className).toContain('hud-quarters__segment--full');
    expect(screen.getByTestId('quarter-1').className).not.toContain('hud-quarters__segment--full');
  });

  it('warns when the day load exceeds capacity', () => {
    mockHooks({
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
      workingHoursPerDay: 8,
    });

    render(<PressureBlock lit />);

    expect(screen.getByText(/over capacity/i)).toBeInTheDocument();
  });

  it('shows a countdown to the next break', () => {
    // 09:30:00 "now" → due at 09:42:00 is exactly 12 minutes out.
    mockHooks({ nextBreakDue: '2026-08-28T09:42:00' });

    render(<PressureBlock lit />);

    expect(screen.getByText('12 min')).toBeInTheDocument();
  });

  it('reads "None today" when no break is due', () => {
    // A normal outcome (e.g. an all-daily routine) — not an error, not a
    // loading state. `nextBreakDue: null` is the resolver's own contract.
    mockHooks({ nextBreakDue: null });

    render(<PressureBlock lit />);

    expect(screen.getByText('None today')).toBeInTheDocument();
  });

  it('reads "Overdue" instead of a negative countdown, and asks for a fresh value', () => {
    // Due a minute ago — the fetched instant is stale the moment it passes.
    const { refetchNextBreakDue } = mockHooks({ nextBreakDue: '2026-08-28T09:29:00' });

    render(<PressureBlock lit />);

    // Two "Overdue" now live on this panel: the break, and any deadline in
    // the past. This fixture has no deadlines, so the one found is the break.
    expect(screen.getByText('Overdue')).toBeInTheDocument();
    expect(refetchNextBreakDue).toHaveBeenCalledTimes(1);
  });

  it('stops ticking the countdown while the surface is hidden, and catches up when it returns', () => {
    setVisibility('hidden');
    mockHooks({ nextBreakDue: '2026-08-28T09:42:00' });

    render(<PressureBlock lit />);
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

  it('survives a day with no reconstructed quarters', () => {
    // `useTimesheet` returns `day: null` before the reconstruction has run —
    // the four segments still draw, none of them current.
    mockHooks({ quarters: [] });

    render(<PressureBlock lit />);

    for (const i of [0, 1, 2, 3]) {
      expect(screen.getByTestId(`quarter-${i}`).className).not.toContain('hud-quarters__segment--current');
    }
  });
});
