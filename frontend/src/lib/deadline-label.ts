import { differenceInCalendarDays, parseISO } from 'date-fns';

/**
 * `Overdue`, `Today`, `Tomorrow` or `In Nd` — how far off a deadline is, in
 * words.
 *
 * A date-only comparison: `deadline`, like the `today` it is measured
 * against, is a bare `YYYY-MM-DD` with no time component.
 *
 * Shared rather than duplicated because the Pressure and Priority blocks sit
 * side by side on the HUD and can show the same task between them. Two
 * spellings of "in three days" a hand's width apart is the kind of drift
 * this file exists to prevent.
 */
export function formatDeadlineLabel(deadline: string, today: string): string {
  const days = differenceInCalendarDays(parseISO(deadline), parseISO(today));
  if (days < 0) return 'Overdue';
  if (days === 0) return 'Today';
  if (days === 1) return 'Tomorrow';
  return `In ${days}d`;
}
