import type { Developer } from '@aggregated-plan/shared-types';
import { createDomainError, err, ok } from '@domain/index';
import type { DomainError, Result } from '@domain/index';
import type { DeveloperRepository } from './developer-repository';
import type { IdProvider } from './providers';

export type CreateDeveloperParams = {
  readonly displayName: string;
  readonly email: string;
  readonly capacityHalfDaysPerWeek?: number;
};

export type DeveloperUseCases = {
  readonly createDeveloper: (
    params: CreateDeveloperParams,
  ) => Promise<Result<Developer, DomainError>>;
  readonly listDevelopers: () => Promise<readonly Developer[]>;
  readonly getDeveloper: (id: string) => Promise<Developer | null>;
};

const ensureValidDeveloper = (
  params: CreateDeveloperParams,
): Result<{ readonly name: string; readonly email: string; readonly capacity: number }, DomainError> => {
  const name = params.displayName.trim();
  if (name.length === 0) {
    return err(createDomainError('invalid-name', 'Developer name is required.'));
  }
  const email = params.email.trim();
  if (!email.includes('@')) {
    return err(createDomainError('invalid-name', 'Developer email must be valid.'));
  }
  const capacity = params.capacityHalfDaysPerWeek ?? 10;
  if (capacity < 1 || capacity > 10) {
    return err(
      createDomainError('invalid-capacity', 'Developer capacity must be between 1 and 10.'),
    );
  }
  return ok({ name, email, capacity });
};

export const createDeveloperUseCases = (deps: {
  readonly developerRepository: DeveloperRepository;
  readonly idProvider: IdProvider;
}): DeveloperUseCases => {
  const createDeveloperHandler = async (
    params: CreateDeveloperParams,
  ): Promise<Result<Developer, DomainError>> => {
    const validation = ensureValidDeveloper(params);
    if (!validation.ok) {
      return validation;
    }

    const developer: Developer = {
      id: deps.idProvider(),
      displayName: validation.value.name,
      email: validation.value.email,
      capacityHalfDaysPerWeek: validation.value.capacity,
    };

    const saved = await deps.developerRepository.save(developer);
    return ok(saved);
  };

  return {
    createDeveloper: createDeveloperHandler,
    listDevelopers: deps.developerRepository.list,
    getDeveloper: deps.developerRepository.getById,
  };
};
