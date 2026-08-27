import { useEffect, useState } from 'react';
import type { BreakCadence, BreakKind, BreakRule, BreakRuleInput, BreakUrgency } from '@/hooks/use-break-rules';

const KIND_OPTIONS: ReadonlyArray<{ readonly value: BreakKind; readonly label: string }> = [
  { value: 'VISUAL', label: 'Visuelle' },
  { value: 'POSTURE', label: 'Posture' },
  { value: 'LONG', label: 'Longue pause' },
  { value: 'STRENGTH', label: 'Renforcement' },
];

const URGENCY_OPTIONS: ReadonlyArray<{ readonly value: BreakUrgency; readonly label: string }> = [
  { value: 'LOW', label: 'Faible' },
  { value: 'NORMAL', label: 'Normale' },
  { value: 'CRITICAL', label: 'Critique' },
];

/** Sensible defaults so switching cadence always leaves the row in a savable shape. */
const DEFAULT_INTERVAL_MINUTES = 30;
const DEFAULT_AT_TIME = '09:00';

const AT_TIME_PATTERN = /^([01]\d|2[0-3]):[0-5]\d$/;

/** Mirrors the server's `MAX_DURATION_SECONDS`: the break tick waits on its
 * notification inline, so a slipped zero would park the scheduler for days. Checked
 * here too, so the user learns before the round trip. */
const MAX_DURATION_SECONDS = 3600;

type EditableFields = Omit<BreakRule, 'id'>;

/** The editable half of a rule — everything but its id. */
function toFields(rule: BreakRule): EditableFields {
  return {
    kind: rule.kind,
    label: rule.label,
    body: rule.body,
    cadence: rule.cadence,
    intervalMinutes: rule.intervalMinutes,
    atTime: rule.atTime,
    durationSeconds: rule.durationSeconds,
    priority: rule.priority,
    enabled: rule.enabled,
    urgency: rule.urgency,
  };
}

/** Value equality over the editable fields, all of them primitives or null. */
function sameFields(a: EditableFields, b: EditableFields): boolean {
  return (Object.keys(a) as ReadonlyArray<keyof EditableFields>).every(k => a[k] === b[k]);
}

/** `null` when the shape satisfies the server's XOR; the French message to show and
 * block the save on, otherwise. */
function validate(fields: EditableFields): string | null {
  if (fields.cadence === 'INTERVAL') {
    if (fields.intervalMinutes === null || fields.intervalMinutes <= 0) {
      return "L'intervalle doit être positif.";
    }
  } else if (!fields.atTime || !AT_TIME_PATTERN.test(fields.atTime)) {
    return "Format HH:MM attendu pour l'heure.";
  }
  if (fields.durationSeconds <= 0) {
    return 'La durée doit être positive.';
  }
  if (fields.durationSeconds > MAX_DURATION_SECONDS) {
    return `La durée ne peut pas dépasser ${MAX_DURATION_SECONDS} secondes.`;
  }
  return null;
}

const fieldClass = 'px-2 py-1 text-sm border border-gray-300 rounded-md';
const labelClass = 'flex flex-col gap-1 text-xs text-gray-500';

export interface BreakRuleRowProps {
  readonly rule: BreakRule;
  readonly onUpdate: (id: string, input: BreakRuleInput) => void;
  readonly onDelete: (id: string) => void;
}

/** One editable row, saving through `onUpdate` without a save button. Two rhythms, on
 * purpose: a toggle or a select is one gesture and one whole decision, so it saves at
 * once; a typed field is only a decision once the user leaves it, so it saves on blur.
 * Saving each keystroke sent a mutation per character, and the refetch each one
 * triggered raced the next — the database could end up holding a prefix of what was
 * typed. A field that would break the server's INTERVAL/DAILY XOR is kept in local
 * state and blocked instead of sent. */
