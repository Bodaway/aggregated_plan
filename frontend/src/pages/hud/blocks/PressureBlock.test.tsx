import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Read directly, same technique FocusBlock.test.tsx uses for its idle-chrono
// regression guard — jsdom does not apply this stylesheet, so a rendered
// element's computed style can't tell us whether an empty state is actually
// styled as deliberate rather than merely present in the DOM.
const HUD_CSS = readFileSync(resolve(__dirname, '../hud.css'), 'utf8');

const dashboardMock = vi.fn();
vi.mock('@/hooks/use-dashboard', () => ({ useDashboard: (...args: unknown[]) => dashboardMock(...args) }));

import { PressureBlock } from './PressureBlock';

function mockDashboard({
  tasks = [] as { id: string; title: string; deadline: string | null }[],
  weeklyWorkload = { capacity: 10, totalPlanned: 30, totalMeetings: 6, overload: false },
} = {}) {
  dashboardMock.mockReturnValue({ data: { tasks, weeklyWorkload } });
}

describe('PressureBlock', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-08-28T09:30:00'));
    dashboardMock.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('lists deadlines sorted by proximity, today in the sanctioned pink', () => {
    // Deliberately out of chronological order in the source data — the
    // component, not the fixture, must do the sorting.
    mockDashboard({
      tasks: [
        { id: 't1', title: 'Cadrage Standards', deadline: '2026-08-30' },
        { id: 't2', title: 'Revue eProject A3', deadline: '2026-08-28' },
      ],
    });

    render(<PressureBlock />);

    expect(screen.getByTestId('pressure-block')).toBeInTheDocument();
    expect(screen.getByText(/pressure/i)).toBeInTheDocument();
    expect(screen.getByText(/2 deadlines/i)).toBeInTheDocument();

    const rows = screen.getAllByTestId('pressure-deadline');
    expect(rows).toHaveLength(2);
    // Today's deadline (08-28) sorts before the later one (08-30).
    expect(rows[0]).toHaveTextContent('Revue eProject A3');
    expect(rows[1]).toHaveTextContent('Cadrage Standards');

    const whens = screen.getAllByTestId('deadline-when');
    expect(whens[0]).toHaveTextContent('Today');
    expect(whens[0].className).toContain('hud-pressure__when--hot');
    expect(whens[1]).toHaveTextContent('In 2d');
    expect(whens[1].className).not.toContain('hud-pressure__when--hot');

    // Capacity: (30 + 6) planned hours over 10 half-days * 4h = 40h → 90%.
    expect(screen.getByText('90%')).toBeInTheDocument();
    const gauge = screen.getByTestId('pressure-gauge');
    expect(gauge.className).not.toContain('hud-gauge--over');
    expect(gauge.querySelector('i')).toHaveStyle({ width: '90%' });
  });

  it('caps the rendered rows at five while the label keeps the true total', () => {
    // Controller ruling: a panel overflowing its grid cell is worse than one
    // that summarises, but the label must still tell the truth. Seven
    // deadlines exercises both halves of that ruling at once.
    const tasks = Array.from({ length: 7 }, (_, i) => ({
      id: `t${i}`,
      title: `Deadline ${i}`,
      deadline: `2026-09-${String(10 + i).padStart(2, '0')}`,
    }));
    mockDashboard({ tasks });

    render(<PressureBlock />);

    expect(screen.getByText(/7 deadlines/i)).toBeInTheDocument();
    expect(screen.getAllByTestId('pressure-deadline')).toHaveLength(5);
  });

  it('reads a deliberate empty state when there are no upcoming deadlines', () => {
    mockDashboard({ tasks: [] });

    render(<PressureBlock />);

    expect(screen.getByText(/no upcoming deadlines/i)).toBeInTheDocument();
    expect(screen.queryAllByTestId('pressure-deadline')).toHaveLength(0);

    // Regression guard, same technique as FocusBlock's idle-chrono test:
    // presence in the DOM is not legibility — assert the CSS actually marks
    // this as a deliberate empty state (italic, muted), not a stray string.
    const emptyRule = HUD_CSS.match(/\.hud-pressure__empty\s*\{[^}]*\}/)?.[0] ?? '';
    expect(emptyRule).toMatch(/font-style:\s*italic/);
  });

  it('marks an overdue deadline as overdue, not as pink', () => {
    // Pink is reserved for a deadline falling TODAY — an overdue one (in the
    // past, not today) must not borrow it.
    mockDashboard({
      tasks: [{ id: 't3', title: 'eActions — mort au démarrage si CSV corrompu', deadline: '2026-08-25' }],
    });

    render(<PressureBlock />);

    const when = screen.getByTestId('deadline-when');
    expect(when).toHaveTextContent('Overdue');
    expect(when.className).not.toContain('hud-pressure__when--hot');
  });

  it('turns the capacity gauge orange once the domain marks the week overloaded', () => {
    // Per the task brief: "plus la capacité en jauge, en orange au-delà du
    // seuil" — the threshold itself is the domain's own `overload` verdict
    // (R16), not re-derived here.
    mockDashboard({
      weeklyWorkload: { capacity: 10, totalPlanned: 40, totalMeetings: 5, overload: true },
    });

    render(<PressureBlock />);

    // 45 planned hours over 40 capacity hours → 113%, gauge bar clamped at 100%.
    expect(screen.getByText('113%')).toBeInTheDocument();
    const gauge = screen.getByTestId('pressure-gauge');
    expect(gauge.className).toContain('hud-gauge--over');
    expect(gauge.querySelector('i')).toHaveStyle({ width: '100%' });

    const overRule = HUD_CSS.match(/\.hud-gauge--over[^{]*\{[^}]*\}/)?.[0] ?? '';
    expect(overRule).toMatch(/var\(--cn-orange\)/);
  });
});
