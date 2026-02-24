import type { ImportResult } from '@aggregated-plan/shared-types';
import { createImportUseCases } from '@application/import-use-cases';
import type { ExcelParserAdapter } from '@application/excel-parser-adapter';
import type { SharePointAdapter } from '@application/sharepoint-adapter';
import type { RawExcelRow } from '@domain/excel-parsing-domain';
import { createInMemoryStore } from '@infrastructure/in-memory-store';
import { createInMemoryRepositories } from '@infrastructure/in-memory-repositories';

const createMockSharePointAdapter = (
  rows: readonly RawExcelRow[],
): { sharePointAdapter: SharePointAdapter; excelParserAdapter: ExcelParserAdapter } => ({
  sharePointAdapter: {
    downloadFile: async () => ({
      buffer: Buffer.from('mock'),
      fileName: 'test.xlsx',
    }),
  },
  excelParserAdapter: {
    parseWorkbook: async () => rows,
  },
});

let idCounter = 0;
const createTestIdProvider = () => () => `id-${++idCounter}`;
const createTestClock = () => () => '2026-01-15' as const;

describe('import-use-cases', () => {
  beforeEach(() => {
    idCounter = 0;
  });

  it('imports a new project from Excel rows', async () => {
    const store = createInMemoryStore();
    const repos = createInMemoryRepositories(store);
    const rawRows: readonly RawExcelRow[] = [
      {
        topic: 'Project Alpha',
        task: '1. Conception, Epic : SCB-267',
        startDate: '16-janv',
        endDate: '20-janv',
        comment: undefined,
      },
      {
        topic: 'Project Alpha',
        task: 'Dev Interface UI',
        startDate: '21-janv',
        endDate: '15-févr',
        comment: 'Main UI work',
      },
    ];
    const { sharePointAdapter, excelParserAdapter } = createMockSharePointAdapter(rawRows);

    const importUseCases = createImportUseCases({
      projectRepository: repos.projectRepository,
      taskRepository: repos.taskRepository,
      milestoneRepository: repos.milestoneRepository,
      sharePointAdapter,
      excelParserAdapter,
      idProvider: createTestIdProvider(),
      clock: createTestClock(),
      referenceYear: 2026,
    });

    const result: ImportResult = await importUseCases.importFromSharePoint('mock-graph-token', 'user-1');

    expect(result.totalRowsParsed).toBe(2);
    expect(result.parseErrors).toHaveLength(0);
    expect(result.projects).toHaveLength(1);
    expect(result.projects[0].projectName).toBe('Project Alpha');
    expect(result.projects[0].action).toBe('created');
    expect(result.projects[0].tasksCreated).toBe(1);
    expect(result.projects[0].milestonesCreated).toBe(1);

    const projects = await repos.projectRepository.list();
    expect(projects).toHaveLength(1);
    expect(projects[0].name).toBe('Project Alpha');
    expect(projects[0].startDate).toBe('2026-01-16');
    expect(projects[0].endDate).toBe('2026-02-15');
  });

  it('updates existing project dates on re-import', async () => {
    const store = createInMemoryStore();
    const repos = createInMemoryRepositories(store);

    // Seed an existing project
    await repos.projectRepository.save({
      id: 'existing-proj',
      name: 'Project Alpha',
      startDate: '2026-01-01',
      endDate: '2026-03-01',
      status: 'active',
      priority: 'high',
      teamIds: ['team-1'],
      client: 'ACME',
      createdAt: '2026-01-01',
      updatedAt: '2026-01-01',
      createdBy: 'admin',
    });

    const rawRows: readonly RawExcelRow[] = [
      {
        topic: 'Project Alpha',
        task: 'Dev Backend',
        startDate: '16-janv',
        endDate: '20-févr',
        comment: undefined,
      },
    ];
    const { sharePointAdapter, excelParserAdapter } = createMockSharePointAdapter(rawRows);

    const importUseCases = createImportUseCases({
      projectRepository: repos.projectRepository,
      taskRepository: repos.taskRepository,
      milestoneRepository: repos.milestoneRepository,
      sharePointAdapter,
      excelParserAdapter,
      idProvider: createTestIdProvider(),
      clock: createTestClock(),
      referenceYear: 2026,
    });

    const result = await importUseCases.importFromSharePoint('mock-token', 'user-1');

    expect(result.projects[0].action).toBe('updated');

    const projects = await repos.projectRepository.list();
    expect(projects).toHaveLength(1);
    expect(projects[0].startDate).toBe('2026-01-16');
    expect(projects[0].endDate).toBe('2026-02-20');
    // Preserved app-specific data
    expect(projects[0].status).toBe('active');
    expect(projects[0].priority).toBe('high');
    expect(projects[0].teamIds).toEqual(['team-1']);
    expect(projects[0].client).toBe('ACME');
  });

  it('reports unchanged for idempotent re-import', async () => {
    const store = createInMemoryStore();
    const repos = createInMemoryRepositories(store);

    await repos.projectRepository.save({
      id: 'existing-proj',
      name: 'Project Alpha',
      startDate: '2026-01-16',
      endDate: '2026-02-20',
      status: 'planning',
      teamIds: [],
      createdAt: '2026-01-01',
      updatedAt: '2026-01-01',
      createdBy: 'admin',
    });

    const rawRows: readonly RawExcelRow[] = [
      {
        topic: 'Project Alpha',
        task: 'Dev Backend',
        startDate: '16-janv',
        endDate: '20-févr',
        comment: undefined,
      },
    ];
    const { sharePointAdapter, excelParserAdapter } = createMockSharePointAdapter(rawRows);

    const importUseCases = createImportUseCases({
      projectRepository: repos.projectRepository,
      taskRepository: repos.taskRepository,
      milestoneRepository: repos.milestoneRepository,
      sharePointAdapter,
      excelParserAdapter,
      idProvider: createTestIdProvider(),
      clock: createTestClock(),
      referenceYear: 2026,
    });

    const result = await importUseCases.importFromSharePoint('mock-token', 'user-1');

    expect(result.projects[0].action).toBe('unchanged');
  });

  it('collects parse errors without failing', async () => {
    const store = createInMemoryStore();
    const repos = createInMemoryRepositories(store);

    const rawRows: readonly RawExcelRow[] = [
      {
        topic: undefined,
        task: 'Orphan task',
        startDate: '16-janv',
        endDate: '20-janv',
        comment: undefined,
      },
      {
        topic: 'Project Beta',
        task: 'Valid task',
        startDate: '01-mars',
        endDate: '15-mars',
        comment: undefined,
      },
    ];
    const { sharePointAdapter, excelParserAdapter } = createMockSharePointAdapter(rawRows);

    const importUseCases = createImportUseCases({
      projectRepository: repos.projectRepository,
      taskRepository: repos.taskRepository,
      milestoneRepository: repos.milestoneRepository,
      sharePointAdapter,
      excelParserAdapter,
      idProvider: createTestIdProvider(),
      clock: createTestClock(),
      referenceYear: 2026,
    });

    const result = await importUseCases.importFromSharePoint('mock-token', 'user-1');

    expect(result.parseErrors).toHaveLength(1);
    expect(result.projects).toHaveLength(1);
    expect(result.projects[0].projectName).toBe('Project Beta');
  });

  it('imports multiple projects from Excel', async () => {
    const store = createInMemoryStore();
    const repos = createInMemoryRepositories(store);

    const rawRows: readonly RawExcelRow[] = [
      {
        topic: 'Project A',
        task: 'Task A1',
        startDate: '01-janv',
        endDate: '15-janv',
        comment: undefined,
      },
      {
        topic: 'Project B',
        task: 'Task B1',
        startDate: '01-févr',
        endDate: '15-févr',
        comment: 'Note',
      },
    ];
    const { sharePointAdapter, excelParserAdapter } = createMockSharePointAdapter(rawRows);

    const importUseCases = createImportUseCases({
      projectRepository: repos.projectRepository,
      taskRepository: repos.taskRepository,
      milestoneRepository: repos.milestoneRepository,
      sharePointAdapter,
      excelParserAdapter,
      idProvider: createTestIdProvider(),
      clock: createTestClock(),
      referenceYear: 2026,
    });

    const result = await importUseCases.importFromSharePoint('mock-token', 'user-1');

    expect(result.projects).toHaveLength(2);
    const projects = await repos.projectRepository.list();
    expect(projects).toHaveLength(2);
  });
});
