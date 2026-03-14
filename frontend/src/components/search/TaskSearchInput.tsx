import { useState, useRef, useEffect } from 'react';
import type { TaskSearchResult } from '@/hooks/use-task-search';

const FIELD_LABELS: Record<string, string> = {
  title: 'Title',
  description: 'Description',
  assignee: 'Assignee',
  source_id: 'Jira Key',
  jira_status: 'Jira Status',
  status: 'Status',
  source: 'Source',
  urgency_text: 'Urgency',
  impact_text: 'Impact',
  project_name: 'Project',
  tag_names: 'Tag',
};

interface TaskSearchInputProps {
  readonly value: string;
  readonly onChange: (value: string) => void;
  readonly onClear: () => void;
  readonly results: readonly TaskSearchResult[];
  readonly isSearching: boolean;
  readonly matchCount: number;
}

export function TaskSearchInput({
  value,
  onChange,
  onClear,
  results,
  isSearching,
  matchCount,
}: TaskSearchInputProps) {
  const [showDropdown, setShowDropdown] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);

  // Close dropdown on click outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (wrapperRef.current && !wrapperRef.current.contains(event.target as Node)) {
        setShowDropdown(false);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const hasResults = results.length > 0;
  const isActive = value.length > 0;

  return (
    <div ref={wrapperRef} className="relative">
      <div className="flex items-center gap-2 bg-white border border-gray-200 rounded-lg px-3 py-1.5 w-80 focus-within:ring-2 focus-within:ring-blue-400 focus-within:border-blue-400 transition-all">
        {/* Search icon */}
        <svg className="w-4 h-4 text-gray-400 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z" />
        </svg>

        <input
          type="text"
          value={value}
          onChange={e => {
            onChange(e.target.value);
            setShowDropdown(true);
          }}
          onFocus={() => setShowDropdown(true)}
          placeholder="Search tasks..."
          className="flex-1 text-sm bg-transparent outline-none placeholder-gray-400"
        />

        {/* Loading spinner */}
        {isSearching && (
          <svg className="w-4 h-4 text-gray-400 animate-spin flex-shrink-0" fill="none" viewBox="0 0 24 24">
            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
          </svg>
        )}

        {/* Match count badge */}
        {isActive && !isSearching && (
          <span className="text-xs text-gray-500 flex-shrink-0">
            {matchCount} match{matchCount !== 1 ? 'es' : ''}
          </span>
        )}

        {/* Clear button */}
        {isActive && (
          <button
            onClick={() => {
              onClear();
              setShowDropdown(false);
            }}
            className="text-gray-400 hover:text-gray-600 flex-shrink-0"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        )}
      </div>

      {/* Autocomplete dropdown */}
      {showDropdown && isActive && hasResults && (
        <div className="absolute top-full left-0 right-0 mt-1 bg-white border border-gray-200 rounded-lg shadow-lg z-50 max-h-64 overflow-y-auto">
          {results.slice(0, 10).map((result) => (
            <div
              key={result.taskId}
              className="flex items-center gap-2 px-3 py-2 hover:bg-gray-50 cursor-default text-sm"
            >
              <span className="inline-flex px-1.5 py-0.5 rounded text-xs font-medium bg-gray-100 text-gray-600 flex-shrink-0">
                {FIELD_LABELS[result.matchedField] ?? result.matchedField}
              </span>
              <span className="text-gray-900 truncate">{result.matchedSnippet}</span>
            </div>
          ))}
        </div>
      )}

      {/* No results message */}
      {showDropdown && isActive && !isSearching && !hasResults && value.trim().length > 0 && (
        <div className="absolute top-full left-0 right-0 mt-1 bg-white border border-gray-200 rounded-lg shadow-lg z-50 px-3 py-2 text-sm text-gray-500">
          No tasks found
        </div>
      )}
    </div>
  );
}
