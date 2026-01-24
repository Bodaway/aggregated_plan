import type { EntityId, IsoDateString, Milestone, MilestoneType } from '@aggregated-plan/shared-types';
import { createDomainError } from './domain-errors';
import type { DomainError } from './domain-errors';
import { err, ok } from './result';
import type { Result } from './result';

export type CreateMilestoneParams = {
  readonly projectId: EntityId;
  readonly name: string;
  readonly date: IsoDateString;
  readonly type?: MilestoneType;
};

export type MilestoneContext = {
  readonly id: EntityId;
  readonly now: IsoDateString;
};

const ensureValidName = (name: string): Result<string, DomainError> => {
  if (name.trim().length === 0) {
    return err(createDomainError('invalid-name', 'Milestone name is required.'));
  }
  return ok(name.trim());
};

export const createMilestone = (
  params: CreateMilestoneParams,
  context: MilestoneContext,
): Result<Milestone, DomainError> => {
  const nameResult = ensureValidName(params.name);
  if (!nameResult.ok) {
    return nameResult;
  }

  const milestone: Milestone = {
    id: context.id,
    projectId: params.projectId,
    name: nameResult.value,
    date: params.date,
    type: params.type ?? 'other',
    createdAt: context.now,
    updatedAt: context.now,
  };

  return ok(milestone);
};
