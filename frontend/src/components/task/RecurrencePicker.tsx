import type {
  RecurrenceConfig,
  RecurrenceRule,
  EndCondition,
  Weekday,
  WeekOfMonth,
} from '@/lib/recurrence';

export interface RecurrencePickerProps {
  value: RecurrenceConfig;
  onChange: (next: RecurrenceConfig) => void;
  disabled?: boolean;
}

// ── French label maps ────────────────────────────────────────────────────────

type FrequencyKey =
  | 'none'
  | 'daily'
  | 'weekdays'
  | 'weekly'
  | 'biweekly'
  | 'monthly_date'
  | 'monthly_weekday';

const FREQUENCY_OPTIONS: { value: FrequencyKey; label: string }[] = [
  { value: 'none', label: 'Ne se répète pas' },
  { value: 'daily', label: 'Tous les jours' },
  { value: 'weekdays', label: 'En semaine (Lun–Ven)' },
  { value: 'weekly', label: 'Toutes les semaines' },
  { value: 'biweekly', label: 'Toutes les 2 semaines' },
  { value: 'monthly_date', label: 'Mensuel le [N]' },
  { value: 'monthly_weekday', label: 'Mensuel le [Nème] [jour]' },
];

type WeekdayMeta = { value: Weekday; label: string };

const WEEKDAYS: WeekdayMeta[] = [
  { value: 'monday', label: 'Lun' },
  { value: 'tuesday', label: 'Mar' },
  { value: 'wednesday', label: 'Mer' },
  { value: 'thursday', label: 'Jeu' },
  { value: 'friday', label: 'Ven' },
  { value: 'saturday', label: 'Sam' },
  { value: 'sunday', label: 'Dim' },
];

const WORKDAYS: Weekday[] = ['monday', 'tuesday', 'wednesday', 'thursday', 'friday'];

const WEEK_OF_MONTH_OPTIONS: { value: WeekOfMonth; label: string }[] = [
  { value: 'first', label: 'Première' },
  { value: 'second', label: 'Deuxième' },
  { value: 'third', label: 'Troisième' },
  { value: 'fourth', label: 'Quatrième' },
  { value: 'last', label: 'Dernière' },
];

const WEEKDAY_FULL_LABELS: { value: Weekday; label: string }[] = [
  { value: 'monday', label: 'Lundi' },
  { value: 'tuesday', label: 'Mardi' },
  { value: 'wednesday', label: 'Mercredi' },
  { value: 'thursday', label: 'Jeudi' },
  { value: 'friday', label: 'Vendredi' },
  { value: 'saturday', label: 'Samedi' },
  { value: 'sunday', label: 'Dimanche' },
];

// ── Helpers ──────────────────────────────────────────────────────────────────

function ruleToFrequencyKey(rule: RecurrenceRule): FrequencyKey {
  if (rule.kind === 'daily') return 'daily';
  if (rule.kind === 'monthly_by_day') return 'monthly_date';
  if (rule.kind === 'monthly_by_weekday') return 'monthly_weekday';
  // weekly
  if (rule.interval === 2) return 'biweekly';
  const days = rule.weekdays;
  const isWeekdays =
    days.length === 5 &&
    WORKDAYS.every(d => days.includes(d)) &&
    !days.includes('saturday') &&
    !days.includes('sunday');
  if (isWeekdays) return 'weekdays';
  return 'weekly';
}

function defaultEndCondition(): EndCondition {
  return { kind: 'never' };
}

function defaultRule(key: FrequencyKey): RecurrenceRule {
  switch (key) {
    case 'daily':
      return { kind: 'daily', interval: 1 };
    case 'weekdays':
      return { kind: 'weekly', interval: 1, weekdays: [...WORKDAYS] };
    case 'weekly':
      return { kind: 'weekly', interval: 1, weekdays: [] };
    case 'biweekly':
      return { kind: 'weekly', interval: 2, weekdays: [] };
    case 'monthly_date':
      return { kind: 'monthly_by_day', interval: 1, day: 1 };
    case 'monthly_weekday':
      return { kind: 'monthly_by_weekday', interval: 1, week: 'first', weekday: 'monday' };
    default:
      return { kind: 'daily', interval: 1 };
  }
}

