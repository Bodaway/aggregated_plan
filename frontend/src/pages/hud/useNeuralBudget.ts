import { useEffect } from 'react';
import { useQuery } from 'urql';
import { NEURAL_BUDGET_QUERY } from '@/graphql/queries/neural-budget';
import { useSurfaceVisibility } from './useSurfaceVisibility';

export interface ModelUsage {
  readonly model: string;
  readonly tokens: number;
}

export interface ProjectUsage {
  readonly name: string;
  readonly tokens: number;
  /** Share of the window's consumption, 0..1. */
  readonly ratio: number;
}

/**
 * Claude token consumption over the rolling window.
 *
 * `consumedTokens` is `input + output + cache_creation`. `cacheReadTokens` is
 * reported beside it and never inside it: on the real corpus it measures 36x the
 * other three combined, so folding it in would turn the gauge into a cache-hit
 * meter.
 */
export interface NeuralBudget {
  readonly windowHours: number;
  readonly consumedTokens: number;
  readonly cacheReadTokens: number;
  /** Typed in by hand — no public API exposes the subscription quota. Zero until
   *  it has been calibrated, which the block must say rather than paper over. */
  readonly declaredCeiling: number;
  readonly consumedRatio: number;
  readonly perDay: readonly number[];
  readonly perModel: readonly ModelUsage[];
  readonly topProject: ProjectUsage | null;
}

interface NeuralBudgetData {
  readonly neuralBudget: NeuralBudget;
}

const WINDOW_HOURS = 5;
const SPARKLINE_DAYS = 10;

/**
 * The Neural budget, re-queried on every opening of the overlay.
 *
 * Same reasoning as `useAgentSessions`: the HUD window is persistent, so a query
 * that ran only on mount would keep showing the burn as it stood when the overlay
 * was launched. Gated on the surface being visible, like every other moving part
 * of the HUD.
 *
 * `null` while the first response is in flight, and after a failure — the index is
 * built by a background job that may never have run, and a panel with no numbers is
 * the honest rendering of that.
 */
export function useNeuralBudget(): NeuralBudget | null {
  const surfaceVisible = useSurfaceVisibility();
  const [result, reexecute] = useQuery<NeuralBudgetData>({
    query: NEURAL_BUDGET_QUERY,
    variables: { windowHours: WINDOW_HOURS, sparklineDays: SPARKLINE_DAYS },
    requestPolicy: 'cache-and-network',
  });

  useEffect(() => {
    if (!surfaceVisible) return;
    reexecute({ requestPolicy: 'network-only' });
  }, [surfaceVisible, reexecute]);

  return result.data?.neuralBudget ?? null;
}
