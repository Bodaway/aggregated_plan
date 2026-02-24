import { render, screen } from '@testing-library/react';
import { KanbanBoard } from '../kanban-board';
import type { Task, Project } from '@aggregated-plan/shared-types';

const mockTasks: readonly Task[] = [
  {
    id: 'task-1',
    name: 'Design UI',
    description: 'Create mockups',
    status: 'todo',
    priority: 'high',
    createdAt: '2024-01-01',
    updatedAt: '2024-01-01',
  },
  {
    id: 'task-2',
    name: 'Build API',
    status: 'in-progress',
    priority: 'medium',
    createdAt: '2024-01-01',
    updatedAt: '2024-01-01',
  },
  {
    id: 'task-3',
    name: 'Write docs',
    status: 'done',
    priority: 'low',
    projectId: 'project-1',
    createdAt: '2024-01-01',
    updatedAt: '2024-01-01',
  },
];

const mockProjects: readonly Project[] = [
  {
    id: 'project-1',
    name: 'Alpha',
    startDate: '2024-01-01',
    endDate: '2024-06-01',
    status: 'active',
    teamIds: [],
    createdAt: '2024-01-01',
    updatedAt: '2024-01-01',
    createdBy: 'user-1',
  },
];

const noop = async (): Promise<void> => {};

describe('KanbanBoard', () => {
  it('renders 3 columns with correct headers', () => {
    render(
      <KanbanBoard
        tasks={[]}
        projects={[]}
        onCreateTask={noop}
        onUpdateTask={noop}
        onDeleteTask={noop}
      />,
    );

    expect(screen.getByText('To Do')).toBeInTheDocument();
    expect(screen.getByText('In Progress')).toBeInTheDocument();
    expect(screen.getByText('Done')).toBeInTheDocument();
  });

  it('tasks appear in correct columns by status', () => {
    render(
      <KanbanBoard
        tasks={mockTasks}
        projects={mockProjects}
        onCreateTask={noop}
        onUpdateTask={noop}
        onDeleteTask={noop}
      />,
    );

    expect(screen.getByText('Design UI')).toBeInTheDocument();
    expect(screen.getByText('Build API')).toBeInTheDocument();
    expect(screen.getByText('Write docs')).toBeInTheDocument();
  });

  it('priority badges render with correct CSS classes', () => {
    render(
      <KanbanBoard
        tasks={mockTasks}
        projects={mockProjects}
        onCreateTask={noop}
        onUpdateTask={noop}
        onDeleteTask={noop}
      />,
    );

    const highBadge = screen.getByText('High');
    expect(highBadge).toHaveClass('priority-badge', 'high');

    const mediumBadge = screen.getByText('Medium');
    expect(mediumBadge).toHaveClass('priority-badge', 'medium');

    const lowBadge = screen.getByText('Low');
    expect(lowBadge).toHaveClass('priority-badge', 'low');
  });

  it('create task buttons are present for each column', () => {
    render(
      <KanbanBoard
        tasks={[]}
        projects={[]}
        onCreateTask={noop}
        onUpdateTask={noop}
        onDeleteTask={noop}
      />,
    );

    const addButtons = screen.getAllByText('+ Add task');
    expect(addButtons).toHaveLength(3);
  });
});
