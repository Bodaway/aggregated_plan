import { eq, sql } from 'drizzle-orm';
import type {
  AllocationRepository,
  AssignmentRepository,
  AvailabilityRepository,
  DeveloperRepository,
  MilestoneRepository,
  ProjectRepository,
  TaskRepository,
} from '@application/index';
import type { EntityId } from '@aggregated-plan/shared-types';
import {
  allocations,
  assignments,
  availabilities,
  developers,
  milestones,
  projects,
  tasks,
} from '../db/schema';
import type { PostgresDatabase } from './db';
import {
  mapAllocationInsert,
  mapAllocationRow,
  mapAssignmentInsert,
  mapAssignmentRow,
  mapAvailabilityInsert,
  mapAvailabilityRow,
  mapDeveloperInsert,
  mapDeveloperRow,
  mapDeveloperUpdate,
  mapMilestoneInsert,
  mapMilestoneRow,
  mapProjectInsert,
  mapProjectRow,
  mapProjectUpdate,
  mapTaskInsert,
  mapTaskRow,
  mapTaskUpdate,
} from './mappers';

export type PostgresRepositories = {
  readonly projectRepository: ProjectRepository;
  readonly milestoneRepository: MilestoneRepository;
  readonly assignmentRepository: AssignmentRepository;
  readonly allocationRepository: AllocationRepository;
  readonly availabilityRepository: AvailabilityRepository;
  readonly developerRepository: DeveloperRepository;
  readonly taskRepository: TaskRepository;
};

export const createPostgresRepositories = (db: PostgresDatabase): PostgresRepositories => ({
  projectRepository: {
    list: async () => {
      const rows = await db.select().from(projects);
      return rows.map(mapProjectRow);
    },
    getById: async (id: EntityId) => {
      const rows = await db.select().from(projects).where(eq(projects.id, id)).limit(1);
      return rows[0] ? mapProjectRow(rows[0]) : null;
    },
    getByName: async (name: string) => {
      const normalized = name.trim().toLowerCase();
      const rows = await db
        .select()
        .from(projects)
        .where(sql`lower(${projects.name}) = ${normalized}`)
        .limit(1);
      return rows[0] ? mapProjectRow(rows[0]) : null;
    },
    save: async (project) => {
      await db.insert(projects).values(mapProjectInsert(project));
      return project;
    },
    update: async (project) => {
      await db
        .update(projects)
        .set(mapProjectUpdate(project))
        .where(eq(projects.id, project.id));
      return project;
    },
    remove: async (id: EntityId) => {
      await db.delete(projects).where(eq(projects.id, id));
    },
  },
  milestoneRepository: {
    list: async () => {
      const rows = await db.select().from(milestones);
      return rows.map(mapMilestoneRow);
    },
    listByProject: async (projectId: EntityId) => {
      const rows = await db
        .select()
        .from(milestones)
        .where(eq(milestones.projectId, projectId));
      return rows.map(mapMilestoneRow);
    },
    save: async (milestone) => {
      await db.insert(milestones).values(mapMilestoneInsert(milestone));
      return milestone;
    },
  },
  assignmentRepository: {
    list: async () => {
      const rows = await db.select().from(assignments);
      return rows.map(mapAssignmentRow);
    },
    listByDeveloper: async (developerId: EntityId) => {
      const rows = await db
        .select()
        .from(assignments)
        .where(eq(assignments.developerId, developerId));
      return rows.map(mapAssignmentRow);
    },
    save: async (assignment) => {
      await db.insert(assignments).values(mapAssignmentInsert(assignment));
      return assignment;
    },
    saveMany: async (batch) => {
      const values = batch.map(mapAssignmentInsert);
      if (values.length === 0) {
        return batch;
      }
      await db.insert(assignments).values(values);
      return batch;
    },
  },
  allocationRepository: {
    list: async () => {
      const rows = await db.select().from(allocations);
      return rows.map(mapAllocationRow);
    },
    listByDeveloper: async (developerId: EntityId) => {
      const rows = await db
        .select()
        .from(allocations)
        .where(eq(allocations.developerId, developerId));
      return rows.map(mapAllocationRow);
    },
    save: async (allocation) => {
      await db.insert(allocations).values(mapAllocationInsert(allocation));
      return allocation;
    },
  },
  availabilityRepository: {
    list: async () => {
      const rows = await db.select().from(availabilities);
      return rows.map(mapAvailabilityRow);
    },
    listByDeveloper: async (developerId: EntityId) => {
      const rows = await db
        .select()
        .from(availabilities)
        .where(eq(availabilities.developerId, developerId));
      return rows.map(mapAvailabilityRow);
    },
    save: async (availability) => {
      await db.insert(availabilities).values(mapAvailabilityInsert(availability));
      return availability;
    },
  },
  developerRepository: {
    list: async () => {
      const rows = await db.select().from(developers);
      return rows.map(mapDeveloperRow);
    },
    getById: async (id: EntityId) => {
      const rows = await db.select().from(developers).where(eq(developers.id, id)).limit(1);
      return rows[0] ? mapDeveloperRow(rows[0]) : null;
    },
    save: async (developer) => {
      await db.insert(developers).values(mapDeveloperInsert(developer));
      return developer;
    },
    update: async (developer) => {
      await db
        .update(developers)
        .set(mapDeveloperUpdate(developer))
        .where(eq(developers.id, developer.id));
      return developer;
    },
  },
  taskRepository: {
    list: async () => {
      const rows = await db.select().from(tasks);
      return rows.map(mapTaskRow);
    },
    getById: async (id: EntityId) => {
      const rows = await db.select().from(tasks).where(eq(tasks.id, id)).limit(1);
      return rows[0] ? mapTaskRow(rows[0]) : null;
    },
    save: async (task) => {
      await db.insert(tasks).values(mapTaskInsert(task));
      return task;
    },
    update: async (task) => {
      await db
        .update(tasks)
        .set(mapTaskUpdate(task))
        .where(eq(tasks.id, task.id));
      return task;
    },
    remove: async (id: EntityId) => {
      await db.delete(tasks).where(eq(tasks.id, id));
    },
  },
});
