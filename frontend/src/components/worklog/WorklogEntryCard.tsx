import { useState, useCallback } from 'react';
import type { WorklogEntry } from '@/hooks/use-worklog';
import { WorklogEntryKebab } from './WorklogEntryKebab';

interface Props {
  readonly entry: WorklogEntry;
  readonly showTaskChip?: boolean;
  readonly onTaskClick?: (taskId: string) => void;
  readonly onSave: (patch: { body?: string; loggedAt?: string }) => Promise<void>;
  readonly onDelete: () => Promise<void>;
}

function formatTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}

function toLocalInputValue(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => n.toString().padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function WorklogEntryCard({ entry, showTaskChip, onTaskClick, onSave, onDelete }: Props) {
  const [mode, setMode] = useState<'view' | 'edit-body' | 'edit-ts'>('view');
  const [body, setBody] = useState(entry.body);
  const [tsInput, setTsInput] = useState(toLocalInputValue(entry.loggedAt));

  const saveBody = useCallback(async () => {
    const trimmed = body.trim();
    if (!trimmed || trimmed === entry.body) {
      setMode('view');
      setBody(entry.body);
      return;
    }
    await onSave({ body: trimmed });
    setMode('view');
  }, [body, entry.body, onSave]);

  const saveTs = useCallback(async () => {
    const dt = new Date(tsInput);
    if (isNaN(dt.getTime())) {
      setMode('view');
      return;
    }
    await onSave({ loggedAt: dt.toISOString() });
    setMode('view');
  }, [tsInput, onSave]);

  return (
    <div className="group rounded-md border border-gray-200 bg-white p-3">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-xs text-gray-500">
            <span>{formatTime(entry.loggedAt)}</span>
            {showTaskChip && entry.task && (
              <button
                type="button"
                onClick={() => onTaskClick?.(entry.task!.id)}
                className="truncate rounded bg-blue-50 px-2 py-0.5 text-xs text-blue-700 hover:bg-blue-100"
              >
                {entry.task.title}
              </button>
            )}
          </div>
          {mode === 'edit-body' ? (
            <div className="mt-2 space-y-1">
              <textarea
                value={body}
                onChange={(e) => setBody(e.target.value)}
                rows={3}
                className="w-full rounded-md border border-gray-300 p-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
                autoFocus
              />
              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  onClick={() => { setBody(entry.body); setMode('view'); }}
                  className="rounded border border-gray-300 px-2 py-1 text-xs hover:bg-gray-50"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  onClick={saveBody}
                  className="rounded bg-blue-600 px-2 py-1 text-xs text-white hover:bg-blue-700"
                >
                  Save
                </button>
              </div>
            </div>
          ) : mode === 'edit-ts' ? (
            <div className="mt-2 flex items-center gap-2">
              <input
                type="datetime-local"
                value={tsInput}
                onChange={(e) => setTsInput(e.target.value)}
                className="rounded-md border border-gray-300 px-2 py-1 text-xs"
              />
              <button
                type="button"
                onClick={() => setMode('view')}
                className="rounded border border-gray-300 px-2 py-1 text-xs hover:bg-gray-50"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={saveTs}
                className="rounded bg-blue-600 px-2 py-1 text-xs text-white hover:bg-blue-700"
              >
                Save
              </button>
            </div>
          ) : (
            <div className="mt-1 whitespace-pre-wrap break-words text-sm text-gray-800">
              {entry.body}
            </div>
          )}
        </div>
        {mode === 'view' && (
          <WorklogEntryKebab
            onEdit={() => setMode('edit-body')}
            onDelete={onDelete}
            onEditTimestamp={() => setMode('edit-ts')}
          />
        )}
      </div>
    </div>
  );
}
