import type { DayOffScope, ProjectOption, ReconstructedDay } from '@/hooks/use-timesheet';
import { projectColor } from './project-colors';

const STATUS_LABELS: Record<string, string> = {
  DRAFT: 'Brouillon',
  VALIDATED: 'Validé',
  SUBMITTED: 'Soumis',
  DAY_OFF: 'Jour off',
};

interface Props {
  day: ReconstructedDay;
  projects?: ProjectOption[];
  onValidate: () => void;
  onMarkOff: (scope: DayOffScope) => void;
  onRefresh: () => void;
  busy: boolean;
}

/**
 * The declaration, READ-ONLY.
 *
 * Hours per project are derived: they are the sum of the quarter shares. Editing them
 * here would be a second source of truth the arbitration could not explain, so the only
 * way to change a number is to change the quarter that produced it.
 */
export function ProjectSummarySidebar({
  day,
  projects = [],
  onValidate,
  onMarkOff,
  onRefresh,
  busy,
}: Props) {
  const total = day.lines.reduce((s, l) => s + l.hours, 0);
  const delta = total - day.targetHours;
  const onTarget = Math.abs(delta) < 1e-6;

  const labelFor = (id: string | null, fallback: string | null): string => {
    if (id === null) return 'Non attribué';
    return projects.find((p) => p.id === id)?.label ?? fallback ?? id;
  };

  /** Which quarters contributed to a project, so a total can be traced back. */
  const quartersFor = (projectId: string | null): number[] =>
    day.quarters
      .filter((q) => q.shares.some((s) => s.gryzzlyProjectId === projectId && s.hours > 0))
      .map((q) => q.index + 1);

  // DAY_OFF stays actionable on purpose: there is no reopen mutation, so locking it
  // would strand the day — reconstructing is the only recovery path.
  const locked = day.status === 'VALIDATED' || day.status === 'SUBMITTED';

  return (
    <div className="w-80 shrink-0 space-y-3 rounded-lg border border-gray-200 bg-white p-4">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold uppercase tracking-wider text-gray-700">
          Heures × projet
        </h2>
        <span
          className={`rounded-full px-2 py-0.5 text-[10px] ${locked ? 'bg-green-100 text-green-700' : 'bg-gray-100 text-gray-600'}`}
        >
          {STATUS_LABELS[day.status] ?? day.status}
        </span>
      </div>

      <div className="space-y-2">
        {day.lines.length === 0 && (
          <p className="text-xs text-gray-400">Rien de déclaré sur cette journée.</p>
        )}
        {day.lines.map((l) => (
          <div key={l.gryzzlyProjectId ?? 'unattributed'} className="space-y-0.5">
            <div className="flex items-center gap-2">
              <span
                className={`inline-block h-3 w-3 rounded-sm ${projectColor(l.gryzzlyProjectId)}`}
              />
              <span
                className={`flex-1 truncate text-sm ${l.gryzzlyProjectId ? 'text-gray-800' : 'font-medium text-amber-700'}`}
              >
                {labelFor(l.gryzzlyProjectId, l.projectName)}
              </span>
              {l.confidence === 'LOW' && (
                <span title="confiance basse" className="text-xs text-amber-500">
                  ▲
                </span>
              )}
              <span className="text-sm tabular-nums text-gray-800">{l.hours.toFixed(2)}</span>
              <span className="text-xs text-gray-400">h</span>
            </div>
            {quartersFor(l.gryzzlyProjectId).length > 0 && (
              <p className="pl-5 text-[10px] text-gray-400">
                depuis Q{quartersFor(l.gryzzlyProjectId).join(', Q')}
              </p>
            )}
          </div>
        ))}
      </div>

      <div className="flex items-center justify-between border-t border-gray-100 pt-2 text-sm">
        <span className="font-medium text-gray-700">
          {total.toFixed(2)} / {day.targetHours.toFixed(1)}h
        </span>
        <span className={onTarget ? 'text-green-600' : 'text-amber-600'}>
          {onTarget ? '✓ conforme' : `${delta > 0 ? '+' : ''}${delta.toFixed(2)}h`}
        </span>
      </div>
      {!onTarget && (
        <p className="text-[10px] text-gray-500">
          Le total est la somme des quarts. L’objectif journalier est un repère, pas un
          facteur d’échelle.
        </p>
      )}

      {!locked && (
        <div className="grid grid-cols-2 gap-2 pt-1">
          <button
            onClick={onValidate}
            disabled={busy}
            className="rounded bg-blue-600 px-2 py-1 text-sm text-white hover:bg-blue-700 disabled:opacity-50"
          >
            Valider et verrouiller
          </button>
          <button
            onClick={onRefresh}
            disabled={busy}
            className="rounded border border-gray-300 bg-white px-2 py-1 text-sm text-gray-700 hover:bg-gray-50 disabled:opacity-50"
          >
            Reconstruire depuis les signaux
          </button>
          <button
            onClick={() => onMarkOff('FULL')}
            disabled={busy}
            className="col-span-2 rounded border border-gray-300 bg-white px-2 py-1 text-sm text-gray-700 hover:bg-gray-50 disabled:opacity-50"
          >
            Jour off
          </button>
        </div>
      )}
    </div>
  );
}
