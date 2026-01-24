import type { EntityId, WeeklyAllocation } from '@aggregated-plan/shared-types';

export type AllocationRepository = {
  readonly list: () => Promise<readonly WeeklyAllocation[]>;
  readonly listByDeveloper: (developerId: EntityId) => Promise<readonly WeeklyAllocation[]>;
  readonly save: (allocation: WeeklyAllocation) => Promise<WeeklyAllocation>;
};
