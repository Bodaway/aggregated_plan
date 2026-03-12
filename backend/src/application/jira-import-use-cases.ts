import type {
  JiraImportConfig,
  JiraImportError,
  JiraImportFilter,
  JiraImportResult,
  JiraIssue,
  Project,
  Task,
} from '@aggregated-plan/shared-types';
import {
  createDomainError,
  createProject,
  mapJiraIssueToTask,
  mapJiraProjectToCreateParams,
  ok,
  err,
} from '@domain/index';
import type {
  DomainError,
  JiraMappingContext,
  ProjectContext,
  Result,
} from '@domain/index';
import type { JiraConnector } from './jira-connector';
import type { ProjectRepository } from './project-repository';
import type { TaskRepository } from './task-repository';
import type { Clock, IdProvider } from './providers';

export type JiraImportUseCases = {
  readonly importIssues: (
    config: JiraImportConfig,
    filter: JiraImportFilter,
    createdBy: string,
  ) => Promise<Result<JiraImportResult, DomainError>>;
  readonly testConnection: (
    config: JiraImportConfig,
  ) => Promise<Result<{ readonly ok: boolean; readonly message: string }, DomainError>>;
  readonly previewIssues: (
    config: JiraImportConfig,
    filter: JiraImportFilter,
  ) => Promise<Result<readonly JiraIssue[], DomainError>>;
};

export const createJiraImportUseCases = (deps: {
  readonly jiraConnector: JiraConnector;
  readonly projectRepository: ProjectRepository;
  readonly taskRepository: TaskRepository;
  readonly idProvider: IdProvider;
  readonly clock: Clock;
}): JiraImportUseCases => {
  const testConnectionHandler = async (
    config: JiraImportConfig,
  ): Promise<Result<{ readonly ok: boolean; readonly message: string }, DomainError>> => {
    try {
      const result = await deps.jiraConnector.testConnection(config);
      return ok(result);
    } catch {
      return err(
        createDomainError('not-found', 'Failed to connect to Jira. Check your configuration.'),
      );
    }
  };

  const previewIssuesHandler = async (
    config: JiraImportConfig,
    filter: JiraImportFilter,
  ): Promise<Result<readonly JiraIssue[], DomainError>> => {
    try {
      const issues = await deps.jiraConnector.fetchIssues(config, filter);
      return ok(issues);
    } catch {
      return err(
        createDomainError('not-found', 'Failed to fetch issues from Jira.'),
      );
    }
  };

  const getOrCreateProject = async (
    issue: JiraIssue,
    createdBy: string,
  ): Promise<Result<Project, DomainError>> => {
    const existingProject = await deps.projectRepository.getByName(issue.projectName);
    if (existingProject) {
      return ok(existingProject);
    }

    const now = deps.clock();
    const context: ProjectContext = { id: deps.idProvider(), now };
    const params = mapJiraProjectToCreateParams(issue, {
      createdBy,
      startDate: now,
      endDate: now,
    });
    const projectResult = createProject(params, context);
    if (!projectResult.ok) {
      return projectResult;
    }

    const saved = await deps.projectRepository.save(projectResult.value);
    return ok(saved);
  };

  const importIssuesHandler = async (
    config: JiraImportConfig,
    filter: JiraImportFilter,
    createdBy: string,
  ): Promise<Result<JiraImportResult, DomainError>> => {
    let issues: readonly JiraIssue[];
    try {
      issues = await deps.jiraConnector.fetchIssues(config, filter);
    } catch {
      return err(
        createDomainError('not-found', 'Failed to fetch issues from Jira.'),
      );
    }

    if (issues.length === 0) {
      const now = deps.clock();
      return ok({
        importedAt: now,
        totalFetched: 0,
        totalImported: 0,
        skipped: 0,
        errors: [],
      });
    }

    const projectResult = await getOrCreateProject(issues[0], createdBy);
    if (!projectResult.ok) {
      return projectResult;
    }
    const project = projectResult.value;

    const now = deps.clock();
    const importedTasks: Task[] = [];
    const errors: JiraImportError[] = [];
    let skipped = 0;

    for (const issue of issues) {
      const existingTask = await deps.taskRepository.getByName(
        `[${issue.key}] ${issue.summary}`,
      );
      if (existingTask) {
        skipped += 1;
        continue;
      }

      try {
        const mappingContext: JiraMappingContext = {
          taskId: deps.idProvider(),
          now,
          projectId: project.id,
          defaultStartDate: project.startDate,
          defaultEndDate: project.endDate,
        };
        const task = mapJiraIssueToTask(issue, mappingContext);
        importedTasks.push(task);
      } catch {
        errors.push({
          issueKey: issue.key,
          reason: 'Failed to map Jira issue to task.',
        });
      }
    }

    if (importedTasks.length > 0) {
      await deps.taskRepository.saveMany(importedTasks);
    }

    return ok({
      importedAt: now,
      totalFetched: issues.length,
      totalImported: importedTasks.length,
      skipped,
      errors,
      projectId: project.id,
    });
  };

  return {
    importIssues: importIssuesHandler,
    testConnection: testConnectionHandler,
    previewIssues: previewIssuesHandler,
  };
};
