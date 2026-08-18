import { useCallback, useState } from 'react';
import { useMutation, useQuery } from 'urql';
import type {
  Brief,
  Memory,
  MemoryImportReport,
  RememberInput,
  ScoredMemory,
} from '@/lib/memory/types';

const MEMORY_FIELDS = `
  id
  kind
  title
  body
  occurredAt
  recordedAt
  invalidatedAt
  supersededBy
  proposedSupersedes
  status
  taskId
  projectId
  stakeholders
`;

const PENDING_MEMORIES_QUERY = `
  query PendingMemories($limit: Int!) {
    pendingMemories(limit: $limit) { ${MEMORY_FIELDS} }
  }
`;

const MEMORY_BRIEF_QUERY = `
  query MemoryBrief {
    brief {
      date
      pendingCount
      decisions { id reference title stakeholders occurredOn }
      decisionTotal
      commitments { id reference title stakeholders occurredOn }
      commitmentTotal
      consolidation { daysAgo stale }
    }
  }
`;

const RECALL_QUERY = `
  query RecallMemories($q: String!, $includeHistory: Boolean!, $limit: Int!) {
    recall(q: $q, includeHistory: $includeHistory, limit: $limit) {
      score
      memory { ${MEMORY_FIELDS} }
    }
  }
`;

// `force` is `Boolean! = false` in the schema, so the variable must carry the
// same default: a nullable variable cannot feed a non-null argument.
const ACCEPT_MEMORY_MUTATION = `
  mutation AcceptMemory($id: ID!, $force: Boolean! = false) {
    acceptMemory(id: $id, force: $force) {
      accepted { ${MEMORY_FIELDS} }
      nearDuplicates { ${MEMORY_FIELDS} }
    }
  }
`;

const REJECT_MEMORY_MUTATION = `
  mutation RejectMemory($id: ID!) {
    rejectMemory(id: $id) { id status }
  }
`;

const MERGE_MEMORY_MUTATION = `
  mutation MergeMemory($id: ID!, $into: ID!) {
    mergeMemory(id: $id, into: $into) {
      survivor { id title }
      discardedId
    }
  }
`;

const SUPERSEDE_MEMORY_MUTATION = `
  mutation SupersedeMemory($old: ID, $by: ID!) {
    supersedeMemory(old: $old, by: $by) {
      invalidated { id invalidatedAt supersededBy }
      successor { id title status }
    }
  }
`;

const REMEMBER_MUTATION = `
  mutation Remember($input: RememberInputGql!) {
    remember(input: $input) { id status }
  }
`;

const IMPORT_MEMORIES_MUTATION = `
  mutation ImportMemories($directory: String!) {
    importMemories(directory: $directory) {
      importedCount
      skippedCount
      imported { ${MEMORY_FIELDS} }
      skipped { fileName reason }
    }
  }
`;

const PENDING_PAGE_SIZE = 100;
const RECALL_LIMIT = 20;

/** Candidate id → the memories `acceptMemory` refused to duplicate. */
type NearDuplicateMap = Readonly<Record<string, readonly Memory[]>>;

/**
 * The validation queue and its verdicts. A verdict that lands refetches the
 * queue and the brief; a verdict the backend refused writes nothing and leaves
 * the arbitration on screen instead.
 */
