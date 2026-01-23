import type {
  Assignment,
  Availability,
  Developer,
  Project,
  WeeklyAllocation,
} from '@aggregated-plan/shared-types';

export type InMemoryStore = {
  projects: Project[];
  assignments: Assignment[];
  allocations: WeeklyAllocation[];
  availabilities: Availability[];
  developers: Developer[];
};

export const createInMemoryStore = (seed?: Partial<InMemoryStore>): InMemoryStore => ({
  projects: seed?.projects ? [...seed.projects] : [],
  assignments: seed?.assignments ? [...seed.assignments] : [],
  allocations: seed?.allocations ? [...seed.allocations] : [],
  availabilities: seed?.availabilities ? [...seed.availabilities] : [],
  developers: seed?.developers ? [...seed.developers] : [],
});
