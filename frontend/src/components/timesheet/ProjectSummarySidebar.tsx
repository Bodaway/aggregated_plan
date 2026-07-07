import { useEffect, useState } from 'react';

import type { DayOffScope, ReconstructedDay, TimesheetLineInput } from '@/hooks/use-timesheet';
import { projectColor } from './project-colors';

interface Props {
  day: ReconstructedDay;
  onSaveLines: (lines: TimesheetLineInput[]) => void;
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

export function ProjectSummarySidebar({ day, onSaveLines, onValidate, onMarkOff, onRefresh, busy }: Props) {
  const [rows, setRows] = useState<EditRow[]>([]);
  // Track which rows the user edited → those get pinned on save.
  const [edited, setEdited] = useState<Set<number>>(new Set());

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
  }, [day]);

  const total = rows.reduce((s, r) => s + (Number.isFinite(r.hours) ? r.hours : 0), 0);
  const delta = total - day.targetHours;
  const balanced = Math.abs(delta) < 1e-6;

  const setHours = (i: number, value: number) => {
    setRows((prev) => prev.map((r, idx) => (idx === i ? { ...r, hours: value } : r)));
    setEdited((prev) => new Set(prev).add(i));
  };

  const save = () => {
    const lines: TimesheetLineInput[] = rows.map((r, i) => ({
      gryzzlyProjectId: r.gryzzlyProjectId,
      hours: r.hours,
      isPinned: r.isPinned || edited.has(i),
    }));
    onSaveLines(lines);
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
          <div key={r.gryzzlyProjectId ?? `unattributed-${i}`} className="flex items-center gap-2">
            <span className={`inline-block w-3 h-3 rounded-sm ${projectColor(r.gryzzlyProjectId)}`} />
            <span className={`flex-1 text-sm truncate ${r.gryzzlyProjectId ? 'text-gray-800' : 'text-amber-700 font-medium'}`}>
              {r.gryzzlyProjectId ? r.label : 'Unattributed'}
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
