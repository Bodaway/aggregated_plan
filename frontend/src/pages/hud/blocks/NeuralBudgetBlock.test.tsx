import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

// Read directly, same technique the other block tests use — jsdom does not apply
// this stylesheet, so a rendered element's computed style can't tell us whether a
// rule is actually styled as deliberate rather than merely present in the DOM.
const HUD_CSS = readFileSync(resolve(__dirname, '../hud.css'), 'utf8');

const budgetMock = vi.fn();
vi.mock('../useNeuralBudget', () => ({ useNeuralBudget: () => budgetMock() }));

import { NeuralBudgetBlock } from './NeuralBudgetBlock';
import type { NeuralBudget } from '../useNeuralBudget';

function makeBudget(overrides: Partial<NeuralBudget> = {}): NeuralBudget {
  return {
    windowHours: 5,
    consumedTokens: 1_700_000,
    cacheReadTokens: 61_000_000,
    declaredCeiling: 2_500_000,
    consumedRatio: 0.68,
    perDay: [180_000, 340_000, 210_000, 460_000, 300_000],
    perModel: [
      { model: 'claude-opus-5', tokens: 1_288_000 },
      { model: 'claude-fable-5-1', tokens: 412_000 },
    ],
    topProject: { name: '/home/mbt/appfactory/aggregated_plan', tokens: 1_037_000, ratio: 0.61 },
    ...overrides,
  };
}

describe('NeuralBudgetBlock', () => {
  beforeEach(() => {
    budgetMock.mockReset();
  });

  it('renders the gauge, the sparkline and the per-model breakdown', () => {
    budgetMock.mockReturnValue(makeBudget());

    render(<NeuralBudgetBlock />);

    expect(screen.getByTestId('neural-block')).toBeInTheDocument();
    expect(screen.getByText(/5h window/i)).toBeInTheDocument();
    expect(screen.getByText('68%')).toBeInTheDocument();
    expect(screen.getByTestId('neural-gauge').querySelector('i')).toHaveStyle({ width: '68%' });
    expect(screen.getAllByTestId('neural-spark-bar')).toHaveLength(5);
    expect(screen.getByText('claude-opus-5')).toBeInTheDocument();
    expect(screen.getByText('1.29M')).toBeInTheDocument();
  });

  it('no longer marks itself as placeholder data', () => {
    // The STUB badge existed because this block rendered fabricated telemetry that
    // read as real on screen. It reads the index now, and a warning left standing
    // next to real data is worse than none.
    budgetMock.mockReturnValue(makeBudget());

    render(<NeuralBudgetBlock />);

    expect(screen.queryByTestId('stub-marker')).not.toBeInTheDocument();
  });

  it('keeps saying the ceiling is not measured', () => {
    // No public API exposes the subscription quota — the gauge lies about its
    // denominator unless that stays visible on screen, not just in a comment.
    budgetMock.mockReturnValue(makeBudget());

    render(<NeuralBudgetBlock />);

    expect(screen.getByTestId('neural-ceiling-note')).toHaveTextContent(/not measured/i);
  });

  it('draws no gauge at all until a ceiling has been set', () => {
    // A bar against a denominator of zero would read as "plenty left" whatever the
    // burn — the one thing this panel must never say by accident. The absolute
    // figure is shown instead, with the command that fixes it.
    budgetMock.mockReturnValue(makeBudget({ declaredCeiling: 0, consumedRatio: 0 }));

    render(<NeuralBudgetBlock />);

    expect(screen.queryByTestId('neural-gauge')).not.toBeInTheDocument();
    expect(screen.getByTestId('neural-uncalibrated')).toHaveTextContent(/no ceiling set/i);
    expect(screen.getByText('1.70M tokens')).toBeInTheDocument();
  });

  it('shows cache reads beside the total, never folded into it', () => {
    // On a real corpus cache reads run 36x everything else. Inside the total they
    // would turn a burn gauge into a cache-hit meter; absent entirely they would
    // hide the cheapest half of what the machine actually does.
    budgetMock.mockReturnValue(makeBudget());

    render(<NeuralBudgetBlock />);

    expect(screen.getByText('Cache reads')).toBeInTheDocument();
    expect(screen.getByText('61.00M')).toBeInTheDocument();
    expect(screen.getByText('68%'), 'the gauge still reads the consumed ratio').toBeInTheDocument();
  });

  it('shows a project by its last path segment', () => {
    // The resolver sends the whole path so two same-named directories stay
    // distinguishable; the panel is three columns wide.
    budgetMock.mockReturnValue(makeBudget());

    render(<NeuralBudgetBlock />);

    expect(screen.getByText('aggregated_plan')).toBeInTheDocument();
    expect(screen.getByText('61% of total')).toBeInTheDocument();
  });

  it('caps the model rows the panel draws', () => {
    budgetMock.mockReturnValue(
      makeBudget({
        perModel: [
          { model: 'claude-opus-5', tokens: 4 },
          { model: 'claude-sonnet-5', tokens: 3 },
          { model: 'claude-fable-5-1', tokens: 2 },
          { model: 'claude-haiku-4-5', tokens: 1 },
        ],
      }),
    );

    render(<NeuralBudgetBlock />);

    expect(screen.getByText('claude-opus-5')).toBeInTheDocument();
    expect(screen.queryByText('claude-haiku-4-5')).not.toBeInTheDocument();
  });

  it('reads a deliberate empty state for a quiet window', () => {
    budgetMock.mockReturnValue(
      makeBudget({ consumedTokens: 0, consumedRatio: 0, perModel: [], topProject: null }),
    );

    render(<NeuralBudgetBlock />);

    expect(screen.getByText(/no usage recorded in this window/i)).toBeInTheDocument();
    expect(screen.queryByTestId('neural-sparkline')).not.toBeInTheDocument();

    const emptyRule = HUD_CSS.match(/\.hud-neural__empty\s*\{[^}]*\}/)?.[0] ?? '';
    expect(emptyRule).toMatch(/font-style:\s*italic/);
  });

  it('says the index has not been built rather than drawing zeros', () => {
    // The index comes from a background job that may never have run — a fresh
    // machine, or one that has never used Claude Code. A row of zeros would read
    // as a quiet day, which is a different thing entirely.
    budgetMock.mockReturnValue(null);

    render(<NeuralBudgetBlock />);

    expect(screen.getByText(/no usage index yet/i)).toBeInTheDocument();
    expect(screen.queryByTestId('neural-gauge')).not.toBeInTheDocument();
  });
});
