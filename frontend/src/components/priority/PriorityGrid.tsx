import { useState } from 'react';
import {
  DndContext,
  DragOverlay,
  pointerWithin,
  useSensor,
  useSensors,
  PointerSensor,
} from '@dnd-kit/core';
import type { DragStartEvent, DragEndEvent } from '@dnd-kit/core';
import { QuadrantColumn, TaskCardOverlay } from './QuadrantColumn';
import { QUADRANT_LABELS } from '@/lib/constants';
import type { PriorityMatrixData, MatrixTask, QuadrantKey } from '@/hooks/use-priority-matrix';

interface PriorityGridProps {
  readonly data: PriorityMatrixData;
  readonly onMoveTask: (taskId: string, targetQuadrant: QuadrantKey) => void;
  readonly onEdit?: (taskId: string) => void;
}

interface QuadrantConfig {
  readonly key: QuadrantKey;
  readonly label: string;
  readonly color: string;
  readonly bgColor: string;
  readonly borderColor: string;
}

const QUADRANT_CONFIGS: readonly QuadrantConfig[] = [
  {
    key: 'urgentImportant',
    label: QUADRANT_LABELS.UrgentImportant,
    color: '#DC2626',
    bgColor: 'bg-red-50/50',
    borderColor: 'border-red-200',
  },
  {
    key: 'important',
    label: QUADRANT_LABELS.Important,
    color: '#2563EB',
    bgColor: 'bg-blue-50/50',
    borderColor: 'border-blue-200',
  },
  {
    key: 'urgent',
    label: QUADRANT_LABELS.Urgent,
    color: '#EA580C',
    bgColor: 'bg-orange-50/50',
    borderColor: 'border-orange-200',
  },
  {
    key: 'neither',
    label: QUADRANT_LABELS.Neither,
    color: '#6B7280',
    bgColor: 'bg-gray-50/50',
    borderColor: 'border-gray-200',
  },
];

/** Finds which quadrant currently contains a given task id. */
function findTaskQuadrant(
  data: PriorityMatrixData,
  taskId: string
): QuadrantKey | null {
  const quadrants: readonly QuadrantKey[] = ['urgentImportant', 'important', 'urgent', 'neither'];
  for (const q of quadrants) {
    if (data[q].some(t => t.id === taskId)) {
      return q;
    }
  }
  return null;
}

function findTask(data: PriorityMatrixData, taskId: string): MatrixTask | null {
  const quadrants: readonly QuadrantKey[] = ['urgentImportant', 'important', 'urgent', 'neither'];
  for (const q of quadrants) {
    const found = data[q].find(t => t.id === taskId);
    if (found) return found;
  }
  return null;
}

export function PriorityGrid({ data, onMoveTask, onEdit }: PriorityGridProps) {
  const [activeTask, setActiveTask] = useState<MatrixTask | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: { distance: 8 },
    })
  );

  const handleDragStart = (event: DragStartEvent) => {
    const taskId = String(event.active.id);
    setActiveTask(findTask(data, taskId));
  };

  const handleDragEnd = (event: DragEndEvent) => {
    setActiveTask(null);
    const { active, over } = event;
    if (!over) return;

    const taskId = String(active.id);
    const targetQuadrant = String(over.id) as QuadrantKey;

    // Only move if dropped in a different quadrant
    const sourceQuadrant = findTaskQuadrant(data, taskId);
    if (sourceQuadrant && sourceQuadrant !== targetQuadrant) {
      onMoveTask(taskId, targetQuadrant);
    }
  };

  const handleDragCancel = () => {
    setActiveTask(null);
  };

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={pointerWithin}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onDragCancel={handleDragCancel}
    >
      {/* Axis labels */}
      <div className="mb-2 flex items-center justify-center">
        <span className="text-xs font-medium text-gray-500 uppercase tracking-wider">
          Urgent
        </span>
        <span className="mx-4 text-xs text-gray-300">|</span>
        <span className="text-xs font-medium text-gray-500 uppercase tracking-wider">
          Not Urgent
        </span>
      </div>

      <div className="grid grid-cols-2 gap-4">
        {/* Row labels */}
        <div className="col-span-2 flex items-center gap-2 -mb-2">
          <span className="text-xs font-medium text-gray-500 uppercase tracking-wider">
            Important
          </span>
          <div className="flex-1 border-t border-gray-200" />
        </div>

        {/* Top row: Urgent+Important | Important */}
        {QUADRANT_CONFIGS.slice(0, 2).map(config => (
          <QuadrantColumn
            key={config.key}
            quadrantKey={config.key}
            label={config.label}
            color={config.color}
            bgColor={config.bgColor}
            borderColor={config.borderColor}
            tasks={data[config.key]}
            onEdit={onEdit}
          />
        ))}

        <div className="col-span-2 flex items-center gap-2 -mb-2 -mt-2">
          <span className="text-xs font-medium text-gray-500 uppercase tracking-wider">
            Not Important
          </span>
          <div className="flex-1 border-t border-gray-200" />
        </div>

        {/* Bottom row: Urgent | Neither */}
        {QUADRANT_CONFIGS.slice(2).map(config => (
          <QuadrantColumn
            key={config.key}
            quadrantKey={config.key}
            label={config.label}
            color={config.color}
            bgColor={config.bgColor}
            borderColor={config.borderColor}
            tasks={data[config.key]}
            onEdit={onEdit}
          />
        ))}
      </div>

      <DragOverlay dropAnimation={null}>
        {activeTask ? <TaskCardOverlay task={activeTask} /> : null}
      </DragOverlay>
    </DndContext>
  );
}
