import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useTaskSearch, MIN_SEARCH_LENGTH } from './use-task-search';

// ── urql mock ───────────────────────────────────────────────────────────────
// The hook calls urql's useQuery({ query, variables, pause }) and reads the
// first element of the returned tuple ({ data, fetching }). We capture the
// args of the most recent call so tests can assert pause/variables, and we
// return a controllable result.

interface CapturedArgs {
  readonly query: string;
  readonly variables?: { readonly term?: string };
  readonly pause?: boolean;
}

let lastUseQueryArgs: CapturedArgs | null = null;
let mockResult: { data?: unknown; fetching: boolean };

const useQuerySpy = vi.fn((args: CapturedArgs) => {
  lastUseQueryArgs = args;
  // urql returns [result, reexecute]
  return [mockResult, vi.fn()] as const;
});

vi.mock('urql', () => ({
  useQuery: (args: CapturedArgs) => useQuerySpy(args),
}));

// A node shape as returned by GraphQL (urgency/impact may be enum strings).
function node(id: string, title: string, extras: Partial<Record<string, unknown>> = {}) {
  return {
    id,
    title,
    plannedStart: null,
    deadline: null,
    urgency: 'MEDIUM',
    impact: 'HIGH',
    ...extras,
  };
}

function dataWith(...nodes: ReturnType<typeof node>[]) {
  return { tasks: { edges: nodes.map(n => ({ node: n })) } };
}

beforeEach(() => {
  vi.useFakeTimers();
  lastUseQueryArgs = null;
  mockResult = { data: undefined, fetching: false };
  useQuerySpy.mockClear();
});

afterEach(() => {
  vi.runOnlyPendingTimers();
  vi.useRealTimers();
});

describe('useTaskSearch — inactive states', () => {
  it('is inactive for a term shorter than MIN_SEARCH_LENGTH', () => {
    const { result } = renderHook(() => useTaskSearch('a'));
    // Even after the debounce window, a 1-char term must not activate.
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(result.current.active).toBe(false);
    expect(result.current.results).toEqual([]);
  });

  it('is inactive before the debounce window elapses', () => {
    const { result } = renderHook(() => useTaskSearch('auth'));
    // Debounced term has not been committed yet.
    expect(result.current.active).toBe(false);
    expect(result.current.results).toEqual([]);
  });

  it('pauses the urql query while inactive', () => {
    renderHook(() => useTaskSearch('a'));
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(lastUseQueryArgs?.pause).toBe(true);
  });

  it('exposes MIN_SEARCH_LENGTH of 2', () => {
    expect(MIN_SEARCH_LENGTH).toBe(2);
  });
});

describe('useTaskSearch — active after debounce', () => {
  it('becomes active once the debounce window elapses for a >= 2 char term', () => {
    const { result } = renderHook(() => useTaskSearch('auth'));
    expect(result.current.active).toBe(false);
    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(result.current.active).toBe(true);
  });

  it('runs the query with the debounced term as the $term variable, unpaused', () => {
    mockResult = { data: dataWith(), fetching: false };
    renderHook(() => useTaskSearch('auth'));
    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(lastUseQueryArgs?.pause).toBe(false);
    expect(lastUseQueryArgs?.variables).toMatchObject({ term: 'auth' });
  });

  it('does not commit a new debounced term until the timer fires again', () => {
    const { rerender, result } = renderHook(({ term }) => useTaskSearch(term), {
      initialProps: { term: 'auth' },
    });
    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(lastUseQueryArgs?.variables).toMatchObject({ term: 'auth' });

    // Type more: the new term must not reach the query before the debounce fires.
    rerender({ term: 'authorization' });
    expect(lastUseQueryArgs?.variables).toMatchObject({ term: 'auth' });

    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(lastUseQueryArgs?.variables).toMatchObject({ term: 'authorization' });
    expect(result.current.active).toBe(true);
  });
});

describe('useTaskSearch — result mapping', () => {
  it('maps GraphQL nodes to TaskPickerItem with numeric urgency/impact', () => {
    mockResult = {
      data: dataWith(
        node('1', 'Refactor auth', { urgency: 'CRITICAL', impact: 'LOW', plannedStart: '2026-06-29' }),
        node('2', 'Auth tests', { urgency: 'MEDIUM', impact: 'HIGH', deadline: '2026-07-01' }),
      ),
      fetching: false,
    };
    const { result } = renderHook(() => useTaskSearch('auth'));
    act(() => {
      vi.advanceTimersByTime(300);
    });

    expect(result.current.results).toEqual([
      { id: '1', title: 'Refactor auth', plannedStart: '2026-06-29', deadline: null, urgency: 4, impact: 1 },
      { id: '2', title: 'Auth tests', plannedStart: null, deadline: '2026-07-01', urgency: 2, impact: 3 },
    ]);
  });

  it('returns [] when active but the query has no data yet', () => {
    mockResult = { data: undefined, fetching: true };
    const { result } = renderHook(() => useTaskSearch('auth'));
    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(result.current.results).toEqual([]);
    expect(result.current.loading).toBe(true);
  });

  it('returns [] when inactive even if stale query data is present', () => {
    mockResult = { data: dataWith(node('1', 'Refactor auth')), fetching: false };
    const { result } = renderHook(() => useTaskSearch('a'));
    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(result.current.active).toBe(false);
    expect(result.current.results).toEqual([]);
  });
});
