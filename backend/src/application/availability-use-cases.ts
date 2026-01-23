import type { Availability } from '@aggregated-plan/shared-types';
import { createAvailability, ok } from '@domain/index';
import type { CreateAvailabilityParams, DomainError, Result } from '@domain/index';
import type { AvailabilityRepository } from './availability-repository';
import type { Clock, IdProvider } from './providers';

export type AvailabilityUseCases = {
  readonly createAvailability: (
    params: CreateAvailabilityParams,
  ) => Promise<Result<Availability, DomainError>>;
  readonly listAvailabilities: () => Promise<readonly Availability[]>;
};

export const createAvailabilityUseCases = (deps: {
  readonly availabilityRepository: AvailabilityRepository;
  readonly idProvider: IdProvider;
  readonly clock: Clock;
}): AvailabilityUseCases => {
  const createAvailabilityHandler = async (
    params: CreateAvailabilityParams,
  ): Promise<Result<Availability, DomainError>> => {
    const result = createAvailability(params, {
      id: deps.idProvider(),
      createdAt: deps.clock(),
    });
    if (!result.ok) {
      return result;
    }
    const saved = await deps.availabilityRepository.save(result.value);
    return ok(saved);
  };

  return {
    createAvailability: createAvailabilityHandler,
    listAvailabilities: deps.availabilityRepository.list,
  };
};
