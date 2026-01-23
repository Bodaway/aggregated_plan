import type { Developer, EntityId } from '@aggregated-plan/shared-types';

export type DeveloperRepository = {
  readonly list: () => Promise<readonly Developer[]>;
  readonly getById: (id: EntityId) => Promise<Developer | null>;
  readonly save: (developer: Developer) => Promise<Developer>;
};
