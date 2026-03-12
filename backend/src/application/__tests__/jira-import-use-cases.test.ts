import type { JiraIssue, JiraImportConfig, JiraImportFilter, Task } from '@aggregated-plan/shared-types';
import type { JiraConnector } from '@application/jira-connector';
import { createJiraImportUseCases } from '@application/jira-import-use-cases';
import { createInMemoryRepositories, createInMemoryStore } from '@infrastructure/index';

const makeConfig = (): JiraImportConfig => ({
  baseUrl: 'https://test.atlassian.net',
  email: 'test@example.com',
  apiToken: 'token-123',
  projectKey: 'PROJ',
});

const makeFilter = (overrides?: Partial<JiraImportFilter>): JiraImportFilter => ({
  projectKey: 'PROJ',
  ...overrides,
});

const makeIssue = (overrides?: Partial<JiraIssue>): JiraIssue => ({
  key: 'PROJ-1',
  summary: 'Implement login',
  description: 'Login feature',
  issueType: 'Story',
  status: 'To Do',
  priority: 'High',
  assignee: {
    accountId: 'user-abc',
    displayName: 'Alice',
  },
  reporter: undefined,
  projectKey: 'PROJ',
  projectName: 'My Project',
  created: '2024-03-01',
  updated: '2024-03-05',
  dueDate: '2024-04-01',
  storyPoints: 3,
  labels: [],
  ...overrides,
});

const createMockConnector = (issues: readonly JiraIssue[] = []): JiraConnector => ({
  fetchIssues: jest.fn().mockResolvedValue(issues),
  testConnection: jest.fn().mockResolvedValue({ ok: true, message: 'Connection successful' }),
});

const createFailingConnector = (): JiraConnector => ({
  fetchIssues: jest.fn().mockRejectedValue(new Error('Network error')),
  testConnection: jest.fn().mockRejectedValue(new Error('Network error')),
});

describe('jira-import-use-cases', () => {
  const createDeps = (connector: JiraConnector) => {
    const store = createInMemoryStore();
    const repositories = createInMemoryRepositories(store);
    const ids = Array.from({ length: 20 }, (_, i) => `id-${i}`);
    let idIndex = 0;
    const idProvider = () => ids[idIndex++] ?? `id-fallback-${idIndex}`;
    const clock = () => '2024-03-10' as const;

    return {
      store,
      repositories,
      useCases: createJiraImportUseCases({
        jiraConnector: connector,
        projectRepository: repositories.projectRepository,
        taskRepository: repositories.taskRepository,
        idProvider,
        clock,
      }),
    };
  };

  describe('testConnection', () => {
    it('returns success when connection succeeds', async () => {
      const connector = createMockConnector();
      const { useCases } = createDeps(connector);

      const result = await useCases.testConnection(makeConfig());

      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value.ok).toBe(true);
        expect(result.value.message).toBe('Connection successful');
      }
    });

    it('returns error when connection fails', async () => {
      const connector = createFailingConnector();
      const { useCases } = createDeps(connector);

      const result = await useCases.testConnection(makeConfig());

      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error.code).toBe('not-found');
      }
    });
  });

  describe('previewIssues', () => {
    it('returns fetched issues', async () => {
      const issues = [makeIssue(), makeIssue({ key: 'PROJ-2', summary: 'Fix bug' })];
      const connector = createMockConnector(issues);
      const { useCases } = createDeps(connector);

      const result = await useCases.previewIssues(makeConfig(), makeFilter());

      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value).toHaveLength(2);
        expect(result.value[0].key).toBe('PROJ-1');
        expect(result.value[1].key).toBe('PROJ-2');
      }
    });

    it('returns error when fetch fails', async () => {
      const connector = createFailingConnector();
      const { useCases } = createDeps(connector);

      const result = await useCases.previewIssues(makeConfig(), makeFilter());

      expect(result.ok).toBe(false);
    });
  });

  describe('importIssues', () => {
    it('imports issues and creates project and tasks', async () => {
      const issues = [
        makeIssue({ key: 'PROJ-1', summary: 'Task one' }),
        makeIssue({ key: 'PROJ-2', summary: 'Task two' }),
      ];
      const connector = createMockConnector(issues);
      const { useCases, store } = createDeps(connector);

      const result = await useCases.importIssues(makeConfig(), makeFilter(), 'user-1');

      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value.totalFetched).toBe(2);
        expect(result.value.totalImported).toBe(2);
        expect(result.value.skipped).toBe(0);
        expect(result.value.errors).toHaveLength(0);
        expect(result.value.projectId).toBeDefined();
      }

      expect(store.projects).toHaveLength(1);
      expect(store.projects[0].name).toBe('My Project');
      expect(store.tasks).toHaveLength(2);
      expect(store.tasks[0].name).toBe('[PROJ-1] Task one');
      expect(store.tasks[1].name).toBe('[PROJ-2] Task two');
    });

    it('reuses existing project with same name', async () => {
      const issues = [makeIssue()];
      const connector = createMockConnector(issues);
      const { useCases, store } = createDeps(connector);

      // Pre-create a project with the same name
      store.projects = [
        {
          id: 'existing-project',
          name: 'My Project',
          startDate: '2024-01-01',
          endDate: '2024-12-31',
          status: 'active',
          teamIds: [],
          createdAt: '2024-01-01',
          updatedAt: '2024-01-01',
          createdBy: 'user-0',
        },
      ];

      const result = await useCases.importIssues(makeConfig(), makeFilter(), 'user-1');

      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value.projectId).toBe('existing-project');
      }
      expect(store.projects).toHaveLength(1);
    });

    it('skips already imported tasks (by name)', async () => {
      const issues = [makeIssue({ key: 'PROJ-1', summary: 'Task one' })];
      const connector = createMockConnector(issues);
      const { useCases, store } = createDeps(connector);

      // Pre-create a task with the same name
      const existingTask: Task = {
        id: 'existing-task',
        projectId: 'some-project',
        name: '[PROJ-1] Task one',
        status: 'todo',
        type: 'development',
        dateRange: { startDate: '2024-01-01', endDate: '2024-12-31' },
        assigneeIds: [],
        createdAt: '2024-01-01',
        updatedAt: '2024-01-01',
      };
      store.tasks = [existingTask];

      const result = await useCases.importIssues(makeConfig(), makeFilter(), 'user-1');

      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value.totalFetched).toBe(1);
        expect(result.value.totalImported).toBe(0);
        expect(result.value.skipped).toBe(1);
      }
      expect(store.tasks).toHaveLength(1);
    });

    it('returns empty result when no issues found', async () => {
      const connector = createMockConnector([]);
      const { useCases } = createDeps(connector);

      const result = await useCases.importIssues(makeConfig(), makeFilter(), 'user-1');

      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value.totalFetched).toBe(0);
        expect(result.value.totalImported).toBe(0);
        expect(result.value.skipped).toBe(0);
      }
    });

    it('returns error when Jira fetch fails', async () => {
      const connector = createFailingConnector();
      const { useCases } = createDeps(connector);

      const result = await useCases.importIssues(makeConfig(), makeFilter(), 'user-1');

      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.error.code).toBe('not-found');
      }
    });
  });
});
