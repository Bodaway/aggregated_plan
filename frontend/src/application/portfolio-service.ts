import type { Project } from '@aggregated-plan/shared-types';
import type { CreateProjectInput } from '@infrastructure/index';
import { createProject, fetchProjects } from '@infrastructure/index';

export const loadProjects = async (): Promise<readonly Project[]> => fetchProjects();

export const submitProject = async (
  input: CreateProjectInput,
): Promise<Project> => createProject(input);
