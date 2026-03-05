import type {
  EntityId,
  IsoDateString,
  JiraIssue,
  JiraIssueStatus,
  JiraIssueType,
  JiraPriority,
  ProjectPriority,
  Task,
  TaskStatus,
  TaskType,
} from '@aggregated-plan/shared-types';
import type { CreateProjectParams } from './project-domain';

export type JiraMappingContext = {
  readonly taskId: EntityId;
  readonly now: IsoDateString;
  readonly projectId: EntityId;
  readonly defaultStartDate: IsoDateString;
  readonly defaultEndDate: IsoDateString;
};

const mapJiraStatusToTaskStatus = (status: JiraIssueStatus): TaskStatus => {
  const normalized = status.toLowerCase().trim();
  if (normalized === 'done' || normalized === 'closed' || normalized === 'resolved') {
    return 'done';
  }
  if (normalized === 'in progress' || normalized === 'in review' || normalized === 'in development') {
    return 'in-progress';
  }
  return 'todo';
};

const mapJiraTypeToTaskType = (issueType: JiraIssueType): TaskType => {
  switch (issueType) {
    case 'Bug':
      return 'development';
    case 'Story':
      return 'development';
    case 'Task':
      return 'development';
    case 'Sub-task':
      return 'development';
    case 'Epic':
      return 'other';
    default:
      return 'other';
  }
};

const mapJiraPriorityToProjectPriority = (
  priority: JiraPriority | undefined,
): ProjectPriority | undefined => {
  if (!priority) return undefined;
  switch (priority) {
    case 'Highest':
    case 'High':
      return 'high';
    case 'Medium':
      return 'medium';
    case 'Low':
    case 'Lowest':
      return 'low';
    default:
      return undefined;
  }
};

const estimateHalfDaysFromStoryPoints = (
  storyPoints: number | undefined,
): number | undefined => {
  if (storyPoints === undefined) return undefined;
  return Math.max(1, Math.round(storyPoints * 2));
};

export const mapJiraIssueToTask = (
  issue: JiraIssue,
  context: JiraMappingContext,
): Task => ({
  id: context.taskId,
  projectId: context.projectId,
  name: `[${issue.key}] ${issue.summary}`,
  status: mapJiraStatusToTaskStatus(issue.status),
  type: mapJiraTypeToTaskType(issue.issueType),
  estimateHalfDays: estimateHalfDaysFromStoryPoints(issue.storyPoints),
  dateRange: {
    startDate: issue.dueDate ?? context.defaultStartDate,
    endDate: issue.dueDate ?? context.defaultEndDate,
  },
  assigneeIds: issue.assignee ? [issue.assignee.accountId] : [],
  createdAt: context.now,
  updatedAt: context.now,
});

export const mapJiraProjectToCreateParams = (
  issue: JiraIssue,
  context: { readonly createdBy: EntityId; readonly startDate: IsoDateString; readonly endDate: IsoDateString },
): CreateProjectParams => ({
  name: issue.projectName,
  description: `Imported from Jira project ${issue.projectKey}`,
  startDate: context.startDate,
  endDate: context.endDate,
  status: 'active',
  client: undefined,
  priority: mapJiraPriorityToProjectPriority(issue.priority),
  createdBy: context.createdBy,
});

export {
  mapJiraStatusToTaskStatus,
  mapJiraTypeToTaskType,
  mapJiraPriorityToProjectPriority,
  estimateHalfDaysFromStoryPoints,
};
