import type { Project } from '@aggregated-plan/shared-types';
import { createDomainError, createProject, err, ok, updateProject } from '@domain/index';
import type {
  CreateProjectParams,
  DomainError,
  ProjectContext,
  Result,
  UpdateProjectParams,
} from '@domain/index';
import type { ProjectRepository } from './project-repository';
import type { Clock, IdProvider } from './providers';

export type ProjectUseCases = {
  readonly createProject: (params: CreateProjectParams) => Promise<Result<Project, DomainError>>;
  readonly updateProject: (
    id: string,
    updates: UpdateProjectParams,
  ) => Promise<Result<Project, DomainError>>;
  readonly deleteProject: (id: string) => Promise<Result<null, DomainError>>;
  readonly getProject: (id: string) => Promise<Project | null>;
  readonly listProjects: () => Promise<readonly Project[]>;
};

export const createProjectUseCases = (deps: {
  readonly projectRepository: ProjectRepository;
  readonly idProvider: IdProvider;
  readonly clock: Clock;
}): ProjectUseCases => {
  const createProjectHandler = async (
    params: CreateProjectParams,
  ): Promise<Result<Project, DomainError>> => {
    const existing = await deps.projectRepository.getByName(params.name.trim());
    if (existing) {
      return err(
        createDomainError('duplicate-name', 'Project name must be unique.', {
          name: params.name,
        }),
      );
    }

    const context: ProjectContext = { id: deps.idProvider(), now: deps.clock() };
    const projectResult = createProject(params, context);
    if (!projectResult.ok) {
      return projectResult;
    }

    const saved = await deps.projectRepository.save(projectResult.value);
    return ok(saved);
  };

  const updateProjectHandler = async (
    id: string,
    updates: UpdateProjectParams,
  ): Promise<Result<Project, DomainError>> => {
    const existing = await deps.projectRepository.getById(id);
    if (!existing) {
      return err(createDomainError('not-found', 'Project not found.'));
    }

    if (updates.name && updates.name.trim() !== existing.name) {
      const nameExists = await deps.projectRepository.getByName(updates.name.trim());
      if (nameExists) {
        return err(
          createDomainError('duplicate-name', 'Project name must be unique.', {
            name: updates.name,
          }),
        );
      }
    }

    const updatedResult = updateProject(existing, updates, { now: deps.clock() });
    if (!updatedResult.ok) {
      return updatedResult;
    }

    const saved = await deps.projectRepository.update(updatedResult.value);
    return ok(saved);
  };

  const deleteProjectHandler = async (id: string): Promise<Result<null, DomainError>> => {
    const existing = await deps.projectRepository.getById(id);
    if (!existing) {
      return err(createDomainError('not-found', 'Project not found.'));
    }
    await deps.projectRepository.remove(id);
    return ok(null);
  };

  return {
    createProject: createProjectHandler,
    updateProject: updateProjectHandler,
    deleteProject: deleteProjectHandler,
    getProject: deps.projectRepository.getById,
    listProjects: deps.projectRepository.list,
  };
};
