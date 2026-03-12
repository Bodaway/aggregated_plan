import type { EntityId } from './user-types';
import type { HalfDay, IsoDateString } from './time-types';

export type ConflictType = 'overlap' | 'capacity' | 'unavailability';

export type Conflict = {
  readonly type: ConflictType;
  readonly developerId: EntityId;
  readonly message: string;
  readonly dates: readonly IsoDateString[];
  readonly halfDay?: HalfDay;
  readonly projectIds?: readonly EntityId[];
  readonly weekStart?: IsoDateString;
  readonly assignedHalfDays?: number;
  readonly capacityHalfDays?: number;
};
