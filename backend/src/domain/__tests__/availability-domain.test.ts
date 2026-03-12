import { createAvailability } from '@domain/availability-domain';

describe('availability-domain', () => {
  it('creates availability for a valid range', () => {
    const result = createAvailability(
      {
        developerId: 'developer-1',
        startDate: '2024-01-10',
        endDate: '2024-01-12',
        type: 'leave',
        description: 'Vacation',
      },
      { id: 'availability-1', createdAt: '2024-01-01' },
    );

    expect(result.ok).toBe(true);
    if (!result.ok) {
      throw new Error('Expected availability to be created');
    }
    expect(result.value.type).toBe('leave');
  });

  it('rejects invalid availability ranges', () => {
    const result = createAvailability(
      {
        developerId: 'developer-1',
        startDate: '2024-02-10',
        endDate: '2024-02-01',
        type: 'training',
      },
      { id: 'availability-2', createdAt: '2024-01-01' },
    );

    expect(result.ok).toBe(false);
    if (result.ok) {
      throw new Error('Expected availability creation to fail');
    }
    expect(result.error.code).toBe('invalid-date-range');
  });
});
