import { useQuery } from 'urql';

interface WorkloadMeeting {
  readonly title: string;
  readonly startTime: string;
  readonly endTime: string;
  readonly durationHours: number;
}

interface WorkloadTask {
  readonly title: string;
  readonly estimatedHours: number | null;
}

export interface WorkloadHalfDay {
  readonly date: string;
  readonly halfDay: string;
  readonly consumption: number;
  readonly isFree: boolean;
  readonly meetings: readonly WorkloadMeeting[];
  readonly tasks: readonly WorkloadTask[];
}

export interface WeeklyWorkloadData {
  readonly weekStart: string;
  readonly capacity: number;
  readonly totalPlanned: number;
  readonly totalMeetings: number;
  readonly overload: boolean;
  readonly halfDays: readonly WorkloadHalfDay[];
}

const WORKLOAD_QUERY = `
  query WeeklyWorkload($weekStart: String!) {
    weeklyWorkload(weekStart: $weekStart) {
      weekStart
      capacity
      totalPlanned
      totalMeetings
      overload
      halfDays {
        date
        halfDay
        consumption
        isFree
        meetings { title startTime endTime durationHours }
        tasks { title estimatedHours }
      }
    }
  }
`;

export function useWorkload(weekStart: string) {
  const [result] = useQuery<{ weeklyWorkload: WeeklyWorkloadData }>({
    query: WORKLOAD_QUERY,
    variables: { weekStart },
  });

  return {
    data: result.data?.weeklyWorkload ?? null,
    loading: result.fetching,
    error: result.error ?? null,
  };
}
