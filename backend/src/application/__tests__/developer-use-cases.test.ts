import { createDeveloperUseCases } from '@application/developer-use-cases';
import { createInMemoryRepositories, createInMemoryStore } from '@infrastructure/index';

describe('developer-use-cases', () => {
  it('creates developers with valid data', async () => {
    const store = createInMemoryStore();
    const repositories = createInMemoryRepositories(store);
    const idProvider = (() => {
      const ids = ['developer-1'];
      return () => ids.shift() ?? 'developer-fallback';
    })();

    const useCases = createDeveloperUseCases({
      developerRepository: repositories.developerRepository,
      idProvider,
    });

    const result = await useCases.createDeveloper({
      displayName: 'Jean Dupont',
      email: 'jean.dupont@example.com',
      capacityHalfDaysPerWeek: 8,
    });

    expect(result.ok).toBe(true);
    if (!result.ok) {
      throw new Error('Expected developer creation to succeed');
    }
    expect(result.value.capacityHalfDaysPerWeek).toBe(8);
  });

  it('rejects invalid developer data', async () => {
    const store = createInMemoryStore();
    const repositories = createInMemoryRepositories(store);
    const idProvider = () => 'developer-1';

    const useCases = createDeveloperUseCases({
      developerRepository: repositories.developerRepository,
      idProvider,
    });

    const result = await useCases.createDeveloper({
      displayName: '',
      email: 'invalid',
    });

    expect(result.ok).toBe(false);
    if (result.ok) {
      throw new Error('Expected developer creation to fail');
    }
    expect(result.error.code).toBe('invalid-name');
  });
});
