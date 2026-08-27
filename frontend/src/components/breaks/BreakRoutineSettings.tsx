import { useMemo } from 'react';
import { useBreakRules } from '@/hooks/use-break-rules';
import type { BreakRuleInput } from '@/hooks/use-break-rules';
import { BreakRuleRow } from './BreakRuleRow';

/** A new rule's starting shape: a gentle posture nudge every 30 minutes. */
const NEW_RULE_DEFAULTS: Omit<BreakRuleInput, 'priority'> = {
  kind: 'POSTURE',
  label: 'Nouvelle pause',
  body: '',
  cadence: 'INTERVAL',
  intervalMinutes: 30,
  atTime: null,
  durationSeconds: 120,
  enabled: true,
  urgency: 'NORMAL',
};

/** `Math.round(a * 100) + ' %'`, or an em dash when nothing was ever shown to the user. */
function formatAdherence(adherence: number | null): string {
  return adherence === null ? '—' : `${Math.round(adherence * 100)} %`;
}

/** The rule list and its 30-day adherence panel. The master switch and the four
 * scalar settings live in `SettingsPage`, wired to the existing `useSettings()`
 * configuration mutation — this component only talks to `useBreakRules()`. */
export function BreakRoutineSettings() {
  const { rules, stats, loading, error, createRule, updateRule, deleteRule } = useBreakRules();

  const sortedRules = useMemo(
    () => [...rules].sort((a, b) => a.priority - b.priority),
    [rules]
  );

  const handleAdd = () => {
    const maxPriority = rules.reduce((max, r) => Math.max(max, r.priority), 0);
    createRule({ ...NEW_RULE_DEFAULTS, priority: maxPriority + 1 });
  };

  // Only the first load may take the screen. Every mutation refetches both queries
  // `network-only`, and unmounting the list on each of those would take the user's
  // focus with it — one keystroke, one lost cursor.
  if (loading && rules.length === 0) {
    return <p className="text-sm text-gray-500">Chargement de la routine de pauses…</p>;
  }

  if (error) {
    return <p className="text-sm text-red-500">Impossible de charger la routine de pauses.</p>;
  }

  return (
    <div className="space-y-4">
      <div>
        {sortedRules.map(rule => (
          <BreakRuleRow key={rule.id} rule={rule} onUpdate={updateRule} onDelete={deleteRule} />
        ))}
      </div>

      <button
        type="button"
        onClick={handleAdd}
        className="px-3 py-1.5 text-xs font-medium text-blue-600 border border-blue-300 rounded-md hover:bg-blue-50 transition-colors"
      >
        Ajouter une pause
      </button>

      <div className="pt-4 border-t border-gray-100">
        <h4 className="text-sm font-medium text-gray-600 mb-3">Adhérence sur 30 jours</h4>
        {stats.perRule.length === 0 ? (
          <p className="text-xs text-gray-400">Pas encore de statistiques.</p>
        ) : (
          <table className="w-full text-xs">
            <thead>
              <tr className="text-left text-gray-500">
                <th className="py-1 font-medium">Règle</th>
                <th className="py-1 font-medium">Prises</th>
                <th className="py-1 font-medium">Reportées</th>
                <th className="py-1 font-medium">Ignorées</th>
                <th className="py-1 font-medium">Adhérence</th>
                <th className="py-1 font-medium">Jamais affichées</th>
              </tr>
            </thead>
            <tbody>
              {stats.perRule.map(row => (
                <tr key={row.ruleId} className="border-t border-gray-100">
                  <td className="py-1.5">{row.label}</td>
                  <td className="py-1.5">{row.taken}</td>
                  <td className="py-1.5">{row.snoozed}</td>
                  <td className="py-1.5">{row.skipped + row.ignored}</td>
                  <td className="py-1.5 font-medium">{formatAdherence(row.adherence)}</td>
                  <td
                    className="py-1.5 text-gray-400"
                    title="Absorbées par une réunion ou expirées avant réponse : jamais montrées à l'écran"
                  >
                    {row.absorbed + row.expired}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
