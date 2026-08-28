import { useEffect, useMemo, useState } from 'react';
import { useActivity } from '@/hooks/use-activity';
import { useTimesheet } from '@/hooks/use-timesheet';
import { useDashboard, type DashboardMeeting } from '@/hooks/use-dashboard';
import { formatDate } from '@/lib/date-utils';
import { getTaskHours } from '@/lib/task-hours';

interface FocusBlockProps {
  /** Whether this block carries the HUD's one glow. Always `true` today —
   *  task 8 will decide it dynamically across the six blocks. */
  readonly lit: boolean;
}

/** Elapsed seconds between an ISO start time and now, floored at zero so a
 *  clock-skewed `startTime` in the future never renders negative. */
function getElapsedSeconds(startTime: string): number {
  return Math.max(0, Math.floor((Date.now() - new Date(startTime).getTime()) / 1000));
}

/** `HH:MM:SS`, zero-padded — the hero chronometer's display format. */
function formatElapsed(totalSeconds: number): string {
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(hours)}:${pad(minutes)}:${pad(seconds)}`;
}

/** `08–10` from minutes-since-midnight boundaries. */
function formatQuarterRange(startMin: number, endMin: number): string {
  const hour = (min: number) => String(Math.floor(min / 60)).padStart(2, '0');
  return `${hour(startMin)}–${hour(endMin)}`;
}

function minutesSinceMidnight(): number {
  const now = new Date();
  return now.getHours() * 60 + now.getMinutes();
}

/** Same rule `DashboardPage` uses to decide whether a meeting eats into
 *  capacity: an all-day free/OOO entry (or the lunch placeholder) doesn't.
 *  Duplicated here — not exported from DashboardPage — because this block
 *  reads `useDashboard()` on its own, outside that page. */
function countsAgainstCapacity(m: DashboardMeeting): boolean {
  if (m.title.toLowerCase() === 'pause midi') return false;
  if (m.showAs === 'free' || m.showAs === 'oof' || m.showAs === 'workingElsewhere') return false;
  return true;
}

function formatHours(hours: number): string {
  return hours.toFixed(1);
}

export function FocusBlock({ lit }: FocusBlockProps) {
  const today = formatDate(new Date());
  const { currentActivity } = useActivity(today);
  const { day } = useTimesheet(new Date());
  const { data } = useDashboard(today);

  const [elapsedSeconds, setElapsedSeconds] = useState(0);

  useEffect(() => {
    if (!currentActivity) {
      setElapsedSeconds(0);
      return;
    }
    setElapsedSeconds(getElapsedSeconds(currentActivity.startTime));
    const id = setInterval(() => {
      setElapsedSeconds(getElapsedSeconds(currentActivity.startTime));
    }, 1000);
    return () => clearInterval(id);
  }, [currentActivity]);

  const quarters = day?.quarters ?? [];
  const currentQuarterIndex = useMemo(() => {
    const mins = minutesSinceMidnight();
    return quarters.findIndex((q) => mins >= q.startMin && mins < q.endMin);
  }, [quarters]);

  const { plannedHours, capacityHours, overloaded } = useMemo(() => {
    if (!data) {
      return { plannedHours: 0, capacityHours: 0, overloaded: false };
    }
    const dayTasks = data.tasks.filter((t) => (t.plannedStart?.slice(0, 10) ?? t.deadline) === today);
    const dayMeetings = data.meetings.filter((m) => m.startTime.slice(0, 10) === today);
    const meetingHours = dayMeetings
      .filter(countsAgainstCapacity)
      .reduce((sum, m) => sum + m.durationHours, 0);
    const taskHours = dayTasks.reduce((sum, t) => sum + getTaskHours(t), 0);
    const planned = taskHours + meetingHours;
    const capacity = data.workingHoursPerDay;
    return { plannedHours: planned, capacityHours: capacity, overloaded: planned > capacity };
  }, [data, today]);

  const panelClass = lit ? 'hud-panel hud-panel--lit hud-focus' : 'hud-panel hud-focus';

  return (
    <div className={panelClass} data-testid="focus-block">
      <div className="hud-label">Focus</div>

      {currentActivity ? (
        <div className="hud-focus__task">
          <b>{currentActivity.task?.title ?? 'Untitled activity'}</b>
        </div>
      ) : (
        <div className="hud-focus__task hud-focus__task--empty">No active task</div>
      )}

      <div className={currentActivity ? 'hud-focus__chrono' : 'hud-focus__chrono hud-focus__chrono--idle'}>
        {currentActivity ? formatElapsed(elapsedSeconds) : '--:--:--'}
      </div>

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

      <div className="hud-focus__foot">
        <div>
          <div className="hud-focus__foot-caption">Day load</div>
          <div className="hud-focus__foot-value">
            {formatHours(plannedHours)}{' '}
            <span className="hud-focus__foot-unit">/ {formatHours(capacityHours)}h</span>
          </div>
          {overloaded && <div className="hud-focus__foot-warning">Over capacity</div>}
        </div>
        <div>
          <div className="hud-focus__foot-caption">Next break</div>
          {/* No due-time field exists on use-break-rules() today (routine
              config + 30-day adherence stats only) — flagged to the
              controller rather than reimplementing the domain engine's
              wall-clock/absorption logic client-side. */}
          <div className="hud-focus__foot-value">—</div>
        </div>
      </div>
    </div>
  );
}
