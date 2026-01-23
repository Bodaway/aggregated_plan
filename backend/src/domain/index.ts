export type { DomainError, DomainErrorCode } from './domain-errors';
export { createDomainError } from './domain-errors';
export type { Result } from './result';
export { err, ok } from './result';
export type {
  CreateProjectParams,
  UpdateProjectParams,
  ProjectContext,
} from './project-domain';
export { createProject, updateProject } from './project-domain';
export type {
  CreateAssignmentParams,
  AssignmentContext,
  AssignmentSeed,
  CreateWeeklyAllocationParams,
  WeeklyAllocationContext,
} from './staffing-domain';
export { createAssignment, createWeeklyAllocation, allocationToAssignmentSeeds } from './staffing-domain';
export type { Conflict } from '@aggregated-plan/shared-types';
export type { ConflictInput, CapacitySnapshot } from './conflict-domain';
export { detectConflicts } from './conflict-domain';
export type { CreateAvailabilityParams, AvailabilityContext } from './availability-domain';
export { createAvailability } from './availability-domain';
