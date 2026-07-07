import { useCallback, useEffect, useRef } from 'react';
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

// Shared selection set for every op that returns a ReconstructedDay.
const DAY_FIELDS = `
  date status targetHours roundingIncrement totalHours dayConfidence unattributedHours
  lines { gryzzlyProjectId projectName hours isPinned confidence sourceRefs }
  unresolved { sourceRef label at }
  blocks { startTime endTime gryzzlyProjectId kind hours sourceRefs }
`;

const TIMESHEET_DRAFT_QUERY = `query TimesheetDraft($date: NaiveDate!) { timesheetDraft(date: $date) { ${DAY_FIELDS} } }`;
const RECONSTRUCT_MUTATION = `mutation RunReconstruction($date: NaiveDate!) { runTimesheetReconstruction(date: $date) { ${DAY_FIELDS} } }`;
const SAVE_DRAFT_MUTATION = `mutation SaveDraft($date: NaiveDate!, $lines: [TimesheetLineInput!]!) { saveTimesheetDraft(date: $date, lines: $lines) { ${DAY_FIELDS} } }`;
const VALIDATE_MUTATION = `mutation Validate($date: NaiveDate!) { validateTimesheet(date: $date) { ${DAY_FIELDS} } }`;
const MARK_DAY_OFF_MUTATION = `mutation MarkDayOff($date: NaiveDate!, $scope: DayOffScopeGql!) { markDayOff(date: $date, scope: $scope) { ${DAY_FIELDS} } }`;

interface DraftData { timesheetDraft: ReconstructedDay | null; }

export function useTimesheet(date: Date) {
  const dateStr = formatDate(date);
  const [result, reexecute] = useQuery<DraftData>({
    query: TIMESHEET_DRAFT_QUERY,
    variables: { date: dateStr },
  });
  const [, execReconstruct] = useMutation(RECONSTRUCT_MUTATION);
  const [, execSave] = useMutation(SAVE_DRAFT_MUTATION);
  const [, execValidate] = useMutation(VALIDATE_MUTATION);
  const [, execMarkOff] = useMutation(MARK_DAY_OFF_MUTATION);

  const refetch = useCallback(
    () => reexecute({ requestPolicy: 'network-only' }),
    [reexecute],
  );

  const reconstruct = useCallback(async () => {
    const res = await execReconstruct({ date: dateStr });
    if (!res.error) refetch();
  }, [execReconstruct, dateStr, refetch]);

  const saveLines = useCallback(
    async (lines: TimesheetLineInput[]) => {
      const res = await execSave({ date: dateStr, lines });
      if (!res.error) refetch();
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
