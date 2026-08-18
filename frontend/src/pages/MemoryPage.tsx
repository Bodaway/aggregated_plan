import { useState } from 'react';
import { MemoryBriefBar } from '@/components/memory/MemoryBriefBar';
import { MemoryImportPanel } from '@/components/memory/MemoryImportPanel';
import { MemoryPickerDialog } from '@/components/memory/MemoryPickerDialog';
import { MemorySearch } from '@/components/memory/MemorySearch';
import { PendingMemoryCard } from '@/components/memory/PendingMemoryCard';
import { RememberSheet } from '@/components/memory/RememberSheet';
import { useMemoryQueue, useMemoryRecall } from '@/hooks/use-memory';
import { MEMORY_IMPORT_DEFAULT_DIR } from '@/lib/constants';

type PickerMode = 'merge' | 'supersede';

interface PickerTarget {
  readonly candidateId: string;
  readonly mode: PickerMode;
}

const PICKER_HEADINGS: Record<PickerMode, string> = {
  merge: 'Merge into… (one row survives, the wording is replaced)',
  supersede: 'Replace… (the old memory is marked no longer true)',
};

export function MemoryPage() {
  const queue = useMemoryQueue();
  const search = useMemoryRecall();
  const picker = useMemoryRecall();

  const [pickerTarget, setPickerTarget] = useState<PickerTarget | null>(null);
  const [sheetOpen, setSheetOpen] = useState(false);

  const openMerge = (candidateId: string) => setPickerTarget({ candidateId, mode: 'merge' });

  const openSupersede = (candidateId: string) => {
    const candidate = queue.pending.find(m => m.id === candidateId);
    // The candidate already names what it contradicts; `supersedeMemory`
    // defaults `old` to it, so there is nothing to pick.
    if (candidate?.proposedSupersedes) {
      void queue.supersede(candidateId, null);
      return;
    }
    setPickerTarget({ candidateId, mode: 'supersede' });
  };

  const pick = (targetId: string) => {
    if (!pickerTarget) return;
    if (pickerTarget.mode === 'merge') {
      void queue.mergeInto(pickerTarget.candidateId, targetId);
    } else {
      void queue.supersede(pickerTarget.candidateId, targetId);
    }
    setPickerTarget(null);
  };

  return (
    <div className="space-y-4 max-w-4xl">
      <MemoryBriefBar brief={queue.brief} />

      {queue.error && (
        <p className="text-sm text-red-600 bg-red-50 border border-red-200 rounded-md px-3 py-2">
          {queue.error}
        </p>
      )}

      <MemorySearch
        results={search.results}
        searched={search.searched}
        loading={search.loading}
        onSearch={search.search}
      />

      <section className="space-y-2">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold text-gray-900">
            Validation queue
            {queue.pending.length > 0 && (
              <span className="ml-1.5 text-gray-400 font-normal">{queue.pending.length}</span>
            )}
          </h2>
          <button
            type="button"
            onClick={() => setSheetOpen(true)}
            className="px-3 py-1.5 text-sm font-medium text-gray-700 border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
          >
            + New memory
          </button>
        </div>

        {queue.pending.length === 0 ? (
          <p className="text-sm text-gray-500 bg-white border border-gray-200 rounded-lg px-4 py-6 text-center">
            Nothing to triage — every candidate has a verdict.
          </p>
        ) : (
          <div className="space-y-2">
            {queue.pending.map(memory => (
              <PendingMemoryCard
                key={memory.id}
                memory={memory}
                nearDuplicates={queue.nearDuplicates[memory.id]}
                busy={queue.busy}
                onAccept={queue.accept}
                onForceAccept={queue.forceAccept}
                onReject={queue.reject}
                onMerge={openMerge}
                onMergeInto={queue.mergeInto}
                onSupersede={openSupersede}
              />
            ))}
          </div>
        )}
      </section>

      <MemoryImportPanel
        defaultDirectory={MEMORY_IMPORT_DEFAULT_DIR}
        report={queue.importReport}
        importing={queue.importing}
        onImport={queue.importDirectory}
      />

      <MemoryPickerDialog
        open={pickerTarget !== null}
        heading={pickerTarget ? PICKER_HEADINGS[pickerTarget.mode] : ''}
        results={picker.results}
        searched={picker.searched}
        loading={picker.loading}
        onSearch={q => picker.search(q, false)}
        onPick={pick}
        onClose={() => setPickerTarget(null)}
      />

      <RememberSheet
        open={sheetOpen}
        saving={queue.busy}
        onClose={() => setSheetOpen(false)}
        onSubmit={input => {
          void queue.remember(input);
          setSheetOpen(false);
        }}
      />
    </div>
  );
}
