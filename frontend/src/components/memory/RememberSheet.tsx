import { useEffect, useState } from 'react';
import { MEMORY_BACKDROP_Z, MEMORY_SHEET_Z } from '@/lib/memory/layers';
import type { MemoryKind, RememberInput } from '@/lib/memory/types';

interface RememberSheetProps {
  readonly open: boolean;
  readonly initialTitle?: string;
  readonly initialBody?: string | null;
  readonly taskId?: string | null;
  readonly saving?: boolean;
  readonly error?: string | null;
  readonly onClose: () => void;
  readonly onSubmit: (input: RememberInput) => void;
}

const KINDS: readonly { value: MemoryKind; label: string }[] = [
  { value: 'FACT', label: 'Fact' },
  { value: 'DECISION', label: 'Decision' },
  { value: 'COMMITMENT', label: 'Commitment' },
  { value: 'PREFERENCE', label: 'Preference' },
];

const FIELD =
  'w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500';

export function RememberSheet({
  open,
  initialTitle = '',
  initialBody = null,
  taskId = null,
  saving = false,
  error = null,
  onClose,
  onSubmit,
}: RememberSheetProps) {
  const [kind, setKind] = useState<MemoryKind>('FACT');
  const [title, setTitle] = useState(initialTitle);
  const [body, setBody] = useState(initialBody ?? '');
  const [confirmed, setConfirmed] = useState(false);

  // A capture reopens the sheet on a different selection; the fields follow it.
  useEffect(() => {
    if (!open) return;
    setKind('FACT');
    setTitle(initialTitle);
    setBody(initialBody ?? '');
    setConfirmed(false);
  }, [open, initialTitle, initialBody]);

  if (!open) return null;

  const canSave = title.trim() !== '' && !saving;

  const submit = () => {
    if (!canSave) return;
    onSubmit({
      kind,
      title: title.trim(),
      body: body.trim() === '' ? null : body.trim(),
      taskId,
      confirmed,
    });
  };

  return (
    <>
      <div
        className="fixed inset-0 bg-black/20"
        style={{ zIndex: MEMORY_BACKDROP_Z }}
        onClick={onClose}
        aria-hidden
      />
      <div
        role="dialog"
        aria-label="New memory"
        className="fixed top-0 right-0 h-full w-full max-w-xl bg-white shadow-xl flex flex-col"
        style={{ zIndex: MEMORY_SHEET_Z }}
      >
        <div className="flex items-center justify-between px-5 py-4 border-b border-gray-200">
          <h2 className="text-base font-semibold text-gray-900">New memory</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="p-1.5 text-gray-400 hover:text-gray-600 rounded-md hover:bg-gray-100 transition-colors"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
          <div>
            <label htmlFor="memory-kind" className="block text-xs font-medium text-gray-700 mb-1">
              Kind
            </label>
            <select
              id="memory-kind"
              value={kind}
              onChange={e => setKind(e.target.value as MemoryKind)}
              className={FIELD}
            >
              {KINDS.map(k => (
                <option key={k.value} value={k.value}>
                  {k.label}
                </option>
              ))}
            </select>
          </div>

          <div>
            <div className="flex items-baseline gap-2 mb-1">
              <label htmlFor="memory-title" className="text-xs font-medium text-gray-700">
                Title
              </label>
              <span className="text-xs text-gray-400">one sentence: what is retained</span>
            </div>
            <input
              id="memory-title"
              value={title}
              onChange={e => setTitle(e.target.value)}
              className={FIELD}
            />
          </div>

          <div>
            <div className="flex items-baseline gap-2 mb-1">
              <label htmlFor="memory-body" className="text-xs font-medium text-gray-700">
                Why
              </label>
              <span className="text-xs text-gray-400">context, alternatives dropped</span>
            </div>
            <textarea
              id="memory-body"
              value={body}
              onChange={e => setBody(e.target.value)}
              rows={5}
              className={FIELD}
            />
          </div>

          {taskId && (
            <p className="text-xs text-gray-500">Attached to the task the selection came from.</p>
          )}

          <label className="flex items-center gap-2 text-sm text-gray-700">
            <input
              type="checkbox"
              checked={confirmed}
              onChange={e => setConfirmed(e.target.checked)}
              className="rounded border-gray-300"
            />
            Validate now <span className="text-gray-400">(skip the queue)</span>
          </label>

          {error && (
            <p className="text-sm text-red-600 bg-red-50 rounded-md px-3 py-2">{error}</p>
          )}
        </div>

        <div className="px-5 py-3 border-t border-gray-200 flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="px-3 py-1.5 text-sm font-medium text-gray-700 border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={submit}
            disabled={!canSave}
            className="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Save
          </button>
        </div>
      </div>
    </>
  );
}
