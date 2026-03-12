// frontend/src/components/task/TaskCreateSheet.tsx
import { useState, useEffect, useCallback } from 'react';
import { useCreateTask } from '@/hooks/use-create-task';

export interface TaskCreateSheetProps {
  plannedDate: string | null; // "YYYY-MM-DD"; null = closed
  onClose: () => void;
  onCreated: () => void;
}

const URGENCY_OPTIONS = [
  { value: 'LOW', label: 'Low' },
  { value: 'MEDIUM', label: 'Medium' },
  { value: 'HIGH', label: 'High' },
  { value: 'CRITICAL', label: 'Critical' },
] as const;

const IMPACT_OPTIONS = [
  { value: 'LOW', label: 'Low' },
  { value: 'MEDIUM', label: 'Medium' },
  { value: 'HIGH', label: 'High' },
  { value: 'CRITICAL', label: 'Critical' },
] as const;

export function TaskCreateSheet({ plannedDate, onClose, onCreated }: TaskCreateSheetProps) {
  const isOpen = plannedDate !== null;
  const { createTask, loading, error } = useCreateTask();

  const [title, setTitle] = useState('');
  const [estimatedHours, setEstimatedHours] = useState('');
  const [urgency, setUrgency] = useState('MEDIUM');
  const [impact, setImpact] = useState('MEDIUM');
  const [description, setDescription] = useState('');

  // Reset form when sheet opens
  useEffect(() => {
    if (isOpen) {
      setTitle('');
      setEstimatedHours('');
      setUrgency('MEDIUM');
      setImpact('MEDIUM');
      setDescription('');
    }
  }, [isOpen]);

  // Close on Escape
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    if (isOpen) {
      document.addEventListener('keydown', handleKeyDown);
      return () => document.removeEventListener('keydown', handleKeyDown);
    }
  }, [isOpen, onClose]);

  const handleSave = useCallback(async () => {
    if (!title.trim() || loading || !plannedDate) return;

    const result = await createTask({
      title: title.trim(),
      plannedStart: `${plannedDate}T08:00:00Z`,
      estimatedHours: estimatedHours ? parseFloat(estimatedHours) : undefined,
      urgency,
      impact,
      description: description.trim() || undefined,
    });

    if (!result.error) {
      onCreated();
      onClose();
    }
  }, [title, estimatedHours, urgency, impact, description, plannedDate, loading, createTask, onCreated, onClose]);

  return (
    <>
      {/* Backdrop */}
      {isOpen && (
        <div
          className="fixed inset-0 bg-black/20 z-40 transition-opacity"
          onClick={onClose}
        />
      )}

      {/* Sheet panel */}
      <div
        className={`fixed top-0 right-0 h-full w-full max-w-md bg-white shadow-xl z-50 transform transition-transform duration-200 ease-in-out ${
          isOpen ? 'translate-x-0' : 'translate-x-full'
        }`}
      >
        {isOpen && (
          <div className="flex flex-col h-full">
            {/* Header */}
            <div className="flex items-center justify-between px-5 py-4 border-b border-gray-200">
              <h2 className="text-base font-semibold text-gray-900">New Task</h2>
              <button
                onClick={onClose}
                className="p-1.5 text-gray-400 hover:text-gray-600 rounded-md hover:bg-gray-100 transition-colors"
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>

            {/* Content */}
            <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
              {/* Title */}
              <div>
                <label className="block text-xs font-medium text-gray-700 mb-1">
                  Title <span className="text-red-500">*</span>
                </label>
                <input
                  type="text"
                  value={title}
                  onChange={e => setTitle(e.target.value)}
                  autoFocus
                  placeholder="Task title..."
                  className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                />
              </div>

              {/* Planned date (read-only display) */}
              {plannedDate && (
                <div className="flex items-center gap-2 text-sm text-gray-500">
                  <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M6.75 3v2.25M17.25 3v2.25M3 18.75V7.5a2.25 2.25 0 012.25-2.25h13.5A2.25 2.25 0 0121 7.5v11.25m-18 0A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0121 18.75m-18 0v-7.5A2.25 2.25 0 015.25 9h13.5A2.25 2.25 0 0121 11.25v7.5" />
                  </svg>
                  <span>Planned for <strong>{plannedDate}</strong></span>
                </div>
              )}

              {/* Priority */}
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs font-medium text-gray-700 mb-1">Urgency</label>
                  <select
                    value={urgency}
                    onChange={e => setUrgency(e.target.value)}
                    className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                  >
                    {URGENCY_OPTIONS.map(o => (
                      <option key={o.value} value={o.value}>{o.label}</option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="block text-xs font-medium text-gray-700 mb-1">Impact</label>
                  <select
                    value={impact}
                    onChange={e => setImpact(e.target.value)}
                    className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                  >
                    {IMPACT_OPTIONS.map(o => (
                      <option key={o.value} value={o.value}>{o.label}</option>
                    ))}
                  </select>
                </div>
              </div>

              {/* Estimated hours */}
              <div>
                <label className="block text-xs font-medium text-gray-700 mb-1">Estimated hours</label>
                <input
                  type="number"
                  step="0.5"
                  min="0"
                  value={estimatedHours}
                  onChange={e => setEstimatedHours(e.target.value)}
                  placeholder="e.g. 2"
                  className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                />
              </div>

              {/* Description */}
              <div>
                <label className="block text-xs font-medium text-gray-700 mb-1">Description</label>
                <textarea
                  value={description}
                  onChange={e => setDescription(e.target.value)}
                  rows={3}
                  placeholder="Optional description..."
                  className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500 resize-none"
                />
              </div>

              {/* Error */}
              {error && (
                <p className="text-sm text-red-600 bg-red-50 rounded-md px-3 py-2">
                  Failed to create task: {error.message}
                </p>
              )}
            </div>

            {/* Footer */}
            <div className="px-5 py-3 border-t border-gray-200 flex items-center justify-end gap-2">
              <button
                onClick={onClose}
                className="px-3 py-1.5 text-sm font-medium text-gray-700 border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleSave}
                disabled={!title.trim() || loading}
                className="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {loading ? 'Creating...' : 'Create Task'}
              </button>
            </div>
          </div>
        )}
      </div>
    </>
  );
}