export function useMemoryQueue() {
  const [pendingResult, reexecutePending] = useQuery<{ pendingMemories: readonly Memory[] }>({
    query: PENDING_MEMORIES_QUERY,
    variables: { limit: PENDING_PAGE_SIZE },
  });
  const [briefResult, reexecuteBrief] = useQuery<{ brief: Brief }>({ query: MEMORY_BRIEF_QUERY });

  const [acceptState, executeAccept] = useMutation(ACCEPT_MEMORY_MUTATION);
  const [rejectState, executeReject] = useMutation(REJECT_MEMORY_MUTATION);
  const [mergeState, executeMerge] = useMutation(MERGE_MEMORY_MUTATION);
  const [supersedeState, executeSupersede] = useMutation(SUPERSEDE_MEMORY_MUTATION);
  const [rememberState, executeRemember] = useMutation(REMEMBER_MUTATION);
  const [importState, executeImport] = useMutation(IMPORT_MEMORIES_MUTATION);

  const [nearDuplicates, setNearDuplicates] = useState<NearDuplicateMap>({});
  const [importReport, setImportReport] = useState<MemoryImportReport | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    reexecutePending({ requestPolicy: 'network-only' });
    reexecuteBrief({ requestPolicy: 'network-only' });
  }, [reexecutePending, reexecuteBrief]);

  const forget = useCallback((id: string) => {
    setNearDuplicates(prev => {
      if (!(id in prev)) return prev;
      const next = { ...prev };
      delete next[id];
      return next;
    });
  }, []);

  const accept = useCallback(
    async (id: string, force = false) => {
      setActionError(null);
      const res = await executeAccept({ id, force });
      if (res.error) {
        setActionError(res.error.message);
        return;
      }
      const payload = res.data?.acceptMemory;
      if (payload && payload.accepted === null) {
        setNearDuplicates(prev => ({ ...prev, [id]: payload.nearDuplicates ?? [] }));
        return;
      }
      forget(id);
      refresh();
    },
    [executeAccept, forget, refresh]
  );

  const forceAccept = useCallback((id: string) => accept(id, true), [accept]);

  const reject = useCallback(
    async (id: string) => {
      setActionError(null);
      const res = await executeReject({ id });
      if (res.error) {
        setActionError(res.error.message);
        return;
      }
      forget(id);
      refresh();
    },
    [executeReject, forget, refresh]
  );

  const mergeInto = useCallback(
    async (id: string, into: string) => {
      setActionError(null);
      const res = await executeMerge({ id, into });
      if (res.error) {
        setActionError(res.error.message);
        return;
      }
      forget(id);
      refresh();
    },
    [executeMerge, forget, refresh]
  );

  const supersede = useCallback(
    async (candidateId: string, oldId: string | null) => {
      setActionError(null);
      const res = await executeSupersede({ old: oldId, by: candidateId });
      if (res.error) {
        setActionError(res.error.message);
        return;
      }
      forget(candidateId);
      refresh();
    },
    [executeSupersede, forget, refresh]
  );

  const remember = useCallback(
    async (input: RememberInput) => {
      setActionError(null);
      const res = await executeRemember({ input });
      if (res.error) {
        setActionError(res.error.message);
        return;
      }
      refresh();
    },
    [executeRemember, refresh]
  );

  const importDirectory = useCallback(
    async (directory: string) => {
      setActionError(null);
      setImportReport(null);
      const res = await executeImport({ directory });
      if (res.error) {
        setActionError(res.error.message);
        return;
      }
      setImportReport(res.data?.importMemories ?? null);
      refresh();
    },
    [executeImport, refresh]
  );

  return {
    pending: pendingResult.data?.pendingMemories ?? [],
    brief: briefResult.data?.brief ?? null,
    loading: pendingResult.fetching || briefResult.fetching,
    error: pendingResult.error?.message ?? briefResult.error?.message ?? actionError,
    busy:
      acceptState.fetching ||
      rejectState.fetching ||
      mergeState.fetching ||
      supersedeState.fetching ||
      rememberState.fetching,
    nearDuplicates,
    accept,
    forceAccept,
    reject,
    mergeInto,
    supersede,
    remember,
    importDirectory,
    importReport,
    importing: importState.fetching,
  };
}

/**
 * One recall search. Paused until the caller actually searches, so mounting the
 * page never fires a query with an empty match expression (the backend refuses
 * one, and rightly).
 */
export function useMemoryRecall() {
  const [query, setQuery] = useState<string | null>(null);
  const [includeHistory, setIncludeHistory] = useState(false);

  const [result] = useQuery<{ recall: readonly ScoredMemory[] }>({
    query: RECALL_QUERY,
    variables: { q: query ?? '', includeHistory, limit: RECALL_LIMIT },
    pause: query === null,
  });

  const search = useCallback((q: string, history = false) => {
    setIncludeHistory(history);
    setQuery(q);
  }, []);

  return {
    results: result.data?.recall ?? [],
    searched: query !== null,
    loading: result.fetching,
    error: result.error?.message ?? null,
    search,
  };
}

/** The capture path: `remember` alone, for the selection chip on the dashboard. */
export function useMemoryCapture() {
  const [state, executeRemember] = useMutation(REMEMBER_MUTATION);
  const [error, setError] = useState<string | null>(null);

  const remember = useCallback(
    async (input: RememberInput) => {
      setError(null);
      const res = await executeRemember({ input });
      if (res.error) setError(res.error.message);
    },
    [executeRemember]
  );

  return { remember, saving: state.fetching, error };
}
