/**
 * True while a task still represents outstanding work.
 *
 * `Done` and `Cancelled` are the two closed states; `Blocked` is not one of
 * them — the work is stalled, not finished. The `status` is the uppercase
 * GraphQL enum: TODO | IN_PROGRESS | DONE | BLOCKED | CANCELLED.
 *
 * Extracted from `task-hours.ts`, which set this convention for workload
 * aggregation, once the deadline rules needed the same verdict. One
 * definition of "open" — two answers to that question, both visible on the
 * same screen, is how the HUD's glow and its gauge start disagreeing.
 */
export function isTaskOpen(status: string): boolean {
  return status !== 'DONE' && status !== 'CANCELLED';
}
