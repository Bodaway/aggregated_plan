import { useEffect, useMemo, useState } from 'react';
import { useQuery } from 'urql';
import { OPEN_CLAUDE_SESSIONS_QUERY } from '@/graphql/queries/claude-sessions';
import { toActiveAgents, type ActiveAgent, type ClaudeSessionRow } from '@/lib/agent-sessions';
import { useSurfaceVisibility } from './useSurfaceVisibility';

interface OpenClaudeSessionsData {
  readonly openClaudeSessions: readonly ClaudeSessionRow[];
}

/** How often the ages re-render. Coarser than the one-second clocks in
 *  Pressure and Agenda because this display is in whole minutes — thirty
 *  seconds keeps the worst-case lag under a minute at a thirtieth of the
 *  renders. */
const TICK_MS = 30_000;

/**
 * The live Claude Code sessions, freshest first.
 *
 * Re-queried on every opening of the overlay rather than only on mount: the
 * HUD window is persistent — `HudGrid` survives a hide/show cycle untouched
 * — so a query that ran once would keep showing whichever sessions existed
 * when the overlay was launched, and never the one started since. Gated on
 * the same visibility signal as every other moving part of the HUD, so a
 * surface nobody is looking at costs nothing.
 */
export function useAgentSessions(): readonly ActiveAgent[] {
  const surfaceVisible = useSurfaceVisibility();
  const [result, reexecute] = useQuery<OpenClaudeSessionsData>({
    query: OPEN_CLAUDE_SESSIONS_QUERY,
    requestPolicy: 'cache-and-network',
  });

  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!surfaceVisible) return;
    setNow(Date.now());
    reexecute({ requestPolicy: 'network-only' });
    const id = setInterval(() => setNow(Date.now()), TICK_MS);
    return () => clearInterval(id);
  }, [surfaceVisible, reexecute]);

  const sessions = result.data?.openClaudeSessions;
  return useMemo(() => toActiveAgents(sessions ?? [], now), [sessions, now]);
}
