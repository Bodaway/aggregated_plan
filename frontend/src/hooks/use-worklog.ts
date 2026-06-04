import { useCallback, useMemo } from 'react';
import { useQuery, useMutation } from 'urql';

const WORKLOG_QUERY = `
  query WorklogEntries($filter: WorklogEntryFilterInput) {
    worklogEntries(filter: $filter) {
      id
      taskId
      task { id title }
      body
      loggedAt
      createdAt
      updatedAt
      occurrenceDate
    }
  }
`;

const ADD = `
  mutation AddWorklogEntry($taskId: ID!, $body: String!, $loggedAt: DateTime) {
    addWorklogEntry(taskId: $taskId, body: $body, loggedAt: $loggedAt) {
      id taskId task { id title } body loggedAt createdAt updatedAt
    }
  }
`;

const UPDATE = `
  mutation UpdateWorklogEntry($id: ID!, $body: String, $loggedAt: DateTime) {
    updateWorklogEntry(id: $id, body: $body, loggedAt: $loggedAt) {
      id taskId task { id title } body loggedAt createdAt updatedAt
    }
  }
`;

const DELETE = `
  mutation DeleteWorklogEntry($id: ID!) { deleteWorklogEntry(id: $id) }
`;

export type WorklogEntry = {
  id: string;
  taskId: string;
  task: { id: string; title: string } | null;
  body: string;
  loggedAt: string;
  createdAt: string;
  updatedAt: string;
  occurrenceDate: string | null;
};

export type WorklogFilter = {
  taskIds?: string[];
  recurrenceId?: string;
  from?: string;
  to?: string;
  limit?: number;
  offset?: number;
};

export function useWorklog(filter: WorklogFilter = {}) {
  const variables = useMemo(() => ({ filter }), [filter]);
  const [result, reexecute] = useQuery<{ worklogEntries: WorklogEntry[] }>({
    query: WORKLOG_QUERY,
    variables,
    requestPolicy: 'cache-and-network',
  });

  const [, executeAdd] = useMutation<{ addWorklogEntry: WorklogEntry }>(ADD);
  const [, executeUpdate] = useMutation<{ updateWorklogEntry: WorklogEntry }>(UPDATE);
  const [, executeDelete] = useMutation<{ deleteWorklogEntry: boolean }>(DELETE);

  const refetch = useCallback(
    () => reexecute({ requestPolicy: 'network-only' }),
    [reexecute]
  );

  const addEntry = useCallback(
    async (input: { taskId: string; body: string; loggedAt?: string }) => {
      const res = await executeAdd(input);
      if (res.error) throw res.error;
      refetch();
      return res.data?.addWorklogEntry;
    },
    [executeAdd, refetch]
  );

  const updateEntry = useCallback(
    async (input: { id: string; body?: string; loggedAt?: string }) => {
      const res = await executeUpdate(input);
      if (res.error) throw res.error;
      refetch();
      return res.data?.updateWorklogEntry;
    },
    [executeUpdate, refetch]
  );

  const deleteEntry = useCallback(
    async (id: string) => {
      const res = await executeDelete({ id });
      if (res.error) throw res.error;
      refetch();
      return res.data?.deleteWorklogEntry ?? false;
    },
    [executeDelete, refetch]
  );

  return {
    entries: result.data?.worklogEntries ?? [],
    loading: result.fetching,
    error: result.error ?? null,
    addEntry,
    updateEntry,
    deleteEntry,
    refetch,
  };
}
