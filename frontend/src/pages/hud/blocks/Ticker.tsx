import { useMemo } from 'react';
import { useDashboard, type DashboardAlert } from '@/hooks/use-dashboard';
import { useBreakRules, type BreakRuleStats } from '@/hooks/use-break-rules';
import { formatDate } from '@/lib/date-utils';

/** Mirrors HudPage's own boot-sequence banner ("aplan cockpit v0.1.0") — the
 *  two are not read from a single shared constant because that string lives
 *  inline in HudPage's own `BOOT_LINES` array, not exported. Keep them in
 *  sync by hand until there's an actual version source to read from. */
const APLAN_VERSION = 'aplan v0.1.0';

/** The shortcut that opens the HUD (docs/plans/2026-08-27-hud-overlay-tauri-
 *  design.md §6: `bind = $mainMod, B, ...`) — static Hyprland config, not
 *  something the app can introspect. */
const SHORTCUT_LABEL = 'super+b';

/** The strip is one line, and this codebase's own precedent (Pressure's
 *  MAX_VISIBLE_DEADLINES, Neural's MAX_VISIBLE_MODELS) is to cap what's
 *  drawn rather than let a panel with a fixed footprint overflow, while
 *  still telling the truth about what was cut. */
const MAX_VISIBLE_ALERTS = 3;

function isCritical(alert: DashboardAlert): boolean {
  return alert.severity === 'CRITICAL';
}

/** `taken / seen` aggregated across every rule, where `seen` excludes
 *  `absorbed` and `expired` from both sides — the same exclusion
 *  documented in CLAUDE.md for the per-rule adherence rate, applied here
 *  across the whole routine instead of one rule at a time. `null` when the
 *  routine has never been shown to the user, so the caller can render a
 *  dash instead of a fabricated 0%. */
function computeOverallAdherencePct(perRule: readonly BreakRuleStats[]): number | null {
  let taken = 0;
  let seen = 0;
  for (const rule of perRule) {
    taken += rule.taken;
    seen += rule.taken + rule.snoozed + rule.skipped + rule.ignored;
  }
  return seen > 0 ? Math.round((taken / seen) * 100) : null;
}

export function Ticker() {
  const today = formatDate(new Date());
  const { data } = useDashboard(today);
  const { stats } = useBreakRules();

  const unresolvedAlerts = useMemo(() => (data?.alerts ?? []).filter((a) => !a.resolved), [data]);
  const adherencePct = useMemo(() => computeOverallAdherencePct(stats.perRule), [stats]);
  const hiddenCount = Math.max(0, unresolvedAlerts.length - MAX_VISIBLE_ALERTS);

  return (
    <div className="hud-ticker" data-testid="ticker-block">
      {unresolvedAlerts.length === 0 ? (
        <span className="hud-ticker__empty">No active alerts</span>
      ) : (
        <span className="hud-ticker__alerts" data-testid="ticker-alerts">
          {unresolvedAlerts.slice(0, MAX_VISIBLE_ALERTS).map((a) => (
            <span
              key={a.id}
              className={isCritical(a) ? 'hud-ticker__alert hud-ticker__alert--critical' : 'hud-ticker__alert'}
              data-testid="ticker-alert"
            >
              {a.message}
            </span>
          ))}
          {hiddenCount > 0 && <span className="hud-ticker__more">+{hiddenCount} more</span>}
        </span>
      )}

      <span className="hud-ticker__adherence">
        Break adherence <b>{adherencePct === null ? '—' : `${adherencePct}%`}</b>
      </span>

      <span className="hud-ticker__version">
        {APLAN_VERSION} · {SHORTCUT_LABEL}
      </span>
    </div>
  );
}
