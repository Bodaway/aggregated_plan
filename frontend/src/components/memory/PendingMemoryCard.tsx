import { formatDayMonth } from '@/lib/date-utils';
import type { Memory, MemoryKind } from '@/lib/memory/types';

interface PendingMemoryCardProps {
  readonly memory: Memory;
  /**
   * Set when `acceptMemory` refused: the backend found memories close enough
   * that a silent add would duplicate one. The card then asks instead of
   * failing — merge into the named memory, or add anyway.
   */
  readonly nearDuplicates?: readonly Memory[];
  readonly busy?: boolean;
  readonly onAccept: (id: string) => void;
  readonly onForceAccept: (id: string) => void;
  readonly onReject: (id: string) => void;
  readonly onMerge: (id: string) => void;
  readonly onMergeInto: (id: string, into: string) => void;
  readonly onSupersede: (id: string) => void;
}

const KIND_STYLES: Record<MemoryKind, string> = {
  DECISION: 'bg-purple-50 text-purple-700 border-purple-200',
  COMMITMENT: 'bg-amber-50 text-amber-700 border-amber-200',
  FACT: 'bg-blue-50 text-blue-700 border-blue-200',
  PREFERENCE: 'bg-emerald-50 text-emerald-700 border-emerald-200',
};

function KindBadge({ kind }: { readonly kind: MemoryKind }) {
  return (
    <span
      className={`px-1.5 py-0.5 rounded border text-[11px] font-medium ${KIND_STYLES[kind]}`}
    >
      {kind.toLowerCase()}
    </span>
  );
}

const ACTION = 'px-2.5 py-1 text-xs font-medium rounded-md border transition-colors disabled:opacity-50 disabled:cursor-not-allowed';

export function PendingMemoryCard({
  memory,
  nearDuplicates,
  busy = false,
  onAccept,
  onForceAccept,
  onReject,
  onMerge,
  onMergeInto,
  onSupersede,
}: PendingMemoryCardProps) {
  const arbitrating = (nearDuplicates?.length ?? 0) > 0;

  return (
    <article className="bg-white border border-gray-200 rounded-lg p-3.5 space-y-2.5">
      <div className="flex items-center gap-2">
        <KindBadge kind={memory.kind} />
        <span className="text-xs text-gray-500">{formatDayMonth(memory.occurredAt)}</span>
        {memory.proposedSupersedes && (
          <span className="text-[11px] text-orange-700 bg-orange-50 border border-orange-200 rounded px-1.5 py-0.5">
            replaces an existing memory
          </span>
        )}
        {memory.stakeholders.length > 0 && (
          <span className="text-xs text-gray-500">→ {memory.stakeholders.join(', ')}</span>
        )}
      </div>

      <h3 className="text-sm font-medium text-gray-900 leading-snug">{memory.title}</h3>
      {memory.body && (
        <p className="text-xs text-gray-600 leading-relaxed whitespace-pre-line">{memory.body}</p>
      )}

      {arbitrating && (
        <div className="rounded-md border border-amber-200 bg-amber-50 p-2.5 space-y-2">
          <p className="text-xs font-medium text-amber-900">
            Looks like an existing memory — nothing was written.
          </p>
          {nearDuplicates?.map(dup => (
            <div key={dup.id} className="space-y-1.5">
              <p className="text-xs text-gray-700">{dup.title}</p>
              <button
                type="button"
                disabled={busy}
                onClick={() => onMergeInto(memory.id, dup.id)}
                className={`${ACTION} border-amber-300 bg-white text-amber-800 hover:bg-amber-100`}
              >
                Merge into it
              </button>
            </div>
          ))}
          <button
            type="button"
            disabled={busy}
            onClick={() => onForceAccept(memory.id)}
            className={`${ACTION} border-gray-300 bg-white text-gray-700 hover:bg-gray-50`}
          >
            Add anyway
          </button>
        </div>
      )}

      <div className="flex items-center gap-2 pt-0.5">
        <button
          type="button"
          disabled={busy}
          onClick={() => onAccept(memory.id)}
          className={`${ACTION} border-transparent bg-blue-600 text-white hover:bg-blue-700`}
        >
          Keep
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => onReject(memory.id)}
          className={`${ACTION} border-gray-300 text-gray-700 hover:bg-gray-50`}
        >
          Discard
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => onMerge(memory.id)}
          className={`${ACTION} border-gray-300 text-gray-700 hover:bg-gray-50`}
        >
          Merge…
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => onSupersede(memory.id)}
          className={`${ACTION} border-gray-300 text-gray-700 hover:bg-gray-50`}
        >
          Replace…
        </button>
      </div>
    </article>
  );
}
