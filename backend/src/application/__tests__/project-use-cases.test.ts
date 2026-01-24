import { createProjectUseCases } from '@application/project-use-cases';
import { createInMemoryRepositories, createInMemoryStore } from '@infrastructure/index';

describe('project-use-cases', () => {
  it('prevents duplicate project names', async () => {
    const store = createInMemoryStore();
    const repositories = createInMemoryRepositories(store);
    const idProvider = (() => {
      const ids = ['project-1', 'project-2'];
      return () => ids.shift() ?? 'project-fallback';
    })();
    const clock = () => '2024-01-01';

    const useCases = createProjectUseCases({
      projectRepository: repositories.projectRepository,
      idProvider,
      clock,
    });

    const first = await useCases.createProject({
      name: 'Alpha',
      startDate: '2024-01-01',
      endDate: '2024-01-10',
      createdBy: 'user-1',
    });

    expect(first.ok).toBe(true);

    const duplicate = await useCases.createProject({
      name: 'Alpha',
      startDate: '2024-02-01',
      endDate: '2024-02-10',
      createdBy: 'user-2',
    });

    expect(duplicate.ok).toBe(false);
    if (duplicate.ok) {
      throw new Error('Expected duplicate project to fail');
    }
    expect(duplicate.error.code).toBe('duplicate-name');
  });
});
