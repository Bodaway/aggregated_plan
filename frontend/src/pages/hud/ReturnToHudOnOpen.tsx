import { useEffect, useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import { isTauri } from '@tauri-apps/api/core';
import { useSurfaceVisibility } from './useSurfaceVisibility';

/**
 * Sends the overlay back to the HUD every time it is opened.
 *
 * The overlay is invoked, not browsed: each SUPER+B is a fresh look at the
 * day, so it starts where the day starts. Without this, closing while on
 * Timesheet and reopening dropped you straight back onto Timesheet — the boot
 * sequence never played either, since it lives in `HudPage` and `HudPage` was
 * not mounted.
 *
 * Renders nothing. It sits inside the router but outside `Routes`, because it
 * has to be mounted whatever the current route is — that is the whole point.
 *
 * Tauri only. In the browser at :3000 this hook's visibility signal is the
 * document's own, which flips on every workspace switch and tab change; acting
 * on it there would yank the page to the HUD while someone was reading
 * something else.
 */
export function useReturnToHudOnOpen(): void {
  const visible = useSurfaceVisibility();
  const navigate = useNavigate();
  // Seeded with the current value so mounting is not itself read as an
  // opening: the first opening is already `HudPage`'s own business, and the
  // window is created on the HUD route anyway.
  const wasVisible = useRef(visible);

  useEffect(() => {
    if (!isTauri()) return;
    const opening = visible && !wasVisible.current;
    wasVisible.current = visible;
    if (opening) navigate('/hud');
  }, [visible, navigate]);
}

export function ReturnToHudOnOpen() {
  useReturnToHudOnOpen();
  return null;
}
