import type {
  Assignment,
  Conflict,
  Developer,
  HalfDay,
  IsoDateString,
  Milestone,
  MilestoneType,
  Project,
  ProjectPriority,
  ProjectStatus,
  Weekday,
  WeeklyAllocation,
} from '@aggregated-plan/shared-types';

const API_BASE_URL =
  import.meta.env.VITE_API_BASE_URL?.toString() ?? 'http://localhost:3001';

const requestJson = async <T>(
  path: string,
  options?: RequestInit,
): Promise<T> => {
  const response = await fetch(`${API_BASE_URL}${path}`, {
    headers: {
      'Content-Type': 'application/json',
      ...(options?.headers ?? {}),
    },
    ...options,
  });

  if (!response.ok) {
    const errorPayload = await response.json().catch(() => null);
    const message =
      typeof errorPayload === 'object' &&
      errorPayload !== null &&
      'error' in errorPayload
        ? JSON.stringify(errorPayload.error)
        : 'Request failed';
    throw new Error(message);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
};

export type CreateProjectInput = {
  readonly name: string;
  readonly description?: string;
  readonly startDate: IsoDateString;
  readonly endDate: IsoDateString;
  readonly status?: ProjectStatus;
  readonly teamIds?: readonly string[];
  readonly client?: string;
  readonly priority?: ProjectPriority;
  readonly createdBy: string;
};

export const fetchProjects = async (): Promise<readonly Project[]> =>
  requestJson<readonly Project[]>('/projects');

export const createProject = async (
  input: CreateProjectInput,
): Promise<Project> =>
  requestJson<Project>('/projects', {
    method: 'POST',
    body: JSON.stringify(input),
  });

export const fetchMilestones = async (): Promise<readonly Milestone[]> =>
  requestJson<readonly Milestone[]>('/milestones');

export type CreateMilestoneInput = {
  readonly projectId: string;
  readonly name: string;
  readonly date: IsoDateString;
  readonly type?: MilestoneType;
};

export const createMilestone = async (
  input: CreateMilestoneInput,
): Promise<Milestone> =>
  requestJson<Milestone>(`/projects/${input.projectId}/milestones`, {
    method: 'POST',
    body: JSON.stringify({
      name: input.name,
      date: input.date,
      type: input.type,
    }),
  });

export const fetchDevelopers = async (): Promise<readonly Developer[]> =>
  requestJson<readonly Developer[]>('/developers');

export type CreateDeveloperInput = {
  readonly displayName: string;
  readonly email: string;
  readonly capacityHalfDaysPerWeek?: number;
};

export const createDeveloper = async (
  input: CreateDeveloperInput,
): Promise<Developer> =>
  requestJson<Developer>('/developers', {
    method: 'POST',
    body: JSON.stringify(input),
  });

export type UpdateDeveloperInput = {
  readonly displayName?: string;
  readonly email?: string;
  readonly capacityHalfDaysPerWeek?: number;
};

export const updateDeveloper = async (
  id: string,
  input: UpdateDeveloperInput,
): Promise<Developer> =>
  requestJson<Developer>(`/developers/${id}`, {
    method: 'PUT',
    body: JSON.stringify(input),
  });

export type CreateAssignmentInput = {
  readonly projectId: string;
  readonly developerId: string;
  readonly date: IsoDateString;
  readonly halfDay: HalfDay;
};

export const createAssignment = async (
  input: CreateAssignmentInput,
): Promise<Assignment> =>
  requestJson<Assignment>('/assignments', {
    method: 'POST',
    body: JSON.stringify(input),
  });

export const fetchAssignments = async (): Promise<readonly Assignment[]> =>
  requestJson<readonly Assignment[]>('/assignments');

export type CreateWeeklyAllocationInput = {
  readonly projectId: string;
  readonly developerId: string;
  readonly startDate: IsoDateString;
  readonly endDate: IsoDateString;
  readonly halfDaysPerWeek: number;
  readonly preferredWeekdays?: readonly Weekday[];
};

export const createWeeklyAllocation = async (
  input: CreateWeeklyAllocationInput,
): Promise<WeeklyAllocation> =>
  requestJson<WeeklyAllocation>('/allocations', {
    method: 'POST',
    body: JSON.stringify(input),
  });

export const fetchConflicts = async (): Promise<readonly Conflict[]> =>
  requestJson<readonly Conflict[]>('/conflicts');
