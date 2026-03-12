import { useState, useCallback } from 'react';
import { usePriorityMatrix } from '@/hooks/use-priority-matrix';
import { PriorityGrid } from '@/components/priority/PriorityGrid';
import { TaskEditSheet } from '@/components/task/TaskEditSheet';
import { TaskCard } from '@/components/task/TaskCard';
import type { QuadrantKey, PriorityMatrixData } from '@/hooks/use-priority-matrix';

export function PriorityMatrixPage() {
  const { data, loading, error, updatePriority } = usePriorityMatrix();
  const [editingTaskId, setEditingTaskId] = useState<string | null>(null);

  const criticalTasks = data
    ? [
        ...data.urgentImportant,
        ...data.important,
        ...data.urgent,
        ...data.neither,
      ].filter(t => t.urgency >= 4)
    : [];

  const filteredData: PriorityMatrixData | null = data
    ? {
        urgentImportant: data.urgentImportant.filter(t => t.urgency < 4),
        important: data.important.filter(t => t.urgency < 4),
        urgent: data.urgent.filter(t => t.urgency < 4),
        neither: data.neither.filter(t => t.urgency < 4),
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
        <div>
          <p className="text-sm text-gray-500">
            Drag tasks between quadrants to update their priority.
          </p>
        </div>
        <span className="text-xs text-gray-400">
          {totalTasks} task{totalTasks !== 1 ? 's' : ''} total
        </span>
      </div>

      {/* Critical section */}
      {criticalTasks.length > 0 && (
        <div className="rounded-lg border border-red-200 bg-red-50 overflow-hidden mb-4">
          <div className="flex items-center gap-2 px-4 py-2 border-b border-red-200">
            <span className="text-xs font-bold tracking-widest text-red-700 uppercase">● Critical</span>
            <span className="text-xs font-semibold text-red-600 bg-red-100 rounded-full px-2 py-0.5">
              {criticalTasks.length}
            </span>
            <span className="ml-auto text-xs text-red-400">Requires immediate attention</span>
          </div>
          <div className="flex flex-wrap gap-3 p-4">
            {criticalTasks.map(task => (
              <div key={task.id} className="w-64">
                <TaskCard
                  id={task.id}
                  title={task.title}
                  source={task.source}
                  sourceId={task.sourceId}
                  status={task.status}
                  jiraStatus={task.jiraStatus}
                  urgency={task.urgency}
                  impact={task.impact}
                  quadrant=""
                  deadline={task.deadline}
                  assignee={task.assignee}
                  projectName={task.project?.name ?? null}
                  effectiveRemainingHours={task.effectiveRemainingHours}
                  effectiveEstimatedHours={task.effectiveEstimatedHours}
                  jiraTimeSpentSeconds={task.jiraTimeSpentSeconds}
                  compact
                  onClick={() => setEditingTaskId(task.id)}
                />
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Priority grid */}
      <PriorityGrid data={filteredData!} onMoveTask={handleMoveTask} onEdit={handleEdit} onDragStartExternal={() => setEditingTaskId(null)} />

      <TaskEditSheet taskId={editingTaskId} onClose={() => setEditingTaskId(null)} />
    </div>
  );
}
