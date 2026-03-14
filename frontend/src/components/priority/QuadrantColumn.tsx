import { useDroppable } from '@dnd-kit/core';
import { useDraggable } from '@dnd-kit/core';
import type { QuadrantKey } from '@/hooks/use-priority-matrix';
import { TaskCard } from '@/components/task/TaskCard';

interface QuadrantTask {
  readonly id: string;
  readonly title: string;
  readonly status: string;
  readonly urgency: number;
  readonly impact: number;
  readonly deadline: string | null;
  readonly assignee: string | null;
  readonly project: { readonly name: string } | null;
  readonly source: string;
  readonly sourceId: string | null;
  readonly jiraStatus: string | null;
  readonly effectiveRemainingHours: number | null;
  readonly effectiveEstimatedHours: number | null;
  readonly jiraTimeSpentSeconds: number | null;
}

interface QuadrantColumnProps {
  readonly quadrantKey: QuadrantKey;
  readonly label: string;
  readonly color: string;
  readonly bgColor: string;
  readonly borderColor: string;
  readonly tasks: readonly QuadrantTask[];
  readonly onEdit?: (taskId: string) => void;
  readonly matchingIds?: Set<string>;
  readonly isSearchActive?: boolean;
}

/** Overlay shown while dragging a task card. */
export function TaskCardOverlay({ task }: { readonly task: QuadrantTask }) {
  return (
    <div className="shadow-lg ring-2 ring-blue-300 rounded-md w-64">
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
      />
    </div>
  );
}

export function DraggableTask({ task, onEdit, highlighted, dimmed }: { readonly task: QuadrantTask; readonly onEdit?: (taskId: string) => void; readonly highlighted?: boolean; readonly dimmed?: boolean }) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({
    id: task.id,
  });

  return (
    <div
      ref={setNodeRef}
      {...listeners}
      {...attributes}
      className={`cursor-grab active:cursor-grabbing ${isDragging ? 'opacity-30' : ''}`}
    >
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
        highlighted={highlighted}
        dimmed={dimmed}
        onClick={onEdit ? () => onEdit(task.id) : undefined}
      />
    </div>
  );
}

export function QuadrantColumn({
  quadrantKey,
  label,
  color,
  bgColor,
  borderColor,
  tasks,
  onEdit,
  matchingIds,
  isSearchActive,
}: QuadrantColumnProps) {
  const { setNodeRef, isOver } = useDroppable({
    id: quadrantKey,
  });

  return (
    <div
      ref={setNodeRef}
      className={`flex flex-col rounded-lg border-2 ${borderColor} ${
        isOver ? 'ring-2 ring-blue-400 bg-blue-50/30' : bgColor
      } transition-colors min-h-[200px]`}
    >
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b" style={{ borderColor: color + '40' }}>
        <h3 className="text-sm font-semibold" style={{ color }}>
          {label}
        </h3>
        <span
          className="inline-flex items-center justify-center w-5 h-5 rounded-full text-xs font-medium text-white"
          style={{ backgroundColor: color }}
        >
          {tasks.length}
        </span>
      </div>

      {/* Task list */}
      <div className="flex-1 p-2 space-y-2 overflow-y-auto">
        {tasks.length === 0 ? (
          <p className="text-xs text-gray-400 text-center py-4">Drop tasks here</p>
        ) : (
          tasks.map(task => (
            <DraggableTask
              key={task.id}
              task={task}
              onEdit={onEdit}
              highlighted={isSearchActive && matchingIds?.has(task.id)}
              dimmed={isSearchActive && !matchingIds?.has(task.id)}
            />
          ))
        )}
      </div>
    </div>
  );
}
