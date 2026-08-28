import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

// Read directly, same technique PressureBlock.test.tsx uses — jsdom does not
// apply this stylesheet, so a rendered element's computed style can't tell
// us whether the empty state is actually styled as deliberate.
const HUD_CSS = readFileSync(resolve(__dirname, '../hud.css'), 'utf8');

const dashboardMock = vi.fn();
const breakRulesMock = vi.fn();
vi.mock('@/hooks/use-dashboard', () => ({ useDashboard: (...args: unknown[]) => dashboardMock(...args) }));
vi.mock('@/hooks/use-break-rules', () => ({ useBreakRules: (...args: unknown[]) => breakRulesMock(...args) }));

import { Ticker } from './Ticker';

function mockAlerts(alerts: { id: string; alertType: string; severity: string; message: string; resolved: boolean }[]) {
  dashboardMock.mockReturnValue({ data: { alerts } });
}

function mockAdherence(perRule: { taken: number; snoozed: number; skipped: number; ignored: number; absorbed: number; expired: number }[]) {
  breakRulesMock.mockReturnValue({ stats: { perRule } });
}

describe('Ticker', () => {
  beforeEach(() => {
    vi.setSystemTime(new Date('2026-08-28T14:52:07'));
    dashboardMock.mockReset();
    breakRulesMock.mockReset();
    mockAlerts([]);
    mockAdherence([]);
  });

  it('reads a deliberate empty state when there are no unresolved alerts, not a dead strip', () => {
    mockAlerts([{ id: 'a1', alertType: 'DEADLINE', severity: 'INFORMATION', message: 'Old, already resolved', resolved: true }]);

    render(<Ticker />);

    expect(screen.getByTestId('ticker-block')).toBeInTheDocument();
    expect(screen.getByText(/no active alerts/i)).toBeInTheDocument();
    expect(screen.queryAllByTestId('ticker-alert')).toHaveLength(0);

    // The strip is never fully dead: the other two segments still render
    // next to the empty-alerts state.
    expect(screen.getByText(/break adherence/i)).toBeInTheDocument();
    expect(screen.getByText(/aplan v0\.1\.0/)).toBeInTheDocument();

    // Regression guard, same technique as PressureBlock's own empty-state
    // test: presence in the DOM is not legibility.
    const emptyRule = HUD_CSS.match(/\.hud-ticker__empty\s*\{[^}]*\}/)?.[0] ?? '';
    expect(emptyRule).toMatch(/font-style:\s*italic/);
  });

  it('lists unresolved alerts and marks a critical one in the sanctioned red, never a decoration', () => {
    mockAlerts([
      { id: 'a1', alertType: 'OVERLOAD', severity: 'WARNING', message: '93% of capacity today', resolved: false },
      { id: 'a2', alertType: 'DEADLINE', severity: 'CRITICAL', message: "Task 'eActions' is overdue by 1 day(s)", resolved: false },
      { id: 'a3', alertType: 'CONFLICT', severity: 'INFORMATION', message: 'Old conflict, already resolved', resolved: true },
    ]);

    render(<Ticker />);

    const rows = screen.getAllByTestId('ticker-alert');
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent('93% of capacity today');
    expect(rows[0].className).not.toContain('--critical');
    expect(rows[1]).toHaveTextContent("Task 'eActions' is overdue by 1 day(s)");
    expect(rows[1].className).toContain('hud-ticker__alert--critical');

    const criticalRule = HUD_CSS.match(/\.hud-ticker__alert--critical\s*\{[^}]*\}/)?.[0] ?? '';
    expect(criticalRule).toMatch(/var\(--cn-red\)/);
  });

  it('caps the rendered alerts while stating the true overflow', () => {
    const alerts = Array.from({ length: 5 }, (_, i) => ({
      id: `a${i}`,
      alertType: 'DEADLINE',
      severity: 'WARNING',
      message: `Deadline alert ${i}`,
      resolved: false,
    }));
    mockAlerts(alerts);

    render(<Ticker />);

    expect(screen.getAllByTestId('ticker-alert')).toHaveLength(3);
    expect(screen.getByText(/\+2 more/i)).toBeInTheDocument();
  });

  it('computes break adherence from perRule stats, excluding absorbed and expired', () => {
    mockAdherence([
      // seen = 10+2+1+1 = 14, taken = 10; absorbed/expired excluded from both sides.
      { taken: 10, snoozed: 2, skipped: 1, ignored: 1, absorbed: 5, expired: 2 },
      // seen = 8+1+1+1 = 11, taken = 8.
      { taken: 8, snoozed: 1, skipped: 1, ignored: 1, absorbed: 0, expired: 0 },
    ]);

    render(<Ticker />);

    // total taken = 18, total seen = 25 -> 72%.
    expect(screen.getByText('72%')).toBeInTheDocument();
  });

  it('shows a dash instead of a fabricated percentage when the routine has never been seen', () => {
    mockAdherence([]);

    render(<Ticker />);

    expect(screen.getByText('—')).toBeInTheDocument();
  });

  it('always shows the version identity, even in the empty-alerts case', () => {
    render(<Ticker />);

    expect(screen.getByText(/aplan v0\.1\.0/)).toBeInTheDocument();
    expect(screen.getByText(/super\+b/i)).toBeInTheDocument();
  });
});
