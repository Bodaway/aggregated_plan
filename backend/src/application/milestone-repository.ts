import type { EntityId, Milestone } from '@aggregated-plan/shared-types';

export type MilestoneRepository = {
  readonly list: () => Promise<readonly Milestone[]>;
  readonly listByProject: (projectId: EntityId) => Promise<readonly Milestone[]>;
  readonly save: (milestone: Milestone) => Promise<Milestone>;
};
