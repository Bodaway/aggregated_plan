import { useState, type FormEvent } from 'react';

interface TaskFormValues {
  readonly title: string;
  readonly description: string;
  readonly projectId: string;
  readonly deadline: string;
  readonly estimatedHours: number | null;
  readonly impact: number;
  readonly urgency: number;
}

interface ProjectOption {
  readonly id: string;
  readonly name: string;
}

interface TaskFormProps {
  readonly initialValues?: Partial<TaskFormValues>;
  readonly projects?: readonly ProjectOption[];
  readonly onSubmit: (values: TaskFormValues) => void;
  readonly onCancel?: () => void;
  readonly submitLabel?: string;
}

const EMPTY_VALUES: TaskFormValues = {
  title: '',
  description: '',
  projectId: '',
  deadline: '',
  estimatedHours: null,
  impact: 2,
  urgency: 2,
};

const SCALE_OPTIONS = [
  { value: 1, label: '1 - Low' },
  { value: 2, label: '2 - Medium' },
  { value: 3, label: '3 - High' },
  { value: 4, label: '4 - Critical' },
];

export function TaskForm({
  initialValues,
  projects = [],
  onSubmit,
  onCancel,
  submitLabel = 'Create Task',
}: TaskFormProps) {
  const [values, setValues] = useState<TaskFormValues>({
    ...EMPTY_VALUES,
    ...initialValues,
  });

  const handleChange = (
    field: keyof TaskFormValues,
    rawValue: string | number | null
  ) => {
    setValues(prev => ({ ...prev, [field]: rawValue }));
  };

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!values.title.trim()) return;
    onSubmit(values);
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      {/* Title */}
      <div>
        <label htmlFor="task-title" className="block text-sm font-medium text-gray-700 mb-1">
          Title <span className="text-red-500">*</span>
        </label>
        <input
          id="task-title"
          type="text"
          required
          value={values.title}
          onChange={e => handleChange('title', e.target.value)}
          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          placeholder="Task title"
        />
      </div>

      {/* Description */}
      <div>
        <label htmlFor="task-description" className="block text-sm font-medium text-gray-700 mb-1">
          Description
        </label>
        <textarea
          id="task-description"
          rows={3}
          value={values.description}
          onChange={e => handleChange('description', e.target.value)}
          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          placeholder="Optional description"
        />
      </div>

      {/* Project */}
      <div>
        <label htmlFor="task-project" className="block text-sm font-medium text-gray-700 mb-1">
          Project
        </label>
        <select
          id="task-project"
          value={values.projectId}
          onChange={e => handleChange('projectId', e.target.value)}
          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
        >
          <option value="">No project</option>
          {projects.map(p => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </div>

      {/* Deadline and Estimated Hours row */}
      <div className="grid grid-cols-2 gap-4">
        <div>
          <label htmlFor="task-deadline" className="block text-sm font-medium text-gray-700 mb-1">
            Deadline
          </label>
          <input
            id="task-deadline"
            type="date"
            value={values.deadline}
            onChange={e => handleChange('deadline', e.target.value)}
            className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          />
        </div>
        <div>
          <label
            htmlFor="task-estimated-hours"
            className="block text-sm font-medium text-gray-700 mb-1"
          >
            Estimated Hours
          </label>
          <input
            id="task-estimated-hours"
            type="number"
            min={0}
            step={0.5}
            value={values.estimatedHours ?? ''}
            onChange={e => {
              const v = e.target.value;
              handleChange('estimatedHours', v === '' ? null : parseFloat(v));
            }}
            className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
            placeholder="0"
          />
        </div>
      </div>

      {/* Impact and Urgency row */}
      <div className="grid grid-cols-2 gap-4">
        <div>
          <label htmlFor="task-impact" className="block text-sm font-medium text-gray-700 mb-1">
            Impact
          </label>
          <select
            id="task-impact"
            value={values.impact}
            onChange={e => handleChange('impact', parseInt(e.target.value, 10))}
            className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          >
            {SCALE_OPTIONS.map(opt => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label htmlFor="task-urgency" className="block text-sm font-medium text-gray-700 mb-1">
            Urgency
          </label>
          <select
            id="task-urgency"
            value={values.urgency}
            onChange={e => handleChange('urgency', parseInt(e.target.value, 10))}
            className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
          >
            {SCALE_OPTIONS.map(opt => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* Buttons */}
      <div className="flex items-center gap-3 pt-2">
        <button
          type="submit"
          className="px-4 py-2 bg-blue-600 text-white text-sm font-medium rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 transition-colors"
        >
          {submitLabel}
        </button>
        {onCancel && (
          <button
            type="button"
            onClick={onCancel}
            className="px-4 py-2 bg-white text-gray-700 text-sm font-medium rounded-md border border-gray-300 hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 transition-colors"
          >
            Cancel
          </button>
        )}
      </div>
    </form>
  );
}
