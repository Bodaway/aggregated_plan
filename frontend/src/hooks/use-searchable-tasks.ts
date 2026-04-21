import { useQuery } from 'urql';
import type { SearchableTask } from '@/lib/search/types';

const SEARCHABLE_TASKS_QUERY = `
  query SearchableTasks {
    searchableTasks {
      id
      title
      sourceId
      source
      assignee
      projectName
      tags
      description
      status
    }
  }
`;

interface UseSearchableTasksResult {
  readonly tasks: readonly SearchableTask[];
  readonly loading: boolean;
  readonly error: Error | null;
  readonly refetch: () => void;
}

export function useSearchableTasks(): UseSearchableTasksResult {
  const [result, reexecute] = useQuery<{ searchableTasks: SearchableTask[] }>({
    query: SEARCHABLE_TASKS_QUERY,
    requestPolicy: 'cache-and-network',
  });

  return {
    tasks: result.data?.searchableTasks ?? [],
    loading: result.fetching,
    error: (result.error as Error | undefined) ?? null,
    refetch: () => reexecute({ requestPolicy: 'network-only' }),
  };
}
