import type { Assignment, Conflict, Developer } from '@aggregated-plan/shared-types';
import type { CreateAssignmentInput, CreateDeveloperInput } from '@infrastructure/index';
import {
  createAssignment,
  createDeveloper,
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

export const loadAssignments = async (): Promise<readonly Assignment[]> => fetchAssignments();

export const loadConflicts = async (): Promise<readonly Conflict[]> => fetchConflicts();
