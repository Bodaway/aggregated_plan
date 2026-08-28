import { useMemo } from 'react';
import { differenceInCalendarDays, parseISO } from 'date-fns';
import { useDashboard } from '@/hooks/use-dashboard';
import { formatDate } from '@/lib/date-utils';
import { computeCapacity, openDeadlines } from '@/lib/pressure-rules';

interface PressureBlockProps {
  /** Whether this block carries the HUD's one glow, as arbitrated by
   *  `useDominantBlock`. */
  readonly lit: boolean;
}

/** Rows shown before the panel's own fixed height runs out. The label still
 *  states the true count — this only caps what's drawn. */
const MAX_VISIBLE_DEADLINES = 5;

/** `Today`, `Tomorrow`, `In Nd` or `Overdue` — a date-only comparison since
 *  `deadline` (like `today`) is a bare `YYYY-MM-DD`, no time component. */
function formatWhen(deadline: string, today: string): string {
  const days = differenceInCalendarDays(parseISO(deadline), parseISO(today));
  if (days < 0) return 'Overdue';
  if (days === 0) return 'Today';
  if (days === 1) return 'Tomorrow';
  return `In ${days}d`;
}

/** Pink is reserved for a deadline falling today — never for overdue, never
 *  for "soon". */
function isHot(deadline: string, today: string): boolean {
  return deadline === today;
}

export function PressureBlock({ lit }: PressureBlockProps) {
  const today = formatDate(new Date());
  const { data } = useDashboard(today);

  const deadlines = useMemo(() => openDeadlines(data?.tasks ?? []), [data]);
  const { pct, overloaded } = computeCapacity(data?.weeklyWorkload ?? null);

  const gaugeClass = overloaded ? 'hud-gauge hud-gauge--over' : 'hud-gauge';
  const panelClass = lit ? 'hud-panel hud-panel--lit hud-pressure' : 'hud-panel hud-pressure';

  return (
    <div className={panelClass} data-testid="pressure-block">
      <div className="hud-label">
        {deadlines.length > 0 ? `▌ Pressure · ${deadlines.length} deadline${deadlines.length === 1 ? '' : 's'}` : '▌ Pressure'}
      </div>

      {deadlines.length === 0 ? (
        <div className="hud-pressure__empty">No upcoming deadlines</div>
      ) : (
        <div className="hud-pressure__list">
          {deadlines.slice(0, MAX_VISIBLE_DEADLINES).map((t) => {
            const hot = isHot(t.deadline, today);
            return (
              <div key={t.id} className="hud-pressure__deadline" data-testid="pressure-deadline">
                <span
                  className={hot ? 'hud-pressure__when hud-pressure__when--hot' : 'hud-pressure__when'}
                  data-testid="deadline-when"
                >
                  {formatWhen(t.deadline, today)}
                </span>
                <span className="hud-pressure__what">{t.title}</span>
              </div>
            );
          })}
        </div>
      )}

      <div className="hud-pressure__capacity">
        <div className="hud-kv">
          <span>Capacity</span>
          <b>{pct}%</b>
        </div>
        <div className={gaugeClass} data-testid="pressure-gauge">
          <i style={{ width: `${Math.min(100, pct)}%` }} />
        </div>
      </div>
    </div>
  );
}
