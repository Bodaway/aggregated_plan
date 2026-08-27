import { isTauri } from '@tauri-apps/api/core';

/**
 * Target of the `/` redirect (and of the catch-all route for any unmatched
 * or mistyped path).
 *
 * The Tauri window must land on the HUD, while the browser app at :3000
 * must keep landing on the dashboard. The catch-all is a defensive net so
 * an unmatched path redirects somewhere sensible instead of rendering blank.
 */
export function landingRoute(): string {
  return isTauri() ? '/hud' : '/dashboard';
}
