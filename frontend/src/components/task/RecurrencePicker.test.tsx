import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { RecurrencePicker } from './RecurrencePicker';
import type { RecurrenceConfig } from '@/lib/recurrence';
import { weekdayBitmask, bitmaskToWeekdays } from '@/lib/recurrence';

// ── Helpers ──────────────────────────────────────────────────────────────────

function renderPicker(value: RecurrenceConfig, onChange = vi.fn()) {
  render(<RecurrencePicker value={value} onChange={onChange} />);
  return { onChange };
}

function getFrequencySelect() {
  return screen.getByRole('combobox', { name: /récurrence/i });
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe('RecurrencePicker — null value (none)', () => {
  it('renders only the frequency select when value is null', () => {
    renderPicker(null);
    // frequency select present
    expect(getFrequencySelect()).toBeTruthy();
    // no day-toggle fieldset
    expect(screen.queryByRole('group', { name: /jours de la semaine/i })).toBeNull();
    // no end-condition select
    expect(screen.queryByRole('combobox', { name: /se termine/i })).toBeNull();
    // no day-of-month input
    expect(screen.queryByRole('spinbutton', { name: /jour du mois/i })).toBeNull();
  });
});

describe('RecurrencePicker — switching to weekly', () => {
  it('reveals the day-toggle fieldset and end-condition select', () => {
    const weeklyValue: RecurrenceConfig = {
      rule: { kind: 'weekly', interval: 1, weekdays: [] },
      end: { kind: 'never' },
    };
    renderPicker(weeklyValue);

    // Day-toggle fieldset present (role=group, tag=FIELDSET)
    const groups = screen.getAllByRole('group');
    expect(groups.some(el => el.tagName === 'FIELDSET')).toBe(true);

    // End-condition select present
    expect(screen.getByRole('combobox', { name: /se termine/i })).toBeTruthy();
  });
});

describe('RecurrencePicker — day toggles', () => {
  const weeklyValue: RecurrenceConfig = {
    rule: { kind: 'weekly', interval: 1, weekdays: [] },
    end: { kind: 'never' },
  };

  it('toggling Monday calls onChange with weekdays containing monday', () => {
    const onChange = vi.fn();
    renderPicker(weeklyValue, onChange);

    const monButton = screen.getByRole('button', { name: 'Lun' });
    fireEvent.click(monButton);

    expect(onChange).toHaveBeenCalledOnce();
    const [arg] = onChange.mock.calls[0] as [RecurrenceConfig];
    expect(arg!.rule.kind).toBe('weekly');
    if (arg!.rule.kind === 'weekly') {
      expect(arg!.rule.weekdays).toContain('monday');
    }
  });

  it('toggling an active day removes it from weekdays', () => {
    const withMon: RecurrenceConfig = {
      rule: { kind: 'weekly', interval: 1, weekdays: ['monday', 'friday'] },
      end: { kind: 'never' },
    };
    const onChange = vi.fn();
    renderPicker(withMon, onChange);

    const monButton = screen.getByRole('button', { name: 'Lun' });
    expect(monButton).toHaveAttribute('aria-pressed', 'true');
    fireEvent.click(monButton);

    const [arg] = onChange.mock.calls[0] as [RecurrenceConfig];
    if (arg!.rule.kind === 'weekly') {
      expect(arg!.rule.weekdays).not.toContain('monday');
      expect(arg!.rule.weekdays).toContain('friday');
    }
  });

  it('aria-pressed reflects active state correctly', () => {
    const withFri: RecurrenceConfig = {
      rule: { kind: 'weekly', interval: 1, weekdays: ['friday'] },
      end: { kind: 'never' },
    };
    renderPicker(withFri);
    expect(screen.getByRole('button', { name: 'Ven' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: 'Lun' })).toHaveAttribute('aria-pressed', 'false');
  });
});

describe('RecurrencePicker — biweekly', () => {
  it('switching to biweekly sets interval=2 and empty weekdays', () => {
    const onChange = vi.fn();
    renderPicker(null, onChange);

    fireEvent.change(getFrequencySelect(), { target: { value: 'biweekly' } });

    const [arg] = onChange.mock.calls[0] as [RecurrenceConfig];
    expect(arg!.rule.kind).toBe('weekly');
    if (arg!.rule.kind === 'weekly') {
      expect(arg!.rule.interval).toBe(2);
      expect(arg!.rule.weekdays).toEqual([]);
    }
  });
});

describe('RecurrencePicker — weekdays preset', () => {
  it('selecting weekdays sets Mon–Fri and disables the toggles', () => {
    const onChange = vi.fn();
    renderPicker(null, onChange);

    fireEvent.change(getFrequencySelect(), { target: { value: 'weekdays' } });

    const [arg] = onChange.mock.calls[0] as [RecurrenceConfig];
    expect(arg!.rule.kind).toBe('weekly');
    if (arg!.rule.kind === 'weekly') {
      expect(arg!.rule.interval).toBe(1);
      expect(arg!.rule.weekdays.sort()).toEqual(
        ['friday', 'monday', 'thursday', 'tuesday', 'wednesday'],
      );
    }

    // Re-render with value; toggles should be disabled
    renderPicker(arg!);
    const buttons = screen.getAllByRole('button').filter(
      b => ['Lun', 'Mar', 'Mer', 'Jeu', 'Ven', 'Sam', 'Dim'].includes(b.textContent ?? ''),
    );
    buttons.forEach(b => expect(b).toBeDisabled());
  });
});

describe('RecurrencePicker — monthly by day', () => {
  it('switching to monthly_date shows day-of-month input', () => {
    const onChange = vi.fn();
    renderPicker(null, onChange);

    fireEvent.change(getFrequencySelect(), { target: { value: 'monthly_date' } });

    const [arg] = onChange.mock.calls[0] as [RecurrenceConfig];
    expect(arg!.rule.kind).toBe('monthly_by_day');

    // Re-render with value
    const { onChange: onChange2 } = renderPicker(arg!, vi.fn());

    const input = screen.getByRole('spinbutton', { name: /jour du mois/i });
    expect(input).toBeTruthy();
    onChange2;
  });

  it('typing 15 calls onChange with day=15', () => {
    const monthlyValue: RecurrenceConfig = {
      rule: { kind: 'monthly_by_day', interval: 1, day: 1 },
      end: { kind: 'never' },
    };
    const onChange = vi.fn();
    renderPicker(monthlyValue, onChange);

    const input = screen.getByRole('spinbutton', { name: /jour du mois/i });
    fireEvent.change(input, { target: { value: '15' } });

    const [arg] = onChange.mock.calls[0] as [RecurrenceConfig];
    expect(arg!.rule).toMatchObject({ kind: 'monthly_by_day', interval: 1, day: 15 });
  });
});

describe('RecurrencePicker — end condition', () => {
  const dailyValue: RecurrenceConfig = {
    rule: { kind: 'daily', interval: 1 },
    end: { kind: 'never' },
  };

  it('end section is visible when frequency is active', () => {
    renderPicker(dailyValue);
    expect(screen.getByRole('combobox', { name: /se termine/i })).toBeTruthy();
  });

  it('selecting "after N" reveals number input', () => {
    const onChange = vi.fn();
    renderPicker(dailyValue, onChange);

    const endSelect = screen.getByRole('combobox', { name: /se termine/i });
    fireEvent.change(endSelect, { target: { value: 'after_n' } });

    const [arg] = onChange.mock.calls[0] as [RecurrenceConfig];
    expect(arg!.end.kind).toBe('after_n');

    // Re-render with updated end condition
    renderPicker(arg!, onChange);
    const countInput = screen.getByRole('spinbutton', { name: /nombre d.occurrences/i });
    expect(countInput).toBeTruthy();
  });

  it('typing count calls onChange with after_n.count set', () => {
    const afterNValue: RecurrenceConfig = {
      rule: { kind: 'daily', interval: 1 },
      end: { kind: 'after_n', count: 1 },
    };
    const onChange = vi.fn();
    renderPicker(afterNValue, onChange);

    const countInput = screen.getByRole('spinbutton', { name: /nombre d.occurrences/i });
    fireEvent.change(countInput, { target: { value: '5' } });

    const [arg] = onChange.mock.calls[0] as [RecurrenceConfig];
    expect(arg!.end).toEqual({ kind: 'after_n', count: 5 });
  });

  it('selecting "on_date" reveals date input', () => {
    const onChange = vi.fn();
    renderPicker(dailyValue, onChange);

    const endSelect = screen.getByRole('combobox', { name: /se termine/i });
    fireEvent.change(endSelect, { target: { value: 'on_date' } });

    const [arg] = onChange.mock.calls[0] as [RecurrenceConfig];
    expect(arg!.end.kind).toBe('on_date');

    // Re-render with on_date end condition; use label query since type=date has no textbox role
    renderPicker(arg!, vi.fn());
    expect(screen.getByLabelText(/date de fin/i)).toBeTruthy();
  });
});

// ── Bitmask helpers (re-tested here for cross-module confidence) ─────────────

describe('bitmask helpers — cross-module', () => {
  it('weekdayBitmask([monday, friday]) === 17', () => {
    expect(weekdayBitmask(['monday', 'friday'])).toBe(17);
  });

  it('bitmaskToWeekdays(17) === [monday, friday]', () => {
    expect(bitmaskToWeekdays(17)).toEqual(['monday', 'friday']);
  });

  it('round-trips saturday + sunday', () => {
    const days = ['saturday', 'sunday'] as const;
    expect(bitmaskToWeekdays(weekdayBitmask([...days]))).toEqual([...days]);
  });
});
