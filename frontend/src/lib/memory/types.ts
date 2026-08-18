/** Memory layer types, mirroring the GraphQL schema (`MemoryGql` and friends). */

export type MemoryKind = 'DECISION' | 'COMMITMENT' | 'FACT' | 'PREFERENCE';

export type MemoryStatus = 'PENDING' | 'ACTIVE' | 'REJECTED';

/**
 * One memory row. `status` carries the VALIDATION verdict
 * (pending/active/rejected); invalidation is bi-temporal and lives in
 * `invalidatedAt` / `supersededBy`, so an invalidated memory still reads
 * `status: 'ACTIVE'`.
 */
export interface Memory {
  readonly id: string;
  readonly kind: MemoryKind;
  readonly title: string;
  readonly body: string | null;
  readonly occurredAt: string;
  readonly recordedAt: string;
  readonly invalidatedAt: string | null;
  readonly supersededBy: string | null;
  /** A supersession the candidate PROPOSES, not one that happened. */
  readonly proposedSupersedes: string | null;
  readonly status: MemoryStatus;
  readonly taskId: string | null;
  readonly projectId: string | null;
  readonly stakeholders: readonly string[];
}

export interface ScoredMemory {
  readonly memory: Memory;
  readonly score: number;
}

export interface BriefMemory {
  readonly id: string;
  readonly reference: string;
  readonly title: string;
  readonly stakeholders: readonly string[];
  readonly occurredOn: string;
}

export interface BriefConsolidation {
  readonly daysAgo: number | null;
  readonly stale: boolean;
}

export interface Brief {
  readonly date: string;
  readonly pendingCount: number;
  readonly decisions: readonly BriefMemory[];
  readonly decisionTotal: number;
  readonly commitments: readonly BriefMemory[];
  readonly commitmentTotal: number;
  readonly consolidation: BriefConsolidation;
}

export interface SkippedMemoryFile {
  readonly fileName: string;
  readonly reason: string;
}

export interface MemoryImportReport {
  readonly imported: readonly Memory[];
  readonly importedCount: number;
  readonly skipped: readonly SkippedMemoryFile[];
  readonly skippedCount: number;
}

/** Input of the `remember` mutation, restricted to what the UI sends. */
export interface RememberInput {
  readonly kind: MemoryKind;
  readonly title: string;
  readonly body?: string | null;
  readonly taskId?: string | null;
  readonly confirmed?: boolean;
}
