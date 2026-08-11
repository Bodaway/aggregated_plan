import { useCallback, useEffect, useMemo, useRef } from 'react';
import { useMutation, useQuery } from 'urql';

import { formatDate } from '@/lib/date-utils';

export type Confidence = 'HIGH' | 'MEDIUM' | 'LOW';
export type TimesheetStatus = 'DRAFT' | 'VALIDATED' | 'SUBMITTED' | 'DAY_OFF';
export type BlockKind = 'MEETING' | 'WORK' | 'OUT_OF_OFFICE';
export type DayOffScope = 'FULL' | 'MORNING' | 'AFTERNOON';

export interface TimesheetLine {
  gryzzlyProjectId: string | null;
  projectName: string | null;
  hours: number;
  isPinned: boolean;
  confidence: Confidence;
  sourceRefs: string[];
}
export interface AttributedBlock {
  startTime: string;
  endTime: string;
  gryzzlyProjectId: string | null;
  kind: BlockKind;
  hours: number;
  sourceRefs: string[];
  /** Name of what the block came from: the owning task's title for a WORK block, the
   *  meeting subject for a MEETING one. Null when the origin has no known name. */
  originLabel: string | null;
}
export interface UnresolvedSignal {
  sourceRef: string;
  label: string;
  at: string;
}
export interface ReconstructedDay {
  date: string;
  status: TimesheetStatus;
  targetHours: number;
  roundingIncrement: number;
  totalHours: number;
  dayConfidence: Confidence;
  lines: TimesheetLine[];
  unattributedHours: number;
  unresolved: UnresolvedSignal[];
  blocks: AttributedBlock[];
}
export interface TimesheetLineInput {
  gryzzlyProjectId: string | null;
  hours: number;
  isPinned: boolean;
}

// Minimal error shape surfaced to components, so callers never import urql types.
export interface MutationError {
  message: string;
}

// Outcome of a reconstruct/refresh, surfaced so the page can show feedback.
export interface ReconstructResult {
  message: string;
  isError: boolean;
}

// A deduped Gryzzly project, ready to feed a <select>.
export interface ProjectOption {
  id: string;
  label: string;
}

// Raw catalog row from the gryzzlyTasks query (many rows per project).
export interface GryzzlyProjectRow {
  gryzzlyProjectId: string;
  projectName: string;
  customerName: string | null;
}

// Shared selection set for every op that returns a ReconstructedDay.
const DAY_FIELDS = `
  date status targetHours roundingIncrement totalHours dayConfidence unattributedHours
  lines { gryzzlyProjectId projectName hours isPinned confidence sourceRefs }
  unresolved { sourceRef label at }
  blocks { startTime endTime gryzzlyProjectId kind hours sourceRefs originLabel }
`;

const TIMESHEET_DRAFT_QUERY = `query TimesheetDraft($date: NaiveDate!) { timesheetDraft(date: $date) { ${DAY_FIELDS} } }`;
const RECONSTRUCT_MUTATION = `mutation RunReconstruction($date: NaiveDate!) { runTimesheetReconstruction(date: $date) { ${DAY_FIELDS} } }`;
const SAVE_DRAFT_MUTATION = `mutation SaveDraft($date: NaiveDate!, $lines: [TimesheetLineInput!]!) { saveTimesheetDraft(date: $date, lines: $lines) { ${DAY_FIELDS} } }`;
const VALIDATE_MUTATION = `mutation Validate($date: NaiveDate!) { validateTimesheet(date: $date) { ${DAY_FIELDS} } }`;
const MARK_DAY_OFF_MUTATION = `mutation MarkDayOff($date: NaiveDate!, $scope: DayOffScopeGql!) { markDayOff(date: $date, scope: $scope) { ${DAY_FIELDS} } }`;
const GRYZZLY_PROJECTS_QUERY = `query GryzzlyProjects { gryzzlyTasks(limit: 500) { gryzzlyProjectId projectName customerName } }`;

interface DraftData { timesheetDraft: ReconstructedDay | null; }
interface ReconstructData { runTimesheetReconstruction: ReconstructedDay; }
interface GryzzlyProjectsData { gryzzlyTasks: GryzzlyProjectRow[]; }

