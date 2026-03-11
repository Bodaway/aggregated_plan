import { useState } from 'react';
import {
  DndContext,
  DragOverlay,
  closestCenter,
  PointerSensor,
  useSensor,
  useSensors,
  type DragStartEvent,
  type DragEndEvent,
} from '@dnd-kit/core';
import { useDroppable } from '@dnd-kit/core';
import { useDraggable } from '@dnd-kit/core';
import { useTriageTasks, type TriageTask } from '@/hooks/use-triage';

function SourceDot({ source }: { readonly source: string }) {
  const color =
    source === 'JIRA'
      ? 'bg-blue-500'
      : source === 'EXCEL'
        ? 'bg-green-500'
        : 'bg-gray-400';
  return <span className={`inline-block w-2 h-2 rounded-full ${color}`} />;
}

function DraggableTaskCard({
  task,
  onDismiss,
}: {
  readonly task: TriageTask;
  readonly onDismiss?: () => void;
}) {
  const { attributes, listeners, setNodeRef, transform, isDragging } =
    useDraggable({ id: task.id });

  const style = {
    transform: transform ? `translate3d(${transform.x}px, ${transform.y}px, 0)` : undefined,
    opacity: isDragging ? 0.4 : 1,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      {...listeners}
      {...attributes}
      className="bg-white border border-gray-200 rounded-lg p-3 shadow-sm cursor-grab active:cursor-grabbing hover:border-blue-300 transition-colors"
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5 mb-1">
            <SourceDot source={task.source} />
            {task.sourceId && (
              <span className="text-xs text-gray-400 font-mono">{task.sourceId}</span>
            )}
          </div>
          <p className="text-sm font-medium text-gray-800 truncate">{task.title}</p>
          <div className="flex items-center gap-2 mt-1.5 text-xs text-gray-500">
            <span className="px-1.5 py-0.5 bg-gray-100 rounded text-gray-600">
              {task.status === 'IN_PROGRESS' ? 'In Progress' : 'Todo'}
            </span>
            {task.assignee && <span className="truncate">{task.assignee}</span>}
            {task.deadline && <span>{task.deadline}</span>}
          </div>
          {task.project?.name && (
            <p className="text-xs text-gray-400 mt-1 truncate">{task.project.name}</p>
          )}
        </div>
        {onDismiss && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              onDismiss();
            }}
            className="p-1 text-gray-400 hover:text-red-500 transition-colors flex-shrink-0"
            title="Dismiss task"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        )}
      </div>
    </div>
  );
}

function TaskCardOverlay({ task }: { readonly task: TriageTask }) {
  return (
    <div className="bg-white border-2 border-blue-400 rounded-lg p-3 shadow-lg w-80">
      <div className="flex items-center gap-1.5 mb-1">
        <SourceDot source={task.source} />
        {task.sourceId && (
          <span className="text-xs text-gray-400 font-mono">{task.sourceId}</span>
        )}
      </div>
      <p className="text-sm font-medium text-gray-800">{task.title}</p>
    </div>
  );
}

function DroppableColumn({
  id,
  title,
  count,
  children,
  accentColor,
  headerAction,
}: {
  readonly id: string;
  readonly title: string;
  readonly count: number;
  readonly children: React.ReactNode;
  readonly accentColor: string;
  readonly headerAction?: React.ReactNode;
}) {
  const { isOver, setNodeRef } = useDroppable({ id });

  return (
    <div
      ref={setNodeRef}
      className={`flex flex-col rounded-lg border-2 transition-colors ${
        isOver ? 'border-blue-400 bg-blue-50/50' : 'border-gray-200 bg-gray-50/50'
      }`}
    >
      <div className={`px-4 py-3 border-b-2 ${accentColor} rounded-t-lg flex items-center justify-between`}>
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider">
            {title}
          </h3>
          <span className="text-xs text-gray-500 bg-white/80 px-2 py-0.5 rounded-full">
            {count}
          </span>
        </div>
        {headerAction}
      </div>
      <div className="flex-1 p-3 space-y-2 overflow-y-auto max-h-[calc(100vh-220px)]">
        {children}
      </div>
    </div>
  );
}

export function TriagePage() {
  const {
    inboxTasks,
    followedTasks,
    inboxCount,
    followedCount,
    loading,
    error,
    followTask,
    dismissTask,
    unfollowTask,
    followAll,
  } = useTriageTasks();

  const [activeTask, setActiveTask] = useState<TriageTask | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } })
  );

  const allTasks = [...inboxTasks, ...followedTasks];

  const handleDragStart = (event: DragStartEvent) => {
    const task = allTasks.find(t => t.id === event.active.id);
    setActiveTask(task ?? null);
  };

  const handleDragEnd = (event: DragEndEvent) => {
    setActiveTask(null);
    const { active, over } = event;
    if (!over) return;

    const taskId = active.id as string;
    const targetColumn = over.id as string;

    const isInInbox = inboxTasks.some(t => t.id === taskId);
    const isInFollowed = followedTasks.some(t => t.id === taskId);

    if (targetColumn === 'followed' && isInInbox) {
      followTask(taskId);
    } else if (targetColumn === 'inbox' && isInFollowed) {
      unfollowTask(taskId);
    }
  };

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <p className="text-red-500 text-sm font-medium">Failed to load tasks</p>
          <p className="text-gray-400 text-xs mt-1">{error.message}</p>
        </div>
      </div>
    );
  }

  if (loading && inboxTasks.length === 0 && followedTasks.length === 0) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin mx-auto mb-2" />
          <p className="text-gray-500 text-sm">Loading tasks...</p>
        </div>
      </div>
    );
  }

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
    >
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 h-full">
        <DroppableColumn
          id="inbox"
          title="Inbox"
          count={inboxCount}
          accentColor="border-amber-300 bg-amber-50"
          headerAction={
            inboxTasks.length > 0 ? (
              <button
                onClick={() => followAll(inboxTasks.map(t => t.id))}
                className="text-xs text-blue-600 hover:text-blue-800 font-medium"
              >
                Follow All
              </button>
            ) : undefined
          }
        >
          {inboxTasks.length === 0 ? (
            <p className="text-sm text-gray-400 text-center py-8">
              No new tasks to review
            </p>
          ) : (
            inboxTasks.map(task => (
              <DraggableTaskCard
                key={task.id}
                task={task}
                onDismiss={() => dismissTask(task.id)}
              />
            ))
          )}
        </DroppableColumn>

        <DroppableColumn
          id="followed"
          title="Following"
          count={followedCount}
          accentColor="border-green-300 bg-green-50"
        >
          {followedTasks.length === 0 ? (
            <p className="text-sm text-gray-400 text-center py-8">
              Drag tasks here to follow them
            </p>
          ) : (
            followedTasks.map(task => (
              <DraggableTaskCard key={task.id} task={task} />
            ))
          )}
        </DroppableColumn>
      </div>

      <DragOverlay>
        {activeTask ? <TaskCardOverlay task={activeTask} /> : null}
      </DragOverlay>
    </DndContext>
  );
}
