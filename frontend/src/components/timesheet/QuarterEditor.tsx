import { useState } from 'react';

import type { MutationError, ProjectOption, Quarter } from '@/hooks/use-timesheet';

function hm(min: number): string {
  return `${String(Math.floor(min / 60)).padStart(2, '0')}:${String(min % 60).padStart(2, '0')}`;
}

const CONFIDENCE_STYLE: Record<string, string> = {
  HIGH: 'text-green-700 bg-green-50',
  MEDIUM: 'text-amber-700 bg-amber-50',
  LOW: 'text-red-700 bg-red-50',
};

interface Props {
  quarter: Quarter;
  projects: ProjectOption[];
  roundingIncrement: number;
  readOnly: boolean;
  onSetShare: (quarterIndex: number, laneKey: string, hours: number) => Promise<MutationError | null>;
  onClearShare: (quarterIndex: number, laneKey: string) => Promise<MutationError | null>;
  onReset: (quarterIndex: number) => Promise<MutationError | null>;
}

/**
 * One quarter-day: the tasks that ran in it, the weight behind each share, and the
 * hours the user can override.
 *
 * Editing a share pins it — the server re-apportions the rest of the quarter around the
 * pin and a later reconstruct preserves it, so the arbitration is never silently undone.
 */
export function QuarterEditor({
  quarter,
  projects,
  roundingIncrement,
  readOnly,
  onSetShare,
  onClearShare,
  onReset,
}: Props) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const declared = quarter.shares.reduce((sum, s) => sum + s.hours, 0);
  const balanced = Math.abs(declared - quarter.declarableHours) < 1e-9;
  const projectName = (id: string | null) =>
    id ? (projects.find((p) => p.id === id)?.label ?? id) : 'sans projet Gryzzly';

  const run = async (fn: () => Promise<MutationError | null>) => {
    setBusy(true);
    const err = await fn();
    setBusy(false);
    setError(err?.message ?? null);
  };

  const step = roundingIncrement > 0 ? roundingIncrement : 0.25;
  const pinned = quarter.shares.filter((s) => s.isPinned).length;

  return (
    <div className="rounded border border-gray-200 p-3">
      <div className="flex items-center gap-2">
        <h3 className="text-sm font-semibold text-gray-800">
          Q{quarter.index + 1} · {hm(quarter.startMin)}–{hm(quarter.endMin)}
        </h3>
        <span
          className={`rounded px-1.5 py-0.5 text-[10px] ${CONFIDENCE_STYLE[quarter.confidence] ?? ''}`}
        >
          {quarter.confidence}
        </span>
        {quarter.oooHours > 0 && (
          <span className="text-[10px] text-gray-500">{quarter.oooHours.toFixed(2)} h absent</span>
        )}
        <span className={`ml-auto text-xs ${balanced ? 'text-gray-500' : 'text-red-600'}`}>
          {declared.toFixed(2)} / {quarter.declarableHours.toFixed(2)} h
        </span>
        {pinned > 0 && !readOnly && (
          <button
            onClick={() => void run(() => onReset(quarter.index))}
            disabled={busy}
            className="text-[11px] text-gray-500 underline hover:text-gray-800 disabled:opacity-50"
          >
            réinitialiser
          </button>
        )}
      </div>

      {quarter.shares.length === 0 ? (
        <p className="mt-2 text-xs text-gray-400">Rien de déclaré sur ce quart.</p>
      ) : (
        <ul className="mt-2 space-y-1">
          {quarter.shares.map((s) => (
            <li key={s.laneKey} className="flex items-center gap-2 text-xs">
              <div className="min-w-0 flex-1">
                <div className="truncate text-gray-700" title={s.label}>
                  {s.isPinned && <span title="épinglé par vous">📌 </span>}
                  {s.label}
                </div>
                <div className="truncate text-[10px] text-gray-400">
                  {projectName(s.gryzzlyProjectId)} · {s.presenceMinutes} min de présence
                </div>
              </div>
              <input
                type="number"
                step={step}
                min={0}
                max={quarter.declarableHours}
                value={s.hours}
                disabled={readOnly || busy}
                onChange={(e) => {
                  const hours = Number(e.target.value);
                  if (Number.isFinite(hours)) void run(() => onSetShare(quarter.index, s.laneKey, hours));
                }}
                className="w-20 rounded border border-gray-300 px-1 py-0.5 text-right disabled:bg-gray-50"
                aria-label={`heures pour ${s.label}`}
              />
              {s.isPinned && !readOnly && (
                <button
                  onClick={() => void run(() => onClearShare(quarter.index, s.laneKey))}
                  disabled={busy}
                  className="text-[11px] text-gray-400 underline hover:text-gray-700 disabled:opacity-50"
                  title="Rendre ce quart aux traces"
                >
                  libérer
                </button>
              )}
            </li>
          ))}
        </ul>
      )}

      {!balanced && (
        <p className="mt-2 text-[11px] text-red-600">
          Les parts ne totalisent pas {quarter.declarableHours.toFixed(2)} h.
        </p>
      )}
      {error && <p className="mt-2 text-[11px] text-red-600">{error}</p>}
    </div>
  );
}
