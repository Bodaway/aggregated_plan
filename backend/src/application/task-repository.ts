import type { EntityId, Task } from '@aggregated-plan/shared-types';

export type TaskRepository = {
  readonly list: () => Promise<readonly Task[]>;
  readonly listByProject: (projectId: EntityId) => Promise<readonly Task[]>;
  readonly getById: (id: EntityId) => Promise<Task | null>;
  readonly getByName: (name: string) => Promise<Task | null>;
  readonly save: (task: Task) => Promise<Task>;
  readonly saveMany: (tasks: readonly Task[]) => Promise<readonly Task[]>;
};
