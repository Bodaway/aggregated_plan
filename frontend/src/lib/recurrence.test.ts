import { describe, it, expect } from 'vitest';
import {
  weekdayBitmask,
  bitmaskToWeekdays,
  weekdayToGql,
  weekdayFromGql,
  weekOfMonthToGql,
  weekOfMonthFromGql,
  ruleToGqlInput,
  ruleFromGql,
} from './recurrence';
import type { Weekday, WeekOfMonth, RecurrenceRule } from './recurrence';

describe('weekdayBitmask', () => {
  it('returns 0 for empty array', () => {
    expect(weekdayBitmask([])).toBe(0);
  });

  it('Monday alone = bit 0 = 1', () => {
    expect(weekdayBitmask(['monday'])).toBe(1);
  });

  it('Tuesday alone = bit 1 = 2', () => {
    expect(weekdayBitmask(['tuesday'])).toBe(2);
  });

  it('Sunday alone = bit 6 = 64', () => {
    expect(weekdayBitmask(['sunday'])).toBe(64);
  });

  it('Monday + Friday = 1 | 16 = 17', () => {
    expect(weekdayBitmask(['monday', 'friday'])).toBe(17);
  });

  it('Mon-Fri workweek = 1|2|4|8|16 = 31', () => {
    expect(weekdayBitmask(['monday', 'tuesday', 'wednesday', 'thursday', 'friday'])).toBe(31);
  });

  it('all days = 127', () => {
    const all: Weekday[] = ['monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday', 'sunday'];
    expect(weekdayBitmask(all)).toBe(127);
  });
});

describe('bitmaskToWeekdays', () => {
  it('returns empty array for 0', () => {
    expect(bitmaskToWeekdays(0)).toEqual([]);
  });

  it('bit 0 → monday', () => {
    expect(bitmaskToWeekdays(1)).toEqual(['monday']);
  });

  it('bit 6 → sunday', () => {
    expect(bitmaskToWeekdays(64)).toEqual(['sunday']);
  });

  it('17 → monday + friday', () => {
    expect(bitmaskToWeekdays(17)).toEqual(['monday', 'friday']);
  });

  it('31 → Mon-Fri in order', () => {
    expect(bitmaskToWeekdays(31)).toEqual(['monday', 'tuesday', 'wednesday', 'thursday', 'friday']);
  });

  it('127 → all 7 days in order', () => {
    expect(bitmaskToWeekdays(127)).toEqual([
      'monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday', 'sunday',
    ]);
  });
});

// ── GQL wire-format helpers ───────────────────────────────────────────────────

describe('weekdayToGql / weekdayFromGql round-trip', () => {
  const days: Weekday[] = ['monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday', 'sunday'];
  days.forEach(d => {
    it(`round-trips ${d}`, () => {
      expect(weekdayFromGql(weekdayToGql(d))).toBe(d);
    });
  });
});

describe('weekOfMonthToGql / weekOfMonthFromGql round-trip', () => {
  const weeks: WeekOfMonth[] = ['first', 'second', 'third', 'fourth', 'last'];
  weeks.forEach(w => {
    it(`round-trips ${w}`, () => {
      expect(weekOfMonthFromGql(weekOfMonthToGql(w))).toBe(w);
    });
  });
});

