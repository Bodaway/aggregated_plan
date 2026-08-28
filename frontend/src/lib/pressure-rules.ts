import { isTaskOpen } from './task-status';

/** A half-day, the unit `weeklyWorkload.capacity` is counted in — 4 hours
 *  (morning 08:00-12:00 or afternoon 13:00-17:00, per CLAUDE.md). Needed to
 *  turn that count into hours comparable with `totalPlanned`/`totalMeetings`. */
export const HOURS_PER_HALF_DAY = 4;

/**
 * Structural shapes for the two questions below. Kept minimal so this helper
 * does not depend on component/GraphQL types — same convention as
 * `task-hours.ts` and `is-real-meeting.ts`.
 */
export interface DeadlineCandidateTask {
  readonly id: string;
  readonly title: string;
  /** GraphQL task status enum (uppercase): TODO | IN_PROGRESS | DONE | BLOCKED | CANCELLED. */
  readonly status: string;
  readonly deadline: string | null;
}

export interface OpenDeadline {
  readonly id: string;
  readonly title: string;
  readonly deadline: string;
}

export interface CapacityWorkload {
  readonly capacity: number;
  readonly totalPlanned: number;
  readonly totalMeetings: number;
  readonly overload: boolean;
}

export interface Capacity {
  readonly pct: number;
  readonly overloaded: boolean;
}

/**
 * Still-open deadlines, earliest first — "proximity" is chronological,
 * overdue and all.
 *
 * Closed tasks are filtered out here rather than at the call site: the
 * backend does not filter for us (`find_by_date_range` selects purely on
 * date range), so `dailyDashboard.tasks` genuinely can carry DONE and
 * CANCELLED rows, and a finished task's deadline is not pressure.
 */
export function openDeadlines(tasks: readonly DeadlineCandidateTask[]): OpenDeadline[] {
  return tasks
    .filter((t): t is DeadlineCandidateTask & { deadline: string } => t.deadline !== null && isTaskOpen(t.status))
    .map((t) => ({ id: t.id, title: t.title, deadline: t.deadline }))
    .sort((a, b) => a.deadline.localeCompare(b.deadline));
}

/** Whether any still-open deadline falls exactly on `day` (a `YYYY-MM-DD`,
 *  the shape `deadline` itself has — a date-only comparison). */
export function hasOpenDeadlineOn(tasks: readonly DeadlineCandidateTask[], day: string): boolean {
  return openDeadlines(tasks).some((d) => d.deadline === day);
}

/**
 * Capacity as a percentage of the week's planned load (tasks + meetings)
 * over its capacity, converted from half-days to hours.
 *
 * `overloaded` is the domain's own R16 verdict, never re-derived from `pct`:
 * R16 is a strict comparison on raw hours while `pct` is rounded, so the two
 * disagree either way around the boundary (40.1h of a 40h week reads 100%
 * and *is* overloaded; 39.9h also reads 100% and is not).
 */
export function computeCapacity(workload: CapacityWorkload | null): Capacity {
  if (!workload) return { pct: 0, overloaded: false };
  const capacityHours = workload.capacity * HOURS_PER_HALF_DAY;
  const plannedHours = workload.totalPlanned + workload.totalMeetings;
  const pct = capacityHours > 0 ? Math.round((plannedHours / capacityHours) * 100) : 0;
  return { pct, overloaded: workload.overload };
}
