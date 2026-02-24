import type { Task } from '@aggregated-plan/shared-types';
import type { CreateTaskInput, UpdateTaskInput } from '@infrastructure/index';
import { createTaskApi, deleteTaskApi, fetchTasks, updateTaskApi } from '@infrastructure/index';

export const loadTasks = async (): Promise<readonly Task[]> => fetchTasks();

export const submitTask = async (
  input: CreateTaskInput,
): Promise<Task> => createTaskApi(input);

export const editTask = async (
  id: string,
  input: UpdateTaskInput,
): Promise<Task> => updateTaskApi(id, input);

export const removeTask = async (id: string): Promise<void> => deleteTaskApi(id);
