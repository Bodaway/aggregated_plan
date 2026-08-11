import { useCallback } from 'react';
import { useMutation } from 'urql';

/** The selection is denormalised onto the task at assign time, so the mutation
 *  echoes the resolved Gryzzly task back — that is what refreshes the trigger
 *  label without a manual refetch. */
const ASSIGN_GRYZZLY_TASK = `
  mutation AssignGryzzlyTask($taskId: ID!, $gryzzlyTaskId: ID) {
    assignGryzzlyTask(taskId: $taskId, gryzzlyTaskId: $gryzzlyTaskId) {
      id
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

/** Assign / clear the Gryzzly task of one plan task.
 *
 * Shared by every surface that lets the user pick one (the edit sheet's
 * full-width picker, the dashboard card's chip) so the mutation document and
 * its selection set exist once. */
export function useAssignGryzzlyTask(taskId: string) {
  const [, executeAssign] = useMutation(ASSIGN_GRYZZLY_TASK);

  const assign = useCallback(
    async (gryzzlyTaskId: string) => {
      await executeAssign({ taskId, gryzzlyTaskId });
    },
    [taskId, executeAssign],
  );

  const clear = useCallback(async () => {
    await executeAssign({ taskId, gryzzlyTaskId: null });
  }, [taskId, executeAssign]);

  return { assign, clear };
}
