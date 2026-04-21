import { useState, useRef, useEffect } from 'react';

interface Props {
  readonly onEdit: () => void;
  readonly onDelete: () => void;
  readonly onEditTimestamp: () => void;
}

export function WorklogEntryKebab({ onEdit, onDelete, onEditTimestamp }: Props) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, [open]);

  return (
    <div ref={containerRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-700"
        aria-label="Entry actions"
      >
        <svg className="h-4 w-4" fill="currentColor" viewBox="0 0 20 20">
          <path d="M10 6a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm0 5.5a1.5 1.5 0 110-3 1.5 1.5 0 010 3zm0 5.5a1.5 1.5 0 110-3 1.5 1.5 0 010 3z" />
        </svg>
      </button>
      {open && (
        <div className="absolute right-0 top-full z-20 mt-1 w-40 rounded-md border border-gray-200 bg-white py-1 shadow-lg">
          <button
            type="button"
            onClick={() => { setOpen(false); onEdit(); }}
            className="block w-full px-3 py-1.5 text-left text-sm hover:bg-gray-50"
          >
            Edit
          </button>
          <button
            type="button"
            onClick={() => { setOpen(false); onEditTimestamp(); }}
            className="block w-full px-3 py-1.5 text-left text-sm hover:bg-gray-50"
          >
            Edit timestamp…
          </button>
          <button
            type="button"
            onClick={() => { setOpen(false); onDelete(); }}
            className="block w-full px-3 py-1.5 text-left text-sm text-red-600 hover:bg-red-50"
          >
            Delete
          </button>
        </div>
      )}
    </div>
  );
}
