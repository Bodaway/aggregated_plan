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

const UPDATE_ACTIVITY_SLOT_MUTATION = `
  mutation UpdateActivitySlot($id: ID!, $input: UpdateActivitySlotInput!) {
    updateActivitySlot(id: $id, input: $input) {
      id startTime endTime halfDay date durationMinutes task { id title }
    }
  }
`;

const CREATE_ACTIVITY_SLOT_MUTATION = `
  mutation CreateActivitySlot($input: CreateActivitySlotInput!) {
    createActivitySlot(input: $input) {
      id startTime endTime halfDay date durationMinutes task { id title }
    }
  }
`;

const APPEND_TASK_NOTES_MUTATION = `
  mutation AppendTaskNotes($taskId: ID!, $text: String!) {
    appendTaskNotes(taskId: $taskId, text: $text) {
      id
      notes
    }
  }
`;

const ACTIVE_TASKS_QUERY = `
  query ActiveTasksForPicker {
    tasks(filter: { status: [TODO, IN_PROGRESS] }) {
      edges {
        node {
          id
          title
        }
      }
    }
  }
`;

export interface TaskPickerItem {
  readonly id: string;
  readonly title: string;
}

interface ActiveTasksData {
  readonly tasks: {
    readonly edges: readonly { readonly node: TaskPickerItem }[];
  };
}

interface ActivityJournalData {
  readonly activityJournal: readonly ActivitySlot[];
  readonly currentActivity: CurrentActivity | null;
}

export function useActivity(date: string) {
  const [result, reexecute] = useQuery<ActivityJournalData>({
    query: ACTIVITY_JOURNAL_QUERY,
    variables: { date },
  });

  const [tasksResult] = useQuery<ActiveTasksData>({
    query: ACTIVE_TASKS_QUERY,
  });

  const [, executeStart] = useMutation(START_ACTIVITY_MUTATION);
  const [, executeStop] = useMutation(STOP_ACTIVITY_MUTATION);
  const [, executeDelete] = useMutation(DELETE_ACTIVITY_SLOT_MUTATION);
  const [, executeUpdate] = useMutation(UPDATE_ACTIVITY_SLOT_MUTATION);
  const [, executeCreate] = useMutation(CREATE_ACTIVITY_SLOT_MUTATION);
  const [, executeAppendNotes] = useMutation(APPEND_TASK_NOTES_MUTATION);

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

  const updateSlot = useCallback(
    async (id: string, input: { taskId?: string | null; startTime?: string; endTime?: string }) => {
      const res = await executeUpdate({ id, input });
      if (!res.error) {
        reexecute({ requestPolicy: 'network-only' });
      }
      return res;
    },
    [executeUpdate, reexecute]
  );

  const createSlot = useCallback(
    async (input: { startTime: string; endTime: string; taskId?: string | null }) => {
      const res = await executeCreate({ input });
      if (!res.error) {
        reexecute({ requestPolicy: 'network-only' });
      }
      return res;
    },
    [executeCreate, reexecute]
  );

  const appendTaskNote = useCallback(
    async (taskId: string, text: string) => {
      const res = await executeAppendNotes({ taskId, text });
      if (res.error) {
        throw res.error;
      }
      // No refetch needed: the timer doesn't display notes, and the next time
      // the task opens in TaskEditSheet it'll be re-queried via cache-and-network.
      return res;
    },
    [executeAppendNotes]
  );

  const availableTasks: TaskPickerItem[] =
    tasksResult.data?.tasks.edges.map(e => e.node) ?? [];

  return {
    slots: result.data?.activityJournal ?? [],
    currentActivity: result.data?.currentActivity ?? null,
    availableTasks,
    loading: result.fetching,
    error: result.error ?? null,
    startActivity,
    stopActivity,
    deleteSlot,
    updateSlot,
    createSlot,
    appendTaskNote,
    refetch: () => reexecute({ requestPolicy: 'network-only' }),
  };
}
