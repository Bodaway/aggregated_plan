import { useState, useCallback, useEffect, useRef } from 'react';
import { useMutation } from 'urql';
import { useGryzzlyTasks } from '@/hooks/use-gryzzly-tasks';
import { buildPickerOptions, AssignedGryzzlyTask } from '@/lib/gryzzly-picker-options';

const ASSIGN_GRYZZLY_TASK = `
  mutation AssignGryzzlyTask($taskId: ID!, $gryzzlyTaskId: ID) {
    assignGryzzlyTask(taskId: $taskId, gryzzlyTaskId: $gryzzlyTaskId) {
      id
      gryzzlyTask {
        gryzzlyTaskId
        name
        projectName
        stale
      }
    }
  }
`;

interface GryzzlyTaskPickerProps {
  readonly taskId: string;
  readonly assigned: AssignedGryzzlyTask | null;
}

export function GryzzlyTaskPicker({ taskId, assigned }: GryzzlyTaskPickerProps) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const menuRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const { options: activeOptions, fetching } = useGryzzlyTasks(search || undefined);
  const options = buildPickerOptions(activeOptions, assigned);

  const [, executeAssign] = useMutation(ASSIGN_GRYZZLY_TASK);

  // Group options by project
  const grouped = options.reduce<Record<string, typeof options>>((acc, opt) => {
    const key = opt.projectName;
    if (!acc[key]) acc[key] = [];
    acc[key].push(opt);
    return acc;
  }, {});

  const handleSelect = useCallback(
    async (gryzzlyTaskId: string) => {
      setOpen(false);
      setSearch('');
      await executeAssign({ taskId, gryzzlyTaskId });
    },
    [taskId, executeAssign],
  );

  const handleClear = useCallback(async () => {
    setOpen(false);
    setSearch('');
    await executeAssign({ taskId, gryzzlyTaskId: null });
  }, [taskId, executeAssign]);

  // Focus search input when opening
  useEffect(() => {
    if (open) {
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  // Close on Escape
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setOpen(false);
        setSearch('');
      }
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
        setSearch('');
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
        </span>
        <svg className="w-4 h-4 flex-shrink-0 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 15L12 18.75 15.75 15m-7.5-6L12 5.25 15.75 9" />
        </svg>
      </button>

      {open && (
        <div
          role="listbox"
          aria-label="Select Gryzzly task"
          className="absolute left-0 top-full mt-1 z-50 w-full min-w-[280px] max-h-64 overflow-y-auto rounded-md border border-gray-200 bg-white shadow-lg"
        >
          {/* Search input */}
          <div className="sticky top-0 bg-white border-b border-gray-100 px-2 py-1.5">
            <input
              ref={inputRef}
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search tasks…"
              className="w-full rounded border border-gray-200 px-2 py-1 text-xs focus:outline-none focus:ring-1 focus:ring-blue-400"
            />
          </div>

          {/* Clear assignment */}
          {assigned && (
            <button
              type="button"
              role="option"
              aria-selected={false}
              onClick={() => void handleClear()}
              className="w-full text-left px-3 py-1.5 text-xs text-red-600 hover:bg-red-50 transition-colors border-b border-gray-100"
            >
              Clear assignment
            </button>
          )}

          {/* Options grouped by project */}
          {fetching ? (
            <div className="px-3 py-2 text-xs text-gray-400">Loading…</div>
          ) : Object.keys(grouped).length === 0 ? (
            <div className="px-3 py-2 text-xs text-gray-400">No tasks found</div>
          ) : (
            Object.entries(grouped).map(([project, items]) => (
              <div key={project}>
                <div className="px-3 py-1 text-[10px] font-semibold text-gray-400 uppercase tracking-wider bg-gray-50 border-b border-gray-100">
                  {project}
                </div>
                {items.map((opt) => (
                  <button
                    key={opt.gryzzlyTaskId}
                    type="button"
                    role="option"
                    aria-selected={opt.gryzzlyTaskId === assigned?.gryzzlyTaskId}
                    onClick={() => void handleSelect(opt.gryzzlyTaskId)}
                    className={`w-full text-left px-3 py-1.5 text-xs hover:bg-gray-50 transition-colors flex items-center justify-between gap-2 ${
                      opt.gryzzlyTaskId === assigned?.gryzzlyTaskId
                        ? 'text-blue-600 bg-blue-50'
                        : 'text-gray-700'
                    }`}
                  >
                    <span className="truncate">{opt.name}</span>
                    {opt.stale && (
                      <span className="inline-flex items-center px-1 py-0.5 rounded text-[9px] font-medium bg-amber-100 text-amber-700 flex-shrink-0">
                        stale
                      </span>
                    )}
                  </button>
                ))}
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
