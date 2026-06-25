export interface SortablePickerTask {
  readonly id: string;
  readonly title: string;
  readonly plannedStart: string | null;
  readonly deadline: string | null;
  readonly urgency: number;
  readonly impact: number;
}

/**
 * Returns true if the task is a "today" task:
 * plannedStart date == today OR deadline == today.
 * Uses the same date comparison as the dashboard (local clock, date-fns formatDate).
 */
function isTodayTask(task: SortablePickerTask, today: string): boolean {
  const plannedDate = task.plannedStart ? task.plannedStart.slice(0, 10) : null;
  const deadlineDate = task.deadline ? task.deadline.slice(0, 10) : null;
  return plannedDate === today || deadlineDate === today;
}

/**
 * Pure comparator: urgency DESC, then impact DESC.
 */
function compareByPriority(a: SortablePickerTask, b: SortablePickerTask): number {
  if (b.urgency !== a.urgency) return b.urgency - a.urgency;
  return b.impact - a.impact;
}

/**
 * Sorts tasks for the activity picker:
 * 1. Today's tasks (plannedStart == today OR deadline == today) come first.
 * 2. Within each group, sort by urgency DESC then impact DESC.
 * Nothing is filtered out.
 *
 * @param tasks - flat list of picker tasks (not mutated)
 * @param today - YYYY-MM-DD string representing "today" (pass formatDate(new Date()))
 */
export function sortTasksForPicker<T extends SortablePickerTask>(tasks: readonly T[], today: string): T[] {
  const todayGroup: T[] = [];
  const restGroup: T[] = [];

  for (const task of tasks) {
    if (isTodayTask(task, today)) {
      todayGroup.push(task);
    } else {
      restGroup.push(task);
    }
  }

  todayGroup.sort(compareByPriority);
  restGroup.sort(compareByPriority);

  return [...todayGroup, ...restGroup];
}
