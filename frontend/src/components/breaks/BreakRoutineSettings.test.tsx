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
let loading = false;

vi.mock('@/hooks/use-break-rules', () => ({
  useBreakRules: () => ({
    rules,
    stats,
    loading,
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
  loading = false;
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
    fireEvent.blur(screen.getByLabelText(/intervalle/i));

    expect(screen.getByText(/doit être positif/i)).toBeInTheDocument();
    expect(updateRule).not.toHaveBeenCalled();
  });

  it('refuses a duration past the server\'s ceiling before the round trip', () => {
    render(<BreakRoutineSettings />);
    const duration = screen.getAllByLabelText(/durée/i)[0];

    fireEvent.change(duration, { target: { value: '100000000' } });
    fireEvent.blur(duration);

    expect(screen.getByText(/ne peut pas dépasser 3600 secondes/i)).toBeInTheDocument();
    expect(updateRule).not.toHaveBeenCalled();
  });

  it('saves a typed field once, on blur, not on every keystroke', () => {
    render(<BreakRoutineSettings />);
    const label = screen.getByDisplayValue('Pause visuelle');

    for (const value of ['P', 'Pa', 'Pau', 'Paus', 'Pause']) {
      fireEvent.change(label, { target: { value } });
    }
    expect(updateRule).not.toHaveBeenCalled();

    fireEvent.blur(label);

    expect(updateRule).toHaveBeenCalledTimes(1);
    expect(updateRule).toHaveBeenCalledWith('r1', expect.objectContaining({ label: 'Pause' }));
  });

  it('does not save on blur when nothing was edited', () => {
    render(<BreakRoutineSettings />);

    fireEvent.blur(screen.getByDisplayValue('Pause visuelle'));

    expect(updateRule).not.toHaveBeenCalled();
  });

  /** A mutation refetches both queries `network-only`, so `loading` flips back to true
   * with rules already on screen. Unmounting the list there would take the user's focus
   * with it after the first character. */
  it('keeps a populated list on screen while a refetch is in flight', () => {
    const { rerender } = render(<BreakRoutineSettings />);
    loading = true;
    rerender(<BreakRoutineSettings />);

    expect(screen.queryByText(/Chargement de la routine/i)).not.toBeInTheDocument();
    expect(screen.getByDisplayValue('Pause visuelle')).toBeInTheDocument();
  });

  it('still shows the loading message on the very first load', () => {
    loading = true;
    rules = [];
    render(<BreakRoutineSettings />);

    expect(screen.getByText(/Chargement de la routine/i)).toBeInTheDocument();
  });

  /** Rows survive a refetch now, so their local state has to follow the server's copy
   * rather than keep showing what was typed before the server normalised it. */
  it('resyncs a row when the server sends back different values', () => {
    const { rerender } = render(<BreakRoutineSettings />);

    fireEvent.change(screen.getByDisplayValue('Pause visuelle'), {
      target: { value: 'À moitié tapé' },
    });
    rules = [{ ...intervalRule, label: 'Pause visuelle normalisée' }, dailyRule];
    rerender(<BreakRoutineSettings />);

    expect(screen.getByDisplayValue('Pause visuelle normalisée')).toBeInTheDocument();
    expect(screen.queryByDisplayValue('À moitié tapé')).not.toBeInTheDocument();
  });
});
