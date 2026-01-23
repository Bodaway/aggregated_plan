import { serve } from '@hono/node-server';
import { Hono } from 'hono';
import { cors } from 'hono/cors';
import { z } from 'zod';
import type { Context } from 'hono';
import type { DomainError, Result } from '@domain/index';
import {
  createAvailabilityUseCases,
  createDeveloperUseCases,
  createProjectUseCases,
  createStaffingUseCases,
} from '@application/index';
import {
  createClock,
  createIdProvider,
  createInMemoryRepositories,
  createInMemoryStore,
} from '@infrastructure/index';

const app = new Hono();
app.use('*', cors());

const store = createInMemoryStore({
  developers: [
    {
      id: 'developer-1',
      displayName: 'Jean Dupont',
      email: 'jean.dupont@example.com',
      capacityHalfDaysPerWeek: 10,
    },
    {
      id: 'developer-2',
      displayName: 'Maria Silva',
      email: 'maria.silva@example.com',
      capacityHalfDaysPerWeek: 8,
    },
  ],
});
const repositories = createInMemoryRepositories(store);
const idProvider = createIdProvider();
const clock = createClock();

const projectUseCases = createProjectUseCases({
  projectRepository: repositories.projectRepository,
  idProvider,
  clock,
});
const staffingUseCases = createStaffingUseCases({
  assignmentRepository: repositories.assignmentRepository,
  allocationRepository: repositories.allocationRepository,
  availabilityRepository: repositories.availabilityRepository,
  developerRepository: repositories.developerRepository,
  idProvider,
  clock,
});
const availabilityUseCases = createAvailabilityUseCases({
  availabilityRepository: repositories.availabilityRepository,
  idProvider,
  clock,
});
const developerUseCases = createDeveloperUseCases({
  developerRepository: repositories.developerRepository,
  idProvider,
});

const isoDateSchema = z
  .string()
  .regex(/^\d{4}-\d{2}-\d{2}$/, 'Date must be YYYY-MM-DD');
const halfDaySchema = z.enum(['morning', 'afternoon']);
const weekdaySchema = z.enum([
  'monday',
  'tuesday',
  'wednesday',
  'thursday',
  'friday',
  'saturday',
  'sunday',
]);
const projectStatusSchema = z.enum([
  'planning',
  'active',
  'paused',
  'completed',
  'cancelled',
]);
const projectPrioritySchema = z.enum(['high', 'medium', 'low']);

const respondWithResult = <T>(
  c: Context,
  result: Result<T, DomainError>,
  successStatus = 200,
): Response => {
  if (result.ok) {
    return c.json(result.value, successStatus);
  }
  const status = (() => {
    switch (result.error.code) {
      case 'not-found':
        return 404;
      case 'duplicate-name':
      case 'assignment-conflict':
        return 409;
      default:
        return 400;
    }
  })();
  return c.json({ error: result.error }, status);
};

app.get('/', (c) => c.json({ message: 'Aggregated Plan API' }));

app.get('/projects', async (c) => {
  const projects = await projectUseCases.listProjects();
  return c.json(projects);
});

app.get('/projects/:id', async (c) => {
  const project = await projectUseCases.getProject(c.req.param('id'));
  if (!project) {
    return c.json({ error: 'Project not found' }, 404);
  }
  return c.json(project);
});

app.post('/projects', async (c) => {
  const schema = z.object({
    name: z.string().min(1),
    description: z.string().optional(),
    startDate: isoDateSchema,
    endDate: isoDateSchema,
    status: projectStatusSchema.optional(),
    teamIds: z.array(z.string()).optional(),
    client: z.string().optional(),
    priority: projectPrioritySchema.optional(),
    createdBy: z.string().min(1),
  });
  const parseResult = schema.safeParse(await c.req.json());
  if (!parseResult.success) {
    return c.json({ error: 'Invalid payload', details: parseResult.error.flatten() }, 400);
  }
  const result = await projectUseCases.createProject(parseResult.data);
  return respondWithResult(c, result, 201);
});

