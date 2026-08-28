import { describe, it, expect } from 'vitest';
import { isTaskOpen } from './task-status';

describe('isTaskOpen', () => {
  it('counts a task still to do as open', () => {
    expect(isTaskOpen('TODO')).toBe(true);
  });

  it('counts a task in progress as open', () => {
    expect(isTaskOpen('IN_PROGRESS')).toBe(true);
  });

  it('counts a blocked task as open — stalled is not finished', () => {
    expect(isTaskOpen('BLOCKED')).toBe(true);
  });

  it('closes a done task', () => {
    expect(isTaskOpen('DONE')).toBe(false);
  });

  it('closes a cancelled task', () => {
    expect(isTaskOpen('CANCELLED')).toBe(false);
  });
});
