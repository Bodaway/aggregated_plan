import type {
  EntityId,
  ImportProjectResult,
  ImportResult,
} from '@aggregated-plan/shared-types';
import {
  parseExcelRow,
  groupRowsByProject,
  extractProjectDates,
  extractPhaseInfo,
} from '@domain/excel-parsing-domain';
import { computeSyncPlan } from '@domain/import-sync-domain';
import type { ExcelProjectSummary } from '@domain/import-sync-domain';
import { createProject, updateProject } from '@domain/project-domain';
import { createTask } from '@domain/task-domain';
import { createMilestone } from '@domain/milestone-domain';
import type { ProjectRepository } from './project-repository';
import type { TaskRepository } from './task-repository';
import type { MilestoneRepository } from './milestone-repository';
import type { SharePointAdapter } from './sharepoint-adapter';
import type { ExcelParserAdapter } from './excel-parser-adapter';
import type { IdProvider, Clock } from './providers';

export type ImportUseCases = {
  readonly importFromSharePoint: (
    graphToken: string,
    userId: EntityId,
  ) => Promise<ImportResult>;
};

type ImportUseCasesDeps = {
  readonly projectRepository: ProjectRepository;
  readonly taskRepository: TaskRepository;
  readonly milestoneRepository: MilestoneRepository;
  readonly sharePointAdapter: SharePointAdapter;
  readonly excelParserAdapter: ExcelParserAdapter;
  readonly idProvider: IdProvider;
  readonly clock: Clock;
  readonly referenceYear: number;
};

export const createImportUseCases = (deps: ImportUseCasesDeps): ImportUseCases => {
  const importFromSharePoint = async (
    graphToken: string,
    userId: EntityId,
  ): Promise<ImportResult> => {
    // 1. Download Excel file
    const { buffer } = await deps.sharePointAdapter.downloadFile(graphToken);

    // 2. Parse workbook to raw rows
    const rawRows = await deps.excelParserAdapter.parseWorkbook(buffer);

    // 3. Parse each raw row
    const parseErrors: string[] = [];
    const parsedRows = rawRows.flatMap((raw) => {
      const result = parseExcelRow(raw, deps.referenceYear);
      if (!result.ok) {
        parseErrors.push(result.error.message);
        return [];
      }
      return [result.value];
    });

    // 4. Group by project
    const grouped = groupRowsByProject(parsedRows);

    // 5. Build project summaries
    const excelProjects: readonly ExcelProjectSummary[] = [...grouped.entries()].map(
      ([name, rows]) => {
        const dates = extractProjectDates(rows);
        return { name, ...dates };
      },
    );

    // 6. Fetch existing projects
    const existingProjects = await deps.projectRepository.list();

    // 7. Compute sync plan
    const syncPlan = computeSyncPlan(excelProjects, existingProjects, userId);

    // 8. Execute sync plan and build project results
    const projectResults: ImportProjectResult[] = [];

    for (const item of syncPlan) {
      const now = deps.clock();
      let projectId: EntityId;

      if (item.action === 'created') {
        const projectResult = createProject(item.createParams, {
          id: deps.idProvider(),
          now,
        });
        if (!projectResult.ok) {
          parseErrors.push(`Failed to create project ${item.projectName}: ${projectResult.error.message}`);
          continue;
        }
        const saved = await deps.projectRepository.save(projectResult.value);
        projectId = saved.id;
      } else if (item.action === 'updated') {
        const existing = await deps.projectRepository.getById(item.existingProjectId);
        if (!existing) {
          parseErrors.push(`Project ${item.projectName} not found for update`);
          continue;
        }
        const updatedResult = updateProject(existing, item.updateParams, { now });
        if (!updatedResult.ok) {
          parseErrors.push(`Failed to update project ${item.projectName}: ${updatedResult.error.message}`);
          continue;
        }
        await deps.projectRepository.update(updatedResult.value);
        projectId = item.existingProjectId;
      } else {
        projectId = item.existingProjectId;
      }

      // 9. Create tasks and milestones for rows in this project
      const projectRows = grouped.get(item.projectName) ?? [];
      let tasksCreated = 0;
      let milestonesCreated = 0;

      for (const row of projectRows) {
        if (row.isPhaseHeader) {
          const phaseInfo = extractPhaseInfo(row.task);
          const msResult = createMilestone(
            {
              projectId,
              name: phaseInfo.name,
              date: row.startDate,
              type: 'other',
            },
            { id: deps.idProvider(), now },
          );
          if (msResult.ok) {
            await deps.milestoneRepository.save(msResult.value);
            milestonesCreated++;
          }
        } else {
          const taskResult = createTask(
            {
              name: row.task,
              description: row.comment,
              projectId,
              dueDate: row.endDate,
            },
            { id: deps.idProvider(), now },
          );
          if (taskResult.ok) {
            await deps.taskRepository.save(taskResult.value);
            tasksCreated++;
          }
        }
      }

      projectResults.push({
        projectName: item.projectName,
        action: item.action,
        tasksCreated,
        milestonesCreated,
      });
    }

    return {
      totalRowsParsed: parsedRows.length,
      parseErrors,
      projects: projectResults,
    };
  };

  return { importFromSharePoint };
};
