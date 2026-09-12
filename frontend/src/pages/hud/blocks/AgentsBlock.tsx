import { useAgentSessions } from '../useAgentSessions';

/**
 * Silence longer than this earns the quiet marker.
 *
 * Raised from the 5 minutes the stub carried — that number was a guess,
 * labelled as one, to be revisited "once real session freshness exists to
 * check it against". It now does. Measured against a real day's
 * `openClaudeSessions`, the split is unambiguous: working sessions sit
 * within a couple of minutes, abandoned ones are hours old. Fifteen minutes
 * falls in that gap with room either side.
 *
 * The wording is "Quiet", not "Idle", and that is not a euphemism: what
 * `lastSeenAt` measures is the last `aplan` call, not the last thought. A
 * session reading code for half an hour without logging anything is silent,
 * and this panel was caught on the real HUD calling a session that was
 * actively working "Idle 6h 34min". The transcript's own mtime is the signal
 * that would actually mean idle, and it arrives with the usage indexer;
 * until then the display says only what it knows.
 */
const IDLE_THRESHOLD_MINUTES = 15;

/** Rows drawn before this panel's fixed grid area runs out — the same cap
 *  the other list blocks apply. It bites here in ordinary use: sessions stay
 *  open until the reaper closes them, so a working day accumulates them.
 *  `useAgentSessions` sorts freshest first, so what a full panel drops is
 *  always the quietest. */
const MAX_VISIBLE_AGENTS = 5;

/** `12 min` under an hour, `1h 12min` beyond it — mirrors PressureBlock's own
 *  countdown formatting (not exported from there), so the whole HUD reads
 *  one convention for "how long". */
function formatMinutes(totalMinutesRaw: number): string {
  const totalMinutes = Math.max(0, Math.round(totalMinutesRaw));
  if (totalMinutes < 60) return `${totalMinutes} min`;
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return `${hours}h ${minutes}min`;
}

/** The live Claude Code sessions and what each one is working on, read from
 *  `openClaudeSessions` — the `sessions` table of migration 014, which
 *  already holds everything the block's original contract asked for. */
export function AgentsBlock() {
  const agents = useAgentSessions();
  const count = agents.length;

  return (
    <div className="hud-panel hud-agents" data-testid="agents-block">
      <div className="hud-label">
        {count > 0 ? `▌ Agents · ${count} session${count === 1 ? '' : 's'}` : '▌ Agents'}
      </div>

      {count === 0 ? (
        <div className="hud-agents__empty">No active session</div>
      ) : (
        <div className="hud-agents__list">
          {agents.slice(0, MAX_VISIBLE_AGENTS).map((a) => {
            const idle = a.lastSeenMinutes > IDLE_THRESHOLD_MINUTES;
            // `taskTitle: null` only means "not linked to a task" — it is
            // not, by itself, silence. A session seen a minute ago reads
            // "Unlinked"; only one that has also stopped calling `aplan`
            // earns the quiet wording.
            const value = a.taskTitle ?? (idle ? `Quiet ${formatMinutes(a.lastSeenMinutes)}` : 'Unlinked');
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
