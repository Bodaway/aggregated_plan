import type { Brief, BriefConsolidation } from '@/lib/memory/types';

interface MemoryBriefBarProps {
  readonly brief: Brief | null;
}

function plural(count: number, one: string): string {
  return `${count} ${one}${count === 1 ? '' : 's'}`;
}

/**
 * The consolidation age is the health signal of the whole layer: a queue that
 * stopped growing usually means the 17:30 job stopped running, not that nothing
 * happened.
 */
function consolidationLabel(c: BriefConsolidation): { text: string; warn: boolean } {
  if (c.daysAgo === null) return { text: 'Never consolidated', warn: true };
  if (c.stale) return { text: `Consolidation has gone quiet (${plural(c.daysAgo, 'day')})`, warn: true };
  if (c.daysAgo === 0) return { text: 'Consolidated today', warn: false };
  return { text: `Consolidated ${plural(c.daysAgo, 'day')} ago`, warn: false };
}

export function MemoryBriefBar({ brief }: MemoryBriefBarProps) {
  if (!brief) {
    return (
      <div className="bg-white border border-gray-200 rounded-lg px-4 py-3 text-sm text-gray-400">
        Loading the brief…
      </div>
    );
  }

  const consolidation = consolidationLabel(brief.consolidation);

  return (
    <div className="bg-white border border-gray-200 rounded-lg px-4 py-3 flex flex-wrap items-center gap-x-5 gap-y-1.5">
      <span className="text-sm font-semibold text-gray-900">
        {brief.pendingCount} to triage
      </span>
      <span className="text-sm text-gray-600">
        {plural(brief.decisionTotal, 'active decision')}
      </span>
      <span className="text-sm text-gray-600">
        {plural(brief.commitmentTotal, 'open commitment')}
      </span>
      <span
        className={`text-sm ${consolidation.warn ? 'text-orange-700 font-medium' : 'text-gray-500'}`}
      >
        {consolidation.text}
      </span>
    </div>
  );
}
