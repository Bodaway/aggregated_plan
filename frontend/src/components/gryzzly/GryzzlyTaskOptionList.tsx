import { CSSProperties, useEffect, useRef, useState } from 'react';
import { useGryzzlyTasks } from '@/hooks/use-gryzzly-tasks';
import { buildPickerOptions, AssignedGryzzlyTask, GryzzlyOption } from '@/lib/gryzzly-picker-options';
import { TerminatedBadge } from './TerminatedBadge';

interface GryzzlyTaskOptionListProps {
  readonly assigned: AssignedGryzzlyTask | null;
  readonly onSelect: (gryzzlyTaskId: string) => void;
  readonly onClear: () => void;
  /** Positioning belongs to the trigger: the edit-sheet picker anchors its list
   *  absolutely, the dashboard chip portals it and positions it with fixed
   *  coordinates. The list only owns what is inside the box. */
  readonly className?: string;
  readonly style?: CSSProperties;
}

/** The searchable, project-grouped body of every Gryzzly task dropdown.
 *
 * One component rather than one per surface: the edit sheet and the dashboard
 * card must show the same stale/terminated markers, and duplicating the list was
 * the obvious way for them to drift.
 *
 * Mounted only while the dropdown is open, so the catalog query does not fire
 * for every closed picker on screen — the dashboard renders dozens of them. */
export function GryzzlyTaskOptionList({
  assigned,
  onSelect,
  onClear,
  className,
  style,
}: GryzzlyTaskOptionListProps) {
  const [search, setSearch] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  const { options: activeOptions, fetching } = useGryzzlyTasks(search || undefined);
  const options = buildPickerOptions(activeOptions, assigned);

  const grouped = options.reduce<Record<string, GryzzlyOption[]>>((acc, opt) => {
    const key = opt.projectName;
    if (!acc[key]) acc[key] = [];
    acc[key].push(opt);
    return acc;
  }, {});

  useEffect(() => {
    const id = setTimeout(() => inputRef.current?.focus(), 0);
    return () => clearTimeout(id);
  }, []);

  return (
    <div role="listbox" aria-label="Select Gryzzly task" className={className} style={style}>
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
          onClick={() => onClear()}
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
            {/* The badge lives on the group header, not on each row: the
                list already groups by project, so one badge per group. */}
            <div className="px-3 py-1 text-[10px] font-semibold text-gray-400 uppercase tracking-wider bg-gray-50 border-b border-gray-100 flex items-center gap-1.5">
              <span className="truncate">{project}</span>
              {items.some((o) => o.projectStatus === 'done') && <TerminatedBadge small />}
            </div>
            {items.map((opt) => (
              <button
                key={opt.gryzzlyTaskId}
                type="button"
                role="option"
                aria-selected={opt.gryzzlyTaskId === assigned?.gryzzlyTaskId}
                onClick={() => onSelect(opt.gryzzlyTaskId)}
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
  );
}
