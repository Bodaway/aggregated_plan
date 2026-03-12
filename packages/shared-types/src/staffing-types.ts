import type { HalfDay, IsoDateString, Weekday } from './time-types';
import type { EntityId } from './user-types';

export type Developer = {
  readonly id: EntityId;
  readonly displayName: string;
  readonly email: string;
  readonly capacityHalfDaysPerWeek: number;
};

export type Assignment = {
  readonly id: EntityId;
  readonly projectId: EntityId;
  readonly developerId: EntityId;
  readonly date: IsoDateString;
  readonly halfDay: HalfDay;
  readonly createdAt: IsoDateString;
};

export type WeeklyAllocation = {
  readonly id: EntityId;
  readonly projectId: EntityId;
  readonly developerId: EntityId;
  readonly startDate: IsoDateString;
  readonly endDate: IsoDateString;
  readonly halfDaysPerWeek: number;
  readonly preferredWeekdays?: readonly Weekday[];
  readonly createdAt: IsoDateString;
};

export type AvailabilityType =
  | 'leave'
  | 'training'
  | 'unavailable'
  | 'other';

export type Availability = {
  readonly id: EntityId;
  readonly developerId: EntityId;
  readonly startDate: IsoDateString;
  readonly endDate: IsoDateString;
  readonly type: AvailabilityType;
  readonly description?: string;
  readonly createdAt: IsoDateString;
};