describe('ruleToGqlInput', () => {
  it('daily: produces correct shape', () => {
    const out = ruleToGqlInput({ kind: 'daily', interval: 1 });
    expect(out).toEqual({ kind: 'DAILY', interval: 1 });
    expect((out as Record<string, unknown>).weekdays).toBeUndefined();
    expect((out as Record<string, unknown>).dayOfMonth).toBeUndefined();
    expect((out as Record<string, unknown>).week).toBeUndefined();
    expect((out as Record<string, unknown>).weekday).toBeUndefined();
  });

  it('weekly: produces correct shape with uppercased weekdays', () => {
    const out = ruleToGqlInput({ kind: 'weekly', interval: 2, weekdays: ['monday', 'friday'] });
    expect(out).toEqual({ kind: 'WEEKLY', interval: 2, weekdays: ['MONDAY', 'FRIDAY'] });
  });

  it('monthly_by_day: produces dayOfMonth, no week/weekday', () => {
    const out = ruleToGqlInput({ kind: 'monthly_by_day', interval: 1, day: 15 });
    expect(out).toEqual({ kind: 'MONTHLY_BY_DAY', interval: 1, dayOfMonth: 15 });
  });

  it('monthly_by_weekday: produces week and weekday uppercased', () => {
    const out = ruleToGqlInput({ kind: 'monthly_by_weekday', interval: 1, week: 'first', weekday: 'tuesday' });
    expect(out).toEqual({ kind: 'MONTHLY_BY_WEEKDAY', interval: 1, week: 'FIRST', weekday: 'TUESDAY' });
  });
});

describe('ruleFromGql', () => {
  it('parses DAILY back to lowercase frontend shape', () => {
    expect(ruleFromGql({ kind: 'DAILY', interval: 1 })).toEqual({ kind: 'daily', interval: 1 });
  });

  it('parses WEEKLY back with lowercased weekdays', () => {
    expect(ruleFromGql({ kind: 'WEEKLY', interval: 2, weekdays: ['MONDAY', 'FRIDAY'] }))
      .toEqual({ kind: 'weekly', interval: 2, weekdays: ['monday', 'friday'] });
  });

  it('parses MONTHLY_BY_DAY back with day field', () => {
    expect(ruleFromGql({ kind: 'MONTHLY_BY_DAY', interval: 1, dayOfMonth: 15 }))
      .toEqual({ kind: 'monthly_by_day', interval: 1, day: 15 });
  });

  it('parses MONTHLY_BY_WEEKDAY back with lowercase week and weekday', () => {
    expect(ruleFromGql({ kind: 'MONTHLY_BY_WEEKDAY', interval: 1, week: 'FIRST', weekday: 'TUESDAY' }))
      .toEqual({ kind: 'monthly_by_weekday', interval: 1, week: 'first', weekday: 'tuesday' });
  });

  it('unknown kind falls back to daily with original interval', () => {
    const result = ruleFromGql({ kind: 'UNKNOWN_KIND', interval: 3 });
    expect(result).toEqual({ kind: 'daily', interval: 3 });
  });
});

describe('ruleToGqlInput / ruleFromGql round-trips', () => {
  const rules: RecurrenceRule[] = [
    { kind: 'daily', interval: 1 },
    { kind: 'weekly', interval: 2, weekdays: ['monday', 'friday'] },
    { kind: 'monthly_by_day', interval: 1, day: 15 },
    { kind: 'monthly_by_weekday', interval: 1, week: 'first', weekday: 'tuesday' },
  ];

  rules.forEach(rule => {
    it(`round-trips ${rule.kind}`, () => {
      expect(ruleFromGql(ruleToGqlInput(rule))).toEqual(rule);
    });
  });
});

describe('round-trip bitmask ↔ weekdays', () => {
  const cases: Weekday[][] = [
    [],
    ['monday'],
    ['saturday', 'sunday'],
    ['monday', 'wednesday', 'friday'],
    ['monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday', 'sunday'],
  ];

  cases.forEach(days => {
    it(`round-trips [${days.join(', ')}]`, () => {
      const mask = weekdayBitmask(days);
      const result = bitmaskToWeekdays(mask);
      // bitmaskToWeekdays returns in canonical order; sort input for comparison
      const sorted = [...days].sort((a, b) => {
        const ORDER = ['monday','tuesday','wednesday','thursday','friday','saturday','sunday'];
        return ORDER.indexOf(a) - ORDER.indexOf(b);
      });
      expect(result).toEqual(sorted);
    });
  });
});
