import { useQuery } from 'urql';

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
  readonly assignee: string | null;
  readonly project: DashboardProject | null;
  readonly tags: readonly DashboardTag[];
}

export interface DashboardMeeting {
  readonly id: string;
  readonly title: string;
  readonly startTime: string;
  readonly endTime: string;
  readonly location: string | null;
  readonly durationHours: number;
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
        assignee
        project { name }
        tags { id name color }
      }
      meetings {
        id
        title
        startTime
        endTime
        location
        durationHours
      }
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
  const [result] = useQuery<{ dailyDashboard: DailyDashboardData }>({
    query: DASHBOARD_QUERY,
    variables: { date },
  });

  return {
    data: result.data?.dailyDashboard ?? null,
    loading: result.fetching,
    error: result.error ?? null,
  };
}
