import { useEffect, useMemo, useState } from 'react';
import { useDashboard, type DashboardMeeting } from '@/hooks/use-dashboard';
import { formatDate } from '@/lib/date-utils';
import { isRealMeeting } from '@/lib/is-real-meeting';
import { useSurfaceVisibility } from '../useSurfaceVisibility';

interface AgendaBlockProps {
  /** Whether this block carries the HUD's one glow, as arbitrated by
   *  `useDominantBlock`. */
  readonly lit: boolean;
}

/** The timeline spans the configured workday window (08:00-17:00, per
 *  CLAUDE.md) — the same bounds FocusBlock's quarters are cut from. There is
 *  no start/end field in the dashboard query to read this from, so it's
 *  hardcoded here the same way FocusBlock hardcodes its quarter boundaries. */
const DAY_START_MIN = 8 * 60;
const DAY_END_MIN = 17 * 60;
const DAY_SPAN_MIN = DAY_END_MIN - DAY_START_MIN;

function minutesSinceMidnight(iso: string): number {
  const d = new Date(iso);
  return d.getHours() * 60 + d.getMinutes();
}

function clampToDayWindow(min: number): number {
  return Math.min(DAY_END_MIN, Math.max(DAY_START_MIN, min));
}

/** Two decimal places — precise enough for a pixel-thin bar, and stable
 *  enough to assert on in a test. */
function pctOfDay(min: number): number {
  return Math.round(((min - DAY_START_MIN) / DAY_SPAN_MIN) * 10000) / 100;
}

function formatClock(min: number): string {
  const h = Math.floor(min / 60);
  const m = min % 60;
  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
}

/** `12 min` under an hour, `1h 12min` beyond it — mirrors FocusBlock's break
 *  countdown formatting (not exported from there), so the whole HUD reads
 *  one convention for "time until". */
function formatMinutes(totalMinutesRaw: number): string {
  const totalMinutes = Math.max(0, Math.round(totalMinutesRaw));
  if (totalMinutes < 60) return `${totalMinutes} min`;
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return `${hours}h ${minutes}min`;
}

function formatCountdown(remainingMs: number): string {
  return formatMinutes(Math.ceil(remainingMs / 60_000));
}

function formatDuration(totalHours: number): string {
  return formatMinutes(totalHours * 60);
}

interface TimelineSegment {
  readonly id: string;
  readonly title: string;
  readonly leftPct: number;
  readonly widthPct: number;
}

function toSegment(m: DashboardMeeting): TimelineSegment | null {
  const start = clampToDayWindow(minutesSinceMidnight(m.startTime));
  const end = clampToDayWindow(minutesSinceMidnight(m.endTime));
  if (end <= start) return null;
  return {
    id: m.id,
    title: m.title,
    leftPct: pctOfDay(start),
    widthPct: Math.round(((end - start) / DAY_SPAN_MIN) * 10000) / 100,
  };
}

export function AgendaBlock({ lit }: AgendaBlockProps) {
  const today = formatDate(new Date());
  const { data } = useDashboard(today);
  const surfaceVisible = useSurfaceVisibility();

  // The countdown and the now-marker both move with time — gated on
  // surface visibility per the HUD's own rule (see FocusBlock's chronometer
  // for the pattern this mirrors).
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!surfaceVisible) return;
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [surfaceVisible]);

  const meetingsToday = useMemo(
    () => (data?.meetings ?? []).filter((m) => m.startTime.slice(0, 10) === today && isRealMeeting(m)),
    [data, today],
  );

  const segments = useMemo(
    () => meetingsToday.map(toSegment).filter((s): s is TimelineSegment => s !== null),
    [meetingsToday],
  );

  const nextMeeting = useMemo(() => {
    const upcoming = meetingsToday
      .filter((m) => new Date(m.endTime).getTime() > now)
      .sort((a, b) => a.startTime.localeCompare(b.startTime));
    return upcoming[0] ?? null;
  }, [meetingsToday, now]);

  const nowMin = new Date(now).getHours() * 60 + new Date(now).getMinutes();
  const nowPct = pctOfDay(clampToDayWindow(nowMin));

  const meetingCount = meetingsToday.length;
  const totalHours = meetingsToday.reduce((sum, m) => sum + m.durationHours, 0);

  const panelClass = lit ? 'hud-panel hud-panel--lit hud-agenda' : 'hud-panel hud-agenda';

  return (
    <div className={panelClass} data-testid="agenda-block">
      <div className="hud-label">▌ Agenda</div>

      {nextMeeting ? (
        <>
          <div className="hud-kv">
            <span>Next</span>
            <b>
              {new Date(nextMeeting.startTime).getTime() <= now
                ? 'Now'
                : formatCountdown(new Date(nextMeeting.startTime).getTime() - now)}
            </b>
          </div>
          <div className="hud-kv">
            <span className="hud-agenda__next-title">{nextMeeting.title}</span>
          </div>
        </>
      ) : (
        <div className="hud-agenda__empty">{meetingCount === 0 ? 'No meetings today' : 'No more meetings today'}</div>
      )}

      <div className="hud-agenda__timeline" data-testid="agenda-timeline">
        {segments.map((s) => (
          <i
            key={s.id}
            className="hud-agenda__segment"
            data-testid="agenda-segment"
            style={{ left: `${s.leftPct}%`, width: `${s.widthPct}%` }}
          />
        ))}
        <u className="hud-agenda__now" data-testid="agenda-now-marker" style={{ left: `${nowPct}%` }} />
      </div>
      <div className="hud-kv">
        <span>{formatClock(DAY_START_MIN)}</span>
        <span>{formatClock(DAY_END_MIN)}</span>
      </div>

      <div className="hud-kv" data-testid="agenda-summary">
        <span>Meetings</span>
        <b>
          {meetingCount} · {formatDuration(totalHours)}
        </b>
      </div>
    </div>
  );
}
