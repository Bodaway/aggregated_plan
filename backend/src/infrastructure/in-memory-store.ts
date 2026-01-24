import type {
  Assignment,
  Availability,
  Developer,
  Milestone,
  Project,
  WeeklyAllocation,
} from '@aggregated-plan/shared-types';

export type InMemoryStore = {
  projects: Project[];
  milestones: Milestone[];
  assignments: Assignment[];
  allocations: WeeklyAllocation[];
  availabilities: Availability[];
  developers: Developer[];
};

export const createInMemoryStore = (seed?: Partial<InMemoryStore>): InMemoryStore => ({
  projects: seed?.projects ? [...seed.projects] : [],
  milestones: seed?.milestones ? [...seed.milestones] : [],
  assignments: seed?.assignments ? [...seed.assignments] : [],
  allocations: seed?.allocations ? [...seed.allocations] : [],
  availabilities: seed?.availabilities ? [...seed.availabilities] : [],
  developers: seed?.developers ? [...seed.developers] : [],
});
