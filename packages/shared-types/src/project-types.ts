import type { DateRange, IsoDateString } from './time-types';
import type { EntityId } from './user-types';

export type ProjectStatus =
  | 'planning'
  | 'active'
  | 'paused'
  | 'completed'
  | 'cancelled';

export type ProjectPriority = 'high' | 'medium' | 'low';

export type Project = {
  readonly id: EntityId;
  readonly name: string;
  readonly description?: string;
  readonly startDate: IsoDateString;
  readonly endDate: IsoDateString;
  readonly status: ProjectStatus;
  readonly teamIds: readonly EntityId[];
  readonly client?: string;
  readonly priority?: ProjectPriority;
  readonly createdAt: IsoDateString;
  readonly updatedAt: IsoDateString;
  readonly createdBy: EntityId;
};

export type TaskStatus = 'todo' | 'in-progress' | 'done';

export type TaskType =
  | 'development'
  | 'test'
  | 'documentation'
  | 'meeting'
  | 'other';

export type Task = {
  readonly id: EntityId;
  readonly projectId: EntityId;
  readonly name: string;
  readonly status: TaskStatus;
  readonly type: TaskType;
  readonly estimateHalfDays?: number;
  readonly dateRange: DateRange;
  readonly assigneeIds: readonly EntityId[];
  readonly createdAt: IsoDateString;
  readonly updatedAt: IsoDateString;
};

export type MilestoneType = 'delivery' | 'review' | 'demo' | 'other';

export type Milestone = {
  readonly id: EntityId;
  readonly projectId: EntityId;
  readonly name: string;
  readonly date: IsoDateString;
  readonly type: MilestoneType;
  readonly createdAt: IsoDateString;
  readonly updatedAt: IsoDateString;
};
