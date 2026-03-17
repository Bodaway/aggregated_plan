import { useQuery } from 'urql';

export interface DailyActivityTotal {
  readonly date: string;
  readonly totalHours: number;
}

export interface TaskActivitySummary {
  readonly taskId: string | null;
  readonly taskTitle: string | null;
  readonly sourceId: string | null;
  readonly totalHours: number;
  readonly dailyHours: readonly number[];
}

export interface WeeklyActivitySummary {
  readonly weekStart: string;
  readonly weekEnd: string;
  readonly totalHours: number;
  readonly dailyTotals: readonly DailyActivityTotal[];
  readonly taskBreakdown: readonly TaskActivitySummary[];
}

const WEEKLY_ACTIVITY_QUERY = `
  query WeeklyActivitySummary($weekStart: NaiveDate!) {
    weeklyActivitySummary(weekStart: $weekStart) {
      weekStart
      weekEnd
      totalHours
      dailyTotals {
        date
        totalHours
      }
      taskBreakdown {
        taskId
        taskTitle
        sourceId
        totalHours
        dailyHours
      }
    }
  }
`;

interface WeeklyActivityData {
  readonly weeklyActivitySummary: WeeklyActivitySummary;
}

export function useWeeklyActivity(weekStart: string) {
  const [result] = useQuery<WeeklyActivityData>({
    query: WEEKLY_ACTIVITY_QUERY,
    variables: { weekStart },
  });

  return {
    summary: result.data?.weeklyActivitySummary ?? null,
    loading: result.fetching,
    error: result.error ?? null,
  };
}
