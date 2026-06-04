import { useState, useCallback, useEffect, useRef } from 'react';
import { useMutation } from 'urql';

const UPDATE_TASK_STATUS = `
  mutation UpdateTaskStatus($id: ID!, $input: UpdateTaskInput!) {
    updateTask(id: $id, input: $input) {
      id
      status
    }
  }
`;

const STATUS_OPTIONS = [
  { value: 'TODO', label: 'To Do' },
  { value: 'IN_PROGRESS', label: 'In Progress' },
  { value: 'DONE', label: 'Done' },
  { value: 'BLOCKED', label: 'Blocked' },
] as const;

type StatusValue = typeof STATUS_OPTIONS[number]['value'];

const STATUS_STYLES: Record<StatusValue, string> = {
  TODO: 'bg-gray-100 text-gray-700',
  IN_PROGRESS: 'bg-blue-100 text-blue-700',
  DONE: 'bg-green-100 text-green-700',
  BLOCKED: 'bg-red-100 text-red-700',
};

interface StatusMenuProps {
  readonly taskId: string;
  readonly status: string;
}

export function StatusMenu({ taskId, status }: StatusMenuProps) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const [, executeUpdate] = useMutation(UPDATE_TASK_STATUS);

  const currentStatus = STATUS_OPTIONS.find((o) => o.value === status) ?? STATUS_OPTIONS[0];
  const styleClass = STATUS_STYLES[currentStatus.value as StatusValue] ?? 'bg-gray-100 text-gray-700';

  const handleSelect = useCallback(
    async (value: StatusValue) => {
      setOpen(false);
      if (value === status) return;
      await executeUpdate({ id: taskId, input: { status: value } });
    },
    [taskId, status, executeUpdate]
  );

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Escape') setOpen(false);
  }, []);

  useEffect(() => {
    if (open) {
      document.addEventListener('keydown', handleKeyDown);
      return () => document.removeEventListener('keydown', handleKeyDown);
    }
  }, [open, handleKeyDown]);

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

  return (
    <div ref={menuRef} className="relative inline-block">
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={(e) => { e.stopPropagation(); setOpen((v) => !v); }}
        className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-xs font-medium transition-colors hover:brightness-95 ${styleClass}`}
      >
        {currentStatus.label}
        <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {open && (
        <div
          role="menu"
          className="absolute left-0 top-full mt-1 z-50 min-w-[110px] rounded-md border border-gray-200 bg-white shadow-lg py-1"
        >
          {STATUS_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              role="menuitem"
              type="button"
              onClick={(e) => { e.stopPropagation(); void handleSelect(opt.value); }}
              className={`w-full text-left px-3 py-1.5 text-xs font-medium hover:bg-gray-50 transition-colors ${
                opt.value === status ? 'text-blue-600' : 'text-gray-700'
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
