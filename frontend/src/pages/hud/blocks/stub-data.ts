/* ============================================================================
 * STUB DATA — DELETE THIS FILE WHEN PLAN 2 LANDS.
 *
 * Neural budget and Active agents both depend on `hud-daemon`, the Claude
 * Code transcript indexer — that is plan 2, and it has not been written yet.
 * Everything exported below is a placeholder standing in for that daemon's
 * eventual GraphQL resolver:
 *
 *   - the two interfaces ARE the deliverable of this task — plan 2 must
 *     satisfy their shape exactly, so their fields matter more than any
 *     value below;
 *   - the two stub constants are NOT the deliverable — they are one
 *     fabricated instance each, with no source of truth, kept only so
 *     NeuralBudgetBlock and AgentsBlock have something to render until a
 *     real hook exists.
 *
 * When plan 2 ships, this file is deleted and the two blocks are pointed at
 * a real data source (a GraphQL query, most likely) instead of the
 * constants below. Do not let these numbers outlive their placeholder
 * status — this project has already shipped a stub that quietly became load
 * -bearing once.
 * ==========================================================================*/

/** Rolling-window Claude token burn against a ceiling the user calibrates by
 *  hand. Design doc §9: the subscription ceiling itself isn't exposed by any
 *  public API — it lives behind an internal endpoint gated on the CLI's own
 *  OAuth token — so this is a burn **measured locally** against a **ceiling
 *  declared by hand**. The gauge only lies about its denominator if that
 *  fact is hidden; NeuralBudgetBlock must keep it visible. */
export interface NeuralBudget {
  readonly windowHours: number; // 5
  readonly consumedRatio: number; // 0..1 against the declared ceiling
  readonly declaredCeiling: number; // tokens, entered by hand by the user
  readonly perDay: readonly number[]; // sparkline, most recent last
  readonly perModel: readonly { model: string; tokens: number }[];
  readonly topProject: { name: string; ratio: number } | null;
}

/** One live Claude Code session — per the design doc, sourced from the
 *  `sessions` table (migration 014) cross-referenced with its `.jsonl`
 *  transcript's freshness, with no new capture needed. */
export interface ActiveAgent {
  readonly sessionName: string;
  readonly taskTitle: string | null; // null = session not linked to a task
  readonly lastSeenMinutes: number; // freshness of the transcript
}

export const STUB_NEURAL_BUDGET: NeuralBudget = {
  windowHours: 5,
  consumedRatio: 0.68,
  declaredCeiling: 2_500_000,
  perDay: [180_000, 340_000, 210_000, 460_000, 300_000, 520_000, 260_000, 390_000, 230_000, 470_000],
  perModel: [
    { model: 'opus-5', tokens: 1_860_000 },
    { model: 'fable-5', tokens: 412_000 },
  ],
  topProject: { name: 'aggregated_plan', ratio: 0.61 },
};

export const STUB_ACTIVE_AGENTS: readonly ActiveAgent[] = [
  { sessionName: 'aggregated-plan-98', taskTitle: 'SCB-455', lastSeenMinutes: 0 },
  { sessionName: 'cicd-safteaction-3d', taskTitle: 'SAFT QRCode', lastSeenMinutes: 1 },
  { sessionName: 'qmkkc-1f', taskTitle: null, lastSeenMinutes: 60 },
];
