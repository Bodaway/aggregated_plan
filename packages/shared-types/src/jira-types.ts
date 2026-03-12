import type { IsoDateString } from './time-types';
import type { EntityId } from './user-types';

export type JiraIssueType = 'Story' | 'Task' | 'Bug' | 'Epic' | 'Sub-task';

export type JiraIssueStatus = 'To Do' | 'In Progress' | 'Done' | string;

export type JiraPriority = 'Highest' | 'High' | 'Medium' | 'Low' | 'Lowest';

export type JiraUser = {
  readonly accountId: string;
  readonly displayName: string;
  readonly emailAddress?: string;
};

export type JiraIssue = {
  readonly key: string;
  readonly summary: string;
  readonly description?: string;
  readonly issueType: JiraIssueType;
  readonly status: JiraIssueStatus;
  readonly priority?: JiraPriority;
  readonly assignee?: JiraUser;
  readonly reporter?: JiraUser;
  readonly projectKey: string;
  readonly projectName: string;
  readonly created: IsoDateString;
  readonly updated: IsoDateString;
  readonly dueDate?: IsoDateString;
  readonly storyPoints?: number;
  readonly labels: readonly string[];
  readonly parentKey?: string;
};

export type JiraImportConfig = {
  readonly baseUrl: string;
  readonly email: string;
  readonly apiToken: string;
  readonly projectKey: string;
};

export type JiraImportFilter = {
  readonly projectKey: string;
  readonly issueTypes?: readonly JiraIssueType[];
  readonly statuses?: readonly string[];
  readonly jql?: string;
  readonly maxResults?: number;
};

export type JiraImportResult = {
  readonly importedAt: IsoDateString;
  readonly totalFetched: number;
  readonly totalImported: number;
  readonly skipped: number;
  readonly errors: readonly JiraImportError[];
  readonly projectId?: EntityId;
};

export type JiraImportError = {
  readonly issueKey: string;
  readonly reason: string;
};

export type JiraSyncStatus = 'idle' | 'syncing' | 'success' | 'error';

export type JiraSyncConfig = {
  readonly enabled: boolean;
  readonly intervalMinutes: number;
  readonly filter: JiraImportFilter;
  readonly lastSyncAt?: IsoDateString;
  readonly status: JiraSyncStatus;
};
