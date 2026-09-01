import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { useTaskEdit } from './use-task-edit';

// ── urql mock ─────────────────────────────────────────────────────────────────
//
// urql resolves a failed mutation: the failure comes back as `result.error`, not
// as a rejection. The panel has no Save button any more, so a write that fails
// silently is invisible data loss — this seam is where the error becomes a
// throw the autosave status can pick up.

interface MutationOutcome {
  readonly data?: unknown;
  readonly error?: unknown;
}

const reexecuteQuery = vi.fn();
const mutationCalls: { name: string; vars: Record<string, unknown> }[] = [];
const outcomes = new Map<string, MutationOutcome>();

vi.mock('urql', () => ({
  useQuery: () => [
    { data: undefined, fetching: false, error: undefined },
    reexecuteQuery,
  ],
  useMutation: (query: string) => {
    const name = /mutation\s+(\w+)/.exec(query)?.[1] ?? 'Unknown';
    return [
      { fetching: false },
      async (vars: Record<string, unknown>) => {
        mutationCalls.push({ name, vars });
        return outcomes.get(name) ?? { data: {} };
      },
    ];
  },
}));

/** The shape urql actually puts in `result.error`. */
function combinedError(message: string): Error {
  const err = new Error(`[Network] ${message}`);
  err.name = 'CombinedError';
  Object.assign(err, { graphQLErrors: [], networkError: new Error(message) });
  return err;
}

function renderTaskEdit(taskId: string | null = 'task-1') {
  return renderHook(() => useTaskEdit(taskId));
}

beforeEach(() => {
  mutationCalls.length = 0;
  outcomes.clear();
  reexecuteQuery.mockReset();
});

// ── updateTask ────────────────────────────────────────────────────────────────

describe('useTaskEdit — updateTask', () => {
  it('sends the id and the input to the UpdateTask mutation', async () => {
    const { result } = renderTaskEdit();

    await act(async () => {
      await result.current.updateTask('task-1', { notes: 'y' });
    });

    expect(mutationCalls).toEqual([
      { name: 'UpdateTask', vars: { id: 'task-1', input: { notes: 'y' } } },
    ]);
  });

  it('refetches the task network-only once the write lands', async () => {
    const { result } = renderTaskEdit();

    await act(async () => {
      await result.current.updateTask('task-1', { notes: 'y' });
    });

    expect(reexecuteQuery).toHaveBeenCalledWith({ requestPolicy: 'network-only' });
  });

  it('rejects when urql answers with an error instead of data', async () => {
    outcomes.set('UpdateTask', { error: combinedError('write refused') });
    const { result } = renderTaskEdit();

    await expect(result.current.updateTask('task-1', { notes: 'y' })).rejects.toThrow();
  });

  it('rejects rather than no-op when the target id is missing', async () => {
    // A resolved promise has to mean the server has it. A silent return would be
    // recorded as saved, which is the whole bug class this hook is guarding.
    const { result } = renderTaskEdit();

    await expect(result.current.updateTask('', { notes: 'y' })).rejects.toThrow();
    expect(mutationCalls).toEqual([]);
  });
});

// ── updatePriority ────────────────────────────────────────────────────────────

describe('useTaskEdit — updatePriority', () => {
  it('sends the task id with both levels', async () => {
    const { result } = renderTaskEdit();

    await act(async () => {
      await result.current.updatePriority('task-1', 'CRITICAL', 'LOW');
    });

    expect(mutationCalls).toEqual([
      {
        name: 'UpdateTaskPriority',
        vars: { taskId: 'task-1', urgency: 'CRITICAL', impact: 'LOW' },
      },
    ]);
  });

  it('rejects when urql answers with an error instead of data', async () => {
    outcomes.set('UpdateTaskPriority', { error: combinedError('priority refused') });
    const { result } = renderTaskEdit();

    await expect(result.current.updatePriority('task-1', 'HIGH', 'HIGH')).rejects.toThrow();
  });
});

