import type { ExcelProjectRow, IsoDateString } from '@aggregated-plan/shared-types';
import { createDomainError } from './domain-errors';
import type { DomainError } from './domain-errors';
import { err, ok } from './result';
import type { Result } from './result';

export type RawExcelRow = {
  readonly topic: string | undefined;
  readonly task: string | undefined;
  readonly startDate: string | undefined;
  readonly endDate: string | undefined;
  readonly comment: string | undefined;
};

export type ExcelProjectPhase = {
  readonly name: string;
  readonly epicRef: string | undefined;
};

const PHASE_HEADER_PATTERN = /^\d+\.\s/;

const FRENCH_MONTHS: Readonly<Record<string, string>> = {
  janv: '01',
  févr: '02',
  mars: '03',
  avr: '04',
  mai: '05',
  juin: '06',
  juil: '07',
  août: '08',
  sept: '09',
  oct: '10',
  nov: '11',
  déc: '12',
};

const ISO_DATE_PATTERN = /^\d{4}-\d{2}-\d{2}$/;

export const isPhaseHeader = (taskName: string): boolean =>
  PHASE_HEADER_PATTERN.test(taskName);

export const extractPhaseInfo = (taskName: string): ExcelProjectPhase => {
  const withoutNumber = taskName.replace(/^\d+\.\s*/, '');
  const epicMatch = withoutNumber.match(/,\s*Epic\s*:\s*(\S+)\s*$/i);
  const name = epicMatch
    ? withoutNumber.slice(0, epicMatch.index).trim()
    : withoutNumber.trim();
  const epicRef = epicMatch ? epicMatch[1].trim() : undefined;
  return { name, epicRef };
};

export const parseFrenchDate = (
  raw: string,
  referenceYear: number,
): Result<IsoDateString, DomainError> => {
  const trimmed = raw.trim();

  if (ISO_DATE_PATTERN.test(trimmed)) {
    return ok(trimmed as IsoDateString);
  }

  const match = trimmed.match(/^(\d{1,2})-(.+)$/);
  if (!match) {
    return err(createDomainError('invalid-date-range', `Cannot parse date: "${raw}"`));
  }

  const day = match[1].padStart(2, '0');
  const monthStr = match[2].toLowerCase();
  const month = FRENCH_MONTHS[monthStr];

  if (!month) {
    return err(
      createDomainError('invalid-date-range', `Unknown French month abbreviation: "${monthStr}"`),
    );
  }

  return ok(`${referenceYear}-${month}-${day}` as IsoDateString);
};

export const parseExcelRow = (
  raw: RawExcelRow,
  referenceYear: number,
): Result<ExcelProjectRow, DomainError> => {
  if (!raw.topic || raw.topic.trim().length === 0) {
    return err(createDomainError('invalid-name', 'Row is missing topic'));
  }
  if (!raw.task || raw.task.trim().length === 0) {
    return err(createDomainError('invalid-name', 'Row is missing task'));
  }
  if (!raw.startDate || !raw.endDate) {
    return err(createDomainError('invalid-date-range', 'Row is missing dates'));
  }

  const startResult = parseFrenchDate(raw.startDate, referenceYear);
  if (!startResult.ok) return startResult;

  const endResult = parseFrenchDate(raw.endDate, referenceYear);
  if (!endResult.ok) return endResult;

  return ok({
    topic: raw.topic.trim(),
    task: raw.task.trim(),
    startDate: startResult.value,
    endDate: endResult.value,
    comment: raw.comment?.trim(),
    isPhaseHeader: isPhaseHeader(raw.task.trim()),
  });
};

export const groupRowsByProject = (
  rows: readonly ExcelProjectRow[],
): ReadonlyMap<string, readonly ExcelProjectRow[]> => {
  const groups = new Map<string, ExcelProjectRow[]>();
  rows.forEach((row) => {
    const existing = groups.get(row.topic);
    if (existing) {
      existing.push(row);
    } else {
      groups.set(row.topic, [row]);
    }
  });
  return groups;
};

export const extractProjectDates = (
  rows: readonly ExcelProjectRow[],
): { readonly startDate: IsoDateString; readonly endDate: IsoDateString } => {
  const startDates = rows.map((r) => r.startDate);
  const endDates = rows.map((r) => r.endDate);
  const sortedStarts = [...startDates].sort();
  const sortedEnds = [...endDates].sort();
  return {
    startDate: sortedStarts[0],
    endDate: sortedEnds[sortedEnds.length - 1],
  };
};
