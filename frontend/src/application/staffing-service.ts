import type { Assignment, Conflict, Developer } from '@aggregated-plan/shared-types';
import type {
  CreateAssignmentInput,
  CreateDeveloperInput,
  UpdateDeveloperInput,
} from '@infrastructure/index';
import {
  createAssignment,
  createDeveloper,
  updateDeveloper,
  fetchAssignments,
  fetchConflicts,
  fetchDevelopers,
} from '@infrastructure/index';

export const loadDevelopers = async (): Promise<readonly Developer[]> => fetchDevelopers();

export const submitAssignment = async (
  input: CreateAssignmentInput,
): Promise<Assignment> => createAssignment(input);

export const submitDeveloper = async (
  input: CreateDeveloperInput,
): Promise<Developer> => createDeveloper(input);

export const editDeveloper = async (
  id: string,
  input: UpdateDeveloperInput,
): Promise<Developer> => updateDeveloper(id, input);

export const loadAssignments = async (): Promise<readonly Assignment[]> => fetchAssignments();

export const loadConflicts = async (): Promise<readonly Conflict[]> => fetchConflicts();
