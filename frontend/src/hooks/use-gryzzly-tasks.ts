import { useQuery } from 'urql';
import { GryzzlyOption } from '@/lib/gryzzly-picker-options';

interface GryzzlyTasksData {
  gryzzlyTasks: GryzzlyOption[];
}

const GRYZZLY_TASKS_QUERY = `
  query GryzzlyTasks($search: String, $projectFilter: String, $limit: Int) {
    gryzzlyTasks(search: $search, projectFilter: $projectFilter, limit: $limit) {
      gryzzlyTaskId
      name
      projectName
      projectStatus
    }
  }
`;

export function useGryzzlyTasks(search?: string) {
  const [result] = useQuery<GryzzlyTasksData>({
    query: GRYZZLY_TASKS_QUERY,
    variables: { search: search ?? null, limit: 100 },
  });

  return {
    options: result.data?.gryzzlyTasks ?? [],
    fetching: result.fetching,
    error: result.error ?? null,
  };
}
