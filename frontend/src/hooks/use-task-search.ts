import { useEffect, useState } from 'react';
import { useQuery } from 'urql';
import { toPickerItem } from './use-activity';
import type { RawTaskPickerNode, TaskPickerItem } from './use-activity';

/** Minimum number of characters before a server-side title search runs. */
export const MIN_SEARCH_LENGTH = 2;

/** Debounce window applied to the search term before it reaches the query. */
const DEBOUNCE_MS = 250;

const TASK_SEARCH_QUERY = `
  query TaskSearch($term: String!) {
    tasks(filter: { titleContains: $term }, first: 50) {
      edges {
        node {
          id
          title
          plannedStart
          deadline
          urgency
          impact
        }
      }
    }
  }
`;

interface TaskSearchData {
  readonly tasks: {
    readonly edges: readonly { readonly node: RawTaskPickerNode }[];
  };
}

export interface TaskSearchResult {
  readonly results: readonly TaskPickerItem[];
  readonly loading: boolean;
  /** True once the debounced term is long enough to drive a server search. */
  readonly active: boolean;
}

/**
 * Debounced, server-side title search across ALL tasks. Stays paused (and
 * returns no results) until the debounced term reaches {@link MIN_SEARCH_LENGTH}.
 */
export function useTaskSearch(term: string): TaskSearchResult {
  const [debouncedTerm, setDebouncedTerm] = useState('');

  useEffect(() => {
    const handle = setTimeout(() => setDebouncedTerm(term), DEBOUNCE_MS);
    return () => clearTimeout(handle);
  }, [term]);

  const active = debouncedTerm.trim().length >= MIN_SEARCH_LENGTH;

  const [result] = useQuery<TaskSearchData>({
    query: TASK_SEARCH_QUERY,
    variables: { term: debouncedTerm },
    pause: !active,
  });

  const results: readonly TaskPickerItem[] = active
    ? (result.data?.tasks.edges ?? []).map(e => toPickerItem(e.node))
    : [];

  return {
    results,
    loading: result.fetching,
    active,
  };
}
