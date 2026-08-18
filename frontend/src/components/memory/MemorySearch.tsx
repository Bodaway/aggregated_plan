import { useState } from 'react';
import { formatDayMonth } from '@/lib/date-utils';
import type { ScoredMemory } from '@/lib/memory/types';

interface MemorySearchProps {
  readonly results: readonly ScoredMemory[];
  /** True once a search has come back, so an empty list means "no match". */
  readonly searched: boolean;
  readonly loading?: boolean;
  readonly onSearch: (q: string, includeHistory: boolean) => void;
}

function ResultRow({ hit }: { readonly hit: ScoredMemory }) {
  const { memory } = hit;
  const invalidated = memory.invalidatedAt !== null;

  return (
    <li className="py-2 border-t border-gray-100 first:border-t-0">
      <div className="flex items-start gap-2">
        <span className="text-[11px] text-gray-400 tabular-nums mt-0.5 w-10 shrink-0">
          {hit.score.toFixed(2)}
        </span>
        <div className="min-w-0 space-y-1">
          <p
            className={`text-sm leading-snug ${
              invalidated ? 'text-gray-400 line-through' : 'text-gray-900'
            }`}
          >
            {memory.title}
          </p>
          <div className="flex flex-wrap items-center gap-2 text-[11px]">
            <span className="text-gray-400">{memory.kind.toLowerCase()}</span>
            <span className="text-gray-400">{formatDayMonth(memory.occurredAt)}</span>
            {memory.status === 'PENDING' && (
              <span className="text-amber-700 bg-amber-50 border border-amber-200 rounded px-1.5 py-0.5">
                awaiting validation
              </span>
            )}
            {invalidated && (
              <span className="text-orange-700">
                No longer true{memory.supersededBy ? ` — replaced by ${memory.supersededBy}` : ''}
              </span>
            )}
          </div>
        </div>
      </div>
    </li>
  );
}

export function MemorySearch({ results, searched, loading = false, onSearch }: MemorySearchProps) {
  const [query, setQuery] = useState('');
  const [includeHistory, setIncludeHistory] = useState(false);

  const search = () => {
    if (query.trim() === '') return;
    onSearch(query.trim(), includeHistory);
  };

  return (
    <div className="bg-white border border-gray-200 rounded-lg px-4 py-3 space-y-3">
      <div className="flex items-center gap-3">
        <input
          type="search"
          value={query}
          onChange={e => setQuery(e.target.value)}
          onKeyDown={e => {
            if (e.key === 'Enter') search();
          }}
          placeholder="Search the memory…"
          className="flex-1 rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
        />
        <label className="flex items-center gap-1.5 text-xs text-gray-600 whitespace-nowrap">
          <input
            type="checkbox"
            aria-label="Include history"
            checked={includeHistory}
            onChange={e => setIncludeHistory(e.target.checked)}
            className="rounded border-gray-300"
          />
          Include history
        </label>
        <button
          type="button"
          onClick={search}
          disabled={loading}
          className="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 transition-colors disabled:opacity-50"
        >
          Search
        </button>
      </div>

      {results.length > 0 && (
        <ul className="divide-y-0">
          {results.map(hit => (
            <ResultRow key={hit.memory.id} hit={hit} />
          ))}
        </ul>
      )}
      {searched && results.length === 0 && !loading && (
        <p className="text-sm text-gray-500">No memory matched that query.</p>
      )}
    </div>
  );
}