export function BreakRuleRow({ rule, onUpdate, onDelete }: BreakRuleRowProps) {
  const [fields, setFields] = useState<EditableFields>(() => toFields(rule));
  const [error, setError] = useState<string | null>(null);

  // The row now survives a refetch instead of being unmounted by it, so it has to
  // follow the server: whatever normalisation came back replaces what is on screen.
  // The dependency is the values, not the object — a `network-only` refetch hands back
  // a fresh object every time, and resyncing on that would wipe a half-typed field.
  const signature = JSON.stringify(toFields(rule));
  useEffect(() => {
    setFields(toFields(rule));
    setError(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signature]);

  /** Update what the row shows, and its error, without saving. */
  const edit = (patch: Partial<EditableFields>): EditableFields => {
    const next = { ...fields, ...patch };
    setFields(next);
    setError(validate(next));
    return next;
  };

  /** Update and save straight away: right for a toggle or a select. */
  const commit = (patch: Partial<EditableFields>) => {
    const next = edit(patch);
    if (!validate(next)) {
      onUpdate(rule.id, next);
    }
  };

  /** Save what the row currently holds, if it differs from the server's copy and is
   * valid. Bound to `onBlur` on every typed field. */
  const commitEdits = () => {
    if (sameFields(fields, toFields(rule)) || validate(fields)) {
      return;
    }
    onUpdate(rule.id, fields);
  };

  const handleCadenceChange = (cadence: BreakCadence) => {
    if (cadence === 'INTERVAL') {
      commit({
        cadence,
        atTime: null,
        intervalMinutes: fields.intervalMinutes ?? DEFAULT_INTERVAL_MINUTES,
      });
    } else {
      commit({
        cadence,
        intervalMinutes: null,
        atTime: fields.atTime ?? DEFAULT_AT_TIME,
      });
    }
  };

  return (
    <div className="grid grid-cols-12 gap-2 items-end py-3 border-b border-gray-100 last:border-b-0">
      <label className={`col-span-1 flex items-center gap-1.5 text-xs text-gray-500 pb-1.5`}>
        <input
          type="checkbox"
          checked={fields.enabled}
          onChange={e => commit({ enabled: e.target.checked })}
        />
        Activée
      </label>

      <label className={`col-span-2 ${labelClass}`}>
        Type
        <select
          value={fields.kind}
          onChange={e => commit({ kind: e.target.value as BreakKind })}
          className={fieldClass}
        >
          {KIND_OPTIONS.map(o => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </label>

      <label className={`col-span-2 ${labelClass}`}>
        Libellé
        <input
          type="text"
          value={fields.label}
          onChange={e => edit({ label: e.target.value })}
          onBlur={commitEdits}
          className={fieldClass}
        />
      </label>

      <label className={`col-span-2 ${labelClass}`}>
        Message
        <input
          type="text"
          value={fields.body}
          onChange={e => edit({ body: e.target.value })}
          onBlur={commitEdits}
          className={fieldClass}
        />
      </label>

      <label className={`col-span-1 ${labelClass}`}>
        Cadence
        <select
          value={fields.cadence}
          onChange={e => handleCadenceChange(e.target.value as BreakCadence)}
          className={fieldClass}
        >
          <option value="INTERVAL">Intervalle</option>
          <option value="DAILY">Quotidienne</option>
        </select>
      </label>

      {fields.cadence === 'INTERVAL' ? (
        <label className={`col-span-1 ${labelClass}`}>
          Intervalle (min)
          <input
            type="number"
            value={fields.intervalMinutes ?? ''}
            onChange={e => edit({ intervalMinutes: Number(e.target.value) })}
            onBlur={commitEdits}
            className={fieldClass}
          />
        </label>
      ) : (
        <label className={`col-span-1 ${labelClass}`}>
          Heure
          <input
            type="time"
            value={fields.atTime ?? ''}
            onChange={e => edit({ atTime: e.target.value })}
            onBlur={commitEdits}
            className={fieldClass}
          />
        </label>
      )}

      <label className={`col-span-1 ${labelClass}`}>
        Durée (s)
        <input
          type="number"
          value={fields.durationSeconds}
          onChange={e => edit({ durationSeconds: Number(e.target.value) })}
          onBlur={commitEdits}
          className={fieldClass}
        />
      </label>

      <label className={`col-span-1 ${labelClass}`}>
        Priorité
        <input
          type="number"
          value={fields.priority}
          onChange={e => edit({ priority: Number(e.target.value) })}
          onBlur={commitEdits}
          className={fieldClass}
        />
      </label>

      <label className={`col-span-1 ${labelClass}`}>
        Urgence
        <select
          value={fields.urgency}
          onChange={e => commit({ urgency: e.target.value as BreakUrgency })}
          className={fieldClass}
        >
          {URGENCY_OPTIONS.map(o => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </label>

      <div className="col-span-12 flex items-center justify-between pt-1">
        {error ? <p className="text-xs text-red-500">{error}</p> : <span />}
        <button
          type="button"
          onClick={() => onDelete(rule.id)}
          className="text-xs text-red-500 hover:text-red-700"
        >
          Supprimer
        </button>
      </div>
    </div>
  );
}
