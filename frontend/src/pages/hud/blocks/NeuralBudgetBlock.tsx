import { useNeuralBudget, type NeuralBudget } from '../useNeuralBudget';

/** A real 5-hour rolling window rarely carries more than a handful of distinct
 *  models — capped the same way the other list blocks cap theirs: the fixed-height
 *  panel does not grow to match an unbounded contract. */
const MAX_VISIBLE_MODELS = 3;

/** "1.86M" above a million, "412k" above a thousand, the raw integer below that. */
function formatTokens(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(2)}M`;
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}k`;
  return String(tokens);
}

/** The last path segment. The resolver sends the whole path so two same-named
 *  directories stay distinguishable; the panel has room for the name. */
function projectName(path: string): string {
  const trimmed = path.replace(/\/+$/, '');
  return trimmed.slice(trimmed.lastIndexOf('/') + 1) || trimmed;
}

/** The burn against a ceiling — only drawn once a ceiling exists. */
function Gauge({ budget }: { readonly budget: NeuralBudget }) {
  const pct = Math.round(budget.consumedRatio * 100);
  return (
    <>
      <div className="hud-kv">
        <span>Consumed</span>
        <b>{pct}%</b>
      </div>
      <div className="hud-gauge hud-gauge--neural" data-testid="neural-gauge">
        {/* The bar clamps, the number above it does not: past the ceiling the
            gauge can only be full, and that is exactly when the figure matters. */}
        <i style={{ width: `${Math.min(100, pct)}%` }} />
      </div>
      <div className="hud-kv">
        <span>Declared ceiling</span>
        <b>{formatTokens(budget.declaredCeiling)} tokens</b>
      </div>
      {/* The one number this app cannot measure, said out loud rather than left
          to a code comment nobody using the HUD will read. */}
      <div className="hud-neural__ceiling-note" data-testid="neural-ceiling-note">
        Set by hand, calibrated against /usage — not measured
      </div>
    </>
  );
}

/**
 * Claude token consumption over the rolling window, read from the local index the
 * background job builds out of `~/.claude/projects`.
 *
 * Cache reads are shown beside the total and never inside it — they run 36x
 * everything else on a real corpus, and folding them in would make this a
 * cache-hit meter rather than a burn gauge.
 */
export function NeuralBudgetBlock() {
  const budget = useNeuralBudget();

  if (!budget) {
    return (
      <div className="hud-panel hud-neural" data-testid="neural-block">
        <div className="hud-label">▌ Neural budget</div>
        {/* The index is built by a background job that may never have run. Saying
            so beats a row of zeros that reads as a quiet day. */}
        <div className="hud-neural__empty">No usage index yet</div>
      </div>
    );
  }

  const hasUsage = budget.consumedTokens > 0;
  const calibrated = budget.declaredCeiling > 0;

  return (
    <div className="hud-panel hud-neural" data-testid="neural-block">
      <div className="hud-label">▌ Neural budget · {budget.windowHours}h window</div>

      {calibrated ? (
        <Gauge budget={budget} />
      ) : (
        <>
          {/* No ceiling, no gauge. A bar drawn against a denominator nobody has
              set would read as "plenty left" whatever the burn — the one thing
              this panel must never say by accident. */}
          <div className="hud-kv">
            <span>Consumed</span>
            <b>{formatTokens(budget.consumedTokens)} tokens</b>
          </div>
          <div className="hud-neural__ceiling-note" data-testid="neural-uncalibrated">
            No ceiling set — aplan config aplan.claude.declared_ceiling_tokens
          </div>
        </>
      )}

      {hasUsage ? (
        <>
          <div className="hud-neural__spark" data-testid="neural-sparkline">
            {budget.perDay.map((value, index) => {
              const peak = Math.max(1, ...budget.perDay);
              return (
                <i
                  key={index}
                  className={
                    index === budget.perDay.length - 1
                      ? 'hud-neural__spark-bar hud-neural__spark-bar--last'
                      : 'hud-neural__spark-bar'
                  }
                  data-testid="neural-spark-bar"
                  style={{ height: `${Math.max(4, Math.round((value / peak) * 100))}%` }}
                />
              );
            })}
          </div>

          {budget.perModel.slice(0, MAX_VISIBLE_MODELS).map((m) => (
            <div className="hud-kv" key={m.model}>
              <span>{m.model}</span>
              <b>{formatTokens(m.tokens)}</b>
            </div>
          ))}

          {budget.topProject && (
            <div className="hud-kv">
              <span>{projectName(budget.topProject.name)}</span>
              <b>{Math.round(budget.topProject.ratio * 100)}% of total</b>
            </div>
          )}

          <div className="hud-kv">
            <span>Cache reads</span>
            <b>{formatTokens(budget.cacheReadTokens)}</b>
          </div>
        </>
      ) : (
        <div className="hud-neural__empty">No usage recorded in this window</div>
      )}
    </div>
  );
}
