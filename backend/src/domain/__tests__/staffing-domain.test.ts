import { allocationToAssignmentSeeds, createWeeklyAllocation } from '@domain/staffing-domain';

describe('staffing-domain', () => {
  it('converts weekly allocation to assignment seeds', () => {
    const allocationResult = createWeeklyAllocation(
      {
        projectId: 'project-1',
        developerId: 'developer-1',
        startDate: '2024-01-01',
        endDate: '2024-01-07',
        halfDaysPerWeek: 3,
        preferredWeekdays: ['monday', 'tuesday'],
      },
      { id: 'allocation-1', createdAt: '2024-01-01' },
    );

    if (!allocationResult.ok) {
      throw new Error('Expected allocation to be valid');
    }

    const seedsResult = allocationToAssignmentSeeds(allocationResult.value);

    expect(seedsResult.ok).toBe(true);
    if (!seedsResult.ok) {
      throw new Error('Expected seed generation to succeed');
    }

    expect(seedsResult.value).toEqual([
      {
        projectId: 'project-1',
        developerId: 'developer-1',
        date: '2024-01-01',
        halfDay: 'morning',
      },
      {
        projectId: 'project-1',
        developerId: 'developer-1',
        date: '2024-01-01',
        halfDay: 'afternoon',
      },
      {
        projectId: 'project-1',
        developerId: 'developer-1',
        date: '2024-01-02',
        halfDay: 'morning',
      },
    ]);
  });

  it('rejects allocations that exceed weekly capacity', () => {
    const allocationResult = createWeeklyAllocation(
      {
        projectId: 'project-2',
        developerId: 'developer-2',
        startDate: '2024-01-01',
        endDate: '2024-01-07',
        halfDaysPerWeek: 11,
      },
      { id: 'allocation-2', createdAt: '2024-01-01' },
    );

    expect(allocationResult.ok).toBe(false);
    if (allocationResult.ok) {
      throw new Error('Expected allocation to fail');
    }
    expect(allocationResult.error.code).toBe('invalid-allocation');
  });
});
