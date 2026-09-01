import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import Fuse, { type FuseResult } from 'fuse.js';
import { useSearchableTasks } from '@/hooks/use-searchable-tasks';
import { TaskEditSheet } from '@/components/task/TaskEditSheet';
import { FUSE_OPTIONS, MAX_MATCHES, MIN_QUERY_LENGTH } from './fuse-config';
import type { SearchableTask } from './types';

interface SearchContextValue {
  readonly query: string;
  readonly setQuery: (q: string) => void;
  readonly matches: readonly FuseResult<SearchableTask>[];
  readonly matchedIds: ReadonlySet<string>;
  readonly highlightActive: boolean;
  readonly openTaskId: string | null;
  /** Async: the panel stays open across a switch, so the outgoing task's pending
   *  write has to land before `openTaskId` moves. */
  readonly openTaskInSheet: (id: string) => Promise<void>;
  readonly closeSheet: () => void;
  readonly clearQuery: () => void;
  readonly loading: boolean;
  readonly error: Error | null;
}

const SearchContext = createContext<SearchContextValue | null>(null);

export function useSearch(): SearchContextValue {
  const ctx = useContext(SearchContext);
  if (!ctx) throw new Error('useSearch must be used within a SearchProvider');
  return ctx;
}

export function SearchProvider({ children }: { readonly children: ReactNode }) {
  const { tasks, loading, error, refetch } = useSearchableTasks();
  const [query, setQuery] = useState('');
  const [openTaskId, setOpenTaskId] = useState<string | null>(null);

  // Refetch on window focus.
  useEffect(() => {
    const onFocus = () => refetch();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [refetch]);

  const fuse = useMemo(
    () => new Fuse<SearchableTask>([...tasks], FUSE_OPTIONS),
    [tasks]
  );

  const highlightActive = query.trim().length >= MIN_QUERY_LENGTH && !loading && !error;

  const matches = useMemo<FuseResult<SearchableTask>[]>(
    () => (highlightActive ? fuse.search(query).slice(0, MAX_MATCHES) : []),
    [fuse, query, highlightActive]
  );

  const matchedIds = useMemo<ReadonlySet<string>>(
    () => new Set(matches.map((m) => m.item.id)),
    [matches]
  );

  // The open sheet's debounce drain, while it is mounted (Cmd+K → Enter switches
  // task without ever closing the panel).
  const pendingFlushRef = useRef<(() => Promise<boolean>) | null>(null);
  const registerPendingFlush = useCallback((flush: (() => Promise<boolean>) | null) => {
    pendingFlushRef.current = flush;
  }, []);

  const clearQuery = useCallback(() => setQuery(''), []);

  // A failed write cancels the switch, exactly as it cancels a close: staying on
  // the task is the only way to keep the edit, since there is no Save button to
  // come back to.
  const openTaskInSheet = useCallback(async (id: string) => {
    if ((await pendingFlushRef.current?.()) ?? true) setOpenTaskId(id);
  }, []);

  const closeSheet = useCallback(() => setOpenTaskId(null), []);

  const value: SearchContextValue = {
    query,
    setQuery,
    matches,
    matchedIds,
    highlightActive,
    openTaskId,
    openTaskInSheet,
    closeSheet,
    clearQuery,
    loading,
    error,
  };

  return (
    <SearchContext.Provider value={value}>
      {children}
      <TaskEditSheet
        taskId={openTaskId}
        onClose={closeSheet}
        onUpdated={refetch}
        registerPendingFlush={registerPendingFlush}
      />
    </SearchContext.Provider>
  );
}
