import { useState, useRef, useCallback, useMemo, useEffect } from 'react';
import { useQuery } from 'urql';

export interface TaskSearchResult {
  readonly taskId: string;
  readonly matchedField: string;
  readonly matchedSnippet: string;
}

const SEARCH_QUERY = `
  query SearchTasks($query: String!, $limit: Int) {
    searchTasks(query: $query, limit: $limit) {
      taskId
      matchedField
      matchedSnippet
    }
  }
`;

interface SearchResponse {
  searchTasks: readonly TaskSearchResult[];
}

const DEBOUNCE_MS = 300;

export function useTaskSearch() {
  const [searchQuery, setSearchQuery] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Debounce the search query
  useEffect(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
    }
    timerRef.current = setTimeout(() => {
      setDebouncedQuery(searchQuery.trim());
    }, DEBOUNCE_MS);

    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }
    };
  }, [searchQuery]);

  const [result] = useQuery<SearchResponse>({
    query: SEARCH_QUERY,
    variables: { query: debouncedQuery, limit: 50 },
    pause: debouncedQuery.length === 0,
  });

  const results: readonly TaskSearchResult[] = result.data?.searchTasks ?? [];

  const matchingIds: Set<string> = useMemo(
    () => new Set(results.map(r => r.taskId)),
    [results],
  );

  const isSearchActive = searchQuery.length > 0;
  const isSearching = result.fetching;

  const clearSearch = useCallback(() => {
    setSearchQuery('');
    setDebouncedQuery('');
  }, []);

  return {
    searchQuery,
    setSearchQuery,
    results,
    matchingIds,
    isSearchActive,
    isSearching,
    clearSearch,
  };
}
