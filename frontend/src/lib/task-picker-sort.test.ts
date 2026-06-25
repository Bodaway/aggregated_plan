import { describe, it, expect } from 'vitest';
import { sortTasksForPicker, SortablePickerTask } from './task-picker-sort';

const TODAY = '2026-06-16';

function makeTask(overrides: Partial<SortablePickerTask> & { id: string }): SortablePickerTask {
  return {
    title: overrides.id,
    plannedStart: null,
    deadline: null,
    urgency: 2,
    impact: 2,
    ...overrides,
  };
}

describe('sortTasksForPicker', () => {
  it('1. task with plannedStart == today sorts above a task with no today date', () => {
    const planned = makeTask({ id: 'planned', plannedStart: TODAY });
    const other = makeTask({ id: 'other', plannedStart: '2026-06-20' });
    const result = sortTasksForPicker([other, planned], TODAY);
    expect(result[0].id).toBe('planned');
    expect(result[1].id).toBe('other');
  });

  it('2. task with deadline == today lands in the today group (before non-today tasks)', () => {
    const deadlineToday = makeTask({ id: 'dl', deadline: TODAY });
    const other = makeTask({ id: 'other' });
    const result = sortTasksForPicker([other, deadlineToday], TODAY);
    expect(result[0].id).toBe('dl');
  });

  it('3. within today group, higher urgency wins; equal urgency → higher impact wins', () => {
    const lowUrgency = makeTask({ id: 'low-u', plannedStart: TODAY, urgency: 1, impact: 4 });
    const highUrgency = makeTask({ id: 'high-u', plannedStart: TODAY, urgency: 3, impact: 1 });
    const sameUrgencyHighImpact = makeTask({ id: 'same-u-hi', plannedStart: TODAY, urgency: 3, impact: 3 });
    const result = sortTasksForPicker([lowUrgency, highUrgency, sameUrgencyHighImpact], TODAY);
    expect(result[0].id).toBe('same-u-hi'); // urgency 3, impact 3
    expect(result[1].id).toBe('high-u');    // urgency 3, impact 1
    expect(result[2].id).toBe('low-u');     // urgency 1
  });

  it('4. within rest group, same urgency DESC → impact DESC tiebreak', () => {
    const a = makeTask({ id: 'a', urgency: 2, impact: 1 });
    const b = makeTask({ id: 'b', urgency: 3, impact: 1 });
    const c = makeTask({ id: 'c', urgency: 3, impact: 4 });
    const result = sortTasksForPicker([a, b, c], TODAY);
    expect(result[0].id).toBe('c'); // urgency 3, impact 4
    expect(result[1].id).toBe('b'); // urgency 3, impact 1
    expect(result[2].id).toBe('a'); // urgency 2
  });

  it('5. length is preserved — nothing is filtered out', () => {
    const tasks = [
      makeTask({ id: 't1', plannedStart: TODAY }),
      makeTask({ id: 't2', deadline: TODAY }),
      makeTask({ id: 't3' }),
      makeTask({ id: 't4', plannedStart: '2026-06-01' }),
    ];
    const result = sortTasksForPicker(tasks, TODAY);
    expect(result).toHaveLength(tasks.length);
  });

  it('6. input array is not mutated', () => {
    const tasks = [
      makeTask({ id: 'x', plannedStart: TODAY, urgency: 1 }),
      makeTask({ id: 'y', plannedStart: TODAY, urgency: 4 }),
    ];
    const originalOrder = tasks.map((t) => t.id);
    sortTasksForPicker(tasks, TODAY);
    expect(tasks.map((t) => t.id)).toEqual(originalOrder);
  });

  it('7. plannedStart with a time component still matches by calendar day', () => {
    const withTime = makeTask({ id: 'with-time', plannedStart: `${TODAY}T08:00:00.000Z` });
    const other = makeTask({ id: 'other' });
    const result = sortTasksForPicker([other, withTime], TODAY);
    expect(result[0].id).toBe('with-time');
  });
});
