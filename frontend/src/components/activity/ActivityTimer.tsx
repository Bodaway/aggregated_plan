import { useState, useEffect, useRef, useCallback, FormEvent } from 'react';

interface CurrentActivityData {
  readonly id: string;
  readonly startTime: string;
  readonly task?: { readonly id: string; readonly title: string } | null;
}

interface TaskOption {
  readonly id: string;
  readonly title: string;
}

interface ActivityTimerProps {
  readonly currentActivity?: CurrentActivityData | null;
  readonly tasks?: readonly TaskOption[];
  readonly onStart: (taskId?: string) => void;
  readonly onStop: () => void;
  /** Optional: append a quick note to the linked task's `notes` field. */
  readonly onAppendNote?: (taskId: string, text: string) => Promise<unknown>;
}

/** Calculates elapsed time in seconds from a start time string to now. */
function getElapsedSeconds(startTime: string): number {
  const start = new Date(startTime).getTime();
  const now = Date.now();
  return Math.max(0, Math.floor((now - start) / 1000));
}

/** Formats seconds into HH:MM:SS display string. */
function formatElapsed(totalSeconds: number): string {
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(hours)}:${pad(minutes)}:${pad(seconds)}`;
}

export function ActivityTimer({ currentActivity, tasks = [], onStart, onStop, onAppendNote }: ActivityTimerProps) {
  const [elapsed, setElapsed] = useState(0);
  const [selectedTaskId, setSelectedTaskId] = useState<string>('');

  // Quick-note state (only used when an active activity is linked to a task)
  const [noteText, setNoteText] = useState('');
  const [noteSaving, setNoteSaving] = useState(false);
  const [noteFlash, setNoteFlash] = useState<'saved' | 'error' | null>(null);
  const noteInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!currentActivity) {
      setElapsed(0);
      return;
    }

    // Set initial elapsed
    setElapsed(getElapsedSeconds(currentActivity.startTime));

    // Update every second
    const interval = setInterval(() => {
      setElapsed(getElapsedSeconds(currentActivity.startTime));
    }, 1000);

    return () => clearInterval(interval);
  }, [currentActivity]);

  // Auto-clear the saved/error flash
  useEffect(() => {
    if (noteFlash === null) return;
    const t = setTimeout(() => setNoteFlash(null), 1500);
    return () => clearTimeout(t);
  }, [noteFlash]);

  const handleSubmitNote = useCallback(
    async (e: FormEvent<HTMLFormElement>) => {
      e.preventDefault();
      if (!onAppendNote || !currentActivity?.task || noteSaving) return;
      const text = noteText.trim();
      if (!text) return;

      const timestamp = new Date().toLocaleTimeString([], {
        hour: '2-digit',
        minute: '2-digit',
        hour12: false,
      });
      const formatted = `[${timestamp}] ${text}`;

      setNoteSaving(true);
      try {
        await onAppendNote(currentActivity.task.id, formatted);
        setNoteText('');
        setNoteFlash('saved');
        noteInputRef.current?.focus();
      } catch {
        setNoteFlash('error');
      } finally {
        setNoteSaving(false);
      }
    },
    [onAppendNote, currentActivity, noteText, noteSaving]
  );

  if (currentActivity) {
    const canAddNote = Boolean(onAppendNote && currentActivity.task);
    return (
      <div className="bg-white rounded-lg border border-green-200 p-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            {/* Pulsing dot indicator */}
            <div className="relative flex-shrink-0">
              <div className="w-3 h-3 bg-green-500 rounded-full" />
              <div className="absolute inset-0 w-3 h-3 bg-green-500 rounded-full animate-ping opacity-75" />
            </div>

            <div>
              <p className="text-sm font-medium text-gray-800">
                {currentActivity.task?.title ?? 'Working...'}
              </p>
              <p className="text-xs text-gray-500">Activity in progress</p>
            </div>
          </div>

          <div className="flex items-center gap-4">
            {/* Elapsed time */}
            <div className="text-right">
              <p className="text-2xl font-mono font-bold text-gray-800">
                {formatElapsed(elapsed)}
              </p>
              <p className="text-xs text-gray-400">Elapsed</p>
            </div>

            {/* Stop button */}
            <button
              type="button"
              onClick={onStop}
              className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-red-600 rounded-md hover:bg-red-700 transition-colors"
            >
              <svg
                className="w-4 h-4"
                fill="currentColor"
                viewBox="0 0 24 24"
              >
                <rect x="6" y="6" width="12" height="12" rx="1" />
              </svg>
              Stop
            </button>
          </div>
        </div>

        {/* Quick-note input — only when there's a linked task to append to */}
        {canAddNote && (
          <div className="mt-3 pt-3 border-t border-gray-100">
            <form onSubmit={handleSubmitNote} className="flex items-center gap-2">
              <input
                ref={noteInputRef}
                type="text"
                value={noteText}
                onChange={e => setNoteText(e.target.value)}
                placeholder="Quick note (Enter to append to task notes)…"
                disabled={noteSaving}
                className="flex-1 rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-green-500 focus:border-transparent disabled:bg-gray-50"
              />
              <button
                type="submit"
                disabled={!noteText.trim() || noteSaving}
                className="px-3 py-1.5 text-sm font-medium text-white bg-green-600 rounded-md hover:bg-green-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {noteSaving ? '…' : 'Add'}
              </button>
            </form>
            {noteFlash === 'saved' && (
              <p className="mt-1 text-xs text-green-600">Added ✓</p>
            )}
            {noteFlash === 'error' && (
              <p className="mt-1 text-xs text-red-600">Failed to add note</p>
            )}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-3 h-3 bg-gray-300 rounded-full flex-shrink-0" />
          <div>
            <p className="text-sm font-medium text-gray-600">No activity running</p>
            <p className="text-xs text-gray-400">Start tracking your work</p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <select
            value={selectedTaskId}
            onChange={e => setSelectedTaskId(e.target.value)}
            className="text-sm border border-gray-300 rounded-md px-2 py-2 text-gray-700 bg-white hover:border-gray-400 focus:outline-none focus:ring-2 focus:ring-green-500 focus:border-transparent max-w-[240px] truncate"
          >
            <option value="">No task</option>
            {tasks.map(task => (
              <option key={task.id} value={task.id}>
                {task.title}
              </option>
            ))}
          </select>

          <button
            type="button"
            onClick={() => onStart(selectedTaskId || undefined)}
            className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-green-600 rounded-md hover:bg-green-700 transition-colors"
          >
            <svg
              className="w-4 h-4"
              fill="currentColor"
              viewBox="0 0 24 24"
            >
              <path d="M8 5.14v14l11-7-11-7z" />
            </svg>
            Start
          </button>
        </div>
      </div>
    </div>
  );
}
