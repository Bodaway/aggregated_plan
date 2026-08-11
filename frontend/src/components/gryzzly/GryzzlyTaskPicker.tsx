import { useState, useCallback, useEffect, useRef } from 'react';
import { useAssignGryzzlyTask } from '@/hooks/use-assign-gryzzly-task';
import { AssignedGryzzlyTask } from '@/lib/gryzzly-picker-options';
import { GryzzlyTaskOptionList } from './GryzzlyTaskOptionList';
import { TerminatedBadge } from './TerminatedBadge';

interface GryzzlyTaskPickerProps {
  readonly taskId: string;
  readonly assigned: AssignedGryzzlyTask | null;
}

/** Full-width Gryzzly task picker for form layouts (the task edit sheet).
 *  The dashboard card uses GryzzlyTaskMenu — same list, chip-sized trigger. */
export function GryzzlyTaskPicker({ taskId, assigned }: GryzzlyTaskPickerProps) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const { assign, clear } = useAssignGryzzlyTask(taskId);

  const handleSelect = useCallback(
    async (gryzzlyTaskId: string) => {
      setOpen(false);
      await assign(gryzzlyTaskId);
    },
    [assign],
  );

  const handleClear = useCallback(async () => {
    setOpen(false);
    await clear();
  }, [clear]);

  // Close on Escape
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [open]);

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  const triggerLabel = assigned
    ? assigned.name ?? '(unknown Gryzzly task)'
    : 'Assign Gryzzly task…';

  return (
    <div ref={menuRef} className="relative">
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className={`w-full flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-md border text-sm transition-colors ${
          assigned
            ? assigned.stale
              ? 'border-amber-300 bg-amber-50 text-amber-800 hover:bg-amber-100'
              : 'border-gray-300 bg-white text-gray-900 hover:bg-gray-50'
            : 'border-gray-300 bg-white text-gray-400 hover:bg-gray-50'
        }`}
      >
        <span className="truncate flex items-center gap-1.5">
          {triggerLabel}
          {assigned?.stale && (
            <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-amber-200 text-amber-800">
              stale
            </span>
          )}
          {assigned?.projectName && !assigned.stale && (
            <span className="text-gray-400 text-xs">— {assigned.projectName}</span>
          )}
          {assigned?.projectStatus === 'done' && <TerminatedBadge />}
        </span>
        <svg className="w-4 h-4 flex-shrink-0 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 15L12 18.75 15.75 15m-7.5-6L12 5.25 15.75 9" />
        </svg>
      </button>

      {open && (
        <GryzzlyTaskOptionList
          assigned={assigned}
          onSelect={(id) => void handleSelect(id)}
          onClear={() => void handleClear()}
          className="absolute left-0 top-full mt-1 z-50 w-full min-w-[280px] max-h-64 overflow-y-auto rounded-md border border-gray-200 bg-white shadow-lg"
        />
      )}
    </div>
  );
}
