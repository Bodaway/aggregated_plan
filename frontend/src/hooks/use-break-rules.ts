import { useCallback, useMemo } from 'react';
import { useMutation, useQuery } from 'urql';
import { BREAK_RULES_QUERY, BREAK_STATS_QUERY } from '@/graphql/queries/break-rules';
import { CREATE_BREAK_RULE, DELETE_BREAK_RULE, UPDATE_BREAK_RULE } from '@/graphql/mutations/break-rules';

export type BreakKind = 'VISUAL' | 'POSTURE' | 'LONG' | 'STRENGTH';
export type BreakUrgency = 'LOW' | 'NORMAL' | 'CRITICAL';
export type BreakCadence = 'INTERVAL' | 'DAILY';

/** One rule in the routine. An `INTERVAL` rule carries `intervalMinutes` and a null
 * `atTime`; a `DAILY` rule the reverse — the server rejects any other shape. */
export interface BreakRule {
  readonly id: string;
  readonly kind: BreakKind;
  readonly label: string;
  readonly body: string;
  readonly cadence: BreakCadence;
  readonly intervalMinutes: number | null;
  readonly atTime: string | null;
  readonly durationSeconds: number;
  readonly priority: number;
  readonly enabled: boolean;
  readonly urgency: BreakUrgency;
}

/** Same fields as `BreakRule` minus `id` — what `createRule`/`updateRule` send as `input`. */
export type BreakRuleInput = Omit<BreakRule, 'id'>;

export interface BreakRuleStats {
  readonly ruleId: string;
  readonly label: string;
  readonly taken: number;
  readonly snoozed: number;
  readonly skipped: number;
  readonly ignored: number;
  readonly absorbed: number;
  readonly expired: number;
  /** `taken / seen`, or `null` when the rule was never shown to the user. */
  readonly adherence: number | null;
}

export interface BreakStats {
  readonly perRule: readonly BreakRuleStats[];
}

interface BreakRulesData {
  readonly breakRules: readonly BreakRule[];
}

interface BreakStatsData {
  readonly breakStats: BreakStats;
}

interface CreateBreakRuleData {
  readonly createBreakRule: { readonly id: string };
}

interface UpdateBreakRuleData {
  readonly updateBreakRule: { readonly id: string };
}

interface DeleteBreakRuleData {
  readonly deleteBreakRule: boolean;
}

const STATS_WINDOW_DAYS = 30;
const MS_PER_DAY = 24 * 60 * 60 * 1000;

/**
 * The break routine and its adherence over the last 30 days, plus the three mutations
 * the settings screen edits it with. Mirrors `useSettings()`'s loading/error contract
 * and re-executes both queries, network-only, once a mutation lands without error.
 */
export function useBreakRules() {
  // Stable for the lifetime of the hook instance so the stats query doesn't refire on
  // every render just because `to` ticked over to a new millisecond.
  const { from, to } = useMemo(() => {
    const now = new Date();
    return {
      from: new Date(now.getTime() - STATS_WINDOW_DAYS * MS_PER_DAY).toISOString(),
      to: now.toISOString(),
    };
  }, []);

  const [rulesResult, reexecuteRules] = useQuery<BreakRulesData>({ query: BREAK_RULES_QUERY });
  const [statsResult, reexecuteStats] = useQuery<BreakStatsData>({
    query: BREAK_STATS_QUERY,
    variables: { from, to },
  });

  const [, executeCreate] = useMutation<CreateBreakRuleData>(CREATE_BREAK_RULE);
  const [, executeUpdate] = useMutation<UpdateBreakRuleData>(UPDATE_BREAK_RULE);
  const [, executeDelete] = useMutation<DeleteBreakRuleData>(DELETE_BREAK_RULE);

  const refresh = useCallback(() => {
    reexecuteRules({ requestPolicy: 'network-only' });
    reexecuteStats({ requestPolicy: 'network-only' });
  }, [reexecuteRules, reexecuteStats]);

  const createRule = useCallback(
    async (input: BreakRuleInput) => {
      const res = await executeCreate({ input });
      if (!res.error) {
        refresh();
      }
      return res;
    },
    [executeCreate, refresh]
  );

  const updateRule = useCallback(
    async (id: string, input: BreakRuleInput) => {
      const res = await executeUpdate({ id, input });
      if (!res.error) {
        refresh();
      }
      return res;
    },
    [executeUpdate, refresh]
  );

  const deleteRule = useCallback(
    async (id: string) => {
      const res = await executeDelete({ id });
      if (!res.error) {
        refresh();
      }
      return res;
    },
    [executeDelete, refresh]
  );

  return {
    rules: rulesResult.data?.breakRules ?? [],
    stats: statsResult.data?.breakStats ?? { perRule: [] },
    loading: rulesResult.fetching || statsResult.fetching,
    error: rulesResult.error ?? statsResult.error ?? null,
    createRule,
    updateRule,
    deleteRule,
  };
}
