import { useEffect, useMemo, useState } from 'react';
import { useDashboard, type DailyDashboardData, type DashboardMeeting } from '@/hooks/use-dashboard';
import { formatDate } from '@/lib/date-utils';
import { isRealMeeting } from '@/lib/is-real-meeting';
import { computeCapacity, hasOpenDeadlineOn } from '@/lib/pressure-rules';
import { useSurfaceVisibility } from './useSurfaceVisibility';

/** The three blocks that can carry the HUD's one glow. Neural budget, Agents
 *  and Station never do — they report, they never demand attention. */
export type DominantBlock = 'focus' | 'pressure' | 'agenda';

/** A meeting this close is imminent. Strictly under ten minutes, and only
 *  ahead of the start: once the meeting is under way the moment whose miss is
 *  irreversible has passed, so the glow goes back to work. The start instant
 *  itself still counts — a zero-length hole there would just be an off-by-one. */
const IMMINENT_MEETING_MS = 10 * 60_000;

/** Minutes since midnight past which an unfinished deadline due today becomes
 *  pressure rather than plan. Inclusive: 15:00:00 sharp is already "after 15h",
 *  a one-second hole ahead of it would mean nothing to anyone. */
const DEADLINE_PRESSURE_MINUTE = 15 * 60;

/** How often the arbitration is re-run. Coarser than the one-second clocks in
 *  Focus and Agenda on purpose: this hook lives in `HudPage`, so every tick
 *  re-renders the whole grid, and both thresholds it watches move at minute
 *  granularity. Fifteen seconds keeps the worst-case lateness invisible
 *  (a "ten minutes" warning arrives no later than T-9:45) at a twentieth of
 *  the renders. */
const TICK_MS = 15_000;

/** Whether a real meeting on `today` starts inside the imminence window.
 *  The `isRealMeeting` + same-day pair is the very filter `AgendaBlock`
 *  applies to its timeline: the glow must never announce a meeting the
 *  agenda itself refuses to draw. */
function hasImminentMeeting(meetings: readonly DashboardMeeting[], today: string, now: number): boolean {
  return meetings.some((m) => {
    if (m.startTime.slice(0, 10) !== today) return false;
    if (!isRealMeeting(m)) return false;
    const untilStart = new Date(m.startTime).getTime() - now;
    return untilStart >= 0 && untilStart < IMMINENT_MEETING_MS;
  });
}

function isPastDeadlineHour(now: number): boolean {
  const d = new Date(now);
  return d.getHours() * 60 + d.getMinutes() >= DEADLINE_PRESSURE_MINUTE;
}

/**
 * The arbitration itself, in strictly decreasing priority:
 *
 * 1. `agenda` — a meeting starts in under ten minutes. It is the only thing
 *    on this screen whose moment, once missed, cannot be recovered.
 * 2. `pressure` — the week is overloaded (the domain's own R16 verdict, not
 *    a rounded gauge), or a deadline due today is still open after 15:00.
 * 3. `focus` — otherwise. The default is work, not alarm.
 */
function arbitrate(data: DailyDashboardData | null, today: string, now: number): DominantBlock {
  if (hasImminentMeeting(data?.meetings ?? [], today, now)) return 'agenda';

  const { overloaded } = computeCapacity(data?.weeklyWorkload ?? null);
  if (overloaded) return 'pressure';
  if (isPastDeadlineHour(now) && hasOpenDeadlineOn(data?.tasks ?? [], today)) return 'pressure';

  return 'focus';
}

/**
 * Which of the six blocks wears the HUD's one glow right now.
 *
 * Exactly one, always: the return type is a single block name, and `HudPage`
 * derives every `lit` prop from it — the invariant cannot be broken by
 * forgetting to turn a panel off.
 */
export function useDominantBlock(): DominantBlock {
  const today = formatDate(new Date());
  const { data } = useDashboard(today);
  const surfaceVisible = useSurfaceVisibility();

  // Gated on visibility like every other moving part of the HUD (see
  // `useSurfaceVisibility`): a HUD nobody is looking at does not need to
  // re-decide where to point. Re-reading the clock as the effect resumes is
  // what makes the glow correct again the instant the surface comes back.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!surfaceVisible) return;
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), TICK_MS);
    return () => clearInterval(id);
  }, [surfaceVisible]);

  return useMemo(() => arbitrate(data, today, now), [data, today, now]);
}
