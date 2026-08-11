import { describe, it, expect, vi } from 'vitest';
import { renderHook } from '@testing-library/react';

const reexecute = vi.fn();
vi.mock('urql', () => ({
  useQuery: () => [{ fetching: false, data: { timesheetDraft: null }, error: undefined }, reexecute],
  useMutation: () => [{ fetching: false }, vi.fn().mockResolvedValue({ error: undefined })],
}));

import { useTimesheet } from './use-timesheet';

describe('useTimesheet', () => {
  it('exposes the timesheet actions and a null day when no draft', () => {
    const { result } = renderHook(() => useTimesheet(new Date('2026-06-08T00:00:00Z')));
    expect(result.current.day).toBeNull();
    expect(typeof result.current.reconstruct).toBe('function');
    expect(typeof result.current.setShare).toBe('function');
    expect(typeof result.current.clearShare).toBe('function');
    expect(typeof result.current.resetQuarter).toBe('function');
    expect(typeof result.current.validate).toBe('function');
    expect(typeof result.current.markOff).toBe('function');
  });
});
