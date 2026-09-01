/**
 * Overdue is a qualification the server derives at read time (R72-R74): no date
 * is ever rewritten, so on the client the delay exists only as paint and as sort
 * order. This module holds that vocabulary, kept out of the components so both
 * the card and the dashboard read the same one.
 */

export type OverdueKind = 'NONE' | 'PLANNED' | 'DEADLINE';

export interface OverdueStyle {
  /** Card surface tint — replaces `bg-white`, never stacks with it. */
  readonly background: string;
  /** Ring drawn *around* the card; the thick left border keeps coding urgency. */
  readonly ring: string;
  readonly badge: string;
}

const OVERDUE_STYLES: Record<Exclude<OverdueKind, 'NONE'>, OverdueStyle> = {
  // A broken commitment (deadline) reads red, a planning slip (plannedStart) amber.
  DEADLINE: {
    background: 'bg-red-50',
    ring: 'ring-2 ring-red-400',
    badge: 'bg-red-100 text-red-800 border border-red-300',
  },
  PLANNED: {
    background: 'bg-amber-50',
    ring: 'ring-2 ring-amber-400',
    badge: 'bg-amber-100 text-amber-800 border border-amber-300',
  },
};

/** The paint for a delay, or `null` when the task is on time. */
export function overdueStyle(kind: OverdueKind | null | undefined): OverdueStyle | null {
  if (!kind || kind === 'NONE') return null;
  return OVERDUE_STYLES[kind];
}

/** `⚠ -5j` — the age of the delay in calendar days. */
export function overdueBadgeLabel(days: number | null | undefined): string {
  return days === null || days === undefined ? '⚠' : `⚠ -${days}j`;
}

export function overdueTitle(kind: OverdueKind | null | undefined, days: number | null | undefined): string {
  const age = days === null || days === undefined ? '' : ` de ${days} jour${days > 1 ? 's' : ''}`;
  return kind === 'DEADLINE' ? `Échéance dépassée${age}` : `Planification dépassée${age}`;
}

/** Sort weight for a day column: the gravest delay leads (R74). */
export function overdueRank(kind: OverdueKind | null | undefined): number {
  if (kind === 'DEADLINE') return 2;
  if (kind === 'PLANNED') return 1;
  return 0;
}
