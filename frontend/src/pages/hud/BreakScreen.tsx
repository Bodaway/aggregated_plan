import { useEffect, useState } from 'react';
import { useMutation } from 'urql';
import { END_BREAK_MUTATION } from '@/graphql/mutations/break-session';
import type { ActiveBreak } from './useActiveBreak';
import { useSurfaceVisibility } from './useSurfaceVisibility';

interface BreakScreenProps {
  /** The running session, as `useActiveBreak` read it off the API. */
  readonly session: ActiveBreak;
}

interface EndBreakData {
  readonly endBreak: boolean;
}

/** The ring's radius in the SVG's own user units — the viewBox is 100 square,
 *  so the stroke has room on either side. Nothing outside this file depends on
 *  the number: the circumference below is derived, never written twice. */
const RING_RADIUS = 46;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

const TICK_MS = 1000;

/** `m:ss`, minutes unpadded — a countdown, not a chronometer. */
function formatRemaining(totalSeconds: number): string {
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, '0')}`;
}

/**
 * The whole overlay for the length of a break: the rule's own words, a ring
 * that empties, the time left, and one way out.
 *
 * Every number here is derived from `endsAt` and the clock at render time, and
 * nothing on screen is decremented. A counter would be simpler and would be
 * wrong: the webview is throttled the moment the compositor stops compositing
 * it, ticks are dropped, and a decremented countdown then reports a break
 * longer than the one the backend is actually timing. Reading the deadline
 * instead means a starved webview is merely late, never wrong.
 *
 * Zero is not this screen's business. The backend owns the clock: it writes
 * `taken` at the deadline and hides the surface. What is below is only what
 * the overlay looks like if it lingers a second past the end.
 */
export function BreakScreen({ session }: BreakScreenProps) {
  const { eventId, kind, label, body, startedAt, endsAt } = session;
  const [, endBreak] = useMutation<EndBreakData>(END_BREAK_MUTATION);
  const surfaceVisible = useSurfaceVisibility();

  // Gated on visibility like every other moving part of the HUD, and safe to
  // gate precisely because the display is a function of the clock: coming back
  // re-reads it and lands on the right second, however long the gap was.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!surfaceVisible) return;
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), TICK_MS);
    return () => clearInterval(id);
  }, [surfaceVisible]);

  const endsAtMs = new Date(endsAt).getTime();
  // Floored at one so a malformed pair can only ever draw a full ring, never
  // divide by zero.
  const totalMs = Math.max(1, endsAtMs - new Date(startedAt).getTime());
  const remainingMs = Math.max(0, endsAtMs - now);
  // Ceiling: a break that has 200ms left still reads "0:01", and "0:00" means
  // the deadline is actually behind us.
  const remaining = formatRemaining(Math.ceil(remainingMs / 1000));
  const spent = Math.min(1, Math.max(0, 1 - remainingMs / totalMs));
  const done = remainingMs === 0;

  const handleReturn = () => {
    // No error handling on purpose: `false` is the tick having closed the row
    // first, in the second the button was pressed, and the surface is being
    // hidden either way. There is nothing to tell anyone.
    void endBreak({ eventId });
  };

  return (
    <div className="hud-break" data-testid="break-screen" data-state={done ? 'done' : 'running'}>
      <div className="hud-label">▌ Pause · {kind}</div>

      <div className="hud-break__ring">
        <svg className="hud-break__ring-svg" viewBox="0 0 100 100" aria-hidden="true">
          <circle className="hud-break__ring-track" cx="50" cy="50" r={RING_RADIUS} />
          {/* Drawn as one dash the length of the whole circle, pushed out of
              view by however much of the break has been spent — so the arc
              that remains IS the time that remains. */}
          <circle
            className="hud-break__ring-run"
            data-testid="break-ring"
            cx="50"
            cy="50"
            r={RING_RADIUS}
            strokeDasharray={RING_CIRCUMFERENCE}
            strokeDashoffset={RING_CIRCUMFERENCE * spent}
          />
        </svg>
        <div className="hud-break__time" role="timer" aria-label="Temps restant">
          {remaining}
        </div>
      </div>

      <div className="hud-break__label">{label}</div>
      <p className="hud-break__body">{body}</p>

      <button type="button" className="hud-break__action" onClick={handleReturn}>
        J'y retourne
      </button>
    </div>
  );
}
