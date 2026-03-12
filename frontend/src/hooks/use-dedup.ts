import { useCallback } from 'react';
import { useQuery, useMutation } from 'urql';

interface DedupProject {
  readonly name: string;
}

export interface DedupTask {
  readonly id: string;
  readonly title: string;
  readonly source: string;
  readonly assignee: string | null;
  readonly project: DedupProject | null;
}

export interface DeduplicationSuggestion {
  readonly id: string;
  readonly taskA: DedupTask;
  readonly taskB: DedupTask;
  readonly confidenceScore: number;
  readonly titleSimilarity: number;
  readonly assigneeMatch: boolean;
  readonly projectMatch: boolean;
}

const DEDUP_SUGGESTIONS_QUERY = `
  query DeduplicationSuggestions {
    deduplicationSuggestions {
      id
      taskA { id title source assignee project { name } }
      taskB { id title source assignee project { name } }
      confidenceScore
      titleSimilarity
      assigneeMatch
      projectMatch
    }
  }
`;

const CONFIRM_DEDUP_MUTATION = `
  mutation ConfirmDeduplication($suggestionId: ID!, $accept: Boolean!) {
    confirmDeduplication(suggestionId: $suggestionId, accept: $accept)
  }
`;

export function useDedup() {
  const [result, reexecute] = useQuery<{
    deduplicationSuggestions: readonly DeduplicationSuggestion[];
  }>({
    query: DEDUP_SUGGESTIONS_QUERY,
  });

  const [, executeConfirm] = useMutation<{ confirmDeduplication: boolean }>(
    CONFIRM_DEDUP_MUTATION
  );

  const confirmDeduplication = useCallback(
    async (suggestionId: string, accept: boolean) => {
      const res = await executeConfirm({ suggestionId, accept });
      if (!res.error) {
        reexecute({ requestPolicy: 'network-only' });
      }
      return res;
    },
    [executeConfirm, reexecute]
  );

  return {
    suggestions: result.data?.deduplicationSuggestions ?? [],
    loading: result.fetching,
    error: result.error ?? null,
    confirmDeduplication,
    refetch: () => reexecute({ requestPolicy: 'network-only' }),
  };
}
