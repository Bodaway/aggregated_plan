import type { Assignment, Conflict, Developer } from '@aggregated-plan/shared-types';
import type { CreateAssignmentInput } from '@infrastructure/index';
import { createAssignment, fetchConflicts, fetchDevelopers } from '@infrastructure/index';

export const loadDevelopers = async (): Promise<readonly Developer[]> => fetchDevelopers();

export const submitAssignment = async (
  input: CreateAssignmentInput,
): Promise<Assignment> => createAssignment(input);

export const loadConflicts = async (): Promise<readonly Conflict[]> => fetchConflicts();
