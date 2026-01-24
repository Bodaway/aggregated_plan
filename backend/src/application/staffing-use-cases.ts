import type {
  Assignment,
  Availability,
  EntityId,
  WeeklyAllocation,
} from '@aggregated-plan/shared-types';
import {
  allocationToAssignmentSeeds,
  createAssignment,
  createWeeklyAllocation,
  detectConflicts,
  ok,
} from '@domain/index';
import type {
  AssignmentContext,
  Conflict,
  CreateAssignmentParams,
  CreateWeeklyAllocationParams,
  DomainError,
  Result,
  WeeklyAllocationContext,
} from '@domain/index';
import type { AllocationRepository } from './allocation-repository';
import type { AssignmentRepository } from './assignment-repository';
import type { AvailabilityRepository } from './availability-repository';
import type { DeveloperRepository } from './developer-repository';
import type { Clock, IdProvider } from './providers';

export type StaffingUseCases = {
  readonly createAssignment: (
    params: CreateAssignmentParams,
  ) => Promise<Result<Assignment, DomainError>>;
  readonly createWeeklyAllocation: (
    params: CreateWeeklyAllocationParams,
  ) => Promise<Result<WeeklyAllocation, DomainError>>;
  readonly listAssignments: () => Promise<readonly Assignment[]>;
  readonly listConflicts: () => Promise<readonly Conflict[]>;
};

export const createStaffingUseCases = (deps: {
  readonly assignmentRepository: AssignmentRepository;
  readonly allocationRepository: AllocationRepository;
  readonly availabilityRepository: AvailabilityRepository;
  readonly developerRepository: DeveloperRepository;
  readonly idProvider: IdProvider;
  readonly clock: Clock;
}): StaffingUseCases => {
  const buildAssignments = (
    seeds: readonly CreateAssignmentParams[],
  ): Result<Assignment[], DomainError> => {
    return seeds.reduce<Result<Assignment[], DomainError>>((accResult, seed) => {
      if (!accResult.ok) {
        return accResult;
      }
      const context: AssignmentContext = {
        id: deps.idProvider(),
        createdAt: deps.clock(),
      };
      const result = createAssignment(seed, context);
      if (!result.ok) {
        return result;
      }
      return ok([...accResult.value, result.value]);
    }, ok([]));
  };

  const loadConflictContext = async (): Promise<{
    readonly assignments: readonly Assignment[];
    readonly availabilities: readonly Availability[];
    readonly capacities: readonly { developerId: EntityId; capacityHalfDaysPerWeek: number }[];
  }> => {
    const [assignments, availabilities, developers] = await Promise.all([
      deps.assignmentRepository.list(),
      deps.availabilityRepository.list(),
      deps.developerRepository.list(),
    ]);
    const capacities = developers.map((developer) => ({
      developerId: developer.id,
      capacityHalfDaysPerWeek: developer.capacityHalfDaysPerWeek,
    }));
    return { assignments, availabilities, capacities };
  };

  const createAssignmentHandler = async (
    params: CreateAssignmentParams,
  ): Promise<Result<Assignment, DomainError>> => {
    const assignmentContext: AssignmentContext = {
      id: deps.idProvider(),
      createdAt: deps.clock(),
    };
    const assignmentResult = createAssignment(params, assignmentContext);
    if (!assignmentResult.ok) {
      return assignmentResult;
    }

    const saved = await deps.assignmentRepository.save(assignmentResult.value);
    return ok(saved);
  };

  const createWeeklyAllocationHandler = async (
    params: CreateWeeklyAllocationParams,
  ): Promise<Result<WeeklyAllocation, DomainError>> => {
    const allocationContext: WeeklyAllocationContext = {
      id: deps.idProvider(),
      createdAt: deps.clock(),
    };
    const allocationResult = createWeeklyAllocation(params, allocationContext);
    if (!allocationResult.ok) {
      return allocationResult;
    }

    const seedsResult = allocationToAssignmentSeeds(allocationResult.value);
    if (!seedsResult.ok) {
      return seedsResult;
    }

    const assignmentSeeds = seedsResult.value.map((seed) => ({
      projectId: seed.projectId,
      developerId: seed.developerId,
      date: seed.date,
      halfDay: seed.halfDay,
    }));
    const newAssignmentsResult = buildAssignments(assignmentSeeds);
    if (!newAssignmentsResult.ok) {
      return newAssignmentsResult;
    }
    const newAssignments = newAssignmentsResult.value;

    const savedAllocation = await deps.allocationRepository.save(allocationResult.value);
    await deps.assignmentRepository.saveMany(newAssignments);
    return ok(savedAllocation);
  };

  const listAssignmentsHandler = async (): Promise<readonly Assignment[]> =>
    deps.assignmentRepository.list();

  const listConflictsHandler = async (): Promise<readonly Conflict[]> => {
    const context = await loadConflictContext();
    return detectConflicts({
      assignments: context.assignments,
      availabilities: context.availabilities,
      capacities: context.capacities,
    });
  };

  return {
    createAssignment: createAssignmentHandler,
    createWeeklyAllocation: createWeeklyAllocationHandler,
    listAssignments: listAssignmentsHandler,
    listConflicts: listConflictsHandler,
  };
};
