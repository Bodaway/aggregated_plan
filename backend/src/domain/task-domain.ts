import type {
  EntityId,
  IsoDateString,
  Task,
  TaskPriority,
  TaskStatus,
} from '@aggregated-plan/shared-types';
import { createDomainError } from './domain-errors';
import type { DomainError } from './domain-errors';
import { err, ok } from './result';
import type { Result } from './result';

export type CreateTaskParams = {
  readonly name: string;
  readonly description?: string;
  readonly projectId?: EntityId;
  readonly status?: TaskStatus;
  readonly priority?: TaskPriority;
  readonly dueDate?: IsoDateString;
};

export type TaskContext = {
  readonly id: EntityId;
  readonly now: IsoDateString;
};

export type UpdateTaskParams = {
  readonly name?: string;
  readonly description?: string;
  readonly projectId?: EntityId;
  readonly status?: TaskStatus;
  readonly priority?: TaskPriority;
  readonly dueDate?: IsoDateString;
};

const ensureValidName = (name: string): Result<string, DomainError> => {
  if (name.trim().length === 0) {
    return err(createDomainError('invalid-name', 'Task name is required.'));
  }
  return ok(name.trim());
};

export const createTask = (
  params: CreateTaskParams,
  context: TaskContext,
): Result<Task, DomainError> => {
  const nameResult = ensureValidName(params.name);
  if (!nameResult.ok) {
    return nameResult;
  }

  const task: Task = {
    id: context.id,
    projectId: params.projectId,
    name: nameResult.value,
    description: params.description,
    status: params.status ?? 'todo',
    priority: params.priority ?? 'medium',
    dueDate: params.dueDate,
    createdAt: context.now,
    updatedAt: context.now,
  };

  return ok(task);
};

export const updateTask = (
  task: Task,
  updates: UpdateTaskParams,
  context: { readonly now: IsoDateString },
): Result<Task, DomainError> => {
  const nameResult = updates.name ? ensureValidName(updates.name) : ok(task.name);
  if (!nameResult.ok) {
    return nameResult;
  }

  const updatedTask: Task = {
    ...task,
    name: nameResult.value,
    description: updates.description ?? task.description,
    projectId: updates.projectId ?? task.projectId,
    status: updates.status ?? task.status,
    priority: updates.priority ?? task.priority,
    dueDate: updates.dueDate ?? task.dueDate,
    updatedAt: context.now,
  };

  return ok(updatedTask);
};
