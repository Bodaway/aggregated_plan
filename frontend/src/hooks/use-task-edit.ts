import { useQuery, useMutation } from 'urql';

export interface FullTask {
  readonly id: string;
  readonly title: string;
  readonly description: string | null;
  readonly notes: string | null;
  readonly source: string;
  readonly sourceId: string | null;
  readonly status: string;
  readonly jiraStatus: string | null;
  readonly urgency: string;   // GraphQL enum: LOW, MEDIUM, HIGH, CRITICAL
  readonly impact: string;    // GraphQL enum: LOW, MEDIUM, HIGH, CRITICAL
  readonly quadrant: string;
  readonly deadline: string | null;
  readonly plannedStart: string | null;
  readonly assignee: string | null;
  readonly delegatedTo: string | null;
  readonly estimatedHours: number | null;
  readonly trackingState: string;
  readonly jiraRemainingSeconds: number | null;
  readonly jiraOriginalEstimateSeconds: number | null;
  readonly jiraTimeSpentSeconds: number | null;
  readonly remainingHoursOverride: number | null;
  readonly estimatedHoursOverride: number | null;
  readonly effectiveRemainingHours: number | null;
  readonly effectiveEstimatedHours: number | null;
  readonly project: { readonly name: string } | null;
  readonly tags: readonly { readonly id: string; readonly name: string; readonly color: string | null }[];
  readonly recurrenceId: string | null;
  readonly occurrenceDate: string | null;
  readonly isRecurring: boolean;
  readonly gryzzlyTask: {
    readonly gryzzlyTaskId: string;
    readonly name: string | null;
    readonly projectName: string | null;
    readonly stale: boolean;
  } | null;
}

const TASK_QUERY = `
  query GetTask($id: ID!) {
    task(id: $id) {
      id
      title
      description
      notes
      source
      sourceId
      status
      jiraStatus
      urgency
      impact
      quadrant
      deadline
      plannedStart
      assignee
      delegatedTo
      estimatedHours
      trackingState
      jiraRemainingSeconds
      jiraOriginalEstimateSeconds
      jiraTimeSpentSeconds
      remainingHoursOverride
      estimatedHoursOverride
      effectiveRemainingHours
      effectiveEstimatedHours
      project { name }
      tags { id name color }
      recurrenceId
      occurrenceDate
      isRecurring
      gryzzlyTask {
        gryzzlyTaskId
        name
        projectName
        projectStatus
        stale
      }
    }
  }
`;

const UPDATE_TASK_MUTATION = `
  mutation UpdateTask($id: ID!, $input: UpdateTaskInput!) {
    updateTask(id: $id, input: $input) {
      id
      title
      description
      notes
      delegatedTo
      status
      urgency
      impact
      quadrant
      estimatedHours
      deadline
      plannedStart
      remainingHoursOverride
      estimatedHoursOverride
      effectiveRemainingHours
      effectiveEstimatedHours
      tags { id name color }
    }
  }
`;

const UPDATE_PRIORITY_MUTATION = `
  mutation UpdateTaskPriority($taskId: ID!, $urgency: UrgencyLevelGql, $impact: ImpactLevelGql) {
    updatePriority(taskId: $taskId, urgency: $urgency, impact: $impact) {
      id urgency impact quadrant
    }
  }
`;

const SKIP_OCCURRENCE_MUTATION = `
  mutation SkipOccurrence($taskId: ID!) {
    skipOccurrence(taskId: $taskId) {
      id
      status
    }
  }
`;

const UPDATE_RECURRING_TASK_MUTATION = `
  mutation UpdateRecurringTask($id: ID!, $input: UpdateRecurringTaskInput!) {
    updateRecurringTask(id: $id, input: $input) {
      id
      title
    }
  }
`;

/**
 * A failed write, raised as an exception: urql resolves with `.error` instead of
 * rejecting, so a caller that only awaits the promise cannot tell a write that
 * landed from one that did not.
 */
export class TaskMutationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'TaskMutationError';
  }
}

export function useTaskEdit(taskId: string | null) {
  const [result, reexecute] = useQuery<{ task: FullTask }>({
    query: TASK_QUERY,
    variables: { id: taskId },
    pause: !taskId,
    requestPolicy: 'cache-and-network',
  });

  const [, executeUpdate] = useMutation(UPDATE_TASK_MUTATION);
  const [, executePriorityUpdate] = useMutation(UPDATE_PRIORITY_MUTATION);
  const [, executeSkipOccurrence] = useMutation(SKIP_OCCURRENCE_MUTATION);
  const [, executeUpdateRecurring] = useMutation(UPDATE_RECURRING_TASK_MUTATION);

  // Nothing changed server-side on a failed write, so we throw instead of
  // spending a round trip re-reading the state we already hold.
  //
  // Every mutation takes the target id explicitly: the caller debounces, so the
  // hook's `taskId` may already point at another task by the time a write goes
  // out. A missing id throws for the same reason — a resolved promise has to mean
  // "the server has it", and a silent no-op would be recorded as saved.
  const updateTask = async (id: string, input: Record<string, unknown>) => {
    if (!id) throw new TaskMutationError('updateTask called without a task id');
    const outcome = await executeUpdate({ id, input });
    if (outcome.error) throw new TaskMutationError(outcome.error.message);
    reexecute({ requestPolicy: 'network-only' });
  };

  const updatePriority = async (id: string, urgency: string, impact: string) => {
    if (!id) throw new TaskMutationError('updatePriority called without a task id');
    const outcome = await executePriorityUpdate({ taskId: id, urgency, impact });
    if (outcome.error) throw new TaskMutationError(outcome.error.message);
    reexecute({ requestPolicy: 'network-only' });
  };

  const skipOccurrence = async (id: string) => {
    if (!id) throw new TaskMutationError('skipOccurrence called without a task id');
    const outcome = await executeSkipOccurrence({ taskId: id });
    if (outcome.error) throw new TaskMutationError(outcome.error.message);
    reexecute({ requestPolicy: 'network-only' });
  };

  const updateRecurringTask = async (recurrenceId: string, input: Record<string, unknown>) => {
    if (!recurrenceId) throw new TaskMutationError('updateRecurringTask called without a series id');
    const outcome = await executeUpdateRecurring({ id: recurrenceId, input });
    if (outcome.error) throw new TaskMutationError(outcome.error.message);
    reexecute({ requestPolicy: 'network-only' });
  };

  return {
    task: result.data?.task ?? null,
    loading: result.fetching,
    error: result.error ?? null,
    updateTask,
    updatePriority,
    skipOccurrence,
    updateRecurringTask,
    refetch: () => reexecute({ requestPolicy: 'network-only' }),
  };
}
