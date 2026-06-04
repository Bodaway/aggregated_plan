// frontend/src/hooks/use-create-recurring-task.ts
import { useMutation } from 'urql';

const CREATE_RECURRING_TASK_MUTATION = `
  mutation CreateRecurringTask($input: CreateRecurringTaskInput!) {
    createRecurringTask(input: $input) {
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

export interface RecurringTaskRuleInput {
  kind: string;
  interval: number;
  weekdays?: string[];
  dayOfMonth?: number;
  week?: string;
  weekday?: string;
}

export interface CreateRecurringTaskInput {
  title: string;
  description?: string;
  notes?: string;
  urgency: string;
  impact: string;
  estimatedHours?: number;
  projectId?: string;
  tagIds?: string[];
  rule: RecurringTaskRuleInput;
  startsOn: string;     // ISO date "YYYY-MM-DD"
  endsOn?: string;      // ISO date "YYYY-MM-DD"
  maxOccurrences?: number;
}

export function useCreateRecurringTask() {
  const [result, execute] = useMutation(CREATE_RECURRING_TASK_MUTATION);

  const createRecurringTask = (input: CreateRecurringTaskInput) =>
    execute({ input });

  return {
    createRecurringTask,
    loading: result.fetching,
    error: result.error ?? null,
  };
}
