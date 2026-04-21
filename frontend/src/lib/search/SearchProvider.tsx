import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
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
  readonly openTaskInSheet: (id: string) => void;
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

  const clearQuery = useCallback(() => setQuery(''), []);
  const openTaskInSheet = useCallback((id: string) => setOpenTaskId(id), []);
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
      <TaskEditSheet taskId={openTaskId} onClose={closeSheet} onUpdated={refetch} />
    </SearchContext.Provider>
  );
}
