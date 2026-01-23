import type { EntityId, Project } from '@aggregated-plan/shared-types';

export type ProjectRepository = {
  readonly list: () => Promise<readonly Project[]>;
  readonly getById: (id: EntityId) => Promise<Project | null>;
  readonly getByName: (name: string) => Promise<Project | null>;
  readonly save: (project: Project) => Promise<Project>;
  readonly update: (project: Project) => Promise<Project>;
  readonly remove: (id: EntityId) => Promise<void>;
};