// ── skipOccurrence ────────────────────────────────────────────────────────────

describe('useTaskEdit — skipOccurrence', () => {
  it('sends the task id', async () => {
    const { result } = renderTaskEdit();

    await act(async () => {
      await result.current.skipOccurrence('task-1');
    });

    expect(mutationCalls).toEqual([
      { name: 'SkipOccurrence', vars: { taskId: 'task-1' } },
    ]);
  });

  it('rejects when urql answers with an error instead of data', async () => {
    // The panel turns this rejection into its skip-error badge: an occurrence
    // that is still there must not look skipped.
    outcomes.set('SkipOccurrence', { error: combinedError('skip refused') });
    const { result } = renderTaskEdit();

    await expect(result.current.skipOccurrence('task-1')).rejects.toThrow();
  });
});

// ── updateRecurringTask ───────────────────────────────────────────────────────

describe('useTaskEdit — updateRecurringTask', () => {
  it('sends the recurrence id and the template input', async () => {
    const { result } = renderTaskEdit();

    await act(async () => {
      await result.current.updateRecurringTask('rec-9', { description: 'série' });
    });

    expect(mutationCalls).toEqual([
      {
        name: 'UpdateRecurringTask',
        vars: { id: 'rec-9', input: { description: 'série' } },
      },
    ]);
  });

  it('rejects when urql answers with an error instead of data', async () => {
    outcomes.set('UpdateRecurringTask', { error: combinedError('template refused') });
    const { result } = renderTaskEdit();

    await expect(
      result.current.updateRecurringTask('rec-9', { description: 'série' }),
    ).rejects.toThrow();
  });
});

// ── the error path costs no round trip ────────────────────────────────────────
//
// SPEC_TECHNIQUE § 24.5: a failed write changed nothing server-side, so all four
// mutations throw *before* `reexecute`. Refetching there would re-read state we
// already hold, on the path where the user is already waiting.

describe('useTaskEdit — a failed write does not refetch', () => {
  it.each([
    ['updateTask', 'UpdateTask', (r: ReturnType<typeof useTaskEdit>) => r.updateTask('task-1', { notes: 'y' })],
    ['updatePriority', 'UpdateTaskPriority', (r: ReturnType<typeof useTaskEdit>) => r.updatePriority('task-1', 'HIGH', 'HIGH')],
    ['skipOccurrence', 'SkipOccurrence', (r: ReturnType<typeof useTaskEdit>) => r.skipOccurrence('task-1')],
    ['updateRecurringTask', 'UpdateRecurringTask', (r: ReturnType<typeof useTaskEdit>) => r.updateRecurringTask('rec-9', {})],
  ])('%s', async (_label, mutation, invoke) => {
    outcomes.set(mutation, { error: combinedError('refused') });
    const { result } = renderTaskEdit();

    await expect(invoke(result.current)).rejects.toThrow();

    expect(mutationCalls).toHaveLength(1);
    expect(reexecuteQuery).not.toHaveBeenCalled();
  });
});

// ── An unaddressed write is a bug, not a no-op ────────────────────────────────

describe('useTaskEdit — a missing target id', () => {
  it.each([
    ['updateTask', (r: ReturnType<typeof useTaskEdit>) => r.updateTask('', { notes: 'y' })],
    ['updatePriority', (r: ReturnType<typeof useTaskEdit>) => r.updatePriority('', 'HIGH', 'HIGH')],
    ['skipOccurrence', (r: ReturnType<typeof useTaskEdit>) => r.skipOccurrence('')],
    ['updateRecurringTask', (r: ReturnType<typeof useTaskEdit>) => r.updateRecurringTask('', {})],
  ])('%s rejects and sends nothing', async (_label, invoke) => {
    const { result } = renderTaskEdit();

    await expect(invoke(result.current)).rejects.toThrow();

    expect(mutationCalls).toEqual([]);
    expect(reexecuteQuery).not.toHaveBeenCalled();
  });
});
