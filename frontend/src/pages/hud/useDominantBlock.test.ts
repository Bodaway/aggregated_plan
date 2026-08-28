import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';

const dashboardMock = vi.fn();
vi.mock('@/hooks/use-dashboard', () => ({ useDashboard: (...args: unknown[]) => dashboardMock(...args) }));

import { useDominantBlock } from './useDominantBlock';

interface MockTask {
  readonly id: string;
  readonly title: string;
  readonly status: string;
  readonly deadline: string | null;
}

interface MockMeeting {
  readonly id: string;
  readonly title: string;
  readonly startTime: string;
  readonly endTime: string;
  readonly showAs: string | null;
}

const CALM_WEEK = { capacity: 10, totalPlanned: 20, totalMeetings: 0, overload: false };

function mockDashboard({
  tasks = [] as MockTask[],
  meetings = [] as MockMeeting[],
  weeklyWorkload = CALM_WEEK,
} = {}) {
  dashboardMock.mockReturnValue({ data: { tasks, meetings, weeklyWorkload } });
}

const meeting = (over: Partial<MockMeeting> = {}): MockMeeting => ({
  id: 'm1',
  title: 'Point hebdo SAFT',
  startTime: '2026-08-28T10:00:00',
  endTime: '2026-08-28T11:00:00',
  showAs: 'busy',
  ...over,
});

const task = (over: Partial<MockTask> = {}): MockTask => ({
  id: 't1',
  title: 'Revue eProject A3',
  status: 'TODO',
  deadline: '2026-08-28',
  ...over,
});

function setVisibility(state: DocumentVisibilityState) {
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => state });
  document.dispatchEvent(new Event('visibilitychange'));
}

function at(clock: string) {
  vi.setSystemTime(new Date(clock));
}

const dominant = () => renderHook(() => useDominantBlock()).result;

describe('useDominantBlock', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    at('2026-08-28T09:30:00');
    dashboardMock.mockReset();
    mockDashboard();
  });

  afterEach(() => {
    setVisibility('visible');
    vi.useRealTimers();
  });

  it('leaves the glow on Focus when nothing is pressing', () => {
    expect(dominant().current).toBe('focus');
  });

  it('leaves the glow on Focus before any data has arrived', () => {
    dashboardMock.mockReturnValue({ data: null });
    expect(dominant().current).toBe('focus');
  });

  // ─── rule 1: an imminent meeting ───

  it('hands the glow to Agenda when a real meeting starts in under ten minutes', () => {
    at('2026-08-28T09:51:00');
    mockDashboard({ meetings: [meeting()] });
    expect(dominant().current).toBe('agenda');
  });

  it('holds Focus at exactly ten minutes out — the window is strictly under ten', () => {
    at('2026-08-28T09:50:00');
    mockDashboard({ meetings: [meeting()] });
    expect(dominant().current).toBe('focus');
  });

  it('still lights Agenda at the start instant itself', () => {
    at('2026-08-28T10:00:00');
    mockDashboard({ meetings: [meeting()] });
    expect(dominant().current).toBe('agenda');
  });

  it('releases the glow once the meeting is under way — the missable moment has passed', () => {
    at('2026-08-28T10:05:00');
    mockDashboard({ meetings: [meeting()] });
    expect(dominant().current).toBe('focus');
  });

  it('ignores a placeholder the Agenda block itself refuses to show', () => {
    at('2026-08-28T11:55:00');
    mockDashboard({
      meetings: [
        meeting({ id: 'lunch', title: 'Pause midi', startTime: '2026-08-28T12:00:00', endTime: '2026-08-28T13:00:00' }),
        meeting({ id: 'ooo', title: 'Congés', startTime: '2026-08-28T12:00:00', endTime: '2026-08-28T13:00:00', showAs: 'free' }),
      ],
    });
    expect(dominant().current).toBe('focus');
  });

  it('ignores a meeting that belongs to another day, as the timeline does', () => {
    // Five minutes out on the wall clock, but tomorrow's — the Agenda block
    // draws today only, so the glow must not announce it.
    at('2026-08-28T23:55:00');
    mockDashboard({ meetings: [meeting({ startTime: '2026-08-29T00:00:00', endTime: '2026-08-29T01:00:00' })] });
    expect(dominant().current).toBe('focus');
  });

  // ─── rule 2: capacity, and a deadline still open in the afternoon ───

  it('hands the glow to Pressure when the domain marks the week overloaded', () => {
    mockDashboard({ weeklyWorkload: { capacity: 10, totalPlanned: 40, totalMeetings: 5, overload: true } });
    expect(dominant().current).toBe('pressure');
  });

  it('trusts the domain verdict over the rounded gauge', () => {
    // 39.9h of a 40h week rounds up to a gauge reading 100%, but R16 says the
    // week is fine. The glow must follow the verdict, not the rounding.
    mockDashboard({ weeklyWorkload: { capacity: 10, totalPlanned: 39.9, totalMeetings: 0, overload: false } });
    expect(dominant().current).toBe('focus');
  });

  it('hands the glow to Pressure at exactly 15:00 with a deadline still open today', () => {
    at('2026-08-28T15:00:00');
    mockDashboard({ tasks: [task()] });
    expect(dominant().current).toBe('pressure');
  });

  it('holds Focus one minute before the threshold', () => {
    at('2026-08-28T14:59:00');
    mockDashboard({ tasks: [task()] });
    expect(dominant().current).toBe('focus');
  });

  it('holds Focus in the afternoon when today’s deadline is already closed', () => {
    at('2026-08-28T16:00:00');
    mockDashboard({ tasks: [task({ status: 'DONE' }), task({ id: 't2', status: 'CANCELLED' })] });
    expect(dominant().current).toBe('focus');
  });

  it('holds Focus in the afternoon for a deadline that is not today’s', () => {
    at('2026-08-28T16:00:00');
    mockDashboard({ tasks: [task({ deadline: '2026-08-30' }), task({ id: 't2', deadline: '2026-08-25' })] });
    expect(dominant().current).toBe('focus');
  });

  // ─── the order itself ───

  it('gives Agenda precedence over Pressure when both would fire', () => {
    at('2026-08-28T15:55:00');
    mockDashboard({
      tasks: [task()],
      meetings: [meeting({ startTime: '2026-08-28T16:00:00', endTime: '2026-08-28T17:00:00' })],
      weeklyWorkload: { capacity: 10, totalPlanned: 40, totalMeetings: 5, overload: true },
    });
    expect(dominant().current).toBe('agenda');
  });

  // ─── it re-arbitrates on the clock, and only while looked at ───

  it('re-arbitrates as the clock crosses the threshold, with no new data', () => {
    at('2026-08-28T14:59:50');
    mockDashboard({ tasks: [task()] });
    const result = dominant();
    expect(result.current).toBe('focus');

    act(() => void vi.advanceTimersByTime(20_000));
    expect(result.current).toBe('pressure');
  });

  it('stops re-arbitrating while the surface is hidden, and catches up on return', () => {
    at('2026-08-28T14:59:50');
    mockDashboard({ tasks: [task()] });
    setVisibility('hidden');
    const result = dominant();
    expect(result.current).toBe('focus');

    act(() => void vi.advanceTimersByTime(60_000));
    expect(result.current).toBe('focus');

    act(() => setVisibility('visible'));
    expect(result.current).toBe('pressure');
  });
});
