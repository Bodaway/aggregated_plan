import { useEffect, useId, useRef, useState } from 'react';
import { useSearch } from '@/lib/search/SearchProvider';
import { SuggestionDropdown } from './SuggestionDropdown';

function isTypingTarget(el: Element | null): boolean {
  if (!el) return false;
  const tag = el.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA') return true;
  return (el as HTMLElement).isContentEditable === true;
}

export function HeaderSearchBar() {
  const { query, setQuery, clearQuery, highlightActive, loading, error } = useSearch();
  const inputRef = useRef<HTMLInputElement>(null);
  const [isFocused, setIsFocused] = useState(false);
  const listboxId = useId();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const input = inputRef.current;
      if (!input) return;

      // Cmd/Ctrl+K — focus unconditionally
      if (e.key.toLowerCase() === 'k' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        input.focus();
        return;
      }

      // "/" — focus, but only when we aren't already typing somewhere else
      if (e.key === '/' && !isTypingTarget(document.activeElement)) {
        e.preventDefault();
        input.focus();
        return;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const placeholder = error
    ? 'Search unavailable — retry'
    : loading
      ? 'Indexing tasks…'
      : 'Search tasks   /';

  const showDropdown = highlightActive && isFocused;

  return (
    <div className="relative w-80">
      <input
        ref={inputRef}
        type="search"
        role="combobox"
        aria-expanded={showDropdown}
        aria-controls={listboxId}
        aria-autocomplete="list"
        value={query}
        placeholder={placeholder}
        onChange={(e) => setQuery(e.target.value)}
        onFocus={() => setIsFocused(true)}
        onBlur={() => {
          // Delay so a click inside the dropdown still registers
          setTimeout(() => setIsFocused(false), 150);
        }}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            clearQuery();
            inputRef.current?.blur();
          }
        }}
        className="w-full rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 disabled:bg-gray-100 disabled:cursor-not-allowed"
        disabled={!!error}
      />
      {query.length > 0 && !error && (
        <button
          type="button"
          aria-label="Clear search"
          onClick={() => {
            clearQuery();
            inputRef.current?.focus();
          }}
          className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-700"
        >
          ×
        </button>
      )}
      {showDropdown && <SuggestionDropdown listboxId={listboxId} />}
    </div>
  );
}
