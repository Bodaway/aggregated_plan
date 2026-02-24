import { createTaskUseCases } from '@application/task-use-cases';
import { createInMemoryStore } from '@infrastructure/in-memory-store';
import { createInMemoryRepositories } from '@infrastructure/in-memory-repositories';

describe('task-use-cases', () => {
  const setup = () => {
    const store = createInMemoryStore();
    const repositories = createInMemoryRepositories(store);
    const ids = ['task-1', 'task-2', 'task-3'];
    const idProvider = () => ids.shift() ?? 'task-fallback';
    const clock = () => '2024-01-01';

    const useCases = createTaskUseCases({
      taskRepository: repositories.taskRepository,
      idProvider,
      clock,
    });

    return { useCases, store };
  };

  it('creates a task', async () => {
    const { useCases } = setup();

    const result = await useCases.createTask({ name: 'My Task' });

    expect(result.ok).toBe(true);
    if (!result.ok) {
      throw new Error('Expected task creation to succeed');
    }
    expect(result.value.name).toBe('My Task');
    expect(result.value.status).toBe('todo');
    expect(result.value.priority).toBe('medium');
  });

  it('updates task status (drag-and-drop)', async () => {
    const { useCases } = setup();

    const created = await useCases.createTask({ name: 'Drag me' });
    if (!created.ok) {
      throw new Error('Expected task creation to succeed');
    }

    const updated = await useCases.updateTask(created.value.id, { status: 'done' });

    expect(updated.ok).toBe(true);
    if (!updated.ok) {
      throw new Error('Expected task update to succeed');
    }
    expect(updated.value.status).toBe('done');
  });

  it('deletes a task', async () => {
    const { useCases } = setup();

    const created = await useCases.createTask({ name: 'Delete me' });
    if (!created.ok) {
      throw new Error('Expected task creation to succeed');
    }

    const deleteResult = await useCases.deleteTask(created.value.id);
    expect(deleteResult.ok).toBe(true);

    const fetched = await useCases.getTask(created.value.id);
    expect(fetched).toBeNull();
  });

  it('returns not-found on update of missing task', async () => {
    const { useCases } = setup();

    const result = await useCases.updateTask('nonexistent', { status: 'done' });

    expect(result.ok).toBe(false);
    if (result.ok) {
      throw new Error('Expected update to fail');
    }
    expect(result.error.code).toBe('not-found');
  });

  it('returns not-found on delete of missing task', async () => {
    const { useCases } = setup();

    const result = await useCases.deleteTask('nonexistent');

    expect(result.ok).toBe(false);
    if (result.ok) {
      throw new Error('Expected delete to fail');
    }
    expect(result.error.code).toBe('not-found');
  });
});
