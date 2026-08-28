import { describe, it, expect } from 'vitest';
import { computeCapacity, hasOpenDeadlineOn, openDeadlines } from './pressure-rules';

const task = (over: Partial<Parameters<typeof openDeadlines>[0][number]> = {}) => ({
  id: 't1',
  title: 'Revue eProject A3',
  status: 'TODO',
  deadline: '2026-08-28',
  ...over,
});

describe('openDeadlines', () => {
  it('keeps only tasks that actually carry a deadline', () => {
    const kept = openDeadlines([task({ id: 'a' }), task({ id: 'b', deadline: null })]);
    expect(kept.map((d) => d.id)).toEqual(['a']);
  });

  it('sorts by proximity, earliest first, overdue included', () => {
    const kept = openDeadlines([
      task({ id: 'later', deadline: '2026-08-30' }),
      task({ id: 'overdue', deadline: '2026-08-25' }),
      task({ id: 'today', deadline: '2026-08-28' }),
    ]);
    expect(kept.map((d) => d.id)).toEqual(['overdue', 'today', 'later']);
  });

  it('drops a done or cancelled task — a closed deadline is not pressure', () => {
    const kept = openDeadlines([
      task({ id: 'done', status: 'DONE' }),
      task({ id: 'cancelled', status: 'CANCELLED' }),
      task({ id: 'blocked', status: 'BLOCKED' }),
    ]);
    expect(kept.map((d) => d.id)).toEqual(['blocked']);
  });
});

describe('hasOpenDeadlineOn', () => {
  it('sees an open deadline falling on the given day', () => {
    expect(hasOpenDeadlineOn([task({ deadline: '2026-08-28' })], '2026-08-28')).toBe(true);
  });

  it('ignores a deadline on another day, overdue ones included', () => {
    expect(hasOpenDeadlineOn([task({ deadline: '2026-08-25' })], '2026-08-28')).toBe(false);
    expect(hasOpenDeadlineOn([task({ deadline: '2026-08-30' })], '2026-08-28')).toBe(false);
  });

  it('ignores a deadline that is already closed', () => {
    expect(hasOpenDeadlineOn([task({ status: 'DONE' })], '2026-08-28')).toBe(false);
  });

  it('is false on an empty day', () => {
    expect(hasOpenDeadlineOn([], '2026-08-28')).toBe(false);
  });
});

describe('computeCapacity', () => {
  it('turns half-day capacity into hours and reports the percentage', () => {
    // 10 half-days * 4h = 40h capacity; 30 + 6 = 36 planned hours → 90%.
    expect(computeCapacity({ capacity: 10, totalPlanned: 30, totalMeetings: 6, overload: false })).toEqual({
      pct: 90,
      overloaded: false,
    });
  });

  it('reports no load at all when there is no workload yet', () => {
    expect(computeCapacity(null)).toEqual({ pct: 0, overloaded: false });
  });

  it('does not divide by a zero capacity', () => {
    expect(computeCapacity({ capacity: 0, totalPlanned: 12, totalMeetings: 0, overload: true })).toEqual({
      pct: 0,
      overloaded: true,
    });
  });

  it('takes the domain overload verdict, which the rounded percentage can contradict', () => {
    // 40.1h over 40h: the domain says overloaded (R16 is a strict >), while the
    // percentage rounds down to a reassuring 100. The verdict wins.
    expect(computeCapacity({ capacity: 10, totalPlanned: 40.1, totalMeetings: 0, overload: true })).toEqual({
      pct: 100,
      overloaded: true,
    });
    // 39.9h over 40h rounds *up* to 100 while the domain says it is fine.
    expect(computeCapacity({ capacity: 10, totalPlanned: 39.9, totalMeetings: 0, overload: false })).toEqual({
      pct: 100,
      overloaded: false,
    });
  });
});
