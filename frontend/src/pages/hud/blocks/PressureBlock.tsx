import { useEffect, useMemo, useRef, useState } from 'react';
import { useDashboard } from '@/hooks/use-dashboard';
import { useTimesheet } from '@/hooks/use-timesheet';
import { useNextBreakDue } from '@/hooks/use-break-rules';
import { formatDate } from '@/lib/date-utils';
import { computeCapacity, openDeadlines } from '@/lib/pressure-rules';
import { formatDeadlineLabel } from '@/lib/deadline-label';
import { getTaskHours } from '@/lib/task-hours';
import { isRealMeeting } from '@/lib/is-real-meeting';
import { useSurfaceVisibility } from '../useSurfaceVisibility';

interface PressureBlockProps {
  /** Whether this block carries the HUD's one glow, as arbitrated by
   *  `useDominantBlock`. */
  readonly lit: boolean;
}

/** Rows shown before the panel's own fixed height runs out. The label still
 *  states the true count — this only caps what's drawn. */
const MAX_VISIBLE_DEADLINES = 5;

/** Pink is reserved for a deadline falling today — never for overdue, never
 *  for "soon". */
function isHot(deadline: string, today: string): boolean {
  return deadline === today;
}

/** `12 min` under an hour, `1h 12min` beyond it. Rounds up so the display
 *  never reads "0 min" for a break that is not due quite yet. */
function formatCountdown(remainingMs: number): string {
  const totalMinutes = Math.ceil(remainingMs / 60_000);
  if (totalMinutes < 60) return `${totalMinutes} min`;
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return `${hours}h ${minutes}min`;
}

/** `08–10` from minutes-since-midnight boundaries. */
function formatQuarterRange(startMin: number, endMin: number): string {
  const hour = (min: number) => String(Math.floor(min / 60)).padStart(2, '0');
  return `${hour(startMin)}–${hour(endMin)}`;
}

function formatHours(hours: number): string {
  return hours.toFixed(1);
}

/**
 * The HUD's dominant block: what is bearing down today.
 *
 * Deadlines by proximity and the week's capacity are its own; the quarters
 * of the day, the day's load and the next break arrived from the Focus block
 * this one replaced. Focus was built around `currentActivity` — the *manual*
 * chronometer — which never runs in a workflow driven by Claude sessions and
 * `aplan log`, so its hero read "No active task / No timer" permanently.
 * Everything below that hero was real, and moving it here rather than
 * deleting it is the whole point of the swap.
 */
export function PressureBlock({ lit }: PressureBlockProps) {
  const today = formatDate(new Date());
  const { data } = useDashboard(today);
  const { day } = useTimesheet(new Date());
  const { nextBreakDue, refetch: refetchNextBreakDue } = useNextBreakDue();
  const surfaceVisible = useSurfaceVisibility();

  // One clock drives the break countdown and the current-quarter marker, and
  // it only runs while the surface is actually visible —
  // `useSurfaceVisibility`'s own contract ("every animation in the HUD must
  // be gated on this"), since a ticking countdown behind a hidden window is
  // exactly the cost that hook exists to avoid.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!surfaceVisible) return;
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [surfaceVisible]);

  const deadlines = useMemo(() => openDeadlines(data?.tasks ?? []), [data]);
  const { pct, overloaded: weekOverloaded } = computeCapacity(data?.weeklyWorkload ?? null);

  const remainingBreakMs = nextBreakDue ? new Date(nextBreakDue).getTime() - now : null;
  const breakOverdue = remainingBreakMs !== null && remainingBreakMs <= 0;

  // The fetched instant goes stale the moment it passes — the routine moves
  // on to whatever comes next, which this query has no way to know about
  // until asked again. Ask again, once, the moment "overdue" is first true.
  const refetchedForOverdue = useRef(false);
  useEffect(() => {
    if (breakOverdue && !refetchedForOverdue.current) {
      refetchedForOverdue.current = true;
      refetchNextBreakDue();
    } else if (!breakOverdue) {
      refetchedForOverdue.current = false;
    }
  }, [breakOverdue, refetchNextBreakDue]);

  const quarters = day?.quarters ?? [];
  // Derived from the same `now` as the countdown rather than from a fresh
  // `Date()`: one clock, so the marker and the countdown can never disagree
  // about what time it is, and both freeze together behind a hidden surface.
  const currentQuarterIndex = useMemo(() => {
    const at = new Date(now);
    const mins = at.getHours() * 60 + at.getMinutes();
    return quarters.findIndex((q) => mins >= q.startMin && mins < q.endMin);
  }, [quarters, now]);

  const { plannedHours, capacityHours, dayOverloaded } = useMemo(() => {
    if (!data) {
      return { plannedHours: 0, capacityHours: 0, dayOverloaded: false };
    }
    const dayTasks = data.tasks.filter((t) => (t.plannedStart?.slice(0, 10) ?? t.deadline) === today);
    const dayMeetings = data.meetings.filter((m) => m.startTime.slice(0, 10) === today);
    const meetingHours = dayMeetings
      .filter(isRealMeeting)
      .reduce((sum, m) => sum + m.durationHours, 0);
    const taskHours = dayTasks.reduce((sum, t) => sum + getTaskHours(t), 0);
    const planned = taskHours + meetingHours;
    const capacity = data.workingHoursPerDay;
    return { plannedHours: planned, capacityHours: capacity, dayOverloaded: planned > capacity };
  }, [data, today]);

  const gaugeClass = weekOverloaded ? 'hud-gauge hud-gauge--over' : 'hud-gauge';
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
                  {formatDeadlineLabel(t.deadline, today)}
                </span>
                <span className="hud-pressure__what">{t.title}</span>
              </div>
            );
          })}
        </div>
      )}

      <div className="hud-pressure__day">
        <div className="hud-glowbar" />

        <div className="hud-quarters">
          {[0, 1, 2, 3].map((i) => {
            const q = quarters[i];
            // "Full" maps to the quarter's own confidence verdict (HIGH), the
            // domain engine's summary judgement of how well-evidenced it is —
            // simpler and more robust than re-deriving it from raw shares here.
            const full = q?.confidence === 'HIGH';
            const current = i === currentQuarterIndex;
            const cls = [
              'hud-quarters__segment',
              full && 'hud-quarters__segment--full',
              current && 'hud-quarters__segment--current',
            ]
              .filter(Boolean)
              .join(' ');
            return <i key={i} className={cls} data-testid={`quarter-${i}`} />;
          })}
        </div>
        <div className="hud-quarters__labels">
          {[0, 1, 2, 3].map((i) => {
            const q = quarters[i];
            return (
              <span key={i}>
                {`Q${i + 1}`}
                {q ? ` ${formatQuarterRange(q.startMin, q.endMin)}` : ''}
              </span>
            );
          })}
        </div>

        <div className="hud-pressure__foot">
          <div>
            <div className="hud-pressure__foot-caption">Day load</div>
            <div className="hud-pressure__foot-value">
              {formatHours(plannedHours)}{' '}
              <span className="hud-pressure__foot-unit">/ {formatHours(capacityHours)}h</span>
            </div>
            {dayOverloaded && <div className="hud-pressure__foot-warning">Over capacity</div>}
          </div>
          <div>
            <div className="hud-pressure__foot-caption">Next break</div>
            <div className="hud-pressure__foot-value">
              {remainingBreakMs === null
                ? 'None today'
                : breakOverdue
                  ? 'Overdue'
                  : formatCountdown(remainingBreakMs)}
            </div>
          </div>
        </div>

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
    </div>
  );
}
