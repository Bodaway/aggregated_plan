import { describe, it, expect } from 'vitest';
import { getTaskHours, HourCountableTask } from './task-hours';

function makeTask(overrides: Partial<HourCountableTask> & { status: string }): HourCountableTask {
  return {
    effectiveRemainingHours: null,
    effectiveEstimatedHours: null,
    ...overrides,
  };
}

describe('getTaskHours', () => {
  it('1. DONE task contributes 0 even when it carries an estimate', () => {
    const task = makeTask({ status: 'DONE', effectiveEstimatedHours: 8, effectiveRemainingHours: 8 });
    expect(getTaskHours(task)).toBe(0);
  });

  it('2. CANCELLED task contributes 0 even when it carries an estimate', () => {
    const task = makeTask({ status: 'CANCELLED', effectiveEstimatedHours: 5, effectiveRemainingHours: 5 });
    expect(getTaskHours(task)).toBe(0);
  });

  it('3. BLOCKED task still counts (work is not finished)', () => {
    const task = makeTask({ status: 'BLOCKED', effectiveEstimatedHours: 6 });
    expect(getTaskHours(task)).toBe(6);
  });

  it('4. IN_PROGRESS task with both remaining and estimated uses remaining', () => {
    const task = makeTask({ status: 'IN_PROGRESS', effectiveRemainingHours: 3, effectiveEstimatedHours: 8 });
    expect(getTaskHours(task)).toBe(3);
  });

  it('5. TODO task with only estimated uses estimated', () => {
    const task = makeTask({ status: 'TODO', effectiveEstimatedHours: 4 });
    expect(getTaskHours(task)).toBe(4);
  });

  it('6. TODO task with neither remaining nor estimated contributes 0', () => {
    const task = makeTask({ status: 'TODO' });
    expect(getTaskHours(task)).toBe(0);
  });
});
