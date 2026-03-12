import type { Milestone, Project } from '@aggregated-plan/shared-types';
import type { CreateMilestoneInput, CreateProjectInput } from '@infrastructure/index';
import { createMilestone, createProject, fetchMilestones, fetchProjects } from '@infrastructure/index';

export const loadProjects = async (): Promise<readonly Project[]> => fetchProjects();

export const submitProject = async (
  input: CreateProjectInput,
): Promise<Project> => createProject(input);

export const loadMilestones = async (): Promise<readonly Milestone[]> => fetchMilestones();

export const submitMilestone = async (
  input: CreateMilestoneInput,
): Promise<Milestone> => createMilestone(input);
