import { useState, useEffect, useCallback } from 'react';
import { useTaskEdit } from '@/hooks/use-task-edit';
import { MarkdownEditor } from '@/components/markdown/MarkdownEditor';

interface TaskEditSheetProps {
  readonly taskId: string | null;
  readonly onClose: () => void;
  readonly onUpdated?: () => void;
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

const STATUS_OPTIONS = [
  { value: 'TODO', label: 'To Do' },
  { value: 'IN_PROGRESS', label: 'In Progress' },
  { value: 'DONE', label: 'Done' },
  { value: 'BLOCKED', label: 'Blocked' },
] as const;

const STATUS_STYLES: Record<string, string> = {
  TODO: 'bg-gray-100 text-gray-700 border-gray-300',
  IN_PROGRESS: 'bg-blue-50 text-blue-700 border-blue-300',
  DONE: 'bg-green-50 text-green-700 border-green-300',
  BLOCKED: 'bg-red-50 text-red-700 border-red-300',
};

/** GraphQL returns urgency/impact as enum strings (LOW, MEDIUM, HIGH, CRITICAL). */
function normalizeEnum(val: string): string {
  const upper = String(val).toUpperCase();
  if (['LOW', 'MEDIUM', 'HIGH', 'CRITICAL'].includes(upper)) return upper;
  return 'MEDIUM';
}

function formatSeconds(seconds: number | null): string {
  if (seconds === null || seconds === undefined) return '-';
  const hours = seconds / 3600;
  if (hours < 1) return `${Math.round(seconds / 60)}m`;
  return `${hours.toFixed(1)}h`;
}

export function TaskEditSheet({ taskId, onClose, onUpdated }: TaskEditSheetProps) {
  const { task, loading, updateTask, updatePriority } = useTaskEdit(taskId);
  const isOpen = taskId !== null;
  const isJira = task?.source === 'JIRA' || task?.source === 'EXCEL';

  // Local form state
  const [description, setDescription] = useState('');
  const [notes, setNotes] = useState('');
  const [estimatedHours, setEstimatedHours] = useState('');
  const [remainingOverride, setRemainingOverride] = useState('');
  const [estimatedOverride, setEstimatedOverride] = useState('');
  const [urgency, setUrgency] = useState('MEDIUM');
  const [impact, setImpact] = useState('MEDIUM');
  const [status, setStatus] = useState('TODO');
  const [plannedDate, setPlannedDate] = useState('');

  // Sync form state when task loads
  useEffect(() => {
    if (task) {
      setDescription(task.description ?? '');
      setNotes(task.notes ?? '');
      setEstimatedHours(task.estimatedHours?.toString() ?? '');
      setRemainingOverride(task.remainingHoursOverride?.toString() ?? '');
      setEstimatedOverride(task.estimatedHoursOverride?.toString() ?? '');
      setUrgency(normalizeEnum(task.urgency));
      setImpact(normalizeEnum(task.impact));
      setStatus(task.status ?? 'TODO');
      // Extract date portion from ISO datetime
      setPlannedDate(task.plannedStart ? task.plannedStart.slice(0, 10) : '');
    }
  }, [task]);

  const handleSave = useCallback(async () => {
    if (!task) return;

    // Update urgency/impact via priority mutation
    const currentUrgency = normalizeEnum(task.urgency);
    const currentImpact = normalizeEnum(task.impact);
    if (urgency !== currentUrgency || impact !== currentImpact) {
      await updatePriority(urgency, impact);
    }

    // Build update input for other fields
    const input: Record<string, unknown> = {};

    if (status !== task.status) {
      input.status = status;
    }

    const newDesc = description || null;
    if (newDesc !== (task.description ?? null)) {
      input.description = newDesc;
    }

    const newNotes = notes || null;
    if (newNotes !== (task.notes ?? null)) {
      input.notes = newNotes;
    }

    if (isJira) {
      // Override fields for Jira/Excel tasks
      const newRemaining = remainingOverride ? parseFloat(remainingOverride) : null;
      if (newRemaining !== task.remainingHoursOverride) {
        input.remainingHoursOverride = newRemaining;
      }
      const newEstOverride = estimatedOverride ? parseFloat(estimatedOverride) : null;
      if (newEstOverride !== task.estimatedHoursOverride) {
        input.estimatedHoursOverride = newEstOverride;
      }
    } else {
      // Personal tasks: write directly to estimatedHours
      const newEst = estimatedHours ? parseFloat(estimatedHours) : null;
      if (newEst !== task.estimatedHours) {
        input.estimatedHours = newEst;
      }
    }

    // Planned date
    const currentPlannedDate = task.plannedStart ? task.plannedStart.slice(0, 10) : '';
    if (plannedDate !== currentPlannedDate) {
      if (plannedDate) {
        // Convert date string to ISO datetime (start of day UTC)
        input.plannedStart = `${plannedDate}T08:00:00Z`;
      } else {
        input.plannedStart = null;
      }
    }

    if (Object.keys(input).length > 0) {
      await updateTask(input);
    }

    onUpdated?.();
    onClose();
  }, [task, status, description, notes, estimatedHours, remainingOverride, estimatedOverride, urgency, impact, plannedDate, isJira, updateTask, updatePriority, onUpdated, onClose]);

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
        className={`fixed top-0 right-0 h-full w-full max-w-2xl bg-white shadow-xl z-50 transform transition-transform duration-200 ease-in-out ${
          isOpen ? 'translate-x-0' : 'translate-x-full'
        }`}
      >
        {isOpen && (
          <div className="flex flex-col h-full">
            {/* Header */}
            <div className="flex items-center justify-between px-5 py-4 border-b border-gray-200">
              <div className="flex items-center gap-2 min-w-0">
                {task?.sourceId && (
                  <span className="text-xs font-mono font-medium text-blue-600 flex-shrink-0">
                    {task.sourceId}
                  </span>
                )}
                <h2 className="text-base font-semibold text-gray-900 truncate">
                  {task?.title ?? 'Loading...'}
                </h2>
              </div>
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
            <div className="flex-1 overflow-y-auto px-5 py-4 space-y-5">
              {loading && !task ? (
                <div className="flex items-center justify-center py-12">
                  <div className="w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
                </div>
              ) : task ? (
                <>
                  {/* Info section */}
                  <div className="space-y-2">
                    <div className="flex items-center gap-2 text-sm text-gray-600">
                      <span className="font-medium w-20">Status:</span>
                      <select
                        value={status}
                        onChange={(e) => setStatus(e.target.value)}
                        className={`rounded-md border px-2 py-0.5 text-xs font-medium focus:outline-none focus:ring-2 focus:ring-blue-500 ${STATUS_STYLES[status] ?? 'bg-gray-100 text-gray-700 border-gray-300'}`}
                      >
                        {STATUS_OPTIONS.map(o => (
                          <option key={o.value} value={o.value}>{o.label}</option>
                        ))}
                      </select>
                      {task.jiraStatus && (
                        <span className="px-2 py-0.5 bg-blue-50 text-blue-700 rounded text-xs font-medium border border-blue-200">
                          {task.jiraStatus}
                        </span>
                      )}
                    </div>
                    {task.assignee && (
                      <div className="flex items-center gap-2 text-sm text-gray-600">
                        <span className="font-medium w-20">Assignee:</span>
                        <span>{task.assignee}</span>
                      </div>
                    )}
                    {task.deadline && (
                      <div className="flex items-center gap-2 text-sm text-gray-600">
                        <span className="font-medium w-20">Deadline:</span>
                        <span>{task.deadline}</span>
                      </div>
                    )}
                    {task.project?.name && (
                      <div className="flex items-center gap-2 text-sm text-gray-600">
                        <span className="font-medium w-20">Project:</span>
                        <span>{task.project.name}</span>
                      </div>
                    )}
                  </div>

                  {/* Jira time tracking (read-only display) */}
                  {isJira && (task.jiraOriginalEstimateSeconds !== null || task.jiraTimeSpentSeconds !== null || task.jiraRemainingSeconds !== null) && (
                    <div className="bg-blue-50 rounded-lg p-3 space-y-1.5">
                      <h4 className="text-xs font-semibold text-blue-800 uppercase tracking-wider">Jira Time Tracking</h4>
                      <div className="grid grid-cols-3 gap-2 text-center">
                        <div>
                          <p className="text-xs text-blue-600">Estimate</p>
                          <p className="text-sm font-medium text-blue-900">{formatSeconds(task.jiraOriginalEstimateSeconds)}</p>
                        </div>
                        <div>
                          <p className="text-xs text-blue-600">Logged</p>
                          <p className="text-sm font-medium text-blue-900">{formatSeconds(task.jiraTimeSpentSeconds)}</p>
                        </div>
                        <div>
                          <p className="text-xs text-blue-600">Remaining</p>
                          <p className="text-sm font-medium text-blue-900">{formatSeconds(task.jiraRemainingSeconds)}</p>
                        </div>
                      </div>
                    </div>
                  )}

                  {/* Editable fields */}
                  <div className="space-y-4">
                    <div>
                      <label className="block text-xs font-medium text-gray-700 mb-1">Planned Date</label>
                      <input
                        type="date"
                        value={plannedDate}
                        onChange={(e) => setPlannedDate(e.target.value)}
                        className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                      />
                      {plannedDate && (
                        <button
                          type="button"
                          onClick={() => setPlannedDate('')}
                          className="mt-1 text-xs text-gray-400 hover:text-red-500 transition-colors"
                        >
                          Clear planned date
                        </button>
                      )}
                    </div>

                    <h4 className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Priority</h4>

                    <div className="grid grid-cols-2 gap-3">
                      <div>
                        <label className="block text-xs font-medium text-gray-700 mb-1">Urgency</label>
                        <select
                          value={urgency}
                          onChange={(e) => setUrgency(e.target.value)}
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
                          onChange={(e) => setImpact(e.target.value)}
                          className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                        >
                          {IMPACT_OPTIONS.map(o => (
                            <option key={o.value} value={o.value}>{o.label}</option>
                          ))}
                        </select>
                      </div>
                    </div>

                    <h4 className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Time Estimates</h4>

                    {isJira ? (
                      <div className="grid grid-cols-2 gap-3">
                        <div>
                          <label className="block text-xs font-medium text-gray-700 mb-1">
                            Remaining (h) <span className="text-gray-400">override</span>
                          </label>
                          <input
                            type="number"
                            step="0.5"
                            min="0"
                            value={remainingOverride}
                            onChange={(e) => setRemainingOverride(e.target.value)}
                            placeholder={task.jiraRemainingSeconds !== null ? formatSeconds(task.jiraRemainingSeconds) : '-'}
                            className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                          />
                        </div>
                        <div>
                          <label className="block text-xs font-medium text-gray-700 mb-1">
                            Estimate (h) <span className="text-gray-400">override</span>
                          </label>
                          <input
                            type="number"
                            step="0.5"
                            min="0"
                            value={estimatedOverride}
                            onChange={(e) => setEstimatedOverride(e.target.value)}
                            placeholder={task.jiraOriginalEstimateSeconds !== null ? formatSeconds(task.jiraOriginalEstimateSeconds) : '-'}
                            className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                          />
                        </div>
                      </div>
                    ) : (
                      <div>
                        <label className="block text-xs font-medium text-gray-700 mb-1">Estimated hours</label>
                        <input
                          type="number"
                          step="0.5"
                          min="0"
                          value={estimatedHours}
                          onChange={(e) => setEstimatedHours(e.target.value)}
                          placeholder="e.g. 4"
                          className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                        />
                      </div>
                    )}

                    <div>
                      <div className="flex items-center justify-between mb-1">
                        <label className="block text-xs font-medium text-gray-700">Description</label>
                        {isJira && (
                          <span className="text-[10px] text-amber-600">
                            synced from Jira — local edits will be overwritten
                          </span>
                        )}
                      </div>
                      <textarea
                        value={description}
                        onChange={(e) => setDescription(e.target.value)}
                        rows={8}
                        className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                        placeholder="Add a description..."
                      />
                    </div>

                    <div>
                      <label className="block text-xs font-medium text-gray-700 mb-1">
                        Notes <span className="text-gray-400">(markdown · local only)</span>
                      </label>
                      <MarkdownEditor
                        value={notes}
                        onChange={setNotes}
                        placeholder="Working notes, decisions, links… (preserved across Jira syncs)"
                      />
                    </div>
                  </div>
                </>
              ) : null}
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
