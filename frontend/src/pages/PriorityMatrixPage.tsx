import { useState, useCallback } from 'react';
import { usePriorityMatrix } from '@/hooks/use-priority-matrix';
import { PriorityGrid } from '@/components/priority/PriorityGrid';
import { TaskEditSheet } from '@/components/task/TaskEditSheet';
import type { QuadrantKey } from '@/hooks/use-priority-matrix';

export function PriorityMatrixPage() {
  const { data, loading, error, updatePriority } = usePriorityMatrix();
  const [editingTaskId, setEditingTaskId] = useState<string | null>(null);

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
        <div>
          <p className="text-sm text-gray-500">
            Drag tasks between quadrants to update their priority.
          </p>
        </div>
        <span className="text-xs text-gray-400">
          {totalTasks} task{totalTasks !== 1 ? 's' : ''} total
        </span>
      </div>

      {/* Priority grid */}
      <PriorityGrid data={data} onMoveTask={handleMoveTask} onEdit={handleEdit} onDragStartExternal={() => setEditingTaskId(null)} />

      <TaskEditSheet taskId={editingTaskId} onClose={() => setEditingTaskId(null)} />
    </div>
  );
}
