import { computeSyncPlan } from '@domain/import-sync-domain';
import type { Project } from '@aggregated-plan/shared-types';

const makeProject = (overrides: Partial<Project> & { readonly name: string }): Project => ({
  id: 'proj-1',
  name: overrides.name,
  startDate: '2026-01-01',
  endDate: '2026-06-30',
  status: 'planning',
  teamIds: [],
  createdAt: '2026-01-01',
  updatedAt: '2026-01-01',
  createdBy: 'admin',
  ...overrides,
});

describe('import-sync-domain', () => {
  describe('computeSyncPlan', () => {
    it('marks new project as created', () => {
      const excelProjects = [
        {
          name: 'Project Alpha',
          startDate: '2026-01-01' as const,
          endDate: '2026-06-30' as const,
        },
      ];
      const existingProjects: readonly Project[] = [];

      const plan = computeSyncPlan(excelProjects, existingProjects, 'user-1');
      expect(plan).toHaveLength(1);
      expect(plan[0].action).toBe('created');
      expect(plan[0].projectName).toBe('Project Alpha');
      if (plan[0].action === 'created') {
        expect(plan[0].createParams.name).toBe('Project Alpha');
        expect(plan[0].createParams.startDate).toBe('2026-01-01');
        expect(plan[0].createParams.endDate).toBe('2026-06-30');
        expect(plan[0].createParams.createdBy).toBe('user-1');
      }
    });

    it('marks existing project with different dates as updated', () => {
      const excelProjects = [
        {
          name: 'Project Alpha',
          startDate: '2026-02-01' as const,
          endDate: '2026-07-15' as const,
        },
      ];
      const existingProjects: readonly Project[] = [
        makeProject({
          id: 'proj-1',
          name: 'Project Alpha',
          startDate: '2026-01-01',
          endDate: '2026-06-30',
        }),
      ];

      const plan = computeSyncPlan(excelProjects, existingProjects, 'user-1');
      expect(plan).toHaveLength(1);
      expect(plan[0].action).toBe('updated');
      if (plan[0].action === 'updated') {
        expect(plan[0].updateParams.startDate).toBe('2026-02-01');
        expect(plan[0].updateParams.endDate).toBe('2026-07-15');
        expect(plan[0].existingProjectId).toBe('proj-1');
      }
    });

    it('marks existing project with same data as unchanged', () => {
      const excelProjects = [
        {
          name: 'Project Alpha',
          startDate: '2026-01-01' as const,
          endDate: '2026-06-30' as const,
        },
      ];
      const existingProjects: readonly Project[] = [
        makeProject({
          id: 'proj-1',
          name: 'Project Alpha',
          startDate: '2026-01-01',
          endDate: '2026-06-30',
        }),
      ];

      const plan = computeSyncPlan(excelProjects, existingProjects, 'user-1');
      expect(plan).toHaveLength(1);
      expect(plan[0].action).toBe('unchanged');
    });

    it('matches by name case-insensitively', () => {
      const excelProjects = [
        {
          name: '  project alpha  ',
          startDate: '2026-01-01' as const,
          endDate: '2026-06-30' as const,
        },
      ];
      const existingProjects: readonly Project[] = [
        makeProject({ id: 'proj-1', name: 'Project Alpha' }),
      ];

      const plan = computeSyncPlan(excelProjects, existingProjects, 'user-1');
      expect(plan).toHaveLength(1);
      expect(plan[0].action).toBe('unchanged');
    });

    it('handles multiple projects with mixed actions', () => {
      const excelProjects = [
        { name: 'New Project', startDate: '2026-03-01' as const, endDate: '2026-09-01' as const },
        { name: 'Existing Same', startDate: '2026-01-01' as const, endDate: '2026-06-30' as const },
        { name: 'Existing Changed', startDate: '2026-02-01' as const, endDate: '2026-08-01' as const },
      ];
      const existingProjects: readonly Project[] = [
        makeProject({ id: 'proj-1', name: 'Existing Same', startDate: '2026-01-01', endDate: '2026-06-30' }),
        makeProject({ id: 'proj-2', name: 'Existing Changed', startDate: '2026-01-01', endDate: '2026-06-30' }),
      ];

      const plan = computeSyncPlan(excelProjects, existingProjects, 'user-1');
      expect(plan).toHaveLength(3);

      const created = plan.find((p) => p.projectName === 'New Project');
      const unchanged = plan.find((p) => p.projectName === 'Existing Same');
      const updated = plan.find((p) => p.projectName === 'Existing Changed');

      expect(created?.action).toBe('created');
      expect(unchanged?.action).toBe('unchanged');
      expect(updated?.action).toBe('updated');
    });

    it('never overwrites status, priority, teamIds, or client on update', () => {
      const excelProjects = [
        {
          name: 'Project Alpha',
          startDate: '2026-02-01' as const,
          endDate: '2026-07-15' as const,
        },
      ];
      const existingProjects: readonly Project[] = [
        makeProject({
          id: 'proj-1',
          name: 'Project Alpha',
          startDate: '2026-01-01',
          endDate: '2026-06-30',
          status: 'active',
          priority: 'high',
          teamIds: ['team-1'],
          client: 'ACME Corp',
        }),
      ];

      const plan = computeSyncPlan(excelProjects, existingProjects, 'user-1');
      expect(plan[0].action).toBe('updated');
      if (plan[0].action === 'updated') {
        const params = plan[0].updateParams;
        expect(params).not.toHaveProperty('status');
        expect(params).not.toHaveProperty('priority');
        expect(params).not.toHaveProperty('teamIds');
        expect(params).not.toHaveProperty('client');
      }
    });

    it('detects name change along with dates', () => {
      const excelProjects = [
        {
          name: 'Project Alpha v2',
          startDate: '2026-01-01' as const,
          endDate: '2026-06-30' as const,
        },
      ];
      // No existing project matches → treated as new
      const existingProjects: readonly Project[] = [
        makeProject({ id: 'proj-1', name: 'Project Alpha' }),
      ];

      const plan = computeSyncPlan(excelProjects, existingProjects, 'user-1');
      expect(plan).toHaveLength(1);
      expect(plan[0].action).toBe('created');
    });
  });
});
