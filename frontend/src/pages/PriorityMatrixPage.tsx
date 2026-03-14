import { useState, useCallback } from 'react';
import { usePriorityMatrix } from '@/hooks/use-priority-matrix';
import { PriorityGrid } from '@/components/priority/PriorityGrid';
import { TaskEditSheet } from '@/components/task/TaskEditSheet';
import { TaskSearchInput } from '@/components/search/TaskSearchInput';
import { useTaskSearch } from '@/hooks/use-task-search';
import type { QuadrantKey, PriorityMatrixData } from '@/hooks/use-priority-matrix';

// urgency arrives as a string enum ("LOW"|"MEDIUM"|"HIGH"|"CRITICAL") from the
// priority-matrix GraphQL resolver (TaskGql returns UrgencyLevelGql, not Int).
const URGENCY_NUM: Record<string, number> = { LOW: 1, MEDIUM: 2, HIGH: 3, CRITICAL: 4 };
function toUrgencyNum(u: unknown): number {
  if (typeof u === 'number') return u;
  return URGENCY_NUM[u as string] ?? 1;
}

export function PriorityMatrixPage() {
  const { searchQuery, setSearchQuery, results: searchResults, matchingIds, isSearchActive, isSearching, clearSearch } = useTaskSearch();
  const { data, loading, error, updatePriority } = usePriorityMatrix();
  const [editingTaskId, setEditingTaskId] = useState<string | null>(null);

  const criticalTasks = data
    ? [
        ...data.urgentImportant,
        ...data.important,
        ...data.urgent,
        ...data.neither,
      ]
        .filter(t => toUrgencyNum(t.urgency) >= 4)
        .map(t => ({ ...t, urgency: toUrgencyNum(t.urgency), impact: toUrgencyNum(t.impact) }))
    : [];

  const filteredData: PriorityMatrixData | null = data
    ? {
        urgentImportant: data.urgentImportant.filter(t => toUrgencyNum(t.urgency) < 4),
        important: data.important.filter(t => toUrgencyNum(t.urgency) < 4),
        urgent: data.urgent.filter(t => toUrgencyNum(t.urgency) < 4),
        neither: data.neither.filter(t => toUrgencyNum(t.urgency) < 4),
      }
    : null;

  const handleMoveTask = (taskId: string, targetQuadrant: QuadrantKey) => {
    void updatePriority(taskId, targetQuadrant);
  };

  const handleEdit = useCallback((taskId: string) => {
    setEditingTaskId(taskId);
  }, []);

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
        <div className="flex items-center gap-4">
          <p className="text-sm text-gray-500">
            Drag tasks between quadrants to update their priority.
          </p>
          <TaskSearchInput
            value={searchQuery}
            onChange={setSearchQuery}
            onClear={clearSearch}
            results={searchResults}
            isSearching={isSearching}
            matchCount={matchingIds.size}
          />
        </div>
        <span className="text-xs text-gray-400">
          {totalTasks} task{totalTasks !== 1 ? 's' : ''} total
        </span>
      </div>

      {/* Priority grid (critical section rendered inside DndContext for drag support) */}
      <PriorityGrid data={filteredData!} criticalTasks={criticalTasks} onMoveTask={handleMoveTask} onEdit={handleEdit} onDragStartExternal={() => setEditingTaskId(null)} matchingIds={matchingIds} isSearchActive={isSearchActive} />

      <TaskEditSheet taskId={editingTaskId} onClose={() => setEditingTaskId(null)} />
    </div>
  );
}
