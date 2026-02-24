import { sql } from 'drizzle-orm';
import { integer, pgTable, text, uniqueIndex } from 'drizzle-orm/pg-core';

export const projects = pgTable(
  'projects',
  {
    id: text('id').primaryKey(),
    name: text('name').notNull(),
    description: text('description'),
    startDate: text('start_date').notNull(),
    endDate: text('end_date').notNull(),
    status: text('status').notNull(),
    teamIds: text('team_ids').array().notNull().default(sql`'{}'::text[]`),
    client: text('client'),
    priority: text('priority'),
    createdAt: text('created_at').notNull(),
    updatedAt: text('updated_at').notNull(),
    createdBy: text('created_by').notNull(),
  },
  (table) => ({
    nameUnique: uniqueIndex('projects_name_unique').on(table.name),
  }),
);

export const developers = pgTable('developers', {
  id: text('id').primaryKey(),
  displayName: text('display_name').notNull(),
  email: text('email').notNull(),
  capacityHalfDaysPerWeek: integer('capacity_half_days_per_week').notNull(),
});

export const assignments = pgTable('assignments', {
  id: text('id').primaryKey(),
  projectId: text('project_id').notNull(),
  developerId: text('developer_id').notNull(),
  date: text('date').notNull(),
  halfDay: text('half_day').notNull(),
  createdAt: text('created_at').notNull(),
});

export const allocations = pgTable('allocations', {
  id: text('id').primaryKey(),
  projectId: text('project_id').notNull(),
  developerId: text('developer_id').notNull(),
  startDate: text('start_date').notNull(),
  endDate: text('end_date').notNull(),
  halfDaysPerWeek: integer('half_days_per_week').notNull(),
  preferredWeekdays: text('preferred_weekdays').array(),
  createdAt: text('created_at').notNull(),
});

export const availabilities = pgTable('availabilities', {
  id: text('id').primaryKey(),
  developerId: text('developer_id').notNull(),
  startDate: text('start_date').notNull(),
  endDate: text('end_date').notNull(),
  type: text('type').notNull(),
  description: text('description'),
  createdAt: text('created_at').notNull(),
});

export const tasks = pgTable('tasks', {
  id: text('id').primaryKey(),
  projectId: text('project_id'),
  name: text('name').notNull(),
  description: text('description'),
  status: text('status').notNull(),
  priority: text('priority').notNull(),
  dueDate: text('due_date'),
  createdAt: text('created_at').notNull(),
  updatedAt: text('updated_at').notNull(),
});

export const milestones = pgTable('milestones', {
  id: text('id').primaryKey(),
  projectId: text('project_id').notNull(),
  name: text('name').notNull(),
  date: text('date').notNull(),
  type: text('type').notNull(),
  createdAt: text('created_at').notNull(),
  updatedAt: text('updated_at').notNull(),
});
