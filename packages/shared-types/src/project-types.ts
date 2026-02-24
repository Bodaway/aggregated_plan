import type { IsoDateString } from './time-types';
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

export type TaskPriority = 'high' | 'medium' | 'low';

export type Task = {
  readonly id: EntityId;
  readonly projectId?: EntityId;
  readonly name: string;
  readonly description?: string;
  readonly status: TaskStatus;
  readonly priority: TaskPriority;
  readonly dueDate?: IsoDateString;
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
