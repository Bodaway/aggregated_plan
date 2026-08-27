import { isTauri } from '@tauri-apps/api/core';

/**
 * Target of the `/` redirect.
 *
 * The Tauri window's static bundle has no SPA fallback for `/hud`, so it
 * boots at `index.html` (the browser entry) and relies on this redirect to
 * reach the HUD. The browser app at :3000 must keep landing on the dashboard.
 */
export function landingRoute(): string {
  return isTauri() ? '/hud' : '/dashboard';
}
