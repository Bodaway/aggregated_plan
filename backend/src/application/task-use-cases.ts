import type { Task } from '@aggregated-plan/shared-types';
import { createDomainError, createTask, err, ok, updateTask } from '@domain/index';
import type {
  CreateTaskParams,
  DomainError,
  Result,
  TaskContext,
  UpdateTaskParams,
} from '@domain/index';
import type { TaskRepository } from './task-repository';
import type { Clock, IdProvider } from './providers';

export type TaskUseCases = {
  readonly createTask: (params: CreateTaskParams) => Promise<Result<Task, DomainError>>;
  readonly updateTask: (
    id: string,
    updates: UpdateTaskParams,
  ) => Promise<Result<Task, DomainError>>;
  readonly deleteTask: (id: string) => Promise<Result<null, DomainError>>;
  readonly getTask: (id: string) => Promise<Task | null>;
  readonly listTasks: () => Promise<readonly Task[]>;
};

export const createTaskUseCases = (deps: {
  readonly taskRepository: TaskRepository;
  readonly idProvider: IdProvider;
  readonly clock: Clock;
}): TaskUseCases => {
  const createTaskHandler = async (
    params: CreateTaskParams,
  ): Promise<Result<Task, DomainError>> => {
    const context: TaskContext = { id: deps.idProvider(), now: deps.clock() };
    const taskResult = createTask(params, context);
    if (!taskResult.ok) {
      return taskResult;
    }

    const saved = await deps.taskRepository.save(taskResult.value);
    return ok(saved);
  };

  const updateTaskHandler = async (
    id: string,
    updates: UpdateTaskParams,
  ): Promise<Result<Task, DomainError>> => {
    const existing = await deps.taskRepository.getById(id);
    if (!existing) {
      return err(createDomainError('not-found', 'Task not found.'));
    }

    const updatedResult = updateTask(existing, updates, { now: deps.clock() });
    if (!updatedResult.ok) {
      return updatedResult;
    }

    const saved = await deps.taskRepository.update(updatedResult.value);
    return ok(saved);
  };

  const deleteTaskHandler = async (id: string): Promise<Result<null, DomainError>> => {
    const existing = await deps.taskRepository.getById(id);
    if (!existing) {
      return err(createDomainError('not-found', 'Task not found.'));
    }
    await deps.taskRepository.remove(id);
    return ok(null);
  };

  return {
    createTask: createTaskHandler,
    updateTask: updateTaskHandler,
    deleteTask: deleteTaskHandler,
    getTask: deps.taskRepository.getById,
    listTasks: deps.taskRepository.list,
  };
};
