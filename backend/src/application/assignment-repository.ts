import type { Assignment, EntityId } from '@aggregated-plan/shared-types';

export type AssignmentRepository = {
  readonly list: () => Promise<readonly Assignment[]>;
  readonly listByDeveloper: (developerId: EntityId) => Promise<readonly Assignment[]>;
  readonly save: (assignment: Assignment) => Promise<Assignment>;
  readonly saveMany: (assignments: readonly Assignment[]) => Promise<readonly Assignment[]>;
};
