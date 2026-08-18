import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import type { Memory } from '@/lib/memory/types';

const candidate: Memory = {
  id: 'cand-1',
  kind: 'FACT',
  title: 'Un candidat en attente',
  body: null,
  occurredAt: '2026-08-17T15:34:43Z',
  recordedAt: '2026-08-17T15:34:43Z',
  invalidatedAt: null,
  supersededBy: null,
  proposedSupersedes: null,
  status: 'PENDING',
  taskId: null,
  projectId: null,
  stakeholders: [],
};

const duplicate: Memory = { ...candidate, id: 'existing-1', status: 'ACTIVE' };

const brief = {
  date: '2026-08-18',
  pendingCount: 40,
  decisions: [],
  decisionTotal: 1,
  commitments: [],
  commitmentTotal: 0,
  consolidation: { daysAgo: 1, stale: false },
};

const reexecutePending = vi.fn();
const acceptMutation = vi.fn();
const rejectMutation = vi.fn();
let recallVariables: Record<string, unknown> = {};
let recallPaused = true;

vi.mock('urql', () => ({
  useQuery: ({ query, variables, pause }: { query: string; variables?: Record<string, unknown>; pause?: boolean }) => {
    if (query.includes('PendingMemories')) {
      return [{ fetching: false, data: { pendingMemories: [candidate] }, error: undefined }, reexecutePending];
    }
    if (query.includes('MemoryBrief')) {
      return [{ fetching: false, data: { brief }, error: undefined }, vi.fn()];
    }
    recallVariables = variables ?? {};
    recallPaused = pause ?? false;
    return [{ fetching: false, data: { recall: [] }, error: undefined }, vi.fn()];
  },
  useMutation: (doc: string) => {
    if (doc.includes('AcceptMemory')) return [{ fetching: false }, acceptMutation];
    if (doc.includes('RejectMemory')) return [{ fetching: false }, rejectMutation];
    return [{ fetching: false }, vi.fn().mockResolvedValue({ data: {} })];
  },
}));

import { useMemoryQueue, useMemoryRecall } from './use-memory';

beforeEach(() => {
  reexecutePending.mockReset();
  acceptMutation.mockReset();
  rejectMutation.mockReset();
  acceptMutation.mockResolvedValue({
    data: { acceptMemory: { accepted: { ...candidate, status: 'ACTIVE' }, nearDuplicates: [] } },
  });
  rejectMutation.mockResolvedValue({ data: { rejectMemory: { id: 'cand-1', status: 'REJECTED' } } });
});

describe('useMemoryQueue', () => {
  it('exposes the pending queue and the brief', () => {
    const { result } = renderHook(() => useMemoryQueue());

    expect(result.current.pending).toEqual([candidate]);
    expect(result.current.brief?.pendingCount).toBe(40);
  });

  it('refetches the queue once a verdict lands', async () => {
    const { result } = renderHook(() => useMemoryQueue());

    await act(async () => {
      await result.current.reject('cand-1');
    });

    expect(rejectMutation).toHaveBeenCalledWith({ id: 'cand-1' });
    expect(reexecutePending).toHaveBeenCalledWith({ requestPolicy: 'network-only' });
  });

  it('keeps the near-duplicates the backend refused the accept on', async () => {
    acceptMutation.mockResolvedValue({
      data: { acceptMemory: { accepted: null, nearDuplicates: [duplicate] } },
    });
    const { result } = renderHook(() => useMemoryQueue());

    await act(async () => {
      await result.current.accept('cand-1');
    });

    expect(result.current.nearDuplicates['cand-1']).toEqual([duplicate]);
    expect(reexecutePending).not.toHaveBeenCalled();
  });

  it('clears the arbitration once the accept is forced through', async () => {
    acceptMutation.mockResolvedValueOnce({
      data: { acceptMemory: { accepted: null, nearDuplicates: [duplicate] } },
    });
    const { result } = renderHook(() => useMemoryQueue());

    await act(async () => {
      await result.current.accept('cand-1');
    });
    await act(async () => {
      await result.current.forceAccept('cand-1');
    });

    expect(acceptMutation).toHaveBeenLastCalledWith({ id: 'cand-1', force: true });
    expect(result.current.nearDuplicates['cand-1']).toBeUndefined();
  });
});

describe('useMemoryRecall', () => {
  it('runs no query until a search is asked for', () => {
    const { result } = renderHook(() => useMemoryRecall());

    expect(recallPaused).toBe(true);
    expect(result.current.searched).toBe(false);
  });

  it('searches with the history flag the caller passed', () => {
    const { result } = renderHook(() => useMemoryRecall());

    act(() => {
      result.current.search('consolidation', true);
    });

    expect(recallPaused).toBe(false);
    expect(recallVariables).toMatchObject({ q: 'consolidation', includeHistory: true });
    expect(result.current.searched).toBe(true);
  });
});
