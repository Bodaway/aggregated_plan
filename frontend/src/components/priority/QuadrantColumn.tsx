import { useDroppable } from '@dnd-kit/core';
import { useDraggable } from '@dnd-kit/core';
import type { QuadrantKey } from '@/hooks/use-priority-matrix';

interface QuadrantTask {
  readonly id: string;
  readonly title: string;
  readonly status: string;
  readonly urgency: number;
  readonly impact: number;
  readonly deadline: string | null;
  readonly assignee: string | null;
  readonly project: { readonly name: string } | null;
}

interface QuadrantColumnProps {
  readonly quadrantKey: QuadrantKey;
  readonly label: string;
  readonly color: string;
  readonly bgColor: string;
  readonly borderColor: string;
  readonly tasks: readonly QuadrantTask[];
}

interface DraggableTaskProps {
  readonly task: QuadrantTask;
}

function DraggableTask({ task }: DraggableTaskProps) {
  const { attributes, listeners, setNodeRef, transform, isDragging } = useDraggable({
    id: task.id,
  });

  const style = transform
    ? {
        transform: `translate(${transform.x}px, ${transform.y}px)`,
        zIndex: isDragging ? 50 : undefined,
        opacity: isDragging ? 0.8 : 1,
      }
    : undefined;

  const STATUS_STYLES: Record<string, string> = {
    TODO: 'bg-gray-100 text-gray-700',
    IN_PROGRESS: 'bg-blue-100 text-blue-700',
    DONE: 'bg-green-100 text-green-700',
    BLOCKED: 'bg-red-100 text-red-700',
    CANCELLED: 'bg-gray-200 text-gray-500',
  };

  const statusStyle = STATUS_STYLES[task.status] ?? 'bg-gray-100 text-gray-700';

  return (
    <div
      ref={setNodeRef}
      style={style}
      {...listeners}
      {...attributes}
      className={`bg-white rounded-md border border-gray-200 p-2.5 cursor-grab active:cursor-grabbing hover:shadow-sm transition-shadow ${
        isDragging ? 'shadow-lg ring-2 ring-blue-300' : ''
      }`}
    >
      <h4 className="text-sm font-medium text-gray-900 mb-1 leading-tight">{task.title}</h4>
      <div className="flex flex-wrap items-center gap-1.5">
        <span className={`inline-flex px-1.5 py-0.5 rounded text-xs font-medium ${statusStyle}`}>
          {task.status.replace('_', ' ')}
        </span>
        {task.project && (
          <span className="text-xs text-gray-500 truncate">{task.project.name}</span>
        )}
      </div>
      {task.deadline && (
        <div className="flex items-center gap-1 mt-1 text-xs text-gray-400">
          <svg
            className="w-3 h-3"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={1.5}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M6.75 3v2.25M17.25 3v2.25M3 18.75V7.5a2.25 2.25 0 012.25-2.25h13.5A2.25 2.25 0 0121 7.5v11.25m-18 0A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75m-18 0v-7.5A2.25 2.25 0 015.25 9h13.5A2.25 2.25 0 0121 11.25v7.5"
            />
          </svg>
          {task.deadline}
        </div>
      )}
      {task.assignee && (
        <div className="flex items-center gap-1 mt-0.5 text-xs text-gray-400">
          <svg
            className="w-3 h-3"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={1.5}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z"
            />
          </svg>
          {task.assignee}
        </div>
      )}
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
          tasks.map(task => <DraggableTask key={task.id} task={task} />)
        )}
      </div>
    </div>
  );
}
