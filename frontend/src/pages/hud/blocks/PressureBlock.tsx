import { useMemo } from 'react';
import { differenceInCalendarDays, parseISO } from 'date-fns';
import { useDashboard, type DashboardTask, type WeeklyWorkloadData } from '@/hooks/use-dashboard';
import { formatDate } from '@/lib/date-utils';

/** A half-day, the unit `weeklyWorkload.capacity` is counted in — 4 hours
 *  (morning 08:00-12:00 or afternoon 13:00-17:00, per CLAUDE.md). Needed to
 *  turn that count into hours comparable with `totalPlanned`/`totalMeetings`. */
const HOURS_PER_HALF_DAY = 4;

/** Rows shown before the panel's own fixed height runs out. The label still
 *  states the true count — this only caps what's drawn. */
const MAX_VISIBLE_DEADLINES = 5;

interface DeadlineTask {
  readonly id: string;
  readonly title: string;
  readonly deadline: string;
}

/** Tasks with a deadline, earliest first — "proximity" is chronological,
 *  overdue and all. */
function sortedDeadlines(tasks: readonly DashboardTask[]): DeadlineTask[] {
  return tasks
    .filter((t): t is DashboardTask & { deadline: string } => t.deadline !== null)
    .map((t) => ({ id: t.id, title: t.title, deadline: t.deadline }))
    .sort((a, b) => a.deadline.localeCompare(b.deadline));
}

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

/** Capacity as a percentage of the week's planned load (tasks + meetings)
 *  over its capacity, converted from half-days to hours. `overloaded` is the
 *  domain's own R16 verdict, not re-derived from the percentage here. */
function computeCapacity(workload: WeeklyWorkloadData | null): { pct: number; overloaded: boolean } {
  if (!workload) return { pct: 0, overloaded: false };
  const capacityHours = workload.capacity * HOURS_PER_HALF_DAY;
  const plannedHours = workload.totalPlanned + workload.totalMeetings;
  const pct = capacityHours > 0 ? Math.round((plannedHours / capacityHours) * 100) : 0;
  return { pct, overloaded: workload.overload };
}

export function PressureBlock() {
  const today = formatDate(new Date());
  const { data } = useDashboard(today);

  const deadlines = useMemo(() => sortedDeadlines(data?.tasks ?? []), [data]);
  const { pct, overloaded } = computeCapacity(data?.weeklyWorkload ?? null);

  const gaugeClass = overloaded ? 'hud-gauge hud-gauge--over' : 'hud-gauge';

  return (
    <div className="hud-panel hud-pressure" data-testid="pressure-block">
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
