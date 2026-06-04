import { useCallback } from 'react';
import { usePriorityMatrix } from '@/hooks/use-priority-matrix';
import { PriorityGrid } from '@/components/priority/PriorityGrid';
import { useSearch } from '@/lib/search/SearchProvider';
import type { MatrixTask, QuadrantKey, PriorityMatrixData } from '@/hooks/use-priority-matrix';

// urgency arrives as a string enum ("LOW"|"MEDIUM"|"HIGH"|"CRITICAL") from the
// priority-matrix GraphQL resolver (TaskGql returns UrgencyLevelGql, not Int).
const URGENCY_NUM: Record<string, number> = { LOW: 1, MEDIUM: 2, HIGH: 3, CRITICAL: 4 };
function toUrgencyNum(u: unknown): number {
  if (typeof u === 'number') return u;
  return URGENCY_NUM[u as string] ?? 1;
}

/**
 * For recurring tasks, keep only the earliest upcoming occurrence per
 * recurrence template. Non-recurring tasks are always included.
 * This prevents duplicate cards for the same recurring series.
 */
function deduplicateRecurring(tasks: readonly MatrixTask[]): readonly MatrixTask[] {
  const seenRecurrenceIds = new Map<string, MatrixTask>();
  const result: MatrixTask[] = [];
  for (const task of tasks) {
    if (!task.recurrenceId) {
      result.push(task);
      continue;
    }
    const existing = seenRecurrenceIds.get(task.recurrenceId);
    if (!existing) {
      seenRecurrenceIds.set(task.recurrenceId, task);
      result.push(task);
    } else {
      // Keep the one with the earlier occurrenceDate
      const existingDate = existing.occurrenceDate ?? '';
      const taskDate = task.occurrenceDate ?? '';
      if (taskDate < existingDate) {
        // Replace existing with this earlier occurrence
        const idx = result.indexOf(existing);
        if (idx !== -1) result[idx] = task;
        seenRecurrenceIds.set(task.recurrenceId, task);
      }
    }
  }
  return result;
}

export function PriorityMatrixPage() {
  const { data, loading, error, updatePriority } = usePriorityMatrix();
  const { openTaskInSheet } = useSearch();

  const criticalTasks = data
    ? deduplicateRecurring([
        ...data.urgentImportant,
        ...data.important,
        ...data.urgent,
        ...data.neither,
      ].filter(t => toUrgencyNum(t.urgency) >= 4))
        .map(t => ({ ...t, urgency: toUrgencyNum(t.urgency), impact: toUrgencyNum(t.impact) }))
    : [];

  const isVisibleInMatrix = (t: { urgency: unknown; status: string; isRecurring: boolean }) =>
    toUrgencyNum(t.urgency) < 4 &&
    t.status !== 'CANCELLED' &&
    !(t.status === 'DONE' && t.isRecurring);

  const filteredData: PriorityMatrixData | null = data
    ? {
        urgentImportant: deduplicateRecurring(data.urgentImportant.filter(isVisibleInMatrix)),
        important: deduplicateRecurring(data.important.filter(isVisibleInMatrix)),
        urgent: deduplicateRecurring(data.urgent.filter(isVisibleInMatrix)),
        neither: deduplicateRecurring(data.neither.filter(isVisibleInMatrix)),
      }
    : null;

  const handleMoveTask = (taskId: string, targetQuadrant: QuadrantKey) => {
    void updatePriority(taskId, targetQuadrant);
  };

  const handleEdit = useCallback((taskId: string) => {
    openTaskInSheet(taskId);
  }, [openTaskInSheet]);

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <p className="text-red-500 text-sm font-medium">Failed to load priority matrix</p>
          <p className="text-gray-400 text-xs mt-1">{error.message}</p>
        </div>
      </div>
    );
  }

  if (loading && !data) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin mx-auto mb-2" />
          <p className="text-gray-500 text-sm">Loading priority matrix...</p>
        </div>
      </div>
    );
  }

  if (!data) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-gray-500 text-sm">No priority data available</p>
      </div>
    );
  }

  const totalTasks =
    data.urgentImportant.length +
    data.important.length +
    data.urgent.length +
    data.neither.length;

  return (
    <div className="space-y-4">
      {/* Header with summary */}
      <div className="flex items-center justify-between">
        <div>
          <p className="text-sm text-gray-500">
            Drag tasks between quadrants to update their priority.
          </p>
        </div>
        <span className="text-xs text-gray-400">
          {totalTasks} task{totalTasks !== 1 ? 's' : ''} total
        </span>
      </div>

      {/* Priority grid (critical section rendered inside DndContext for drag support) */}
      <PriorityGrid data={filteredData!} criticalTasks={criticalTasks} onMoveTask={handleMoveTask} onEdit={handleEdit} onDragStartExternal={() => { /* sheet state is owned by SearchProvider */ }} />
    </div>
  );
}
