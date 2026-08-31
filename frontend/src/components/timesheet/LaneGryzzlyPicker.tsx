import { useEffect, useRef, useState } from 'react';

import { GryzzlyTaskOptionList } from '@/components/gryzzly/GryzzlyTaskOptionList';

interface LaneGryzzlyPickerProps {
  /** The lane's own label, so the control has an accessible name of its own — a row of
   *  identical "projet Gryzzly" buttons would be unusable to a screen reader. */
  readonly laneLabel: string;
  /** What the row shows today: the resolved project name, or the "sans projet" wording. */
  readonly projectLabel: string;
  readonly hasProject: boolean;
  /** `null` detaches the task from Gryzzly altogether. */
  readonly onAssign: (gryzzlyTaskId: string | null) => void;
}

/**
 * Click-to-change Gryzzly project, on the lane row where the mistake is visible.
 *
 * The list picks a Gryzzly *task*, not a project: hours are booked on a task, and the
 * project is a snapshot the backend takes from the catalog at assign time. Choosing a
 * project alone would produce hours Gryzzly cannot receive. The list groups by project,
 * so picking one still reads as picking a project.
 *
 * Positioned absolutely rather than portalled — unlike the dashboard card, no lane
 * ancestor is an `overflow-hidden` scroller, so there is nothing here to clip it.
 */
export function LaneGryzzlyPicker({
  laneLabel,
  projectLabel,
  hasProject,
  onAssign,
}: LaneGryzzlyPickerProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  // Escape, and any mousedown outside the control. Scroll deliberately does NOT close:
  // focusing the list's search box scrolls its own container, which would shut the
  // dropdown the instant it opened — the trap the dashboard chip already fell into.
  useEffect(() => {
    if (!open) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    const onMouseDown = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('keydown', onKeyDown);
    document.addEventListener('mousedown', onMouseDown);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('mousedown', onMouseDown);
    };
  }, [open]);

  const pick = (gryzzlyTaskId: string | null) => {
    setOpen(false);
    onAssign(gryzzlyTaskId);
  };

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`projet Gryzzly de ${laneLabel}`}
        title="Changer le projet Gryzzly de cette tâche"
        onClick={() => setOpen((v) => !v)}
        className={`flex w-full items-center gap-0.5 rounded text-left text-[10px] hover:bg-gray-100 ${
          hasProject ? 'text-gray-500' : 'text-amber-600'
        }`}
      >
        <span className="truncate">{projectLabel}</span>
        <svg className="h-2.5 w-2.5 flex-shrink-0 opacity-60" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={3}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {open && (
        <div className="absolute left-0 top-full z-30 mt-1 w-72 rounded-md border border-gray-200 bg-white shadow-lg">
          <GryzzlyTaskOptionList
            assigned={null}
            onSelect={(id) => pick(id)}
            onClear={() => pick(null)}
            className="max-h-64 overflow-y-auto"
          />
          {/* The list only offers "clear" for an assignment it can see, and a lane
              carries the project snapshot without the task it came from. */}
          {hasProject && (
            <button
              type="button"
              onClick={() => pick(null)}
              className="w-full border-t border-gray-100 px-3 py-1.5 text-left text-xs text-red-600 transition-colors hover:bg-red-50"
            >
              Retirer le projet Gryzzly
            </button>
          )}
        </div>
      )}
    </div>
  );
}
