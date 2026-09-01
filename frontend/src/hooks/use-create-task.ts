// frontend/src/hooks/use-create-task.ts
import { useMutation } from 'urql';

const CREATE_TASK_MUTATION = `
  mutation CreateInternalTask($input: CreateTaskInput!) {
    createTask(input: $input) {
      id
      title
      plannedStart
      deadline
      status
      urgency
      impact
      quadrant
    }
  }
`;

export interface NewTaskInput {
  title: string;
  plannedStart?: string;   // ISO 8601 e.g. "2026-03-12T08:00:00Z"
  deadline?: string;       // plain date "YYYY-MM-DD"; omit to leave it unset
  estimatedHours?: number;
  urgency?: string;        // "LOW" | "MEDIUM" | "HIGH" | "CRITICAL"
  impact?: string;
  description?: string;
  notes?: string;
}

export function useCreateTask() {
  const [result, execute] = useMutation(CREATE_TASK_MUTATION);

  const createTask = (input: NewTaskInput) =>
    execute({ input });

  return {
    createTask,
    loading: result.fetching,
    error: result.error ?? null,
  };
}
