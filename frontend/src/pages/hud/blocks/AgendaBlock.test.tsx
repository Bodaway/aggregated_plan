import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { render, screen, cleanup } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Same source-level technique FocusBlock.test.tsx and PressureBlock.test.tsx
// use: jsdom never applies this stylesheet, so an empty state's legibility
// has to be checked against the rule itself, not the computed style.
const HUD_CSS = readFileSync(resolve(__dirname, '../hud.css'), 'utf8');

const dashboardMock = vi.fn();
vi.mock('@/hooks/use-dashboard', () => ({ useDashboard: (...args: unknown[]) => dashboardMock(...args) }));

import { AgendaBlock } from './AgendaBlock';

interface MockMeeting {
  readonly id: string;
  readonly title: string;
  readonly startTime: string;
  readonly endTime: string;
  readonly durationHours: number;
  readonly showAs?: string | null;
}

function mockDashboard(meetings: MockMeeting[] = []) {
  dashboardMock.mockReturnValue({ data: { meetings } });
}

describe('AgendaBlock', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-08-28T09:30:00'));
    dashboardMock.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it('counts down to the next real meeting and lays out the day timeline', () => {
    mockDashboard([
      // Excluded from both the countdown and the timeline: a lunch placeholder.
      {
        id: 'lunch',
        title: 'Pause midi',
        startTime: '2026-08-28T12:00:00',
        endTime: '2026-08-28T13:00:00',
        durationHours: 1,
      },
      // Excluded: a different day, still in the week's data.
      {
        id: 'yesterday',
        title: 'Old standup',
        startTime: '2026-08-27T10:00:00',
        endTime: '2026-08-27T10:30:00',
        durationHours: 0.5,
      },
      {
        id: 'm2',
        title: 'Point hebdo SAFT',
        startTime: '2026-08-28T10:00:00',
        endTime: '2026-08-28T11:00:00',
        durationHours: 1,
      },
      {
        id: 'm3',
        title: 'Revue technique',
        startTime: '2026-08-28T14:00:00',
        endTime: '2026-08-28T15:00:00',
        durationHours: 1,
      },
    ]);

    render(<AgendaBlock />);

    expect(screen.getByTestId('agenda-block')).toBeInTheDocument();
    expect(screen.getByText(/agenda/i)).toBeInTheDocument();

    // 09:30 → 10:00 is exactly 30 minutes out, and it's the earliest real
    // meeting left today.
    expect(screen.getByText('30 min')).toBeInTheDocument();
    expect(screen.getByText('Point hebdo SAFT')).toBeInTheDocument();

    // Only the two real, today meetings become segments.
    const segments = screen.getAllByTestId('agenda-segment');
    expect(segments).toHaveLength(2);
    expect(segments[0]).toHaveStyle({ left: '22.22%', width: '11.11%' });
    expect(segments[1]).toHaveStyle({ left: '66.67%', width: '11.11%' });

    expect(screen.getByTestId('agenda-now-marker')).toBeInTheDocument();
    expect(screen.getByText('08:00')).toBeInTheDocument();
    expect(screen.getByText('17:00')).toBeInTheDocument();

    const summary = screen.getByTestId('agenda-summary');
    expect(summary).toHaveTextContent('2');
    expect(summary).toHaveTextContent('2h 0min');
  });

  it('reads a deliberate empty state when no meetings are scheduled today at all', () => {
    mockDashboard([
      {
        id: 'yesterday',
        title: 'Old standup',
        startTime: '2026-08-27T10:00:00',
        endTime: '2026-08-27T10:30:00',
        durationHours: 0.5,
      },
    ]);

    render(<AgendaBlock />);

    expect(screen.getByText(/no meetings today/i)).toBeInTheDocument();
    expect(screen.queryAllByTestId('agenda-segment')).toHaveLength(0);
    // The timeline itself, and the now marker, are not "the meeting list" —
    // they stay put even with a bare day.
    expect(screen.getByTestId('agenda-now-marker')).toBeInTheDocument();

    const emptyRule = HUD_CSS.match(/\.hud-agenda__empty\s*\{[^}]*\}/)?.[0] ?? '';
    expect(emptyRule).toMatch(/font-style:\s*italic/);
  });

  it('distinguishes "no meetings left" from "no meetings at all"', () => {
    // Two real meetings today, both already over by 09:30 — the day was not
    // empty, it's just done.
    mockDashboard([
      {
        id: 'standup',
        title: 'Standup',
        startTime: '2026-08-28T08:00:00',
        endTime: '2026-08-28T08:15:00',
        durationHours: 0.25,
      },
      {
        id: 'budget',
        title: 'Revue budget',
        startTime: '2026-08-28T08:30:00',
        endTime: '2026-08-28T09:00:00',
        durationHours: 0.5,
      },
    ]);

    render(<AgendaBlock />);

    expect(screen.getByText(/no more meetings today/i)).toBeInTheDocument();
    // Distinct wording from the true-empty case above — same information
    // ("nothing next") but a different day.
    expect(screen.queryByText(/no meetings today/i)).not.toBeInTheDocument();

    // The past meetings are still real history on the timeline.
    expect(screen.getAllByTestId('agenda-segment')).toHaveLength(2);
  });

  it('reads "Now" instead of a negative countdown for a meeting already in progress', () => {
    mockDashboard([
      {
        id: 'standup',
        title: 'Point quotidien',
        startTime: '2026-08-28T09:00:00',
        endTime: '2026-08-28T10:00:00',
        durationHours: 1,
      },
    ]);

    render(<AgendaBlock />);

    expect(screen.getByText('Now')).toBeInTheDocument();
    expect(screen.getByText('Point quotidien')).toBeInTheDocument();
  });
});