// ── Sub-components (inlined to keep the single-file contract) ────────────────

interface DayTogglesProps {
  weekdays: Weekday[];
  locked: boolean;
  disabled: boolean;
  onChange: (next: Weekday[]) => void;
}

function DayToggles({ weekdays, locked, disabled, onChange }: DayTogglesProps) {
  function toggle(day: Weekday) {
    if (disabled || locked) return;
    const next = weekdays.includes(day)
      ? weekdays.filter(d => d !== day)
      : [...weekdays, day];
    onChange(next);
  }

  return (
    <fieldset className="space-y-1">
      <legend className="sr-only">Jours de la semaine</legend>
      <div className="flex gap-1.5 flex-wrap">
        {WEEKDAYS.map(({ value, label }) => {
          const active = weekdays.includes(value);
          const isDisabled = disabled || locked;
          return (
            <button
              key={value}
              type="button"
              aria-pressed={active}
              disabled={isDisabled}
              onClick={() => toggle(value)}
              className={[
                'px-2.5 py-1 text-xs font-medium rounded-md border transition-colors',
                active
                  ? 'bg-blue-600 text-white border-blue-600'
                  : 'bg-white text-gray-700 border-gray-300',
                isDisabled ? 'opacity-50 cursor-not-allowed' : 'hover:border-blue-400',
              ].join(' ')}
            >
              {label}
            </button>
          );
        })}
      </div>
    </fieldset>
  );
}

// ── Main component ───────────────────────────────────────────────────────────

