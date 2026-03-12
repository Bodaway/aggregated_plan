import { format, startOfWeek, endOfWeek, addDays, addWeeks, isToday, isSameDay, parseISO } from 'date-fns';

export const formatDate = (date: Date | string): string => {
  const d = typeof date === 'string' ? parseISO(date) : date;
  return format(d, 'yyyy-MM-dd');
};

export const formatDisplayDate = (date: Date | string): string => {
  const d = typeof date === 'string' ? parseISO(date) : date;
  return format(d, 'EEEE d MMMM yyyy');
};

export const formatWeekRange = (weekStart: Date, workingDays: readonly number[] = [1, 2, 3, 4, 5]): string => {
  const firstDay = addDays(weekStart, workingDays[0] - 1);
  const lastDay = addDays(weekStart, workingDays[workingDays.length - 1] - 1);
  if (firstDay.getMonth() === lastDay.getMonth()) {
    return `${format(firstDay, 'd')} - ${format(lastDay, 'd MMMM yyyy')}`;
  }
  return `${format(firstDay, 'd MMM')} - ${format(lastDay, 'd MMM yyyy')}`;
};

export const formatDayShort = (date: Date): string => format(date, 'EEE d');

export const getWeekStart = (date: Date): Date => startOfWeek(date, { weekStartsOn: 1 });
export const getWeekEnd = (date: Date): Date => endOfWeek(date, { weekStartsOn: 1 });
export const getNextDay = (date: Date): Date => addDays(date, 1);
export const getPrevDay = (date: Date): Date => addDays(date, -1);
export const getNextWeek = (date: Date): Date => addWeeks(date, 1);
export const getPrevWeek = (date: Date): Date => addWeeks(date, -1);
export const getWeekDays = (weekStart: Date, workingDays: readonly number[] = [1, 2, 3, 4, 5]): Date[] =>
  workingDays.map(d => addDays(weekStart, d - 1));
export { addDays, isToday, isSameDay, parseISO };
