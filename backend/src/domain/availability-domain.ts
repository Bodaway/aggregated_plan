import type { Availability, AvailabilityType, EntityId, IsoDateString } from '@aggregated-plan/shared-types';
import { compareIsoDates } from '@aggregated-plan/shared-utils';
import { createDomainError } from './domain-errors';
import type { DomainError } from './domain-errors';
import { err, ok } from './result';
import type { Result } from './result';

export type CreateAvailabilityParams = {
  readonly developerId: EntityId;
  readonly startDate: IsoDateString;
  readonly endDate: IsoDateString;
  readonly type: AvailabilityType;
  readonly description?: string;
};

export type AvailabilityContext = {
  readonly id: EntityId;
  readonly createdAt: IsoDateString;
};

export const createAvailability = (
  params: CreateAvailabilityParams,
  context: AvailabilityContext,
): Result<Availability, DomainError> => {
  if (compareIsoDates(params.startDate, params.endDate) > 0) {
    return err(
      createDomainError('invalid-date-range', 'Start date must be before end date.'),
    );
  }

  return ok({
    id: context.id,
    developerId: params.developerId,
    startDate: params.startDate,
    endDate: params.endDate,
    type: params.type,
    description: params.description,
    createdAt: context.createdAt,
  });
};
