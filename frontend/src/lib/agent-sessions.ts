/**
 * Structural shape of a row of `openClaudeSessions`, kept minimal so this
 * helper does not depend on component or GraphQL types — same convention as
 * `pressure-rules.ts` and `matrix-top.ts`.
 */
export interface ClaudeSessionRow {
  readonly id: string;
  readonly task: { readonly title: string } | null;
  readonly mode: 'TRACKING' | 'OFF';
  /** The session's working directory, as `aplan session bind` records it —
   *  or whatever `--label` was passed instead. Nullable in the schema. */
  readonly label: string | null;
  readonly lastSeenAt: string;
}

/**
 * One live Claude Code session as the HUD shows it.
 *
 * This interface was the deliverable of the block's first pass, declared in
 * `stub-data.ts` against a daemon that was never written. It moved here when
 * the block was wired to `openClaudeSessions` instead: the `sessions` table
 * (migration 014) already holds everything the contract asked for.
 */
export interface ActiveAgent {
  readonly sessionName: string;
  /** `null` = the session is not linked to a task (it declined tracking, or
   *  has not bound one yet). Not staleness — see `AgentsBlock`. */
  readonly taskTitle: string | null;
  readonly lastSeenMinutes: number;
}

/** How many characters of the session id disambiguate the name. Two, because
 *  four sessions of the same repository open at once is the ordinary case
 *  here and the bare directory name would render four identical rows. */
const ID_SLICE = 2;

/** How much of the id stands alone when there is no label to qualify. Eight,
 *  the short-hash convention: a full 36-character UUID does not fit a panel
 *  three columns wide, and this block's whole job is to be readable at a
 *  glance. */
const BARE_ID_SLICE = 8;

/** Last path segment of a directory, trailing slash tolerated. A `label`
 *  that is not a path (an explicit `--label`) comes back unchanged. */
function basename(label: string): string {
  const trimmed = label.replace(/\/+$/, '');
  return trimmed.slice(trimmed.lastIndexOf('/') + 1);
}

/** `cicd-safteaction-3d`. With no label there is nothing to qualify, so a
 *  short id stands alone rather than being suffixed with a slice of itself. */
function sessionName(session: ClaudeSessionRow): string {
  if (!session.label) return session.id.slice(0, BARE_ID_SLICE);
  return `${basename(session.label)}-${session.id.slice(0, ID_SLICE)}`;
}

/**
 * The open sessions, freshest first, as the Agents block renders them.
 *
 * Order matters beyond looks: the block caps its rows, so a session that
 * went quiet three hours ago must not push out one working right now.
 *
 * Freshness comes from `lastSeenAt`, which the API stamps on every `aplan`
 * call. That is a floor, not the truth: a session thinking hard without
 * logging anything looks idle. The transcript's own mtime is the real
 * signal, and it arrives with the Claude usage indexer.
 */
export function toActiveAgents(sessions: readonly ClaudeSessionRow[], now: number): ActiveAgent[] {
  return sessions
    .map((s) => ({
      sessionName: sessionName(s),
      taskTitle: s.task?.title ?? null,
      // Floored at zero so a clock skew putting "last seen" in the future
      // never renders a negative age.
      lastSeenMinutes: Math.max(0, Math.floor((now - new Date(s.lastSeenAt).getTime()) / 60_000)),
    }))
    .sort((a, b) => a.lastSeenMinutes - b.lastSeenMinutes);
}
