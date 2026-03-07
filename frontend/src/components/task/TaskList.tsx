import { TaskCard } from './TaskCard';
import type { TaskCardProps } from './TaskCard';

interface TaskListProps {
  readonly tasks: readonly TaskCardProps[];
  readonly emptyMessage?: string;
}

export function TaskList({ tasks, emptyMessage = 'No tasks' }: TaskListProps) {
  if (tasks.length === 0) {
    return <p className="text-gray-500 text-sm py-4">{emptyMessage}</p>;
  }

  return (
    <div className="space-y-2">
      {tasks.map(task => (
        <TaskCard key={task.id} {...task} />
      ))}
    </div>
  );
}
