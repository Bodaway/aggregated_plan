import React, { useState } from 'react';
import { DragDropContext, Droppable, Draggable } from '@hello-pangea/dnd';
import type { DropResult } from '@hello-pangea/dnd';
import type { Project, Task, TaskPriority, TaskStatus, IsoDateString } from '@domain/index';
import type { CreateTaskInput, UpdateTaskInput } from '@infrastructure/index';

type KanbanBoardProps = {
  readonly tasks: readonly Task[];
  readonly projects: readonly Project[];
  readonly onCreateTask: (input: CreateTaskInput) => Promise<void>;
  readonly onUpdateTask: (id: string, input: UpdateTaskInput) => Promise<void>;
  readonly onDeleteTask: (id: string) => Promise<void>;
};

type TaskFormState = {
  readonly name: string;
  readonly description: string;
  readonly priority: TaskPriority;
  readonly dueDate: string;
  readonly projectId: string;
  readonly status: TaskStatus;
};

const COLUMNS: readonly { readonly id: TaskStatus; readonly label: string }[] = [
  { id: 'todo', label: 'To Do' },
  { id: 'in-progress', label: 'In Progress' },
  { id: 'done', label: 'Done' },
];

const DEFAULT_FORM: TaskFormState = {
  name: '',
  description: '',
  priority: 'medium',
  dueDate: '',
  projectId: '',
  status: 'todo',
};

const priorityLabel = (priority: TaskPriority): string => {
  switch (priority) {
    case 'high':
      return 'High';
    case 'medium':
      return 'Medium';
    case 'low':
      return 'Low';
  }
};

