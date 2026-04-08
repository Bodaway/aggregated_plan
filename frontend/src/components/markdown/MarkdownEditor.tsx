import { useState, useEffect, useRef, useCallback, KeyboardEvent } from 'react';
import { MarkdownView } from './MarkdownView';

interface MarkdownEditorProps {
  readonly value: string;
  readonly onChange: (value: string) => void;
  readonly placeholder?: string;
  readonly minRows?: number;
  readonly maxRows?: number;
  readonly emptyText?: string;
}

/**
 * Markdown editor with a view ↔ edit toggle.
 *
 * - Defaults to view mode when `value` is non-empty, edit mode when empty.
 * - Edit mode is an autogrowing textarea constrained between minRows and maxRows.
 * - Cmd/Ctrl+Enter exits edit mode.
 * - Purely controlled — the parent owns the state and decides when to persist.
 */
export function MarkdownEditor({
  value,
  onChange,
  placeholder = 'Write notes in markdown…',
  minRows = 8,
  maxRows = 20,
  emptyText = 'No notes yet — click Edit to add some.',
}: MarkdownEditorProps) {
  const [mode, setMode] = useState<'view' | 'edit'>(value && value.trim().length > 0 ? 'view' : 'edit');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Switch to view mode if the value becomes non-empty *after* a re-mount
  // (e.g. when the parent loads task data asynchronously). We don't want to
  // bounce a user out of an active edit session, so we only do it once on
  // initial load — handled via key on the parent if needed.
  // (No effect here.)

  // Autogrow the textarea on every value change
  useEffect(() => {
    if (mode !== 'edit') return;
    const el = textareaRef.current;
    if (!el) return;

    // Reset to auto so scrollHeight reflects content, then clamp.
    el.style.height = 'auto';
    const lineHeight = 20; // ~text-sm leading-5
    const minPx = minRows * lineHeight;
    const maxPx = maxRows * lineHeight;
    const next = Math.max(minPx, Math.min(el.scrollHeight, maxPx));
    el.style.height = `${next}px`;
    el.style.overflowY = el.scrollHeight > maxPx ? 'auto' : 'hidden';
  }, [value, mode, minRows, maxRows]);

  // Focus the textarea when entering edit mode
  useEffect(() => {
    if (mode === 'edit') {
      textareaRef.current?.focus();
    }
  }, [mode]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLTextAreaElement>) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
        e.preventDefault();
        setMode('view');
      }
    },
    []
  );

  if (mode === 'view') {
    return (
      <div className="rounded-md border border-gray-200 bg-gray-50/50 px-3 py-2 relative group">
        <button
          type="button"
          onClick={() => setMode('edit')}
          className="absolute top-1.5 right-1.5 px-2 py-0.5 text-xs font-medium text-gray-500 bg-white border border-gray-200 rounded opacity-0 group-hover:opacity-100 hover:text-gray-700 hover:border-gray-300 transition"
        >
          Edit
        </button>
        <MarkdownView value={value} emptyText={emptyText} />
      </div>
    );
  }

  return (
    <div className="rounded-md border border-gray-300 focus-within:border-blue-500 focus-within:ring-2 focus-within:ring-blue-500/20 transition">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={e => onChange(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        rows={minRows}
        className="w-full px-3 py-2 text-sm font-mono text-gray-800 bg-white rounded-t-md border-0 focus:outline-none resize-none leading-5"
      />
      <div className="flex items-center justify-between px-2 py-1 border-t border-gray-200 bg-gray-50 rounded-b-md">
        <span className="text-xs text-gray-400">Markdown · Ctrl+Enter to preview</span>
        <button
          type="button"
          onClick={() => setMode('view')}
          className="px-2 py-0.5 text-xs font-medium text-blue-600 hover:text-blue-800"
        >
          Done
        </button>
      </div>
    </div>
  );
}
