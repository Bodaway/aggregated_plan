import { useContext, useEffect, useState } from 'react';
import { Context, type Client } from 'urql';
import { ACTIVE_BREAK_QUERY } from '@/graphql/queries/break-session';
import type { BreakKind } from '@/hooks/use-break-rules';
import { useSurfaceVisibility } from './useSurfaceVisibility';

/** The break being served right now, as the API describes it.
 *
 *  `label` and `body` are copied off the rule at read time, and `endsAt` is the
 *  deadline the backend froze when the break opened — not `startedAt` plus a
 *  duration this side could recompute. One absolute instant, read by both
 *  ends, is what keeps the HUD's countdown and the backend's own close from
 *  drifting apart. */
export interface ActiveBreak {
  readonly eventId: string;
  readonly kind: BreakKind;
  readonly label: string;
  readonly body: string;
  readonly startedAt: string;
  readonly endsAt: string;
}

interface ActiveBreakData {
  readonly activeBreak: ActiveBreak | null;
}

/** Fast enough that the overlay the backend just revealed is showing the break
 *  before the eye settles on it, slow enough to be nothing on the wire. */
const POLL_MS = 2000;

/**
 * The urql client, or null when there is no Provider above.
 *
 * `useClient()` throws in that case, and throwing is right for every other
 * consumer in the app — but this hook runs in `HudPage` above the boot gate,
 * earlier than anything else in the overlay queries, so it is the one place
 * the shell can find itself mounted without the data layer around it. No
 * client has to read as "no break is running", not as an overlay that refuses
 * to paint.
 */
function useOptionalClient(): Client | null {
  const value = useContext(Context);
  return 'executeQuery' in value ? (value as Client) : null;
}

/**
 * The break session the HUD must show instead of its grid, or null.
 *
 * Polled rather than subscribed, and only while the surface is actually
 * visible: the HUD spends most of the day on a hidden Hyprland workspace, and
 * a query running behind it would be the one part of the overlay still costing
 * something at rest. The backend reveals the surface itself when a break
 * opens, so `surface-visibility` arrives first and the poll starts with an
 * immediate ask rather than waiting out an interval.
 *
 * The session is dropped on the way out, not kept: a break can end behind the
 * curtain, and reopening onto a countdown that finished an hour ago would be a
 * lie the user would see before the first answer came back.
 */
export function useActiveBreak(): ActiveBreak | null {
  const client = useOptionalClient();
  const visible = useSurfaceVisibility();
  const [session, setSession] = useState<ActiveBreak | null>(null);

  useEffect(() => {
    if (!visible) {
      setSession(null);
      return;
    }
    if (!client) return;

    let live = true;
    const ask = () => {
      client
        .query<ActiveBreakData>(ACTIVE_BREAK_QUERY, {}, { requestPolicy: 'network-only' })
        .toPromise()
        .then((result) => {
          // A blip on the wire says nothing about the break: answering it with
          // `null` would drop the user out of a break that is still running.
          if (!live || result.error) return;
          setSession(result.data?.activeBreak ?? null);
        })
        .catch(() => undefined);
    };

    ask();
    const id = setInterval(ask, POLL_MS);
    return () => {
      live = false;
      clearInterval(id);
    };
  }, [client, visible]);

  return session;
}
