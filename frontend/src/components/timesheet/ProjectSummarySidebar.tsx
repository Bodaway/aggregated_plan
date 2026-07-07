import { useEffect, useState } from 'react';

import type {
  DayOffScope,
  MutationError,
  ProjectOption,
  ReconstructedDay,
  TimesheetLineInput,
} from '@/hooks/use-timesheet';
import { projectColor } from './project-colors';

interface Props {
  day: ReconstructedDay;
  projects?: ProjectOption[];
  onSaveLines: (lines: TimesheetLineInput[]) => Promise<MutationError | null> | void;
  onValidate: () => void;
  onMarkOff: (scope: DayOffScope) => void;
  onRefresh: () => void;
  busy: boolean;
}

interface EditRow {
  gryzzlyProjectId: string | null;
  label: string;
  hours: number;
  isPinned: boolean;
  confidence: string;
}

export function ProjectSummarySidebar({ day, projects = [], onSaveLines, onValidate, onMarkOff, onRefresh, busy }: Props) {
  const [rows, setRows] = useState<EditRow[]>([]);
  // Track which rows the user edited → those get pinned on save.
  const [edited, setEdited] = useState<Set<number>>(new Set());
  const [saveError, setSaveError] = useState<string | null>(null);

  useEffect(() => {
    setRows(
      day.lines.map((l) => ({
        gryzzlyProjectId: l.gryzzlyProjectId,
        label: l.projectName ?? l.gryzzlyProjectId ?? 'Unattributed',
        hours: l.hours,
        isPinned: l.isPinned,
        confidence: l.confidence,
      })),
    );
    setEdited(new Set());
    setSaveError(null);
  }, [day]);

  const total = rows.reduce((s, r) => s + (Number.isFinite(r.hours) ? r.hours : 0), 0);
  const delta = total - day.targetHours;
  const balanced = Math.abs(delta) < 1e-6;

  // Label to show for a row: follows the selected project, falls back to the
  // line's original name, then to "Unattributed" for null.
  const labelFor = (r: EditRow): string => {
    if (r.gryzzlyProjectId === null) return 'Unattributed';
    return projects.find((p) => p.id === r.gryzzlyProjectId)?.label ?? r.label;
  };

  const setHours = (i: number, value: number) => {
    setRows((prev) => prev.map((r, idx) => (idx === i ? { ...r, hours: value } : r)));
    setEdited((prev) => new Set(prev).add(i));
  };

  const setProject = (i: number, id: string | null) => {
    setRows((prev) => prev.map((r, idx) => (idx === i ? { ...r, gryzzlyProjectId: id } : r)));
    setEdited((prev) => new Set(prev).add(i));
  };

  // Collapse rows sharing the same non-null project into a single line
  // (sum hours, OR the pinned flags) so we never send duplicate project lines.
  const buildLines = (): TimesheetLineInput[] => {
    const byProject = new Map<string, TimesheetLineInput>();
    let unattributed: TimesheetLineInput | null = null;
    rows.forEach((r, i) => {
      const hours = Number.isFinite(r.hours) ? r.hours : 0;
      const isPinned = r.isPinned || edited.has(i);
      if (r.gryzzlyProjectId === null) {
        if (unattributed) {
          unattributed.hours += hours;
          unattributed.isPinned = unattributed.isPinned || isPinned;
        } else {
          unattributed = { gryzzlyProjectId: null, hours, isPinned };
        }
        return;
      }
      const existing = byProject.get(r.gryzzlyProjectId);
      if (existing) {
        existing.hours += hours;
        existing.isPinned = existing.isPinned || isPinned;
      } else {
        byProject.set(r.gryzzlyProjectId, { gryzzlyProjectId: r.gryzzlyProjectId, hours, isPinned });
      }
    });
    return unattributed ? [...byProject.values(), unattributed] : [...byProject.values()];
  };

  const save = async () => {
    const err = await onSaveLines(buildLines());
    setSaveError(err?.message ?? null);
  };

  const locked = day.status === 'VALIDATED' || day.status === 'SUBMITTED';

  return (
    <div className="w-80 shrink-0 bg-white rounded-lg border border-gray-200 p-4 space-y-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-gray-700 uppercase tracking-wider">Hours × project</h2>
        <span className={`text-[10px] px-2 py-0.5 rounded-full ${locked ? 'bg-green-100 text-green-700' : 'bg-gray-100 text-gray-600'}`}>
          {day.status}
        </span>
      </div>

      <div className="space-y-2">
        {rows.map((r, i) => (
          <div key={i} className="space-y-1">
            <div className="flex items-center gap-2">
              <span className={`inline-block w-3 h-3 rounded-sm ${projectColor(r.gryzzlyProjectId)}`} />
              <span className={`flex-1 text-sm truncate ${r.gryzzlyProjectId ? 'text-gray-800' : 'text-amber-700 font-medium'}`}>
                {labelFor(r)}
              </span>
              {r.confidence === 'LOW' && <span title="low confidence" className="text-amber-500 text-xs">▲</span>}
              <input
                type="number"
                step={day.roundingIncrement}
                min={0}
                value={r.hours}
                disabled={locked || busy}
                onChange={(e) => setHours(i, Math.max(0, parseFloat(e.target.value) || 0))}
                className="w-16 text-right text-sm border border-gray-300 rounded px-1 py-0.5 disabled:bg-gray-100"
              />
              <span className="text-xs text-gray-400">h</span>
            </div>
            <select
              value={r.gryzzlyProjectId ?? ''}
              disabled={locked || busy}
              onChange={(e) => setProject(i, e.target.value === '' ? null : e.target.value)}
              className="w-full text-xs border border-gray-300 rounded px-1 py-0.5 bg-white disabled:bg-gray-100"
            >
              <option value="">— Unattributed —</option>
              {projects.map((p) => (
                <option key={p.id} value={p.id}>{p.label}</option>
              ))}
            </select>
          </div>
        ))}
      </div>

      <div className="flex items-center justify-between border-t border-gray-100 pt-2 text-sm">
        <span className="font-medium text-gray-700">
          {total.toFixed(2)} / {day.targetHours.toFixed(1)}h
        </span>
        <span className={balanced ? 'text-green-600' : 'text-amber-600'}>
          {balanced ? '✓ balanced' : `${delta > 0 ? '+' : ''}${delta.toFixed(2)}h`}
        </span>
      </div>

      {saveError && <div className="text-xs text-red-600">{saveError}</div>}

      {!locked && (
        <div className="grid grid-cols-2 gap-2 pt-1">
          <button onClick={save} disabled={busy} className="bg-gray-100 text-gray-800 text-sm rounded px-2 py-1 hover:bg-gray-200 disabled:opacity-50">Save</button>
          <button onClick={onValidate} disabled={busy} className="bg-blue-600 text-white text-sm rounded px-2 py-1 hover:bg-blue-700 disabled:opacity-50">Validate &amp; lock</button>
          <button onClick={onRefresh} disabled={busy} className="bg-white border border-gray-300 text-gray-700 text-sm rounded px-2 py-1 hover:bg-gray-50 disabled:opacity-50">Refresh from signals</button>
          <button onClick={() => onMarkOff('FULL')} disabled={busy} className="bg-white border border-gray-300 text-gray-700 text-sm rounded px-2 py-1 hover:bg-gray-50 disabled:opacity-50">Day off</button>
        </div>
      )}
    </div>
  );
}
