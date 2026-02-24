import { createTask, updateTask } from '@domain/task-domain';

describe('task-domain', () => {
  it('creates a task with defaults', () => {
    const result = createTask(
      { name: 'My Task' },
      { id: 'task-1', now: '2024-01-01' },
    );

    expect(result.ok).toBe(true);
    if (!result.ok) {
      throw new Error('Expected task creation to succeed');
    }
    expect(result.value.status).toBe('todo');
    expect(result.value.priority).toBe('medium');
    expect(result.value.projectId).toBeUndefined();
    expect(result.value.createdAt).toBe('2024-01-01');
  });

  it('creates a task with all fields', () => {
    const result = createTask(
      {
        name: 'Full Task',
        description: 'A detailed description',
        projectId: 'project-1',
        status: 'in-progress',
        priority: 'high',
        dueDate: '2024-02-15',
      },
      { id: 'task-2', now: '2024-01-05' },
    );

    expect(result.ok).toBe(true);
    if (!result.ok) {
      throw new Error('Expected task creation to succeed');
    }
    expect(result.value.name).toBe('Full Task');
    expect(result.value.description).toBe('A detailed description');
    expect(result.value.projectId).toBe('project-1');
    expect(result.value.status).toBe('in-progress');
    expect(result.value.priority).toBe('high');
    expect(result.value.dueDate).toBe('2024-02-15');
  });

  it('rejects empty name', () => {
    const result = createTask(
      { name: '   ' },
      { id: 'task-3', now: '2024-01-01' },
    );

    expect(result.ok).toBe(false);
    if (result.ok) {
      throw new Error('Expected task creation to fail');
    }
    expect(result.error.code).toBe('invalid-name');
  });

  it('updates status immutably', () => {
    const created = createTask(
      { name: 'Move me' },
      { id: 'task-4', now: '2024-01-01' },
    );

    if (!created.ok) {
      throw new Error('Expected task creation to succeed');
    }

    const updated = updateTask(
      created.value,
      { status: 'done' },
      { now: '2024-01-10' },
    );

    expect(updated.ok).toBe(true);
    if (!updated.ok) {
      throw new Error('Expected task update to succeed');
    }
    expect(updated.value.status).toBe('done');
    expect(updated.value.updatedAt).toBe('2024-01-10');
    expect(created.value.status).toBe('todo');
  });

  it('updates priority and description', () => {
    const created = createTask(
      { name: 'Update me' },
      { id: 'task-5', now: '2024-01-01' },
    );

    if (!created.ok) {
      throw new Error('Expected task creation to succeed');
    }

    const updated = updateTask(
      created.value,
      { priority: 'high', description: 'Now urgent' },
      { now: '2024-01-05' },
    );

    expect(updated.ok).toBe(true);
    if (!updated.ok) {
      throw new Error('Expected task update to succeed');
    }
    expect(updated.value.priority).toBe('high');
    expect(updated.value.description).toBe('Now urgent');
  });
});
