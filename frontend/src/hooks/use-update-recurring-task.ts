import { useMutation } from 'urql';

const UPDATE_RECURRING_TASK_MUTATION = `
  mutation UpdateRecurringTask($id: ID!, $input: UpdateRecurringTaskInput!) {
    updateRecurringTask(id: $id, input: $input) {
      id
      title
      rule {
        kind
        interval
        weekdays
        dayOfMonth
        week
        weekday
      }
      startsOn
      endsOn
      maxOccurrences
      active
    }
  }
`;

export interface UpdateRecurringTaskInput {
  title?: string;
  description?: string | null;
  urgency?: string;
  impact?: string;
  estimatedHours?: number | null;
  projectId?: string | null;
  tagIds?: string[];
}

export function useUpdateRecurringTask() {
  const [result, execute] = useMutation(UPDATE_RECURRING_TASK_MUTATION);

  const updateRecurringTask = (id: string, input: UpdateRecurringTaskInput) =>
    execute({ id, input });

  return {
    updateRecurringTask,
    loading: result.fetching,
    error: result.error ?? null,
  };
}
