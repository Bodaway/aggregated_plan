import {
  addDays,
  compareIsoDates,
  getWeekStart,
  isIsoDateString,
  listIsoDatesInRange,
} from '../date-utils';

describe('date-utils', () => {
  it('validates ISO dates', () => {
    expect(isIsoDateString('2024-02-29')).toBe(true);
    expect(isIsoDateString('2024-02-30')).toBe(false);
    expect(isIsoDateString('2024-2-1')).toBe(false);
    expect(isIsoDateString('invalid')).toBe(false);
  });

  it('compares dates consistently', () => {
    expect(compareIsoDates('2024-01-01', '2024-01-01')).toBe(0);
    expect(compareIsoDates('2024-01-01', '2024-01-02')).toBe(-1);
    expect(compareIsoDates('2024-01-02', '2024-01-01')).toBe(1);
  });

  it('adds days and lists ranges', () => {
    expect(addDays('2024-01-01', 2)).toBe('2024-01-03');
    expect(addDays('2024-01-01', -1)).toBe('2023-12-31');
    expect(listIsoDatesInRange('2024-01-01', '2024-01-03')).toEqual([
      '2024-01-01',
      '2024-01-02',
      '2024-01-03',
    ]);
  });

  it('calculates week start on Monday', () => {
    expect(getWeekStart('2024-01-01')).toBe('2024-01-01');
    expect(getWeekStart('2024-01-07')).toBe('2024-01-01');
    expect(getWeekStart('2024-01-08')).toBe('2024-01-08');
  });
});
