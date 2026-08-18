import { useState } from 'react';
import { formatDayMonth } from '@/lib/date-utils';
import type { ScoredMemory } from '@/lib/memory/types';

interface MemoryPickerDialogProps {
  readonly open: boolean;
  readonly heading: string;
  readonly results: readonly ScoredMemory[];
  readonly searched: boolean;
  readonly loading?: boolean;
  readonly onSearch: (q: string) => void;
  readonly onPick: (id: string) => void;
  readonly onClose: () => void;
}

export function MemoryPickerDialog({
  open,
  heading,
  results,
  searched,
  loading = false,
  onSearch,
  onPick,
  onClose,
}: MemoryPickerDialogProps) {
  const [query, setQuery] = useState('');

  if (!open) return null;

  const search = () => {
    if (query.trim() === '') return;
    onSearch(query.trim());
  };

  return (
    <>
      <div className="fixed inset-0 bg-black/20 z-40" onClick={onClose} aria-hidden />
      <div
        role="dialog"
        aria-label={heading}
        className="fixed left-1/2 top-24 -translate-x-1/2 w-full max-w-xl bg-white rounded-lg shadow-xl z-50 flex flex-col max-h-[70vh]"
      >
        <div className="px-5 py-3 border-b border-gray-200">
          <h2 className="text-sm font-semibold text-gray-900">{heading}</h2>
        </div>

        <div className="px-5 py-3 flex items-center gap-2">
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
          <button
            type="button"
            onClick={search}
            disabled={loading}
            className="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 transition-colors disabled:opacity-50"
          >
            Search
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 pb-3">
          {results.length > 0 ? (
            <ul className="space-y-1">
              {results.map(hit => (
                <li key={hit.memory.id}>
                  <button
                    type="button"
                    onClick={() => onPick(hit.memory.id)}
                    className="w-full text-left px-2.5 py-2 rounded-md border border-gray-200 hover:border-blue-300 hover:bg-blue-50 transition-colors"
                  >
                    <span className="block text-sm text-gray-900">{hit.memory.title}</span>
                    <span aria-hidden className="block text-[11px] text-gray-400 mt-0.5">
                      {hit.memory.kind.toLowerCase()} · {formatDayMonth(hit.memory.occurredAt)}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            searched && !loading && (
              <p className="text-sm text-gray-500">No memory matched that query.</p>
            )
          )}
        </div>

        <div className="px-5 py-3 border-t border-gray-200 flex justify-end">
          <button
            type="button"
            onClick={onClose}
            className="px-3 py-1.5 text-sm font-medium text-gray-700 border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
          >
            Cancel
          </button>
        </div>
      </div>
    </>
  );
}
