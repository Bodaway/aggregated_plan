import type { IsoDateString, Weekday } from '@aggregated-plan/shared-types';

const MILLISECONDS_PER_DAY = 24 * 60 * 60 * 1000;

const padNumber = (value: number): string => value.toString().padStart(2, '0');

const toIsoDateString = (date: Date): IsoDateString => {
  const year = date.getUTCFullYear();
  const month = padNumber(date.getUTCMonth() + 1);
  const day = padNumber(date.getUTCDate());
  return `${year}-${month}-${day}` as IsoDateString;
};

const toUtcDate = (date: IsoDateString): Date => {
  const [yearText, monthText, dayText] = date.split('-');
  const year = Number(yearText);
  const month = Number(monthText);
  const day = Number(dayText);
  return new Date(Date.UTC(year, month - 1, day));
};

export const isIsoDateString = (value: string): value is IsoDateString => {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) {
    return false;
  }
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(Date.UTC(year, month - 1, day));
  return (
    date.getUTCFullYear() === year &&
    date.getUTCMonth() + 1 === month &&
    date.getUTCDate() === day
  );
};

export const toEpochDay = (date: IsoDateString): number =>
  Math.floor(toUtcDate(date).getTime() / MILLISECONDS_PER_DAY);

export const compareIsoDates = (left: IsoDateString, right: IsoDateString): number => {
  const leftEpoch = toEpochDay(left);
  const rightEpoch = toEpochDay(right);
  if (leftEpoch === rightEpoch) {
    return 0;
  }
  return leftEpoch < rightEpoch ? -1 : 1;
};

export const addDays = (date: IsoDateString, days: number): IsoDateString => {
  const epochDay = toEpochDay(date) + days;
  return toIsoDateString(new Date(epochDay * MILLISECONDS_PER_DAY));
};

export const listIsoDatesInRange = (
  startDate: IsoDateString,
  endDate: IsoDateString,
): IsoDateString[] => {
  if (compareIsoDates(startDate, endDate) > 0) {
    return [];
  }
  const days = toEpochDay(endDate) - toEpochDay(startDate);
  return Array.from({ length: days + 1 }, (_, index) => addDays(startDate, index));
};

export const getWeekday = (date: IsoDateString): Weekday => {
  const dayIndex = toUtcDate(date).getUTCDay();
  switch (dayIndex) {
    case 0:
      return 'sunday';
    case 1:
      return 'monday';
    case 2:
      return 'tuesday';
    case 3:
      return 'wednesday';
    case 4:
      return 'thursday';
    case 5:
      return 'friday';
    default:
      return 'saturday';
  }
};

export const getWeekStart = (date: IsoDateString): IsoDateString => {
  const dayIndex = toUtcDate(date).getUTCDay();
  const offset = (dayIndex + 6) % 7;
  return addDays(date, -offset);
};

export const listWeekStartsInRange = (
  startDate: IsoDateString,
  endDate: IsoDateString,
): IsoDateString[] => {
  if (compareIsoDates(startDate, endDate) > 0) {
    return [];
  }
  const first = getWeekStart(startDate);
  const totalWeeks =
    Math.floor((toEpochDay(endDate) - toEpochDay(first)) / 7) + 1;
  return Array.from({ length: totalWeeks }, (_, index) => addDays(first, index * 7));
};
