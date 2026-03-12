import { createProject, updateProject } from '@domain/project-domain';

describe('project-domain', () => {
  it('creates a project with defaults', () => {
    const result = createProject(
      {
        name: 'Project Alpha',
        description: 'Core platform work',
        startDate: '2024-01-01',
        endDate: '2024-01-10',
        createdBy: 'user-1',
      },
      { id: 'project-1', now: '2024-01-01' },
    );

    expect(result.ok).toBe(true);
    if (!result.ok) {
      throw new Error('Expected project creation to succeed');
    }
    expect(result.value.status).toBe('planning');
    expect(result.value.teamIds).toEqual([]);
    expect(result.value.createdAt).toBe('2024-01-01');
  });

  it('rejects invalid project dates', () => {
    const result = createProject(
      {
        name: 'Project Beta',
        startDate: '2024-02-10',
        endDate: '2024-01-01',
        createdBy: 'user-1',
      },
      { id: 'project-2', now: '2024-01-01' },
    );

    expect(result.ok).toBe(false);
    if (result.ok) {
      throw new Error('Expected project creation to fail');
    }
    expect(result.error.code).toBe('invalid-date-range');
  });

  it('updates project data immutably', () => {
    const created = createProject(
      {
        name: 'Project Gamma',
        startDate: '2024-01-01',
        endDate: '2024-01-31',
        createdBy: 'user-1',
      },
      { id: 'project-3', now: '2024-01-01' },
    );

    if (!created.ok) {
      throw new Error('Expected project creation to succeed');
    }

    const updated = updateProject(
      created.value,
      { name: 'Project Gamma X', endDate: '2024-02-05' },
      { now: '2024-01-05' },
    );

    expect(updated.ok).toBe(true);
    if (!updated.ok) {
      throw new Error('Expected project update to succeed');
    }
    expect(updated.value.name).toBe('Project Gamma X');
    expect(updated.value.endDate).toBe('2024-02-05');
    expect(updated.value.updatedAt).toBe('2024-01-05');
  });
});
