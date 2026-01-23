import type { Assignment, Availability } from '@aggregated-plan/shared-types';
import { detectConflicts } from '@domain/conflict-domain';

describe('conflict-domain', () => {
  it('detects overlapping assignments', () => {
    const assignments: Assignment[] = [
      {
        id: 'assignment-1',
        projectId: 'project-1',
        developerId: 'developer-1',
        date: '2024-01-01',
        halfDay: 'morning',
        createdAt: '2024-01-01',
      },
      {
        id: 'assignment-2',
        projectId: 'project-2',
        developerId: 'developer-1',
        date: '2024-01-01',
        halfDay: 'morning',
        createdAt: '2024-01-01',
      },
    ];

    const conflicts = detectConflicts({
      assignments,
      availabilities: [],
      capacities: [{ developerId: 'developer-1', capacityHalfDaysPerWeek: 10 }],
    });

    expect(conflicts.some((conflict) => conflict.type === 'overlap')).toBe(true);
  });

  it('detects capacity conflicts', () => {
    const assignments: Assignment[] = [
      {
        id: 'assignment-1',
        projectId: 'project-1',
        developerId: 'developer-2',
        date: '2024-01-01',
        halfDay: 'morning',
        createdAt: '2024-01-01',
      },
      {
        id: 'assignment-2',
        projectId: 'project-1',
        developerId: 'developer-2',
        date: '2024-01-01',
        halfDay: 'afternoon',
        createdAt: '2024-01-01',
      },
      {
        id: 'assignment-3',
        projectId: 'project-1',
        developerId: 'developer-2',
        date: '2024-01-02',
        halfDay: 'morning',
        createdAt: '2024-01-01',
      },
    ];

    const conflicts = detectConflicts({
      assignments,
      availabilities: [],
      capacities: [{ developerId: 'developer-2', capacityHalfDaysPerWeek: 2 }],
    });

    expect(conflicts.some((conflict) => conflict.type === 'capacity')).toBe(true);
  });

  it('detects conflicts with unavailability', () => {
    const assignments: Assignment[] = [
      {
        id: 'assignment-1',
        projectId: 'project-1',
        developerId: 'developer-3',
        date: '2024-01-03',
        halfDay: 'morning',
        createdAt: '2024-01-01',
      },
    ];

    const availabilities: Availability[] = [
      {
        id: 'availability-1',
        developerId: 'developer-3',
        startDate: '2024-01-03',
        endDate: '2024-01-03',
        type: 'leave',
        createdAt: '2024-01-01',
      },
    ];

    const conflicts = detectConflicts({
      assignments,
      availabilities,
      capacities: [{ developerId: 'developer-3', capacityHalfDaysPerWeek: 10 }],
    });

    expect(conflicts.some((conflict) => conflict.type === 'unavailability')).toBe(true);
  });
});
