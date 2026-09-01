import { describe, it, expect } from 'vitest';
import {
  overdueBadgeLabel,
  overdueRank,
  overdueStyle,
  overdueTitle,
  type OverdueKind,
} from './overdue';

describe('overdueBadgeLabel — the age pill (R74)', () => {
  it('renders the day count as `⚠ -Nj`', () => {
    expect(overdueBadgeLabel(5)).toBe('⚠ -5j');
  });

  it('keeps the same shape for a single day', () => {
    expect(overdueBadgeLabel(1)).toBe('⚠ -1j');
  });

  it('still prints the count when it is zero', () => {
    // A zero-day delay is a server-side possibility (same-day deadline crossing);
    // the pill must not silently degrade to the bare warning sign.
    expect(overdueBadgeLabel(0)).toBe('⚠ -0j');
  });

  it('renders a bare warning sign when the day count is missing', () => {
    expect(overdueBadgeLabel(null)).toBe('⚠');
    expect(overdueBadgeLabel(undefined)).toBe('⚠');
  });

  it('does not clamp large delays', () => {
    expect(overdueBadgeLabel(365)).toBe('⚠ -365j');
  });
});

describe('overdueRank — day-column sort weight (R74)', () => {
  it('ranks DEADLINE above PLANNED above on-time', () => {
    expect(overdueRank('DEADLINE')).toBeGreaterThan(overdueRank('PLANNED'));
    expect(overdueRank('PLANNED')).toBeGreaterThan(overdueRank('NONE'));
  });

  it('gives an on-time task the neutral weight', () => {
    expect(overdueRank('NONE')).toBe(0);
    expect(overdueRank(null)).toBe(0);
    expect(overdueRank(undefined)).toBe(0);
  });

  it('orders a mixed column gravest-first when sorted descending', () => {
    const kinds: OverdueKind[] = ['NONE', 'PLANNED', 'DEADLINE', 'NONE', 'PLANNED'];

    const sorted = [...kinds].sort((a, b) => overdueRank(b) - overdueRank(a));

    expect(sorted).toEqual(['DEADLINE', 'PLANNED', 'PLANNED', 'NONE', 'NONE']);
  });
});

describe('overdueStyle — the paint', () => {
  it('paints a broken commitment red', () => {
    const style = overdueStyle('DEADLINE');

    expect(style?.background).toBe('bg-red-50');
    expect(style?.ring).toMatch(/ring-red-400/);
    expect(style?.badge).toMatch(/red/);
  });

  it('paints a planning slip amber', () => {
    const style = overdueStyle('PLANNED');

    expect(style?.background).toBe('bg-amber-50');
    expect(style?.ring).toMatch(/ring-amber-400/);
    expect(style?.badge).toMatch(/amber/);
  });

  it('paints nothing for a task that is on time', () => {
    expect(overdueStyle('NONE')).toBeNull();
    expect(overdueStyle(null)).toBeNull();
    expect(overdueStyle(undefined)).toBeNull();
  });

  it('never touches the left border, which belongs to urgency (R74)', () => {
    // The delay adds a ring and a tint; the thick left border keeps coding
    // urgency. A `border-l-*` utility leaking in here would overwrite it.
    for (const kind of ['DEADLINE', 'PLANNED'] as const) {
      const style = overdueStyle(kind);
      const all = `${style?.background} ${style?.ring} ${style?.badge}`;
      expect(all).not.toMatch(/border-l/);
    }
  });
});

describe('overdueTitle — the French tooltip', () => {
  it('names a broken commitment', () => {
    expect(overdueTitle('DEADLINE', 5)).toBe('Échéance dépassée de 5 jours');
  });

  it('names a planning slip', () => {
    expect(overdueTitle('PLANNED', 3)).toBe('Planification dépassée de 3 jours');
  });

  it('keeps the singular for one day', () => {
    expect(overdueTitle('DEADLINE', 1)).toBe('Échéance dépassée de 1 jour');
  });

  it('drops the age when the day count is missing', () => {
    expect(overdueTitle('DEADLINE', null)).toBe('Échéance dépassée');
    expect(overdueTitle('PLANNED', undefined)).toBe('Planification dépassée');
  });
});
