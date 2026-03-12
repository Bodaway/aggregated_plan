import { useQuery } from 'urql';

const URGENCY_NUM: Record<string, number> = { LOW: 1, MEDIUM: 2, HIGH: 3, CRITICAL: 4 };
const IMPACT_NUM: Record<string, number> = { LOW: 1, MEDIUM: 2, HIGH: 3, CRITICAL: 4 };

function toNum(map: Record<string, number>, v: unknown): number {
  if (typeof v === 'number') return v;
  return map[v as string] ?? 1;
}

interface DashboardTag {
  readonly id: string;
  readonly name: string;
  readonly color: string | null;
}

interface DashboardProject {
  readonly name: string;
}

export interface DashboardTask {
  readonly id: string;
  readonly title: string;
  readonly source: string;
  readonly sourceId: string | null;
  readonly trackingState: string;
  readonly status: string;
  readonly jiraStatus: string | null;
  readonly urgency: number;
  readonly impact: number;
  readonly quadrant: string;
  readonly deadline: string | null;
  readonly plannedStart: string | null;
  readonly assignee: string | null;
  readonly project: DashboardProject | null;
  readonly tags: readonly DashboardTag[];
  readonly effectiveRemainingHours: number | null;
  readonly effectiveEstimatedHours: number | null;
  readonly jiraTimeSpentSeconds: number | null;
}

export interface DashboardMeeting {
  readonly id: string;
  readonly title: string;
  readonly startTime: string;
  readonly endTime: string;
  readonly location: string | null;
  readonly durationHours: number;
  readonly showAs: string | null;
}

export interface DashboardAlert {
  readonly id: string;
  readonly alertType: string;
  readonly severity: string;
  readonly message: string;
  readonly resolved: boolean;
  readonly createdAt: string;
}

interface HalfDayMeeting {
  readonly title: string;
  readonly startTime: string;
  readonly endTime: string;
}

interface HalfDayTask {
  readonly title: string;
}

export interface WorkloadHalfDay {
  readonly date: string;
  readonly halfDay: string;
  readonly consumption: number;
  readonly isFree: boolean;
  readonly meetings: readonly HalfDayMeeting[];
  readonly tasks: readonly HalfDayTask[];
}

export interface WeeklyWorkloadData {
  readonly weekStart: string;
  readonly capacity: number;
  readonly totalPlanned: number;
  readonly totalMeetings: number;
  readonly overload: boolean;
  readonly workingDays: readonly number[];
  readonly halfDays: readonly WorkloadHalfDay[];
}

export interface SyncStatus {
  readonly source: string;
  readonly status: string;
  readonly lastSyncAt: string | null;
  readonly errorMessage: string | null;
}

export interface DailyDashboardData {
  readonly date: string;
  readonly tasks: readonly DashboardTask[];
  readonly meetings: readonly DashboardMeeting[];
  readonly alerts: readonly DashboardAlert[];
  readonly weeklyWorkload: WeeklyWorkloadData | null;
  readonly syncStatuses: readonly SyncStatus[];
  readonly workingHoursPerDay: number;
  readonly workingDays: readonly number[];
}

const DASHBOARD_QUERY = `
  query DailyDashboard($date: String!) {
    dailyDashboard(date: $date) {
      date
      tasks {
        id
        title
        source
        sourceId
        trackingState
        status
        jiraStatus
        urgency
        impact
        quadrant
        deadline
        plannedStart
        assignee
        project { name }
        tags { id name color }
        effectiveRemainingHours
        effectiveEstimatedHours
        jiraTimeSpentSeconds
      }
      meetings {
        id
        title
        startTime
        endTime
        location
        durationHours
        showAs
      }
      workingHoursPerDay
      workingDays
      alerts {
        id
        alertType
        severity
        message
        resolved
        createdAt
      }
      weeklyWorkload {
        weekStart
        capacity
        totalPlanned
        totalMeetings
        overload
        workingDays
        halfDays {
          date
          halfDay
          consumption
          isFree
          meetings { title startTime endTime }
          tasks { title }
        }
      }
      syncStatuses {
        source
        status
        lastSyncAt
        errorMessage
      }
    }
  }
`;

export function useDashboard(date: string) {
  const [result, reexecute] = useQuery<{ dailyDashboard: DailyDashboardData }>({
    query: DASHBOARD_QUERY,
    variables: { date },
    requestPolicy: 'cache-and-network',
  });

  const raw = result.data?.dailyDashboard ?? null;
  const data = raw
    ? {
        ...raw,
        tasks: raw.tasks.map(t => ({
          ...t,
          urgency: toNum(URGENCY_NUM, t.urgency),
          impact: toNum(IMPACT_NUM, t.impact),
        })),
      }
    : null;

  return {
    data,
    loading: result.fetching,
    error: result.error ?? null,
    refetch: () => reexecute({ requestPolicy: 'network-only' }),
  };
}
