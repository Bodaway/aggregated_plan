/**
 * Structural shape of a task for hour-counting purposes.
 * Kept minimal so this helper does not depend on component/GraphQL types.
 */
export interface HourCountableTask {
  /** GraphQL task status enum (uppercase): TODO | IN_PROGRESS | DONE | BLOCKED | CANCELLED. */
  readonly status: string;
  readonly effectiveRemainingHours?: number | null;
  readonly effectiveEstimatedHours?: number | null;
}

/**
 * Hours a task contributes to dashboard workload aggregations (per-day and weekly totals).
 *
 * Done and Cancelled tasks contribute 0 — they still carry an estimate (Jira original
 * estimate / personal estimate) but no longer represent outstanding work, so counting
 * them would inflate the totals. Blocked tasks still count (the work is not finished).
 *
 * For counting tasks, falls back: effective remaining > effective estimated > 0.
 */
export function getTaskHours(t: HourCountableTask): number {
  if (t.status === 'DONE' || t.status === 'CANCELLED') return 0;
  return t.effectiveRemainingHours ?? t.effectiveEstimatedHours ?? 0;
}
