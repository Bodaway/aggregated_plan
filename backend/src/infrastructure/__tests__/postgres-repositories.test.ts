import * as path from 'node:path';
import { sql } from 'drizzle-orm';
import { getDatabaseConfig } from '../db-config';
import { applyPendingMigrations } from '../db/migration-runner';
import { createPostgresConnection } from '../postgres/db';
import type { PostgresRepositories } from '../postgres/postgres-repositories';
import { createPostgresRepositories } from '../postgres/postgres-repositories';

const migrationsFolder = path.resolve(__dirname, '../../../drizzle');

let connection: ReturnType<typeof createPostgresConnection>;
let repositories: PostgresRepositories;

const truncateAll = async (): Promise<void> => {
  await connection.db.execute(sql`
    TRUNCATE TABLE assignments,
      allocations,
      availabilities,
      milestones,
      projects,
      developers
  `);
};

const isPostgresAvailable = async (): Promise<boolean> => {
  try {
    connection = createPostgresConnection(getDatabaseConfig(process.env));
    await connection.db.execute(sql`SELECT 1`);
    return true;
  } catch {
    return false;
  }
};

describe('postgres repositories', () => {
  let dbAvailable = false;

  beforeAll(async () => {
    dbAvailable = await isPostgresAvailable();
    if (!dbAvailable) {
      return;
    }
    repositories = createPostgresRepositories(connection.db);
    await applyPendingMigrations(connection.db, migrationsFolder);
  });

  afterEach(async () => {
    if (dbAvailable) {
      await truncateAll();
    }
  });

  afterAll(async () => {
    if (dbAvailable) {
      await connection.close();
    }
  });

  it('persists project lifecycle', async () => {
    if (!dbAvailable) {
      console.warn('Skipping: PostgreSQL not available');
      return;
    }

    const project = {
      id: 'project-1',
      name: 'Alpha',
      description: 'First project',
      startDate: '2024-01-01',
      endDate: '2024-02-01',
      status: 'planning' as const,
      teamIds: ['dev-1'],
      client: 'Acme',
      priority: 'high' as const,
      createdAt: '2024-01-01',
      updatedAt: '2024-01-01',
      createdBy: 'user-1',
    };

    await repositories.projectRepository.save(project);
    expect(await repositories.projectRepository.list()).toEqual([project]);
    expect(await repositories.projectRepository.getById(project.id)).toEqual(project);
    expect(await repositories.projectRepository.getByName('alpha')).toEqual(project);

    const updated = { ...project, name: 'Beta', updatedAt: '2024-01-02' };
    await repositories.projectRepository.update(updated);
    expect(await repositories.projectRepository.getById(project.id)).toEqual(updated);

    await repositories.projectRepository.remove(project.id);
    expect(await repositories.projectRepository.getById(project.id)).toBeNull();
  });

  it('persists developers', async () => {
    if (!dbAvailable) {
      console.warn('Skipping: PostgreSQL not available');
      return;
    }

    const developer = {
      id: 'dev-1',
      displayName: 'Jean Dupont',
      email: 'jean@example.com',
      capacityHalfDaysPerWeek: 8,
    };

    await repositories.developerRepository.save(developer);
    expect(await repositories.developerRepository.list()).toEqual([developer]);

    const updated = { ...developer, displayName: 'Jean D.' };
    await repositories.developerRepository.update(updated);
    expect(await repositories.developerRepository.getById(developer.id)).toEqual(updated);
  });

  it('persists assignments with listByDeveloper', async () => {
    if (!dbAvailable) {
      console.warn('Skipping: PostgreSQL not available');
      return;
    }

    const assignment = {
      id: 'assign-1',
      projectId: 'project-1',
      developerId: 'dev-1',
      date: '2024-01-10',
      halfDay: 'morning' as const,
      createdAt: '2024-01-10',
    };

    await repositories.assignmentRepository.save(assignment);
    expect(await repositories.assignmentRepository.list()).toEqual([assignment]);
    expect(await repositories.assignmentRepository.listByDeveloper('dev-1')).toEqual([
      assignment,
    ]);
  });

  it('persists assignment batches', async () => {
    if (!dbAvailable) {
      console.warn('Skipping: PostgreSQL not available');
      return;
    }

    const assignments = [
      {
        id: 'assign-1',
        projectId: 'project-1',
        developerId: 'dev-1',
        date: '2024-01-10',
        halfDay: 'morning' as const,
        createdAt: '2024-01-10',
      },
      {
        id: 'assign-2',
        projectId: 'project-1',
        developerId: 'dev-1',
        date: '2024-01-10',
        halfDay: 'afternoon' as const,
        createdAt: '2024-01-10',
      },
    ];

    await repositories.assignmentRepository.saveMany(assignments);
    expect(await repositories.assignmentRepository.list()).toEqual(assignments);
  });

  it('persists allocations with listByDeveloper', async () => {
    if (!dbAvailable) {
      console.warn('Skipping: PostgreSQL not available');
      return;
    }

    const allocation = {
      id: 'alloc-1',
      projectId: 'project-1',
      developerId: 'dev-1',
      startDate: '2024-01-01',
      endDate: '2024-01-31',
      halfDaysPerWeek: 6,
      preferredWeekdays: ['monday', 'tuesday'],
      createdAt: '2024-01-01',
    };

    await repositories.allocationRepository.save(allocation);
    expect(await repositories.allocationRepository.list()).toEqual([allocation]);
    expect(await repositories.allocationRepository.listByDeveloper('dev-1')).toEqual([
      allocation,
    ]);
  });

  it('persists availabilities with listByDeveloper', async () => {
    if (!dbAvailable) {
      console.warn('Skipping: PostgreSQL not available');
      return;
    }

    const availability = {
      id: 'avail-1',
      developerId: 'dev-1',
      startDate: '2024-01-15',
      endDate: '2024-01-20',
      type: 'leave' as const,
      description: 'Vacation',
      createdAt: '2024-01-10',
    };

    await repositories.availabilityRepository.save(availability);
    expect(await repositories.availabilityRepository.list()).toEqual([availability]);
    expect(await repositories.availabilityRepository.listByDeveloper('dev-1')).toEqual([
      availability,
    ]);
  });

  it('persists milestones with listByProject', async () => {
    if (!dbAvailable) {
      console.warn('Skipping: PostgreSQL not available');
      return;
    }

    const milestone = {
      id: 'milestone-1',
      projectId: 'project-1',
      name: 'Kickoff',
      date: '2024-01-05',
      type: 'delivery' as const,
      createdAt: '2024-01-01',
      updatedAt: '2024-01-01',
    };

    await repositories.milestoneRepository.save(milestone);
    expect(await repositories.milestoneRepository.list()).toEqual([milestone]);
    expect(await repositories.milestoneRepository.listByProject('project-1')).toEqual([
      milestone,
    ]);
  });
});
