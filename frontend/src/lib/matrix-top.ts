import { isTaskOpen } from './task-status';

/**
 * Structural shape of a task as the `priorityMatrix` query returns it, kept
 * minimal so this helper does not depend on component or GraphQL types —
 * same convention as `pressure-rules.ts` and `task-hours.ts`.
 */
export interface MatrixCandidateTask {
  readonly id: string;
  readonly title: string;
  /** GraphQL task status enum (uppercase): TODO | IN_PROGRESS | DONE | BLOCKED | CANCELLED. */
  readonly status: string;
  /** `UrgencyLevelGql` (LOW|MEDIUM|HIGH|CRITICAL) or the 1-4 number. */
  readonly urgency: number | string;
  readonly impact: number | string;
  readonly deadline: string | null;
  readonly project: { readonly name: string } | null;
  readonly isRecurring: boolean;
  readonly recurrenceId?: string | null;
  readonly occurrenceDate?: string | null;
}

export interface MatrixQuadrants {
  readonly urgentImportant: readonly MatrixCandidateTask[];
  readonly important: readonly MatrixCandidateTask[];
  readonly urgent: readonly MatrixCandidateTask[];
  readonly neither: readonly MatrixCandidateTask[];
}

/** One row of the block: what to do, and when it is due if it is.
 *
 *  No project: measured on this database, 5 of 54 open followed tasks carry
 *  one, against 8 with a deadline — a column blank nine times in ten reads
 *  as a value that failed to load rather than one that does not exist. */
export interface TopTask {
  readonly id: string;
  readonly title: string;
  readonly deadline: string | null;
  /** Urgency 4. Sorts above the quadrant and is marked differently. */
  readonly critical: boolean;
}

const URGENCY_NUM: Record<string, number> = { LOW: 1, MEDIUM: 2, HIGH: 3, CRITICAL: 4 };

/** `priorityMatrix` resolves urgency as `UrgencyLevelGql`, not `Int` — the
 *  priority page carries the same conversion. A block that only understood
 *  numbers would read every CRITICAL as 1 and silently lose the whole
 *  critical band. */
function toUrgencyNum(urgency: number | string): number {
  if (typeof urgency === 'number') return urgency;
  return URGENCY_NUM[urgency] ?? 1;
}

function isCritical(task: MatrixCandidateTask): boolean {
  return toUrgencyNum(task.urgency) >= 4;
}

/**
 * One row per recurring series — the earliest upcoming occurrence — and every
 * non-recurring task untouched.
 *
 * A weekly template with four occurrences ahead of it would otherwise fill
 * the block with the same title four times; the priority page collapses them
 * for exactly this reason.
 */
function deduplicateRecurring(tasks: readonly MatrixCandidateTask[]): MatrixCandidateTask[] {
  const earliestPerSeries = new Map<string, MatrixCandidateTask>();
  const result: MatrixCandidateTask[] = [];

  for (const task of tasks) {
    if (!task.recurrenceId) {
      result.push(task);
      continue;
    }
    const kept = earliestPerSeries.get(task.recurrenceId);
    if (!kept) {
      earliestPerSeries.set(task.recurrenceId, task);
      result.push(task);
      continue;
    }
    if ((task.occurrenceDate ?? '') < (kept.occurrenceDate ?? '')) {
      result[result.indexOf(kept)] = task;
      earliestPerSeries.set(task.recurrenceId, task);
    }
  }

  return result;
}

/** Earliest deadline first, undated last — "proximity" the same way
 *  `openDeadlines` means it, except that a task with no deadline is still
 *  something to do and is kept rather than dropped. */
function byDeadline(a: MatrixCandidateTask, b: MatrixCandidateTask): number {
  if (a.deadline === b.deadline) return 0;
  if (a.deadline === null) return 1;
  if (b.deadline === null) return -1;
  return a.deadline.localeCompare(b.deadline);
}

function toTopTask(task: MatrixCandidateTask): TopTask {
  return {
    id: task.id,
    title: task.title,
    deadline: task.deadline,
    critical: isCritical(task),
  };
}

/** Every open task of the matrix, one row per recurring series. The single
 *  pass both readings below start from, so they can never disagree about
 *  which tasks exist. */
function openTasks(data: MatrixQuadrants): MatrixCandidateTask[] {
  return deduplicateRecurring(
    [...data.urgentImportant, ...data.important, ...data.urgent, ...data.neither].filter((t) =>
      isTaskOpen(t.status),
    ),
  );
}

/**
 * The top of the Eisenhower matrix, in the order the priority page shows it:
 * the critical band first — urgency 4, wherever in the grid it sits — then
 * the urgent-and-important quadrant.
 *
 * The other three quadrants are deliberately absent. This is a "what now"
 * list, not a ranking of everything open: important-not-urgent, urgent-not-
 * important and neither belong to the page, which has room to argue about
 * them. What they weigh is `matrixBacklog`'s answer.
 */
export function topOfMatrix(data: MatrixQuadrants | null): TopTask[] {
  if (!data) return [];

  const tasks = openTasks(data);
  const inQuadrant = new Set(data.urgentImportant.map((t) => t.id));

  const critical = tasks.filter(isCritical);
  const quadrant = tasks.filter((t) => !isCritical(t) && inQuadrant.has(t.id));

  return [...critical.sort(byDeadline), ...quadrant.sort(byDeadline)].map(toTopTask);
}

/** How much open work sits below the line, quadrant by quadrant. */
export interface MatrixBacklog {
  readonly important: number;
  readonly urgent: number;
  readonly neither: number;
}

/**
 * What `topOfMatrix` deliberately leaves out: the three lower quadrants,
 * counted rather than listed.
 *
 * Critical tasks are excluded wherever they sit, because they are already
 * shown above — a count that included them would tell the reader to look for
 * something that is not down there. This is the difference between "what I
 * am not showing you" and "what exists", and only the first is worth a line
 * on a panel whose whole job is to say what to do now.
 */
export function matrixBacklog(data: MatrixQuadrants | null): MatrixBacklog {
  if (!data) return { important: 0, urgent: 0, neither: 0 };

  const belowTheLine = new Set(openTasks(data).filter((t) => !isCritical(t)).map((t) => t.id));
  const count = (quadrant: readonly MatrixCandidateTask[]) =>
    quadrant.filter((t) => belowTheLine.has(t.id)).length;

  return {
    important: count(data.important),
    urgent: count(data.urgent),
    neither: count(data.neither),
  };
}
