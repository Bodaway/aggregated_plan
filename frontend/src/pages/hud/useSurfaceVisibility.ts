import { useEffect, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** The event `src-tauri/src/main.rs` emits when the toggle script signals it.
 *  Payload: `true` shown, `false` hidden. */
const SURFACE_VISIBILITY = 'surface-visibility';

/**
 * True while the HUD surface is actually being looked at.
 *
 * Every animation in the HUD is gated on this. The design benchmark showed
 * that continuous animation accounts for ~99% of CPU cost, and that no native
 * toolkit stops on its own when covered — so we stop explicitly.
 *
 * There are two sources, because neither one covers both worlds:
 *
 * - In a browser, `document.visibilityState` is the answer, and the only one
 *   available.
 * - In the Tauri overlay it is WRONG, and silently so. Measured on the real
 *   compositor: while Hyprland holds the window on a hidden special
 *   workspace, the webview goes on reporting "visible" indefinitely. Every
 *   gate downstream of this hook was therefore doing nothing at all. The
 *   toggle script performs the hide, so it is the only party that knows; it
 *   signals the shell, which re-emits as `surface-visibility`.
 *
 * The Tauri seed is `true`, and that is a fact about how the overlay is
 * launched rather than an optimistic guess: `aplan-hud-toggle` starts the
 * binary only when no HUD is running, and shows the workspace in the same
 * breath — so mount and first opening are the same moment. Seeding `false`
 * instead would strand the very first opening behind a boot screen no signal
 * ever clears, because the signal announcing it is sent while the webview is
 * still loading. Every hide and show after that arrives as an event.
 */
export function useSurfaceVisibility(): boolean {
  const [visible, setVisible] = useState(() =>
    isTauri() ? true : document.visibilityState === 'visible',
  );

  useEffect(() => {
    if (isTauri()) return;
    const onChange = () => setVisible(document.visibilityState === 'visible');
    document.addEventListener('visibilitychange', onChange);
    return () => document.removeEventListener('visibilitychange', onChange);
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    // `listen` resolves to its own unsubscribe, and can resolve after this
    // effect is already torn down (a fast unmount, or React 18's double-invoke
    // in development). The flag makes that case unsubscribe immediately
    // instead of leaving a listener behind writing to a dead component.
    let stale = false;
    let unlisten: (() => void) | undefined;
    listen<boolean>(SURFACE_VISIBILITY, (event) => setVisible(event.payload))
      .then((off) => {
        if (stale) off();
        else unlisten = off;
      })
      .catch((error: unknown) => {
        // Never swallow this. Tauri v2 refuses `listen()` unless a capability
        // grants `core:event:default`, and with none declared the generated
        // set is empty — the subscription then fails here while the Rust side
        // goes on reporting successful emits, which is exactly the shape of
        // bug that wastes an afternoon. See `src-tauri/capabilities/`.
        console.error('aplan HUD: surface-visibility subscription failed', error);
      });
    return () => {
      stale = true;
      unlisten?.();
    };
  }, []);

  return visible;
}
