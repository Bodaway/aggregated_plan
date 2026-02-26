import type { JiraIssue } from '@aggregated-plan/shared-types';
import {
  mapJiraIssueToTask,
  mapJiraProjectToCreateParams,
  mapJiraStatusToTaskStatus,
  mapJiraTypeToTaskType,
  mapJiraPriorityToProjectPriority,
  estimateHalfDaysFromStoryPoints,
} from '@domain/jira-mapping';
import type { JiraMappingContext } from '@domain/jira-mapping';

const makeIssue = (overrides?: Partial<JiraIssue>): JiraIssue => ({
  key: 'PROJ-1',
  summary: 'Implement login',
  description: 'User authentication feature',
  issueType: 'Story',
  status: 'To Do',
  priority: 'High',
  assignee: {
    accountId: 'user-abc',
    displayName: 'Alice',
    emailAddress: 'alice@example.com',
  },
  reporter: {
    accountId: 'user-def',
    displayName: 'Bob',
  },
  projectKey: 'PROJ',
  projectName: 'My Project',
  created: '2024-03-01',
  updated: '2024-03-05',
  dueDate: '2024-04-01',
  storyPoints: 5,
  labels: ['backend', 'auth'],
  ...overrides,
});

const makeContext = (overrides?: Partial<JiraMappingContext>): JiraMappingContext => ({
  taskId: 'task-1',
  now: '2024-03-10',
  projectId: 'project-1',
  defaultStartDate: '2024-01-01',
  defaultEndDate: '2024-12-31',
  ...overrides,
});

describe('jira-mapping', () => {
  describe('mapJiraStatusToTaskStatus', () => {
    it('maps "To Do" to todo', () => {
      expect(mapJiraStatusToTaskStatus('To Do')).toBe('todo');
    });

    it('maps "In Progress" to in-progress', () => {
      expect(mapJiraStatusToTaskStatus('In Progress')).toBe('in-progress');
    });

    it('maps "Done" to done', () => {
      expect(mapJiraStatusToTaskStatus('Done')).toBe('done');
    });

    it('maps "Closed" to done', () => {
      expect(mapJiraStatusToTaskStatus('Closed')).toBe('done');
    });

    it('maps "Resolved" to done', () => {
      expect(mapJiraStatusToTaskStatus('Resolved')).toBe('done');
    });

    it('maps "In Review" to in-progress', () => {
      expect(mapJiraStatusToTaskStatus('In Review')).toBe('in-progress');
    });

    it('maps unknown status to todo', () => {
      expect(mapJiraStatusToTaskStatus('Backlog')).toBe('todo');
    });
  });

  describe('mapJiraTypeToTaskType', () => {
    it('maps Story to development', () => {
      expect(mapJiraTypeToTaskType('Story')).toBe('development');
    });

    it('maps Bug to development', () => {
      expect(mapJiraTypeToTaskType('Bug')).toBe('development');
    });

    it('maps Task to development', () => {
      expect(mapJiraTypeToTaskType('Task')).toBe('development');
    });

    it('maps Epic to other', () => {
      expect(mapJiraTypeToTaskType('Epic')).toBe('other');
    });

    it('maps Sub-task to development', () => {
      expect(mapJiraTypeToTaskType('Sub-task')).toBe('development');
    });
  });

  describe('mapJiraPriorityToProjectPriority', () => {
    it('maps Highest to high', () => {
      expect(mapJiraPriorityToProjectPriority('Highest')).toBe('high');
    });

    it('maps High to high', () => {
      expect(mapJiraPriorityToProjectPriority('High')).toBe('high');
    });

    it('maps Medium to medium', () => {
      expect(mapJiraPriorityToProjectPriority('Medium')).toBe('medium');
    });

    it('maps Low to low', () => {
      expect(mapJiraPriorityToProjectPriority('Low')).toBe('low');
    });

    it('maps Lowest to low', () => {
      expect(mapJiraPriorityToProjectPriority('Lowest')).toBe('low');
    });

    it('returns undefined for undefined input', () => {
      expect(mapJiraPriorityToProjectPriority(undefined)).toBeUndefined();
    });
  });

  describe('estimateHalfDaysFromStoryPoints', () => {
    it('converts story points to half days (x2)', () => {
      expect(estimateHalfDaysFromStoryPoints(5)).toBe(10);
    });

    it('returns minimum 1 for small values', () => {
      expect(estimateHalfDaysFromStoryPoints(0.3)).toBe(1);
    });

    it('rounds to nearest integer', () => {
      expect(estimateHalfDaysFromStoryPoints(3)).toBe(6);
    });

    it('returns undefined for undefined input', () => {
      expect(estimateHalfDaysFromStoryPoints(undefined)).toBeUndefined();
    });
  });

  describe('mapJiraIssueToTask', () => {
    it('maps a complete Jira issue to a Task', () => {
      const issue = makeIssue();
      const context = makeContext();
      const task = mapJiraIssueToTask(issue, context);

      expect(task).toEqual({
        id: 'task-1',
        projectId: 'project-1',
        name: '[PROJ-1] Implement login',
        status: 'todo',
        type: 'development',
        estimateHalfDays: 10,
        dateRange: {
          startDate: '2024-04-01',
          endDate: '2024-04-01',
        },
        assigneeIds: ['user-abc'],
        createdAt: '2024-03-10',
        updatedAt: '2024-03-10',
      });
    });

    it('uses default dates when issue has no due date', () => {
      const issue = makeIssue({ dueDate: undefined });
      const context = makeContext();
      const task = mapJiraIssueToTask(issue, context);

      expect(task.dateRange).toEqual({
        startDate: '2024-01-01',
        endDate: '2024-12-31',
      });
    });

    it('sets empty assigneeIds when no assignee', () => {
      const issue = makeIssue({ assignee: undefined });
      const context = makeContext();
      const task = mapJiraIssueToTask(issue, context);

      expect(task.assigneeIds).toEqual([]);
    });

    it('sets undefined estimateHalfDays when no story points', () => {
      const issue = makeIssue({ storyPoints: undefined });
      const context = makeContext();
      const task = mapJiraIssueToTask(issue, context);

      expect(task.estimateHalfDays).toBeUndefined();
    });
  });

  describe('mapJiraProjectToCreateParams', () => {
    it('creates project params from a Jira issue', () => {
      const issue = makeIssue({ priority: 'High' });
      const params = mapJiraProjectToCreateParams(issue, {
        createdBy: 'user-1',
        startDate: '2024-01-01',
        endDate: '2024-12-31',
      });

      expect(params).toEqual({
        name: 'My Project',
        description: 'Imported from Jira project PROJ',
        startDate: '2024-01-01',
        endDate: '2024-12-31',
        status: 'active',
        client: undefined,
        priority: 'high',
        createdBy: 'user-1',
      });
    });

    it('maps undefined priority', () => {
      const issue = makeIssue({ priority: undefined });
      const params = mapJiraProjectToCreateParams(issue, {
        createdBy: 'user-1',
        startDate: '2024-01-01',
        endDate: '2024-12-31',
      });

      expect(params.priority).toBeUndefined();
    });
  });
});
