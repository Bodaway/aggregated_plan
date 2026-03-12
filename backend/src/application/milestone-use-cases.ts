import type { Milestone } from '@aggregated-plan/shared-types';
import { compareIsoDates } from '@aggregated-plan/shared-utils';
import { createDomainError, createMilestone, err, ok } from '@domain/index';
import type {
  CreateMilestoneParams,
  DomainError,
  MilestoneContext,
  Result,
} from '@domain/index';
import type { MilestoneRepository } from './milestone-repository';
import type { ProjectRepository } from './project-repository';
import type { Clock, IdProvider } from './providers';

export type MilestoneUseCases = {
  readonly createMilestone: (
    params: CreateMilestoneParams,
  ) => Promise<Result<Milestone, DomainError>>;
  readonly listMilestones: () => Promise<readonly Milestone[]>;
  readonly listMilestonesByProject: (projectId: string) => Promise<readonly Milestone[]>;
};

export const createMilestoneUseCases = (deps: {
  readonly milestoneRepository: MilestoneRepository;
  readonly projectRepository: ProjectRepository;
  readonly idProvider: IdProvider;
  readonly clock: Clock;
}): MilestoneUseCases => {
  const createMilestoneHandler = async (
    params: CreateMilestoneParams,
  ): Promise<Result<Milestone, DomainError>> => {
    const project = await deps.projectRepository.getById(params.projectId);
    if (!project) {
      return err(createDomainError('not-found', 'Project not found.'));
    }
    if (compareIsoDates(params.date, project.startDate) < 0) {
      return err(
        createDomainError('invalid-date-range', 'Milestone date must be within project dates.'),
      );
    }
    if (compareIsoDates(params.date, project.endDate) > 0) {
      return err(
        createDomainError('invalid-date-range', 'Milestone date must be within project dates.'),
      );
    }

    const context: MilestoneContext = {
      id: deps.idProvider(),
      now: deps.clock(),
    };
    const result = createMilestone(params, context);
    if (!result.ok) {
      return result;
    }
    const saved = await deps.milestoneRepository.save(result.value);
    return ok(saved);
  };

  const listMilestonesHandler = async (): Promise<readonly Milestone[]> =>
    deps.milestoneRepository.list();

  const listMilestonesByProjectHandler = async (
    projectId: string,
  ): Promise<readonly Milestone[]> => deps.milestoneRepository.listByProject(projectId);

  return {
    createMilestone: createMilestoneHandler,
    listMilestones: listMilestonesHandler,
    listMilestonesByProject: listMilestonesByProjectHandler,
  };
};
