import type { IsoDateString } from './time-types';

export type ExcelProjectRow = {
  readonly topic: string;
  readonly task: string;
  readonly startDate: IsoDateString;
  readonly endDate: IsoDateString;
  readonly comment: string | undefined;
  readonly isPhaseHeader: boolean;
};

export type ImportSyncAction = 'created' | 'updated' | 'unchanged';

export type ImportProjectResult = {
  readonly projectName: string;
  readonly action: ImportSyncAction;
  readonly tasksCreated: number;
  readonly milestonesCreated: number;
};

export type ImportResult = {
  readonly totalRowsParsed: number;
  readonly parseErrors: readonly string[];
  readonly projects: readonly ImportProjectResult[];
};
