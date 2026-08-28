/**
 * Single source for the HUD's own version number, read by both the boot
 * sequence banner (`HudPage.tsx`) and the Ticker's version identity
 * (`blocks/Ticker.tsx`). The two callers keep their own distinct wording
 * ("aplan cockpit vX" vs. "aplan vX") — only the version digits themselves
 * were a manual-sync burden, per this plan's own precedent of extracting a
 * shared helper at the second copy (see `lib/is-real-meeting.ts`).
 */
export const HUD_VERSION = '0.1.0';
