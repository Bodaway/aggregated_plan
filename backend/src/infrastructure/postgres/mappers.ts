import type {
  Assignment,
  Availability,
  Developer,
  Milestone,
  Project,
  Task,
  WeeklyAllocation,
} from '@aggregated-plan/shared-types';
import { projects, developers, assignments, allocations, availabilities, milestones, tasks } from '../db/schema';

type ProjectRow = typeof projects.$inferSelect;
type ProjectInsert = typeof projects.$inferInsert;
type DeveloperRow = typeof developers.$inferSelect;
type DeveloperInsert = typeof developers.$inferInsert;
type AssignmentRow = typeof assignments.$inferSelect;
type AssignmentInsert = typeof assignments.$inferInsert;
type AllocationRow = typeof allocations.$inferSelect;
type AllocationInsert = typeof allocations.$inferInsert;
type AvailabilityRow = typeof availabilities.$inferSelect;
type AvailabilityInsert = typeof availabilities.$inferInsert;
type MilestoneRow = typeof milestones.$inferSelect;
type MilestoneInsert = typeof milestones.$inferInsert;
type TaskRow = typeof tasks.$inferSelect;
type TaskInsert = typeof tasks.$inferInsert;

const optional = <T>(value: T | null): T | undefined => value ?? undefined;

export const mapProjectRow = (row: ProjectRow): Project => ({
  id: row.id,
  name: row.name,
  description: optional(row.description),
  startDate: row.startDate,
  endDate: row.endDate,
  status: row.status,
  teamIds: [...row.teamIds],
  client: optional(row.client),
  priority: optional(row.priority),
  createdAt: row.createdAt,
  updatedAt: row.updatedAt,
  createdBy: row.createdBy,
});

export const mapProjectInsert = (project: Project): ProjectInsert => ({
  id: project.id,
  name: project.name,
  description: project.description ?? null,
  startDate: project.startDate,
  endDate: project.endDate,
  status: project.status,
  teamIds: [...project.teamIds],
  client: project.client ?? null,
  priority: project.priority ?? null,
  createdAt: project.createdAt,
  updatedAt: project.updatedAt,
  createdBy: project.createdBy,
});

export const mapProjectUpdate = (project: Project): Omit<ProjectInsert, 'id'> => ({
  name: project.name,
  description: project.description ?? null,
  startDate: project.startDate,
  endDate: project.endDate,
  status: project.status,
  teamIds: [...project.teamIds],
  client: project.client ?? null,
  priority: project.priority ?? null,
  createdAt: project.createdAt,
  updatedAt: project.updatedAt,
  createdBy: project.createdBy,
});

export const mapDeveloperRow = (row: DeveloperRow): Developer => ({
  id: row.id,
  displayName: row.displayName,
  email: row.email,
  capacityHalfDaysPerWeek: row.capacityHalfDaysPerWeek,
});

export const mapDeveloperInsert = (developer: Developer): DeveloperInsert => ({
  id: developer.id,
  displayName: developer.displayName,
  email: developer.email,
  capacityHalfDaysPerWeek: developer.capacityHalfDaysPerWeek,
});

export const mapDeveloperUpdate = (
  developer: Developer,
): Omit<DeveloperInsert, 'id'> => ({
  displayName: developer.displayName,
  email: developer.email,
  capacityHalfDaysPerWeek: developer.capacityHalfDaysPerWeek,
});

export const mapAssignmentRow = (row: AssignmentRow): Assignment => ({
  id: row.id,
  projectId: row.projectId,
  developerId: row.developerId,
  date: row.date,
  halfDay: row.halfDay,
  createdAt: row.createdAt,
});

export const mapAssignmentInsert = (assignment: Assignment): AssignmentInsert => ({
  id: assignment.id,
  projectId: assignment.projectId,
  developerId: assignment.developerId,
  date: assignment.date,
  halfDay: assignment.halfDay,
  createdAt: assignment.createdAt,
});

export const mapAllocationRow = (row: AllocationRow): WeeklyAllocation => ({
  id: row.id,
  projectId: row.projectId,
  developerId: row.developerId,
  startDate: row.startDate,
  endDate: row.endDate,
  halfDaysPerWeek: row.halfDaysPerWeek,
  preferredWeekdays: row.preferredWeekdays ? [...row.preferredWeekdays] : undefined,
  createdAt: row.createdAt,
});

export const mapAllocationInsert = (allocation: WeeklyAllocation): AllocationInsert => ({
  id: allocation.id,
  projectId: allocation.projectId,
  developerId: allocation.developerId,
  startDate: allocation.startDate,
  endDate: allocation.endDate,
  halfDaysPerWeek: allocation.halfDaysPerWeek,
  preferredWeekdays: allocation.preferredWeekdays ? [...allocation.preferredWeekdays] : null,
  createdAt: allocation.createdAt,
});

export const mapAvailabilityRow = (row: AvailabilityRow): Availability => ({
  id: row.id,
  developerId: row.developerId,
  startDate: row.startDate,
  endDate: row.endDate,
  type: row.type,
  description: optional(row.description),
  createdAt: row.createdAt,
});

export const mapAvailabilityInsert = (availability: Availability): AvailabilityInsert => ({
  id: availability.id,
  developerId: availability.developerId,
  startDate: availability.startDate,
  endDate: availability.endDate,
  type: availability.type,
  description: availability.description ?? null,
  createdAt: availability.createdAt,
});

export const mapMilestoneRow = (row: MilestoneRow): Milestone => ({
  id: row.id,
  projectId: row.projectId,
  name: row.name,
  date: row.date,
  type: row.type,
  createdAt: row.createdAt,
  updatedAt: row.updatedAt,
});

export const mapMilestoneInsert = (milestone: Milestone): MilestoneInsert => ({
  id: milestone.id,
  projectId: milestone.projectId,
  name: milestone.name,
  date: milestone.date,
  type: milestone.type,
  createdAt: milestone.createdAt,
  updatedAt: milestone.updatedAt,
});

export const mapTaskRow = (row: TaskRow): Task => ({
  id: row.id,
  projectId: optional(row.projectId),
  name: row.name,
  description: optional(row.description),
  status: row.status,
  priority: row.priority,
  dueDate: optional(row.dueDate),
  createdAt: row.createdAt,
  updatedAt: row.updatedAt,
});

export const mapTaskInsert = (task: Task): TaskInsert => ({
  id: task.id,
  projectId: task.projectId ?? null,
  name: task.name,
  description: task.description ?? null,
  status: task.status,
  priority: task.priority,
  dueDate: task.dueDate ?? null,
  createdAt: task.createdAt,
  updatedAt: task.updatedAt,
});

export const mapTaskUpdate = (task: Task): Omit<TaskInsert, 'id'> => ({
  projectId: task.projectId ?? null,
  name: task.name,
  description: task.description ?? null,
  status: task.status,
  priority: task.priority,
  dueDate: task.dueDate ?? null,
  createdAt: task.createdAt,
  updatedAt: task.updatedAt,
});