// Catalog of distinct Gryzzly projects for the reassignment dropdown.
// The gryzzlyTasks query returns one row per task, so we dedupe by project id.
export function useGryzzlyProjects() {
  const [result] = useQuery<GryzzlyProjectsData>({ query: GRYZZLY_PROJECTS_QUERY });

  const projects = useMemo<ProjectOption[]>(() => {
    const byId = new Map<string, { id: string; projectName: string; label: string }>();
    for (const row of result.data?.gryzzlyTasks ?? []) {
      if (!row.gryzzlyProjectId || byId.has(row.gryzzlyProjectId)) continue;
      const label = row.customerName ? `${row.projectName} — ${row.customerName}` : row.projectName;
      byId.set(row.gryzzlyProjectId, { id: row.gryzzlyProjectId, projectName: row.projectName, label });
    }
    return [...byId.values()]
      .sort((a, b) => a.projectName.localeCompare(b.projectName) || a.label.localeCompare(b.label))
      .map(({ id, label }) => ({ id, label }));
  }, [result.data]);

  return { projects, loading: result.fetching, error: result.error ?? null };
}

export function useTimesheet(date: Date) {
  const dateStr = formatDate(date);
  const [result, reexecute] = useQuery<DraftData>({
    query: TIMESHEET_DRAFT_QUERY,
    variables: { date: dateStr },
  });
  const [, execReconstruct] = useMutation<ReconstructData>(RECONSTRUCT_MUTATION);
  const [, execSave] = useMutation(SAVE_DRAFT_MUTATION);
  const [, execValidate] = useMutation(VALIDATE_MUTATION);
  const [, execMarkOff] = useMutation(MARK_DAY_OFF_MUTATION);

  const refetch = useCallback(
    () => reexecute({ requestPolicy: 'network-only' }),
    [reexecute],
  );

  const reconstruct = useCallback(async (): Promise<ReconstructResult> => {
    const res = await execReconstruct({ date: dateStr });
    if (res.error) {
      return {
        message: res.error.graphQLErrors[0]?.message ?? res.error.message,
        isError: true,
      };
    }
    refetch();
    const rebuilt = res.data?.runTimesheetReconstruction;
    if (!rebuilt) {
      return { message: 'Reconstruit.', isError: false };
    }
    const projectCount = rebuilt.lines.filter((l) => l.gryzzlyProjectId !== null).length;
    const message = `Reconstruit : ${rebuilt.totalHours}h au total, ${rebuilt.unattributedHours}h non attribuées, ${projectCount} projet(s), ${rebuilt.unresolved.length} signal(aux) non résolu(s).`;
    return { message, isError: false };
  }, [execReconstruct, dateStr, refetch]);

  const saveLines = useCallback(
    async (lines: TimesheetLineInput[]): Promise<MutationError | null> => {
      const res = await execSave({ date: dateStr, lines });
      if (res.error) {
        return { message: res.error.graphQLErrors[0]?.message ?? res.error.message };
      }
      refetch();
      return null;
    },
    [execSave, dateStr, refetch],
  );

  const validate = useCallback(async () => {
    const res = await execValidate({ date: dateStr });
    if (!res.error) refetch();
  }, [execValidate, dateStr, refetch]);

  const markOff = useCallback(
    async (scope: DayOffScope) => {
      const res = await execMarkOff({ date: dateStr, scope });
      if (!res.error) refetch();
    },
    [execMarkOff, dateStr, refetch],
  );

  // Auto-reconstruct ONCE per date when no draft exists yet (so the screen is useful immediately).
  const autoRanFor = useRef<string | null>(null);
  useEffect(() => {
    if (
      !result.fetching &&
      !result.error &&
      result.data &&
      result.data.timesheetDraft === null &&
      autoRanFor.current !== dateStr
    ) {
      autoRanFor.current = dateStr;
      void reconstruct();
    }
  }, [result.fetching, result.error, result.data, dateStr, reconstruct]);

  return {
    day: result.data?.timesheetDraft ?? null,
    loading: result.fetching,
    error: result.error ?? null,
    reconstruct,
    saveLines,
    validate,
    markOff,
    refetch,
  };
}
