import type { EntityId, Task } from '@aggregated-plan/shared-types';

export type TaskRepository = {
  readonly list: () => Promise<readonly Task[]>;
  readonly getById: (id: EntityId) => Promise<Task | null>;
  readonly save: (task: Task) => Promise<Task>;
  readonly update: (task: Task) => Promise<Task>;
  readonly remove: (id: EntityId) => Promise<void>;
};
