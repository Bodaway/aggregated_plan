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
      status
      urgency
      impact
      quadrant
      estimatedHours
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

export function useTaskEdit(taskId: string | null) {
  const [result, reexecute] = useQuery<{ task: FullTask }>({
    query: TASK_QUERY,
    variables: { id: taskId },
    pause: !taskId,
    requestPolicy: 'cache-and-network',
  });

  const [, executeUpdate] = useMutation(UPDATE_TASK_MUTATION);
  const [, executePriorityUpdate] = useMutation(UPDATE_PRIORITY_MUTATION);

  const updateTask = async (input: Record<string, unknown>) => {
    if (!taskId) return;
    await executeUpdate({ id: taskId, input });
    reexecute({ requestPolicy: 'network-only' });
  };

  const updatePriority = async (urgency: string, impact: string) => {
    if (!taskId) return;
    await executePriorityUpdate({ taskId, urgency, impact });
    reexecute({ requestPolicy: 'network-only' });
  };

  return {
    task: result.data?.task ?? null,
    loading: result.fetching,
    error: result.error ?? null,
    updateTask,
    updatePriority,
    refetch: () => reexecute({ requestPolicy: 'network-only' }),
  };
}
