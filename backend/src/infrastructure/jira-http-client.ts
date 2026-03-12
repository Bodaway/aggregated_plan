import type {
  JiraIssue,
  JiraIssueType,
  JiraImportConfig,
  JiraImportFilter,
  JiraPriority,
  IsoDateString,
} from '@aggregated-plan/shared-types';
import type { JiraConnector } from '@application/index';

type JiraApiIssue = {
  readonly key: string;
  readonly fields: {
    readonly summary: string;
    readonly description?: string | null;
    readonly issuetype: { readonly name: string };
    readonly status: { readonly name: string };
    readonly priority?: { readonly name: string } | null;
    readonly assignee?: {
      readonly accountId: string;
      readonly displayName: string;
      readonly emailAddress?: string;
    } | null;
    readonly reporter?: {
      readonly accountId: string;
      readonly displayName: string;
      readonly emailAddress?: string;
    } | null;
    readonly project: {
      readonly key: string;
      readonly name: string;
    };
    readonly created: string;
    readonly updated: string;
    readonly duedate?: string | null;
    readonly customfield_10016?: number | null;
    readonly labels: readonly string[];
    readonly parent?: { readonly key: string } | null;
  };
};

type JiraSearchResponse = {
  readonly issues: readonly JiraApiIssue[];
  readonly total: number;
  readonly maxResults: number;
  readonly startAt: number;
};

const toIsoDate = (dateString: string): IsoDateString => {
  const date = dateString.slice(0, 10);
  return date as IsoDateString;
};

const mapApiIssueToJiraIssue = (apiIssue: JiraApiIssue): JiraIssue => ({
  key: apiIssue.key,
  summary: apiIssue.fields.summary,
  description: apiIssue.fields.description ?? undefined,
  issueType: apiIssue.fields.issuetype.name as JiraIssueType,
  status: apiIssue.fields.status.name,
  priority: (apiIssue.fields.priority?.name as JiraPriority) ?? undefined,
  assignee: apiIssue.fields.assignee
    ? {
        accountId: apiIssue.fields.assignee.accountId,
        displayName: apiIssue.fields.assignee.displayName,
        emailAddress: apiIssue.fields.assignee.emailAddress,
      }
    : undefined,
  reporter: apiIssue.fields.reporter
    ? {
        accountId: apiIssue.fields.reporter.accountId,
        displayName: apiIssue.fields.reporter.displayName,
        emailAddress: apiIssue.fields.reporter.emailAddress,
      }
    : undefined,
  projectKey: apiIssue.fields.project.key,
  projectName: apiIssue.fields.project.name,
  created: toIsoDate(apiIssue.fields.created),
  updated: toIsoDate(apiIssue.fields.updated),
  dueDate: apiIssue.fields.duedate ? toIsoDate(apiIssue.fields.duedate) : undefined,
  storyPoints: apiIssue.fields.customfield_10016 ?? undefined,
  labels: apiIssue.fields.labels,
  parentKey: apiIssue.fields.parent?.key ?? undefined,
});

const buildJql = (filter: JiraImportFilter): string => {
  if (filter.jql) {
    return filter.jql;
  }
  const clauses: string[] = [`project = "${filter.projectKey}"`];
  if (filter.issueTypes && filter.issueTypes.length > 0) {
    const types = filter.issueTypes.map((t: string) => `"${t}"`).join(', ');
    clauses.push(`issuetype in (${types})`);
  }
  if (filter.statuses && filter.statuses.length > 0) {
    const statuses = filter.statuses.map((s: string) => `"${s}"`).join(', ');
    clauses.push(`status in (${statuses})`);
  }
  return clauses.join(' AND ');
};

const createAuthHeader = (config: JiraImportConfig): string => {
  const credentials = Buffer.from(`${config.email}:${config.apiToken}`).toString('base64');
  return `Basic ${credentials}`;
};

export const createJiraHttpClient = (): JiraConnector => {
  const fetchIssues = async (
    config: JiraImportConfig,
    filter: JiraImportFilter,
  ): Promise<readonly JiraIssue[]> => {
    const jql = buildJql(filter);
    const maxResults = filter.maxResults ?? 50;
    const url = `${config.baseUrl}/rest/api/3/search`;
    const body = JSON.stringify({
      jql,
      maxResults,
      fields: [
        'summary',
        'description',
        'issuetype',
        'status',
        'priority',
        'assignee',
        'reporter',
        'project',
        'created',
        'updated',
        'duedate',
        'customfield_10016',
        'labels',
        'parent',
      ],
    });

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Authorization': createAuthHeader(config),
        'Content-Type': 'application/json',
        'Accept': 'application/json',
      },
      body,
    });

    if (!response.ok) {
      const text = await response.text();
      throw new Error(`Jira API error ${response.status}: ${text}`);
    }

    const data = (await response.json()) as JiraSearchResponse;
    return data.issues.map(mapApiIssueToJiraIssue);
  };

  const testConnection = async (
    config: JiraImportConfig,
  ): Promise<{ readonly ok: boolean; readonly message: string }> => {
    const url = `${config.baseUrl}/rest/api/3/myself`;
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        'Authorization': createAuthHeader(config),
        'Accept': 'application/json',
      },
    });

    if (!response.ok) {
      return { ok: false, message: `Connection failed with status ${response.status}` };
    }

    return { ok: true, message: 'Connection successful' };
  };

  return { fetchIssues, testConnection };
};

export { buildJql, mapApiIssueToJiraIssue, toIsoDate };
