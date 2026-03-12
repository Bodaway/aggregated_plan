import type {
  EntityId,
  IsoDateString,
  Project,
  ProjectPriority,
  ProjectStatus,
} from '@aggregated-plan/shared-types';
import { compareIsoDates } from '@aggregated-plan/shared-utils';
import { createDomainError } from './domain-errors';
import type { DomainError } from './domain-errors';
import { err, ok } from './result';
import type { Result } from './result';

export type CreateProjectParams = {
  readonly name: string;
  readonly description?: string;
  readonly startDate: IsoDateString;
  readonly endDate: IsoDateString;
  readonly status?: ProjectStatus;
  readonly teamIds?: readonly EntityId[];
  readonly client?: string;
  readonly priority?: ProjectPriority;
  readonly createdBy: EntityId;
};

export type ProjectContext = {
  readonly id: EntityId;
  readonly now: IsoDateString;
};

export type UpdateProjectParams = {
  readonly name?: string;
  readonly description?: string;
  readonly startDate?: IsoDateString;
  readonly endDate?: IsoDateString;
  readonly status?: ProjectStatus;
  readonly teamIds?: readonly EntityId[];
  readonly client?: string;
  readonly priority?: ProjectPriority;
};

const ensureValidName = (name: string): Result<string, DomainError> => {
  if (name.trim().length === 0) {
    return err(createDomainError('invalid-name', 'Project name is required.'));
  }
  return ok(name.trim());
};

const ensureValidDateRange = (
  startDate: IsoDateString,
  endDate: IsoDateString,
): Result<null, DomainError> => {
  if (compareIsoDates(startDate, endDate) > 0) {
    return err(
      createDomainError('invalid-date-range', 'Start date must be before end date.'),
    );
  }
  return ok(null);
};

export const createProject = (
  params: CreateProjectParams,
  context: ProjectContext,
): Result<Project, DomainError> => {
  const nameResult = ensureValidName(params.name);
  if (!nameResult.ok) {
    return nameResult;
  }

  const rangeResult = ensureValidDateRange(params.startDate, params.endDate);
  if (!rangeResult.ok) {
    return rangeResult;
  }

  const project: Project = {
    id: context.id,
    name: nameResult.value,
    description: params.description,
    startDate: params.startDate,
    endDate: params.endDate,
    status: params.status ?? 'planning',
    teamIds: params.teamIds ?? [],
    client: params.client,
    priority: params.priority,
    createdAt: context.now,
    updatedAt: context.now,
    createdBy: params.createdBy,
  };

  return ok(project);
};

export const updateProject = (
  project: Project,
  updates: UpdateProjectParams,
  context: { readonly now: IsoDateString },
): Result<Project, DomainError> => {
  const nameResult = updates.name ? ensureValidName(updates.name) : ok(project.name);
  if (!nameResult.ok) {
    return nameResult;
  }

  const startDate = updates.startDate ?? project.startDate;
  const endDate = updates.endDate ?? project.endDate;
  const rangeResult = ensureValidDateRange(startDate, endDate);
  if (!rangeResult.ok) {
    return rangeResult;
  }

  const updatedProject: Project = {
    ...project,
    name: nameResult.value,
    description: updates.description ?? project.description,
    startDate,
    endDate,
    status: updates.status ?? project.status,
    teamIds: updates.teamIds ?? project.teamIds,
    client: updates.client ?? project.client,
    priority: updates.priority ?? project.priority,
    updatedAt: context.now,
  };

  return ok(updatedProject);
};
