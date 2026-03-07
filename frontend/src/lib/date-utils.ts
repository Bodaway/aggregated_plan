import { format, startOfWeek, endOfWeek, addDays, isToday, parseISO } from 'date-fns';

export const formatDate = (date: Date | string): string => {
  const d = typeof date === 'string' ? parseISO(date) : date;
  return format(d, 'yyyy-MM-dd');
};

export const formatDisplayDate = (date: Date | string): string => {
  const d = typeof date === 'string' ? parseISO(date) : date;
  return format(d, 'EEEE d MMMM yyyy');
};

export const getWeekStart = (date: Date): Date => startOfWeek(date, { weekStartsOn: 1 });
export const getWeekEnd = (date: Date): Date => endOfWeek(date, { weekStartsOn: 1 });
export const getNextDay = (date: Date): Date => addDays(date, 1);
export const getPrevDay = (date: Date): Date => addDays(date, -1);
export { isToday, parseISO };