export const KanbanBoard: React.FC<KanbanBoardProps> = ({
  tasks,
  projects,
  onCreateTask,
  onUpdateTask,
  onDeleteTask,
}) => {
  const [modalOpen, setModalOpen] = useState(false);
  const [editingTask, setEditingTask] = useState<Task | null>(null);
  const [form, setForm] = useState<TaskFormState>(DEFAULT_FORM);
  const [modalStatus, setModalStatus] = useState<TaskStatus>('todo');

  const tasksByStatus = (status: TaskStatus): readonly Task[] =>
    tasks.filter((t) => t.status === status);

  const getProjectName = (projectId: string | undefined): string | undefined => {
    if (!projectId) return undefined;
    return projects.find((p) => p.id === projectId)?.name;
  };

  const handleDragEnd = (result: DropResult): void => {
    if (!result.destination) return;
    const newStatus = result.destination.droppableId as TaskStatus;
    const taskId = result.draggableId;
    const task = tasks.find((t) => t.id === taskId);
    if (!task || task.status === newStatus) return;
    void onUpdateTask(taskId, { status: newStatus });
  };

  const openCreateModal = (status: TaskStatus): void => {
    setEditingTask(null);
    setForm({ ...DEFAULT_FORM, status });
    setModalStatus(status);
    setModalOpen(true);
  };

  const openEditModal = (task: Task): void => {
    setEditingTask(task);
    setForm({
      name: task.name,
      description: task.description ?? '',
      priority: task.priority,
      dueDate: task.dueDate ?? '',
      projectId: task.projectId ?? '',
      status: task.status,
    });
    setModalStatus(task.status);
    setModalOpen(true);
  };

  const closeModal = (): void => {
    setModalOpen(false);
    setEditingTask(null);
    setForm(DEFAULT_FORM);
  };

  const handleSubmit = async (e: React.FormEvent<HTMLFormElement>): Promise<void> => {
    e.preventDefault();
    if (!form.name.trim()) return;

    if (editingTask) {
      await onUpdateTask(editingTask.id, {
        name: form.name,
        description: form.description || undefined,
        priority: form.priority,
        dueDate: (form.dueDate || undefined) as IsoDateString | undefined,
        projectId: form.projectId || undefined,
        status: form.status,
      });
    } else {
      await onCreateTask({
        name: form.name,
        description: form.description || undefined,
        priority: form.priority,
        dueDate: (form.dueDate || undefined) as IsoDateString | undefined,
        projectId: form.projectId || undefined,
        status: modalStatus,
      });
    }
    closeModal();
  };

  const updateForm = (field: keyof TaskFormState, value: string): void => {
    setForm((prev) => ({ ...prev, [field]: value }));
  };

  return (
    <section className="kanban-section">
      <h2>Board</h2>
      <DragDropContext onDragEnd={handleDragEnd}>
        <div className="kanban-board">
          {COLUMNS.map((col) => {
            const columnTasks = tasksByStatus(col.id);
            return (
              <div key={col.id} className="kanban-column">
                <div className="kanban-column-header">
                  <span>{col.label}</span>
                  <span className="kanban-count">{columnTasks.length}</span>
                </div>
                <Droppable droppableId={col.id}>
                  {(provided) => (
                    <div
                      className="kanban-column-body"
                      ref={provided.innerRef}
                      {...provided.droppableProps}
                    >
                      {columnTasks.map((task, index) => (
                        <Draggable key={task.id} draggableId={task.id} index={index}>
                          {(dragProvided) => (
                            <div
                              className="kanban-card"
                              ref={dragProvided.innerRef}
                              {...dragProvided.draggableProps}
                              {...dragProvided.dragHandleProps}
                            >
                              <div className="kanban-card-top">
                                <span className="kanban-card-name">{task.name}</span>
                                <span className={`priority-badge ${task.priority}`}>
                                  {priorityLabel(task.priority)}
                                </span>
                              </div>
                              {task.description ? (
                                <p className="kanban-card-desc">{task.description}</p>
                              ) : null}
                              <div className="kanban-card-meta">
                                {task.dueDate ? (
                                  <span className="kanban-card-due">{task.dueDate}</span>
                                ) : null}
                                {getProjectName(task.projectId) ? (
                                  <span className="kanban-card-project">
                                    {getProjectName(task.projectId)}
                                  </span>
                                ) : null}
                              </div>
                              <div className="kanban-card-actions">
                                <button
                                  type="button"
                                  className="kanban-edit-btn"
                                  onClick={() => openEditModal(task)}
                                >
                                  Edit
                                </button>
                                <button
                                  type="button"
                                  className="kanban-delete-btn"
                                  onClick={() => void onDeleteTask(task.id)}
                                >
                                  Delete
                                </button>
                              </div>
                            </div>
                          )}
                        </Draggable>
                      ))}
                      {provided.placeholder}
                    </div>
                  )}
                </Droppable>
                <button
                  type="button"
                  className="kanban-add-btn"
                  onClick={() => openCreateModal(col.id)}
                >
                  + Add task
                </button>
              </div>
            );
          })}
        </div>
      </DragDropContext>

      {modalOpen ? (
        <div className="task-modal-overlay" onClick={closeModal}>
          <div className="task-modal" onClick={(e) => e.stopPropagation()}>
            <h3>{editingTask ? 'Edit Task' : 'New Task'}</h3>
            <form onSubmit={(e) => void handleSubmit(e)}>
              <label>
                Name
                <input
                  type="text"
                  value={form.name}
                  onChange={(e) => updateForm('name', e.target.value)}
                  required
                />
              </label>
              <label>
                Description
                <textarea
                  className="task-textarea"
                  value={form.description}
                  onChange={(e) => updateForm('description', e.target.value)}
                />
              </label>
              <label>
                Priority
                <select
                  value={form.priority}
                  onChange={(e) => updateForm('priority', e.target.value)}
                >
                  <option value="high">High</option>
                  <option value="medium">Medium</option>
                  <option value="low">Low</option>
                </select>
              </label>
              <label>
                Due date
                <input
                  type="date"
                  value={form.dueDate}
                  onChange={(e) => updateForm('dueDate', e.target.value)}
                />
              </label>
              <label>
                Project
                <select
                  value={form.projectId}
                  onChange={(e) => updateForm('projectId', e.target.value)}
                >
                  <option value="">None</option>
                  {projects.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </label>
              {editingTask ? (
                <label>
                  Status
                  <select
                    value={form.status}
                    onChange={(e) => updateForm('status', e.target.value)}
                  >
                    <option value="todo">To Do</option>
                    <option value="in-progress">In Progress</option>
                    <option value="done">Done</option>
                  </select>
                </label>
              ) : null}
              <div className="task-modal-buttons">
                <button type="submit">
                  {editingTask ? 'Save' : 'Create'}
                </button>
                <button type="button" className="task-cancel-btn" onClick={closeModal}>
                  Cancel
                </button>
              </div>
            </form>
          </div>
        </div>
      ) : null}
    </section>
  );
};
