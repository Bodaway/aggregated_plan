import type {
  JiraIssue,
  JiraImportConfig,
  JiraImportFilter,
} from '@aggregated-plan/shared-types';

export type JiraConnector = {
  readonly fetchIssues: (
    config: JiraImportConfig,
    filter: JiraImportFilter,
  ) => Promise<readonly JiraIssue[]>;
  readonly testConnection: (
    config: JiraImportConfig,
  ) => Promise<{ readonly ok: boolean; readonly message: string }>;
};
