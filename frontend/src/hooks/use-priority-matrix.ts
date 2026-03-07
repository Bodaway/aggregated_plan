import { useQuery, useMutation } from 'urql';

interface MatrixProject {
  readonly name: string;
}

export interface MatrixTask {
  readonly id: string;
  readonly title: string;
  readonly status: string;
  readonly urgency: number;
  readonly impact: number;
  readonly deadline: string | null;
  readonly assignee: string | null;
  readonly project: MatrixProject | null;
}

export interface PriorityMatrixData {
  readonly urgentImportant: readonly MatrixTask[];
  readonly important: readonly MatrixTask[];
  readonly urgent: readonly MatrixTask[];
  readonly neither: readonly MatrixTask[];
}

interface UpdatePriorityResult {
  readonly updatePriority: {
    readonly id: string;
    readonly urgency: number;
    readonly impact: number;
    readonly quadrant: string;
  };
}

const PRIORITY_MATRIX_QUERY = `
  query PriorityMatrix {
    priorityMatrix {
      urgentImportant {
        id title status urgency impact deadline assignee
        project { name }
      }
      important {
        id title status urgency impact deadline assignee
        project { name }
      }
      urgent {
        id title status urgency impact deadline assignee
        project { name }
      }
      neither {
        id title status urgency impact deadline assignee
        project { name }
      }
    }
  }
`;

const UPDATE_PRIORITY_MUTATION = `
  mutation UpdatePriority($taskId: ID!, $urgency: Int, $impact: Int) {
    updatePriority(taskId: $taskId, urgency: $urgency, impact: $impact) {
      id urgency impact quadrant
    }
  }
`;

export type QuadrantKey = 'urgentImportant' | 'important' | 'urgent' | 'neither';

/** Maps quadrant keys to the urgency/impact values that define them. */
const QUADRANT_VALUES: Record<QuadrantKey, { urgency: number; impact: number }> = {
  urgentImportant: { urgency: 3, impact: 3 },
  important: { urgency: 1, impact: 3 },
  urgent: { urgency: 3, impact: 1 },
  neither: { urgency: 1, impact: 1 },
};

export function usePriorityMatrix() {
  const [result, reexecute] = useQuery<{ priorityMatrix: PriorityMatrixData }>({
    query: PRIORITY_MATRIX_QUERY,
  });

  const [, executeMutation] = useMutation<UpdatePriorityResult>(UPDATE_PRIORITY_MUTATION);

  const updatePriority = async (taskId: string, targetQuadrant: QuadrantKey) => {
    const values = QUADRANT_VALUES[targetQuadrant];
    await executeMutation({
      taskId,
      urgency: values.urgency,
      impact: values.impact,
    });
    reexecute({ requestPolicy: 'network-only' });
  };

  return {
    data: result.data?.priorityMatrix ?? null,
    loading: result.fetching,
    error: result.error ?? null,
    updatePriority,
  };
}
