import type { Availability, EntityId } from '@aggregated-plan/shared-types';

export type AvailabilityRepository = {
  readonly list: () => Promise<readonly Availability[]>;
  readonly listByDeveloper: (developerId: EntityId) => Promise<readonly Availability[]>;
  readonly save: (availability: Availability) => Promise<Availability>;
};
