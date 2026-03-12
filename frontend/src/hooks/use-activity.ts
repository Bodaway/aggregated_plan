import { useCallback } from 'react';
import { useQuery, useMutation } from 'urql';

interface ActivityTask {
  readonly id: string;
  readonly title: string;
}

export interface ActivitySlot {
  readonly id: string;
  readonly task: ActivityTask | null;
  readonly startTime: string;
  readonly endTime: string | null;
  readonly halfDay: string;
  readonly date: string;
  readonly durationMinutes: number | null;
}

export interface CurrentActivity {
  readonly id: string;
  readonly task: ActivityTask | null;
  readonly startTime: string;
  readonly halfDay: string;
}

const ACTIVITY_JOURNAL_QUERY = `
  query ActivityJournal($date: NaiveDate!) {
    activityJournal(date: $date) {
      id
      task { id title }
      startTime
      endTime
      halfDay
      date
      durationMinutes
    }
    currentActivity {
      id
      task { id title }
      startTime
      halfDay
    }
  }
`;

const START_ACTIVITY_MUTATION = `
  mutation StartActivity($taskId: ID) {
    startActivity(taskId: $taskId) {
      id startTime halfDay task { id title }
    }
  }
`;

const STOP_ACTIVITY_MUTATION = `
  mutation StopActivity {
    stopActivity {
      id startTime endTime halfDay durationMinutes
    }
  }
`;

const DELETE_ACTIVITY_SLOT_MUTATION = `
  mutation DeleteActivitySlot($id: ID!) {
    deleteActivitySlot(id: $id)
  }
`;

interface ActivityJournalData {
  readonly activityJournal: readonly ActivitySlot[];
  readonly currentActivity: CurrentActivity | null;
}

export function useActivity(date: string) {
  const [result, reexecute] = useQuery<ActivityJournalData>({
    query: ACTIVITY_JOURNAL_QUERY,
    variables: { date },
  });

  const [, executeStart] = useMutation(START_ACTIVITY_MUTATION);
  const [, executeStop] = useMutation(STOP_ACTIVITY_MUTATION);
  const [, executeDelete] = useMutation(DELETE_ACTIVITY_SLOT_MUTATION);

  const startActivity = useCallback(
    async (taskId?: string) => {
      const res = await executeStart({ taskId: taskId ?? null });
      if (!res.error) {
        reexecute({ requestPolicy: 'network-only' });
      }
      return res;
    },
    [executeStart, reexecute]
  );

  const stopActivity = useCallback(async () => {
    const res = await executeStop({});
    if (!res.error) {
      reexecute({ requestPolicy: 'network-only' });
    }
    return res;
  }, [executeStop, reexecute]);

  const deleteSlot = useCallback(
    async (id: string) => {
      const res = await executeDelete({ id });
      if (!res.error) {
        reexecute({ requestPolicy: 'network-only' });
      }
      return res;
    },
    [executeDelete, reexecute]
  );

  return {
    slots: result.data?.activityJournal ?? [],
    currentActivity: result.data?.currentActivity ?? null,
    loading: result.fetching,
    error: result.error ?? null,
    startActivity,
    stopActivity,
    deleteSlot,
    refetch: () => reexecute({ requestPolicy: 'network-only' }),
  };
}
