/**
 * Structural shape of a meeting for real-vs-placeholder purposes.
 * Kept minimal so this helper does not depend on component/GraphQL types.
 */
export interface MeetingLike {
  readonly title: string;
  readonly showAs: string | null;
}

/**
 * True when `m` is an actual commitment on the calendar, as opposed to an
 * all-day free/OOO/working-elsewhere placeholder or the lunch entry — none
 * of which should count as "a meeting" for capacity, countdown or timeline
 * purposes.
 *
 * Previously duplicated verbatim in `FocusBlock` and `AgendaBlock` (each
 * reads `useDashboard()` on its own); extracted here after a review flagged
 * the second copy as where that drift starts. `DashboardPage` still carries
 * its own non-exported version of this same rule — out of scope here, but
 * a candidate for the same treatment if it's ever touched again.
 */
export function isRealMeeting(m: MeetingLike): boolean {
  if (m.title.toLowerCase() === 'pause midi') return false;
  if (m.showAs === 'free' || m.showAs === 'oof' || m.showAs === 'workingElsewhere') return false;
  return true;
}
