import type { ActiveAgent } from './stub-data';
import { STUB_ACTIVE_AGENTS } from './stub-data';

/** A transcript unseen for longer than this reads as a session that has
 *  gone quiet rather than one still working.
 *
 *  This is a display heuristic chosen without data — unlike
 *  PressureBlock's MAX_VISIBLE_DEADLINES, which follows from the panel's
 *  fixed layout, nothing grounds "5" here. It is a guess, named as one so
 *  it doesn't quietly harden into a fact; revisit once plan 2's
 *  `hud-daemon` supplies real session freshness to check it against. */
const IDLE_THRESHOLD_MINUTES = 5;

/** `12 min` under an hour, `1h 12min` beyond it — mirrors FocusBlock's own
 *  countdown formatting (not exported from there), so the whole HUD reads
 *  one convention for "how long". */
function formatMinutes(totalMinutesRaw: number): string {
  const totalMinutes = Math.max(0, Math.round(totalMinutesRaw));
  if (totalMinutes < 60) return `${totalMinutes} min`;
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return `${hours}h ${minutes}min`;
}

interface AgentsBlockProps {
  /** Defaults to the plan-2 stub (see stub-data.ts). A real caller passes
   *  the daemon's own instance once plan 2 exists. */
  readonly agents?: readonly ActiveAgent[];
}

export function AgentsBlock({ agents = STUB_ACTIVE_AGENTS }: AgentsBlockProps) {
  const count = agents.length;

  return (
    <div className="hud-panel hud-agents" data-testid="agents-block">
      <div className="hud-label">
        {count > 0 ? `▌ Agents · ${count} session${count === 1 ? '' : 's'}` : '▌ Agents'}
        {/* Review finding: this block runs on the plan-2 stub (stub-data.ts)
            and reads as real telemetry on screen without this marker — see
            the rule's own comment in hud.css. Plan 2 must remove this
            alongside stub-data.ts, not leave it standing next to real data. */}
        <span className="hud-label__stub" data-testid="stub-marker">
          STUB
        </span>
      </div>

      {count === 0 ? (
        <div className="hud-agents__empty">No active session</div>
      ) : (
        <div className="hud-agents__list">
          {agents.map((a) => {
            const idle = a.lastSeenMinutes > IDLE_THRESHOLD_MINUTES;
            // `taskTitle: null` only means "not linked to a task" (per the
            // contract's own comment) — it is not, by itself, staleness. A
            // session seen a minute ago reads "Unlinked", not "Idle"; only
            // a session that has also gone quiet earns the idle wording.
            const value = a.taskTitle ?? (idle ? `Idle ${formatMinutes(a.lastSeenMinutes)}` : 'Unlinked');
            return (
              <div key={a.sessionName} className="hud-agents__row" data-testid="agent-row">
                <i
                  className={idle ? 'hud-agents__dot hud-agents__dot--idle' : 'hud-agents__dot'}
                  data-testid="agent-dot"
                />
                <span className="hud-agents__who">{a.sessionName}</span>
                <b>{value}</b>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
