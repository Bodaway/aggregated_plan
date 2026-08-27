import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import type { BreakRule, BreakStats } from '@/hooks/use-break-rules';

const updateRule = vi.fn();
const createRule = vi.fn();
const deleteRule = vi.fn();

/** INTERVAL shape: carries `intervalMinutes`, `atTime` is null. */
const intervalRule: BreakRule = {
  id: 'r1',
  kind: 'VISUAL',
  label: 'Pause visuelle',
  body: 'Regarde au loin',
  cadence: 'INTERVAL',
  intervalMinutes: 20,
  atTime: null,
  durationSeconds: 30,
  priority: 1,
  enabled: true,
  urgency: 'LOW',
};

/** DAILY shape: carries `atTime`, `intervalMinutes` is null. */
const dailyRule: BreakRule = {
  id: 'r4',
  kind: 'STRENGTH',
  label: 'Renfo épaule',
  body: 'Élastique',
  cadence: 'DAILY',
  intervalMinutes: null,
  atTime: '14:00',
  durationSeconds: 120,
  priority: 4,
  enabled: false,
  urgency: 'NORMAL',
};

const statsWithAdherence: BreakStats = {
  perRule: [
    {
      ruleId: 'r1',
      label: 'Pause visuelle',
      taken: 3,
      snoozed: 0,
      skipped: 1,
      ignored: 0,
      absorbed: 9,
      expired: 2,
      adherence: 0.75,
    },
  ],
};

let rules: readonly BreakRule[] = [intervalRule, dailyRule];
let stats: BreakStats = statsWithAdherence;

vi.mock('@/hooks/use-break-rules', () => ({
  useBreakRules: () => ({
    rules,
    stats,
    loading: false,
    error: null,
    createRule,
    updateRule,
    deleteRule,
  }),
}));

import { BreakRoutineSettings } from './BreakRoutineSettings';

beforeEach(() => {
  rules = [intervalRule, dailyRule];
  stats = statsWithAdherence;
  [updateRule, createRule, deleteRule].forEach(fn => fn.mockReset());
});

describe('BreakRoutineSettings', () => {
  it('renders each rule with its cadence in its own shape', () => {
    render(<BreakRoutineSettings />);

    expect(screen.getByDisplayValue('Pause visuelle')).toBeInTheDocument();
    // The INTERVAL rule shows a minute input; the DAILY rule shows a time input.
    // Each label matches exactly one row, so a single match proves the other
    // row does not also render that control.
    expect(screen.getByLabelText(/intervalle/i)).toHaveValue(20);
    expect(screen.getByLabelText(/heure/i)).toHaveValue('14:00');
  });

  it("toggling a rule's enabled state calls updateRule with the flipped flag", () => {
    render(<BreakRoutineSettings />);

    fireEvent.click(screen.getAllByRole('checkbox', { name: /activ/i })[0]);

    expect(updateRule).toHaveBeenCalledWith('r1', expect.objectContaining({ enabled: false }));
  });

  it('shows adherence as a percentage of what the user actually saw', () => {
    render(<BreakRoutineSettings />);

    expect(screen.getByText('75 %')).toBeInTheDocument();
  });

  it('renders a null adherence as an em dash, not as 0 %', () => {
    stats = {
      perRule: [{ ...statsWithAdherence.perRule[0], adherence: null }],
    };
    render(<BreakRoutineSettings />);

    expect(screen.getByText('—')).toBeInTheDocument();
    expect(screen.queryByText('0 %')).not.toBeInTheDocument();
  });

  it('refuses to save a non-positive interval and does not call updateRule', () => {
    render(<BreakRoutineSettings />);

    fireEvent.change(screen.getByLabelText(/intervalle/i), { target: { value: '0' } });

    expect(screen.getByText(/doit être positif/i)).toBeInTheDocument();
    expect(updateRule).not.toHaveBeenCalled();
  });
});
