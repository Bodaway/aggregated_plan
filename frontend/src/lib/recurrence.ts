export type Weekday =
  | 'monday'
  | 'tuesday'
  | 'wednesday'
  | 'thursday'
  | 'friday'
  | 'saturday'
  | 'sunday';

export type WeekOfMonth = 'first' | 'second' | 'third' | 'fourth' | 'last';

export type RecurrenceRule =
  | { kind: 'daily'; interval: number }
  | { kind: 'weekly'; interval: number; weekdays: Weekday[] }
  | { kind: 'monthly_by_day'; interval: number; day: number }
  | { kind: 'monthly_by_weekday'; interval: number; week: WeekOfMonth; weekday: Weekday };

export type EndCondition =
  | { kind: 'never' }
  | { kind: 'on_date'; date: string } // ISO YYYY-MM-DD
  | { kind: 'after_n'; count: number };

export type RecurrenceConfig = { rule: RecurrenceRule; end: EndCondition } | null;

// Bitmask encoding: Mon=bit 0 (1<<0), Tue=bit 1 (1<<1), ..., Sun=bit 6 (1<<6).
// Mirrors the backend WeekdaySet bitmask shape.

const WEEKDAY_ORDER: Weekday[] = [
  'monday',
  'tuesday',
  'wednesday',
  'thursday',
  'friday',
  'saturday',
  'sunday',
];

export function weekdayBitmask(days: Weekday[]): number {
  return days.reduce((acc, day) => {
    const idx = WEEKDAY_ORDER.indexOf(day);
    if (idx === -1) return acc;
    return acc | (1 << idx);
  }, 0);
}

export function bitmaskToWeekdays(n: number): Weekday[] {
  return WEEKDAY_ORDER.filter((_, idx) => (n & (1 << idx)) !== 0);
}

// ── GraphQL wire-format mapping helpers ──────────────────────────────────────
// The backend GQL enum uses uppercase (MONDAY, TUESDAY, ...).
// The frontend Weekday type uses lowercase ('monday', 'tuesday', ...).

export function weekdayToGql(d: Weekday): string {
  return d.toUpperCase();
}

export function weekdayFromGql(s: string): Weekday {
  return s.toLowerCase() as Weekday;
}

export function weekOfMonthToGql(w: WeekOfMonth): string {
  return w.toUpperCase();
}

export function weekOfMonthFromGql(s: string): WeekOfMonth {
  return s.toLowerCase() as WeekOfMonth;
}

// Convert frontend RecurrenceRule → flat GraphQL input shape
export function ruleToGqlInput(rule: RecurrenceRule): {
  kind: string;
  interval: number;
  weekdays?: string[];
  dayOfMonth?: number;
  week?: string;
  weekday?: string;
} {
  switch (rule.kind) {
    case 'daily':
      return { kind: 'DAILY', interval: rule.interval };
    case 'weekly':
      return {
        kind: 'WEEKLY',
        interval: rule.interval,
        weekdays: rule.weekdays.map(weekdayToGql),
      };
    case 'monthly_by_day':
      return { kind: 'MONTHLY_BY_DAY', interval: rule.interval, dayOfMonth: rule.day };
    case 'monthly_by_weekday':
      return {
        kind: 'MONTHLY_BY_WEEKDAY',
        interval: rule.interval,
        week: weekOfMonthToGql(rule.week),
        weekday: weekdayToGql(rule.weekday),
      };
  }
}

// Convert GraphQL RecurrenceRuleGql output → frontend RecurrenceRule
export function ruleFromGql(gql: {
  kind: string;
  interval: number;
  weekdays?: string[] | null;
  dayOfMonth?: number | null;
  week?: string | null;
  weekday?: string | null;
}): RecurrenceRule {
  switch (gql.kind.toUpperCase()) {
    case 'DAILY':
      return { kind: 'daily', interval: gql.interval };
    case 'WEEKLY':
      return {
        kind: 'weekly',
        interval: gql.interval,
        weekdays: (gql.weekdays ?? []).map(weekdayFromGql),
      };
    case 'MONTHLY_BY_DAY':
      return { kind: 'monthly_by_day', interval: gql.interval, day: gql.dayOfMonth ?? 1 };
    case 'MONTHLY_BY_WEEKDAY':
      return {
        kind: 'monthly_by_weekday',
        interval: gql.interval,
        week: weekOfMonthFromGql(gql.week ?? 'FIRST'),
        weekday: weekdayFromGql(gql.weekday ?? 'MONDAY'),
      };
    default:
      // Fallback for unknown kinds — treat as daily
      return { kind: 'daily', interval: gql.interval };
  }
}
