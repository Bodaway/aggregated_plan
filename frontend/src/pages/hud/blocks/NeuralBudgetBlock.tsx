import type { NeuralBudget } from './stub-data';
import { STUB_NEURAL_BUDGET } from './stub-data';

/** A real 5-hour rolling window rarely carries more than a handful of
 *  distinct models — capped the same way PressureBlock caps its deadline
 *  list: the label states the true count implicitly through the rows
 *  actually shown, the fixed-height panel does not grow to match an
 *  unbounded contract. */
const MAX_VISIBLE_MODELS = 3;

/** "1.86M" above a million, "412k" above a thousand, the raw integer below
 *  that — mirrors the mockup's own token formatting, translated from its
 *  French decimal comma to this codebase's English convention. */
function formatTokens(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(2)}M`;
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}k`;
  return String(tokens);
}

interface NeuralBudgetBlockProps {
  /** Defaults to the plan-2 stub (see stub-data.ts). A real caller passes
   *  the daemon's own instance once plan 2 exists. */
  readonly budget?: NeuralBudget;
}

export function NeuralBudgetBlock({ budget = STUB_NEURAL_BUDGET }: NeuralBudgetBlockProps) {
  const { windowHours, consumedRatio, declaredCeiling, perDay, perModel, topProject } = budget;
  const pct = Math.round(consumedRatio * 100);
  const hasUsage = perDay.length > 0 && perModel.length > 0;
  const maxDay = Math.max(1, ...perDay);

  return (
    <div className="hud-panel hud-neural" data-testid="neural-block">
      <div className="hud-label">
        ▌ Neural budget · {windowHours}h window
        {/* Review finding: this block runs on the plan-2 stub (stub-data.ts)
            and reads as real telemetry on screen without this marker — see
            the rule's own comment in hud.css. Plan 2 must remove this
            alongside stub-data.ts, not leave it standing next to real data. */}
        <span className="hud-label__stub" data-testid="stub-marker">
          STUB
        </span>
      </div>

      <div className="hud-kv">
        <span>Consumed</span>
        <b>{pct}%</b>
      </div>
      <div className="hud-gauge hud-gauge--neural" data-testid="neural-gauge">
        <i style={{ width: `${Math.min(100, pct)}%` }} />
      </div>

      {/* Design doc §9's stated limitation: this denominator is typed by the
          user once, by hand, calibrated against `/usage` — the app never
          measures it. A gauge that implied otherwise would be lying about
          the one number it cannot know, so both this row's label and the
          caption right below say so out loud rather than leaving it to a
          code comment nobody using the HUD will ever read. */}
      <div className="hud-kv">
        <span>Declared ceiling</span>
        <b>{formatTokens(declaredCeiling)} tokens</b>
      </div>
      <div className="hud-neural__ceiling-note" data-testid="neural-ceiling-note">
        Set by hand, calibrated against /usage — not measured
      </div>

      {hasUsage ? (
        <>
          <div className="hud-neural__spark" data-testid="neural-sparkline">
            {perDay.map((v, i) => (
              <i
                key={i}
                className={
                  i === perDay.length - 1
                    ? 'hud-neural__spark-bar hud-neural__spark-bar--last'
                    : 'hud-neural__spark-bar'
                }
                data-testid="neural-spark-bar"
                style={{ height: `${Math.max(4, Math.round((v / maxDay) * 100))}%` }}
              />
            ))}
          </div>

          {perModel.slice(0, MAX_VISIBLE_MODELS).map((m) => (
            <div className="hud-kv" key={m.model}>
              <span>{m.model}</span>
              <b>{formatTokens(m.tokens)}</b>
            </div>
          ))}

          {topProject && (
            <div className="hud-kv">
              <span>{topProject.name}</span>
              <b>{Math.round(topProject.ratio * 100)}% of total</b>
            </div>
          )}
        </>
      ) : (
        <div className="hud-neural__empty">No usage recorded in this window</div>
      )}
    </div>
  );
}
