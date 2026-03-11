import { useQuery, useMutation } from 'urql';

export interface TriageTask {
  readonly id: string;
  readonly title: string;
  readonly source: string;
  readonly sourceId: string | null;
  readonly status: string;
  readonly jiraStatus: string | null;
  readonly urgency: number;
  readonly impact: number;
  readonly quadrant: string;
  readonly trackingState: string;
  readonly deadline: string | null;
  readonly assignee: string | null;
  readonly project: { readonly name: string } | null;
  readonly effectiveRemainingHours: number | null;
  readonly effectiveEstimatedHours: number | null;
  readonly jiraTimeSpentSeconds: number | null;
}

const TRIAGE_TASKS_QUERY = `
  query TriageTasks($trackingState: [TrackingStateGql!]) {
    tasks(filter: { status: [TODO, IN_PROGRESS], trackingState: $trackingState }) {
      edges {
        node {
          id
          title
          source
          sourceId
          status
          jiraStatus
          urgency
          impact
          quadrant
          trackingState
          deadline
          assignee
          project { name }
          effectiveRemainingHours
          effectiveEstimatedHours
          jiraTimeSpentSeconds
        }
      }
      totalCount
    }
  }
`;

const SET_TRACKING_STATE = `
  mutation SetTrackingState($taskId: ID!, $state: TrackingStateGql!) {
    setTrackingState(taskId: $taskId, state: $state) {
      id
      trackingState
    }
  }
`;

const SET_TRACKING_STATE_BATCH = `
  mutation SetTrackingStateBatch($taskIds: [ID!]!, $state: TrackingStateGql!) {
    setTrackingStateBatch(taskIds: $taskIds, state: $state) {
      id
      trackingState
    }
  }
`;

interface TasksResponse {
  tasks: {
    edges: readonly { node: TriageTask }[];
    totalCount: number;
  };
}

export function useTriageTasks() {
  const [inboxResult, reexecuteInbox] = useQuery<TasksResponse>({
    query: TRIAGE_TASKS_QUERY,
    variables: { trackingState: ['INBOX'] },
  });

  const [followedResult, reexecuteFollowed] = useQuery<TasksResponse>({
    query: TRIAGE_TASKS_QUERY,
    variables: { trackingState: ['FOLLOWED'] },
  });

  const [, setTrackingState] = useMutation(SET_TRACKING_STATE);
  const [, setTrackingStateBatch] = useMutation(SET_TRACKING_STATE_BATCH);

  const refetch = () => {
    reexecuteInbox({ requestPolicy: 'network-only' });
    reexecuteFollowed({ requestPolicy: 'network-only' });
  };

  const followTask = async (taskId: string) => {
    await setTrackingState({ taskId, state: 'FOLLOWED' });
    refetch();
  };

  const dismissTask = async (taskId: string) => {
    await setTrackingState({ taskId, state: 'DISMISSED' });
    refetch();
  };

  const unfollowTask = async (taskId: string) => {
    await setTrackingState({ taskId, state: 'INBOX' });
    refetch();
  };

  const followAll = async (taskIds: string[]) => {
    await setTrackingStateBatch({ taskIds, state: 'FOLLOWED' });
    refetch();
  };

  const inboxTasks = inboxResult.data?.tasks.edges.map(e => e.node) ?? [];
  const followedTasks = followedResult.data?.tasks.edges.map(e => e.node) ?? [];

  return {
    inboxTasks,
    followedTasks,
    inboxCount: inboxResult.data?.tasks.totalCount ?? 0,
    followedCount: followedResult.data?.tasks.totalCount ?? 0,
    loading: inboxResult.fetching || followedResult.fetching,
    error: inboxResult.error ?? followedResult.error ?? null,
    followTask,
    dismissTask,
    unfollowTask,
    followAll,
    refetch,
  };
}
