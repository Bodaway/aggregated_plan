import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';

// Read directly, same technique PressureBlock.test.tsx uses for its
// empty-state regression guard — jsdom does not apply this stylesheet, so a
// rendered element's computed style can't tell us whether a rule is actually
// styled as deliberate rather than merely present in the DOM.
const HUD_CSS = readFileSync(resolve(__dirname, '../hud.css'), 'utf8');

import { NeuralBudgetBlock } from './NeuralBudgetBlock';
import type { NeuralBudget } from './stub-data';

function makeBudget(overrides: Partial<NeuralBudget> = {}): NeuralBudget {
  return {
    windowHours: 5,
    consumedRatio: 0.68,
    declaredCeiling: 2_500_000,
    perDay: [180_000, 340_000, 210_000, 460_000, 300_000],
    perModel: [
      { model: 'opus-5', tokens: 1_860_000 },
      { model: 'fable-5', tokens: 412_000 },
    ],
    topProject: { name: 'aggregated_plan', ratio: 0.61 },
    ...overrides,
  };
}

describe('NeuralBudgetBlock', () => {
  it('renders consumption, the sparkline and the per-model/per-project breakdown from the contract', () => {
    render(<NeuralBudgetBlock budget={makeBudget()} />);

    expect(screen.getByTestId('neural-block')).toBeInTheDocument();
    expect(screen.getByText(/neural budget/i)).toBeInTheDocument();
    expect(screen.getByText(/5h window/i)).toBeInTheDocument();
    expect(screen.getByText('68%')).toBeInTheDocument();

    const gauge = screen.getByTestId('neural-gauge');
    expect(gauge.querySelector('i')).toHaveStyle({ width: '68%' });

    expect(screen.getAllByTestId('neural-spark-bar')).toHaveLength(5);
    expect(screen.getByText('opus-5')).toBeInTheDocument();
    expect(screen.getByText('1.86M')).toBeInTheDocument();
    expect(screen.getByText('fable-5')).toBeInTheDocument();
    expect(screen.getByText('412k')).toBeInTheDocument();
    expect(screen.getByText('aggregated_plan')).toBeInTheDocument();
    expect(screen.getByText('61% of total')).toBeInTheDocument();
  });

  it('surfaces the ceiling as declared, not measured, and paints the gauge in the sanctioned pink', () => {
    // Design doc §9: the subscription ceiling isn't exposed by any public
    // API, so this app measures a burn locally against a number the user
    // typed in by hand. The gauge must not silently imply a measured cap.
    render(<NeuralBudgetBlock budget={makeBudget({ declaredCeiling: 2_500_000 })} />);

    expect(screen.getByText(/declared ceiling/i)).toBeInTheDocument();
    expect(screen.getByText('2.50M tokens')).toBeInTheDocument();
    expect(screen.getByTestId('neural-ceiling-note')).toHaveTextContent(/not measured/i);

    const gaugeRule = HUD_CSS.match(/\.hud-gauge--neural[^{]*\{[^}]*\}/)?.[0] ?? '';
    expect(gaugeRule).toMatch(/var\(--cn-red\)/);
  });

  it('omits the top-project row when the contract reports none', () => {
    render(<NeuralBudgetBlock budget={makeBudget({ topProject: null })} />);

    expect(screen.queryByText(/of total/i)).not.toBeInTheDocument();
  });

  it('reads a deliberate empty state when there is no usage in the window', () => {
    render(<NeuralBudgetBlock budget={makeBudget({ perDay: [], perModel: [] })} />);

    expect(screen.getByText(/no usage recorded/i)).toBeInTheDocument();
    expect(screen.queryAllByTestId('neural-spark-bar')).toHaveLength(0);

    // Regression guard, same technique as PressureBlock's empty-state test:
    // presence in the DOM is not legibility — assert the CSS actually marks
    // this as a deliberate empty state, not a stray string.
    const emptyRule = HUD_CSS.match(/\.hud-neural__empty\s*\{[^}]*\}/)?.[0] ?? '';
    expect(emptyRule).toMatch(/font-style:\s*italic/);
  });

  it('renders the plan-2 stub by default when no budget prop is given', () => {
    render(<NeuralBudgetBlock />);

    expect(screen.getByTestId('neural-block')).toBeInTheDocument();
  });
});