app.put('/projects/:id', async (c) => {
  const schema = z.object({
    name: z.string().min(1).optional(),
    description: z.string().optional(),
    startDate: isoDateSchema.optional(),
    endDate: isoDateSchema.optional(),
    status: projectStatusSchema.optional(),
    teamIds: z.array(z.string()).optional(),
    client: z.string().optional(),
    priority: projectPrioritySchema.optional(),
  });
  const parseResult = schema.safeParse(await c.req.json());
  if (!parseResult.success) {
    return c.json({ error: 'Invalid payload', details: parseResult.error.flatten() }, 400);
  }
  const result = await projectUseCases.updateProject(c.req.param('id'), parseResult.data);
  return respondWithResult(c, result);
});

app.delete('/projects/:id', async (c) => {
  const result = await projectUseCases.deleteProject(c.req.param('id'));
  if (result.ok) {
    return c.body(null, 204);
  }
  return respondWithResult(c, result);
});

app.get('/developers', async (c) => {
  const developers = await developerUseCases.listDevelopers();
  return c.json(developers);
});

app.post('/developers', async (c) => {
  const schema = z.object({
    displayName: z.string().min(1),
    email: z.string().min(3),
    capacityHalfDaysPerWeek: z.number().int().min(1).max(10).optional(),
  });
  const parseResult = schema.safeParse(await c.req.json());
  if (!parseResult.success) {
    return c.json({ error: 'Invalid payload', details: parseResult.error.flatten() }, 400);
  }
  const result = await developerUseCases.createDeveloper(parseResult.data);
  return respondWithResult(c, result, 201);
});

app.get('/assignments', async (c) => {
  const assignments = await staffingUseCases.listAssignments();
  return c.json(assignments);
});

app.post('/assignments', async (c) => {
  const schema = z.object({
    projectId: z.string().min(1),
    developerId: z.string().min(1),
    date: isoDateSchema,
    halfDay: halfDaySchema,
  });
  const parseResult = schema.safeParse(await c.req.json());
  if (!parseResult.success) {
    return c.json({ error: 'Invalid payload', details: parseResult.error.flatten() }, 400);
  }
  const result = await staffingUseCases.createAssignment(parseResult.data);
  return respondWithResult(c, result, 201);
});

app.post('/allocations', async (c) => {
  const schema = z.object({
    projectId: z.string().min(1),
    developerId: z.string().min(1),
    startDate: isoDateSchema,
    endDate: isoDateSchema,
    halfDaysPerWeek: z.number().int().min(1).max(10),
    preferredWeekdays: z.array(weekdaySchema).optional(),
  });
  const parseResult = schema.safeParse(await c.req.json());
  if (!parseResult.success) {
    return c.json({ error: 'Invalid payload', details: parseResult.error.flatten() }, 400);
  }
  const result = await staffingUseCases.createWeeklyAllocation(parseResult.data);
  return respondWithResult(c, result, 201);
});

app.get('/conflicts', async (c) => {
  const conflicts = await staffingUseCases.listConflicts();
  return c.json(conflicts);
});

app.get('/availabilities', async (c) => {
  const availabilities = await availabilityUseCases.listAvailabilities();
  return c.json(availabilities);
});

app.post('/availabilities', async (c) => {
  const schema = z.object({
    developerId: z.string().min(1),
    startDate: isoDateSchema,
    endDate: isoDateSchema,
    type: z.enum(['leave', 'training', 'unavailable', 'other']),
    description: z.string().optional(),
  });
  const parseResult = schema.safeParse(await c.req.json());
  if (!parseResult.success) {
    return c.json({ error: 'Invalid payload', details: parseResult.error.flatten() }, 400);
  }
  const result = await availabilityUseCases.createAvailability(parseResult.data);
  return respondWithResult(c, result, 201);
});

const port = 3001;
console.log(`Server is running on port ${port}`);

serve({
  fetch: app.fetch,
  port,
});
