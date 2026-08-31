import { useCallback, useEffect, useMemo, useRef } from 'react';
import { useMutation, useQuery } from 'urql';

import { useAssignAnyGryzzlyTask } from '@/hooks/use-assign-gryzzly-task';
import { formatDate } from '@/lib/date-utils';

export type Confidence = 'HIGH' | 'MEDIUM' | 'LOW';
export type TimesheetStatus = 'DRAFT' | 'VALIDATED' | 'SUBMITTED' | 'DAY_OFF';
export type DayOffScope = 'FULL' | 'MORNING' | 'AFTERNOON';

export interface TimesheetLine {
  gryzzlyProjectId: string | null;
  projectName: string | null;
  hours: number;
  isPinned: boolean;
  confidence: Confidence;
  sourceRefs: string[];
}
/** One stretch of a lane, in local minutes from midnight. */
export interface LaneInterval {
  startMin: number;
  endMin: number;
}
/** One task's presence across the day. Lanes OVERLAP — that is the concurrent view. */
export interface Lane {
  laneKey: string;
  /** The plan task behind the lane, or null for a meeting / unmatched repo. */
  taskId: string | null;
  label: string;
  gryzzlyProjectId: string | null;
  intervals: LaneInterval[];
  outsideMinutes: number;
}
/** What one lane declares inside one quarter, with the weight it came from. */
export interface QuarterShare {
  laneKey: string;
  taskId: string | null;
  label: string;
  gryzzlyProjectId: string | null;
  presenceMinutes: number;
  hours: number;
  isPinned: boolean;
}
/** A quarter-day. `shares` always sums to `declarableHours`. */
export interface Quarter {
  index: number;
  startMin: number;
  endMin: number;
  hours: number;
  oooHours: number;
  declarableHours: number;
  confidence: Confidence;
  shares: QuarterShare[];
}
export interface OutsideWork {
  laneKey: string;
  label: string;
  minutes: number;
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
  lanes: Lane[];
  quarters: Quarter[];
  outsideWorkday: OutsideWork[];
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
  lanes { laneKey taskId label gryzzlyProjectId outsideMinutes intervals { startMin endMin } }
  quarters {
    index startMin endMin hours oooHours declarableHours confidence
    shares { laneKey taskId label gryzzlyProjectId presenceMinutes hours isPinned }
  }
  outsideWorkday { laneKey label minutes }
`;

const TIMESHEET_DRAFT_QUERY = `query TimesheetDraft($date: NaiveDate!) { timesheetDraft(date: $date) { ${DAY_FIELDS} } }`;
const RECONSTRUCT_MUTATION = `mutation RunReconstruction($date: NaiveDate!) { runTimesheetReconstruction(date: $date) { ${DAY_FIELDS} } }`;
const SET_SHARE_MUTATION = `mutation SetShare($date: NaiveDate!, $quarterIndex: Int!, $laneKey: String!, $hours: Float!) { setQuarterShare(date: $date, quarterIndex: $quarterIndex, laneKey: $laneKey, hours: $hours) { ${DAY_FIELDS} } }`;
const CLEAR_SHARE_MUTATION = `mutation ClearShare($date: NaiveDate!, $quarterIndex: Int!, $laneKey: String!) { clearQuarterShare(date: $date, quarterIndex: $quarterIndex, laneKey: $laneKey) { ${DAY_FIELDS} } }`;
const RESET_QUARTER_MUTATION = `mutation ResetQuarter($date: NaiveDate!, $quarterIndex: Int!) { resetQuarter(date: $date, quarterIndex: $quarterIndex) { ${DAY_FIELDS} } }`;
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
  const [, execSetShare] = useMutation(SET_SHARE_MUTATION);
  const [, execClearShare] = useMutation(CLEAR_SHARE_MUTATION);
  const [, execResetQuarter] = useMutation(RESET_QUARTER_MUTATION);
  const [, execValidate] = useMutation(VALIDATE_MUTATION);
  const [, execMarkOff] = useMutation(MARK_DAY_OFF_MUTATION);
  const assignGryzzlyTask = useAssignAnyGryzzlyTask();

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

  // Reassigning the Gryzzly task of a lane's task is a DURABLE fix: the backend
  // snapshots the project onto the task, so every future day resolves it too. Today's
  // day still has to be rebuilt — lanes and quarter shares carry the old project id in
  // the database — and a reconstruct is safe here because it preserves pinned shares.
  const assignLaneGryzzlyTask = useCallback(
    async (taskId: string, gryzzlyTaskId: string | null): Promise<ReconstructResult> => {
      const res = await assignGryzzlyTask(taskId, gryzzlyTaskId);
      if (res.error) {
        return {
          message: res.error.graphQLErrors[0]?.message ?? res.error.message,
          isError: true,
        };
      }
      const rebuilt = await reconstruct();
      if (rebuilt.isError) return rebuilt;
      return { message: `Projet Gryzzly mis à jour. ${rebuilt.message}`, isError: false };
    },
    [assignGryzzlyTask, reconstruct],
  );

  // Pinning one share re-apportions the rest of its quarter server-side, so every
  // editing call refetches rather than patching the day locally.
  const setShare = useCallback(
    async (quarterIndex: number, laneKey: string, hours: number): Promise<MutationError | null> => {
      const res = await execSetShare({ date: dateStr, quarterIndex, laneKey, hours });
      if (res.error) {
        return { message: res.error.graphQLErrors[0]?.message ?? res.error.message };
      }
      refetch();
      return null;
    },
    [execSetShare, dateStr, refetch],
  );

  const clearShare = useCallback(
    async (quarterIndex: number, laneKey: string): Promise<MutationError | null> => {
      const res = await execClearShare({ date: dateStr, quarterIndex, laneKey });
      if (res.error) {
        return { message: res.error.graphQLErrors[0]?.message ?? res.error.message };
      }
      refetch();
      return null;
    },
    [execClearShare, dateStr, refetch],
  );

  const resetQuarter = useCallback(
    async (quarterIndex: number): Promise<MutationError | null> => {
      const res = await execResetQuarter({ date: dateStr, quarterIndex });
      if (res.error) {
        return { message: res.error.graphQLErrors[0]?.message ?? res.error.message };
      }
      refetch();
      return null;
    },
    [execResetQuarter, dateStr, refetch],
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
    assignLaneGryzzlyTask,
    setShare,
    clearShare,
    resetQuarter,
    validate,
    markOff,
    refetch,
  };
}