export function RecurrencePicker({ value, onChange, disabled = false }: RecurrencePickerProps) {
  // ── Derived state ────────────────────────────────────────────────────────

  const currentKey: FrequencyKey = value == null ? 'none' : ruleToFrequencyKey(value.rule);
  const currentRule = value?.rule ?? null;
  const currentEnd: EndCondition = value?.end ?? defaultEndCondition();

  // ── Frequency change ─────────────────────────────────────────────────────

  function handleFrequencyChange(key: FrequencyKey) {
    if (key === 'none') {
      onChange(null);
      return;
    }
    const rule = defaultRule(key);
    onChange({ rule, end: value?.end ?? defaultEndCondition() });
  }

  // ── Rule field changes ───────────────────────────────────────────────────

  function handleWeekdaysChange(days: Weekday[]) {
    if (!value || currentRule?.kind !== 'weekly') return;
    onChange({
      rule: { ...currentRule, weekdays: days },
      end: currentEnd,
    });
  }

  function handleMonthDayChange(dayStr: string) {
    const day = parseInt(dayStr, 10);
    if (!value || currentRule?.kind !== 'monthly_by_day') return;
    onChange({ rule: { ...currentRule, day: isNaN(day) ? 1 : day }, end: currentEnd });
  }

  function handleWeekOfMonthChange(week: WeekOfMonth) {
    if (!value || currentRule?.kind !== 'monthly_by_weekday') return;
    onChange({ rule: { ...currentRule, week }, end: currentEnd });
  }

  function handleMonthlyWeekdayChange(weekday: Weekday) {
    if (!value || currentRule?.kind !== 'monthly_by_weekday') return;
    onChange({ rule: { ...currentRule, weekday }, end: currentEnd });
  }

  // ── End condition changes ────────────────────────────────────────────────

  function handleEndKindChange(kind: EndCondition['kind']) {
    if (!value) return;
    let end: EndCondition;
    switch (kind) {
      case 'never':
        end = { kind: 'never' };
        break;
      case 'on_date':
        end = { kind: 'on_date', date: '' };
        break;
      case 'after_n':
        end = { kind: 'after_n', count: 1 };
        break;
    }
    onChange({ rule: value.rule, end });
  }

  function handleEndDateChange(date: string) {
    if (!value) return;
    onChange({ rule: value.rule, end: { kind: 'on_date', date } });
  }

  function handleEndCountChange(countStr: string) {
    if (!value) return;
    const count = parseInt(countStr, 10);
    onChange({ rule: value.rule, end: { kind: 'after_n', count: isNaN(count) ? 1 : count } });
  }

  // ── Render ───────────────────────────────────────────────────────────────

  const inputClass =
    'w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500';
  const labelClass = 'block text-xs font-medium text-gray-700 mb-1';

  const isWeekdaysPreset = currentKey === 'weekdays';
  const showDayToggles =
    currentKey === 'weekly' || currentKey === 'biweekly' || isWeekdaysPreset;

  return (
    <div className="space-y-2">
      {/* Frequency select */}
      <div>
        <label htmlFor="recurrence-frequency" className={labelClass}>Récurrence</label>
        <select
          id="recurrence-frequency"
          value={currentKey}
          disabled={disabled}
          onChange={e => handleFrequencyChange(e.target.value as FrequencyKey)}
          className={inputClass}
        >
          {FREQUENCY_OPTIONS.map(({ value: v, label }) => (
            <option key={v} value={v}>
              {label}
            </option>
          ))}
        </select>
      </div>

      {/* Day toggles — weekly / biweekly / weekdays preset */}
      {showDayToggles && currentRule?.kind === 'weekly' && (
        <div>
          <DayToggles
            weekdays={currentRule.weekdays}
            locked={isWeekdaysPreset}
            disabled={disabled}
            onChange={handleWeekdaysChange}
          />
        </div>
      )}

      {/* Monthly by day */}
      {currentKey === 'monthly_date' && currentRule?.kind === 'monthly_by_day' && (
        <div>
          <label htmlFor="recurrence-month-day" className={labelClass}>Jour du mois</label>
          <input
            id="recurrence-month-day"
            type="number"
            min={1}
            max={31}
            value={currentRule.day}
            disabled={disabled}
            placeholder="Jour du mois (ex. 15)"
            onChange={e => handleMonthDayChange(e.target.value)}
            className={inputClass}
          />
        </div>
      )}

      {/* Monthly by weekday */}
      {currentKey === 'monthly_weekday' && currentRule?.kind === 'monthly_by_weekday' && (
        <div className="grid grid-cols-2 gap-2">
          <div>
            <label htmlFor="recurrence-week-of-month" className={labelClass}>Semaine</label>
            <select
              id="recurrence-week-of-month"
              value={currentRule.week}
              disabled={disabled}
              onChange={e => handleWeekOfMonthChange(e.target.value as WeekOfMonth)}
              className={inputClass}
            >
              {WEEK_OF_MONTH_OPTIONS.map(({ value: v, label }) => (
                <option key={v} value={v}>
                  {label}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label htmlFor="recurrence-monthly-weekday" className={labelClass}>Jour</label>
            <select
              id="recurrence-monthly-weekday"
              value={currentRule.weekday}
              disabled={disabled}
              onChange={e => handleMonthlyWeekdayChange(e.target.value as Weekday)}
              className={inputClass}
            >
              {WEEKDAY_FULL_LABELS.map(({ value: v, label }) => (
                <option key={v} value={v}>
                  {label}
                </option>
              ))}
            </select>
          </div>
        </div>
      )}

      {/* End condition — only when a frequency is active */}
      {value !== null && (
        <div className="space-y-2 pt-1 border-t border-gray-100">
          <div>
            <label htmlFor="recurrence-end-kind" className={labelClass}>Se termine</label>
            <select
              id="recurrence-end-kind"
              value={currentEnd.kind}
              disabled={disabled}
              onChange={e => handleEndKindChange(e.target.value as EndCondition['kind'])}
              className={inputClass}
            >
              <option value="never">Jamais</option>
              <option value="on_date">À une date</option>
              <option value="after_n">Après N occurrences</option>
            </select>
          </div>

          {currentEnd.kind === 'on_date' && (
            <div>
              <label htmlFor="recurrence-end-date" className={labelClass}>Date de fin</label>
              <input
                id="recurrence-end-date"
                type="date"
                value={currentEnd.date}
                disabled={disabled}
                onChange={e => handleEndDateChange(e.target.value)}
                className={inputClass}
              />
            </div>
          )}

          {currentEnd.kind === 'after_n' && (
            <div>
              <label htmlFor="recurrence-end-count" className={labelClass}>Nombre d&apos;occurrences</label>
              <input
                id="recurrence-end-count"
                type="number"
                min={1}
                value={currentEnd.count}
                disabled={disabled}
                onChange={e => handleEndCountChange(e.target.value)}
                className={inputClass}
              />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
