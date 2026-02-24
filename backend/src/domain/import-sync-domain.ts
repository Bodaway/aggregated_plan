import type {
  EntityId,
  IsoDateString,
  Project,
} from '@aggregated-plan/shared-types';
import type { CreateProjectParams, UpdateProjectParams } from './project-domain';

export type ExcelProjectSummary = {
  readonly name: string;
  readonly startDate: IsoDateString;
  readonly endDate: IsoDateString;
};

export type ProjectSyncItemCreated = {
  readonly action: 'created';
  readonly projectName: string;
  readonly createParams: CreateProjectParams;
};

export type ProjectSyncItemUpdated = {
  readonly action: 'updated';
  readonly projectName: string;
  readonly existingProjectId: EntityId;
  readonly updateParams: UpdateProjectParams;
};

export type ProjectSyncItemUnchanged = {
  readonly action: 'unchanged';
  readonly projectName: string;
  readonly existingProjectId: EntityId;
};

export type ProjectSyncItem =
  | ProjectSyncItemCreated
  | ProjectSyncItemUpdated
  | ProjectSyncItemUnchanged;

const normalizeProjectName = (name: string): string =>
  name.trim().toLowerCase();

const findMatchingProject = (
  name: string,
  existingProjects: readonly Project[],
): Project | undefined =>
  existingProjects.find(
    (p) => normalizeProjectName(p.name) === normalizeProjectName(name),
  );

const hasDateChanges = (
  excel: ExcelProjectSummary,
  existing: Project,
): boolean =>
  excel.startDate !== existing.startDate || excel.endDate !== existing.endDate;

const hasNameChange = (
  excel: ExcelProjectSummary,
  existing: Project,
): boolean =>
  normalizeProjectName(excel.name) !== normalizeProjectName(existing.name);

export const computeSyncPlan = (
  excelProjects: readonly ExcelProjectSummary[],
  existingProjects: readonly Project[],
  createdBy: EntityId,
): readonly ProjectSyncItem[] =>
  excelProjects.map((excel): ProjectSyncItem => {
    const existing = findMatchingProject(excel.name, existingProjects);

    if (!existing) {
      return {
        action: 'created',
        projectName: excel.name,
        createParams: {
          name: excel.name,
          startDate: excel.startDate,
          endDate: excel.endDate,
          createdBy,
        },
      };
    }

    if (hasDateChanges(excel, existing) || hasNameChange(excel, existing)) {
      const updateParams: UpdateProjectParams = {
        ...(hasNameChange(excel, existing) ? { name: excel.name.trim() } : {}),
        startDate: excel.startDate,
        endDate: excel.endDate,
      };

      return {
        action: 'updated',
        projectName: excel.name,
        existingProjectId: existing.id,
        updateParams,
      };
    }

    return {
      action: 'unchanged',
      projectName: excel.name,
      existingProjectId: existing.id,
    };
  });
