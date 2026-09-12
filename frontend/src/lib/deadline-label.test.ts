import { describe, it, expect } from 'vitest';
import { formatDeadlineLabel } from './deadline-label';

const TODAY = '2026-09-11';

describe('formatDeadlineLabel', () => {
  it('names today, tomorrow and the days beyond', () => {
    expect(formatDeadlineLabel('2026-09-11', TODAY)).toBe('Today');
    expect(formatDeadlineLabel('2026-09-12', TODAY)).toBe('Tomorrow');
    expect(formatDeadlineLabel('2026-09-14', TODAY)).toBe('In 3d');
  });

  it('says overdue for anything in the past, however far', () => {
    expect(formatDeadlineLabel('2026-09-10', TODAY)).toBe('Overdue');
    expect(formatDeadlineLabel('2025-01-01', TODAY)).toBe('Overdue');
  });

  it('counts calendar days, not elapsed hours', () => {
    // A date-only comparison: both sides are bare YYYY-MM-DD, so a deadline
    // "tomorrow" is tomorrow whether it is 08:00 or 23:59 today.
    expect(formatDeadlineLabel('2026-09-12', '2026-09-11')).toBe('Tomorrow');
  });

  it('crosses a month boundary without wrapping', () => {
    expect(formatDeadlineLabel('2026-10-02', '2026-09-30')).toBe('In 2d');
  });
});
