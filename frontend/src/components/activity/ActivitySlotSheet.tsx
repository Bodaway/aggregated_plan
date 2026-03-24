import { useState, useEffect, useCallback } from 'react';
import type { ActivitySlot, TaskPickerItem } from '@/hooks/use-activity';

interface ActivitySlotSheetProps {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
  readonly mode: 'create' | 'edit';
  readonly slot?: ActivitySlot;
  readonly tasks: readonly TaskPickerItem[];
  readonly onSave: (data: {
    taskId?: string | null;
    startTime: string;
    endTime: string;
  }) => void;
}

function todayStr(): string {
  const d = new Date();
  return d.toISOString().slice(0, 10);
}

/** Extract HH:mm from an ISO datetime string (UTC). */
function extractTime(isoStr: string): string {
  try {
    const d = new Date(isoStr);
    if (!isNaN(d.getTime())) {
      return d.getUTCHours().toString().padStart(2, '0') + ':' + d.getUTCMinutes().toString().padStart(2, '0');
    }
  } catch {
    // fall through
  }
  return '';
}

/** Extract YYYY-MM-DD from an ISO datetime or date string. */
function extractDate(str: string): string {
  return str.slice(0, 10);
}

export function ActivitySlotSheet({
  open,
  onOpenChange,
  mode,
  slot,
  tasks,
  onSave,
}: ActivitySlotSheetProps) {
  const [date, setDate] = useState(todayStr);
  const [startTime, setStartTime] = useState('');
  const [endTime, setEndTime] = useState('');
  const [taskId, setTaskId] = useState<string>('');
  const [error, setError] = useState<string | null>(null);

  // Sync form state when slot changes (edit mode) or mode changes
  useEffect(() => {
    if (open) {
      if (mode === 'edit' && slot) {
        setDate(extractDate(slot.date));
        setStartTime(extractTime(slot.startTime));
        setEndTime(slot.endTime ? extractTime(slot.endTime) : '');
        setTaskId(slot.task?.id ?? '');
      } else {
        // Create mode
        setDate(todayStr());
        setStartTime('');
        setEndTime('');
        setTaskId('');
      }
      setError(null);
    }
  }, [open, mode, slot]);

  const handleClose = useCallback(() => {
    onOpenChange(false);
  }, [onOpenChange]);

  const handleSave = useCallback(() => {
    // Validation
    if (!startTime) {
      setError('Start time is required');
      return;
    }
    if (!endTime) {
      setError('End time is required');
      return;
    }
    if (endTime <= startTime) {
      setError('End time must be after start time');
      return;
    }

    setError(null);

    // Build ISO datetime strings from date + time
    const startIso = `${date}T${startTime}:00Z`;
    const endIso = `${date}T${endTime}:00Z`;

    onSave({
      taskId: taskId || null,
      startTime: startIso,
      endTime: endIso,
    });

    onOpenChange(false);
  }, [date, startTime, endTime, taskId, onSave, onOpenChange]);

  // Close on Escape
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') handleClose();
    };
    if (open) {
      document.addEventListener('keydown', handleKeyDown);
      return () => document.removeEventListener('keydown', handleKeyDown);
    }
  }, [open, handleClose]);

  return (
    <>
      {/* Backdrop */}
      {open && (
        <div
          className="fixed inset-0 bg-black/20 z-40 transition-opacity"
          onClick={handleClose}
        />
      )}

      {/* Sheet panel */}
      <div
        className={`fixed top-0 right-0 h-full w-full max-w-md bg-white shadow-xl z-50 transform transition-transform duration-200 ease-in-out ${
          open ? 'translate-x-0' : 'translate-x-full'
        }`}
      >
        {open && (
          <div className="flex flex-col h-full">
            {/* Header */}
            <div className="flex items-center justify-between px-5 py-4 border-b border-gray-200">
              <h2 className="text-base font-semibold text-gray-900">
                {mode === 'create' ? 'Add Activity' : 'Edit Activity'}
              </h2>
              <button
                onClick={handleClose}
                className="p-1.5 text-gray-400 hover:text-gray-600 rounded-md hover:bg-gray-100 transition-colors"
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>

            {/* Content */}
            <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
              {/* Error message */}
              {error && (
                <div className="p-2.5 bg-red-50 border border-red-200 rounded-md">
                  <p className="text-sm text-red-600">{error}</p>
                </div>
              )}

              {/* Date */}
              <div>
                <label className="block text-xs font-medium text-gray-700 mb-1">Date</label>
                <input
                  type="date"
                  value={date}
                  onChange={(e) => setDate(e.target.value)}
                  disabled={mode === 'edit'}
                  className={`w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500 ${
                    mode === 'edit' ? 'bg-gray-50 text-gray-500 cursor-not-allowed' : ''
                  }`}
                />
              </div>

              {/* Start / End time */}
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs font-medium text-gray-700 mb-1">Start Time</label>
                  <input
                    type="time"
                    value={startTime}
                    onChange={(e) => setStartTime(e.target.value)}
                    className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                  />
                </div>
                <div>
                  <label className="block text-xs font-medium text-gray-700 mb-1">End Time</label>
                  <input
                    type="time"
                    value={endTime}
                    onChange={(e) => setEndTime(e.target.value)}
                    className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                  />
                </div>
              </div>

              {/* Task selector */}
              <div>
                <label className="block text-xs font-medium text-gray-700 mb-1">Task (optional)</label>
                <select
                  value={taskId}
                  onChange={(e) => setTaskId(e.target.value)}
                  className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                >
                  <option value="">No task</option>
                  {tasks.map(t => (
                    <option key={t.id} value={t.id}>{t.title}</option>
                  ))}
                </select>
              </div>
            </div>

            {/* Footer */}
            <div className="px-5 py-3 border-t border-gray-200 flex items-center justify-end gap-2">
              <button
                onClick={handleClose}
                className="px-3 py-1.5 text-sm font-medium text-gray-700 border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleSave}
                className="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 transition-colors"
              >
                Save
              </button>
            </div>
          </div>
        )}
      </div>
    </>
  );
}
