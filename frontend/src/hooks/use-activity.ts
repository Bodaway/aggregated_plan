import { useCallback } from 'react';
import { useQuery, useMutation } from 'urql';
import { formatDate } from '@/lib/date-utils';
import { sortTasksForPicker } from '@/lib/task-picker-sort';

const URGENCY_NUM: Record<string, number> = { LOW: 1, MEDIUM: 2, HIGH: 3, CRITICAL: 4 };
const IMPACT_NUM: Record<string, number> = { LOW: 1, MEDIUM: 2, HIGH: 3, CRITICAL: 4 };

function toNum(map: Record<string, number>, v: unknown): number {
  if (typeof v === 'number') return v;
  return map[v as string] ?? 1;
}

/**
 * Convert a raw GraphQL task node (urgency/impact as enum strings or numbers)
 * into the canonical {@link TaskPickerItem}. Shared by the activity picker and
 * the task-search hook so the enum → number mapping lives in one place.
 */
export function toPickerItem(node: RawTaskPickerNode): TaskPickerItem {
  return {
    id: node.id,
    title: node.title,
    plannedStart: node.plannedStart,
    deadline: node.deadline,
    urgency: toNum(URGENCY_NUM, node.urgency),
    impact: toNum(IMPACT_NUM, node.impact),
  };
}

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

const ADD_WORKLOG_ENTRY_FROM_TIMER_MUTATION = `
  mutation AddWorklogEntryFromTimer($taskId: ID!, $body: String!) {
    addWorklogEntry(taskId: $taskId, body: $body) {
      id
    }
  }
`;

const ACTIVE_TASKS_QUERY = `
  query ActiveTasksForPicker {
    tasks(filter: { trackingState: [FOLLOWED], status: [TODO, IN_PROGRESS, BLOCKED] }, first: 500) {
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

export interface TaskPickerItem {
  readonly id: string;
  readonly title: string;
  readonly plannedStart: string | null;
  readonly deadline: string | null;
  readonly urgency: number;
  readonly impact: number;
}

/** Raw shape returned by GraphQL before enum → number conversion. */
export interface RawTaskPickerNode {
  readonly id: string;
  readonly title: string;
  readonly plannedStart: string | null;
  readonly deadline: string | null;
  readonly urgency: string | number;
  readonly impact: string | number;
}

interface ActiveTasksData {
  readonly tasks: {
    readonly edges: readonly { readonly node: RawTaskPickerNode }[];
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
  const [, executeAddWorklogEntry] = useMutation(ADD_WORKLOG_ENTRY_FROM_TIMER_MUTATION);

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
      const res = await executeAddWorklogEntry({ taskId, body: text });
      if (res.error) {
        throw res.error;
      }
      return res;
    },
    [executeAddWorklogEntry]
  );

  const availableTasks: TaskPickerItem[] = sortTasksForPicker(
    (tasksResult.data?.tasks.edges ?? []).map(e => toPickerItem(e.node)),
    formatDate(new Date()),
  );

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
