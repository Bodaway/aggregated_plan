import { useEffect, useId, useRef, useState } from 'react';
import { useSearch } from '@/lib/search/SearchProvider';
import { isTypingTarget } from '@/lib/is-typing-target';
import { SuggestionDropdown } from './SuggestionDropdown';

export function HeaderSearchBar() {
  const { query, setQuery, clearQuery, highlightActive, matches, openTaskInSheet, loading, error } = useSearch();
  const inputRef = useRef<HTMLInputElement>(null);
  const [isFocused, setIsFocused] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const listboxId = useId();

  // Reset active index whenever the match list changes.
  useEffect(() => {
    setActiveIndex(0);
  }, [matches]);

  // Global shortcut: "/" and Cmd/Ctrl+K focus the search input.
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
  const hasMatches = matches.length > 0;

  // Stable id for the currently highlighted option (used by aria-activedescendant).
  const activeDescendant =
    showDropdown && hasMatches ? `${listboxId}-option-${activeIndex}` : undefined;

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Escape') {
      clearQuery();
      inputRef.current?.blur();
      return;
    }

    // Arrow/Enter navigation only makes sense when the dropdown is open with results.
    if (!showDropdown || !hasMatches) return;

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActiveIndex((i) => Math.min(i + 1, matches.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActiveIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const picked = matches[activeIndex];
      if (picked) {
        openTaskInSheet(picked.item.id);
        clearQuery();
      }
    }
  }

  return (
    <div className="relative w-80">
      <input
        ref={inputRef}
        type="search"
        role="combobox"
        aria-expanded={showDropdown}
        aria-controls={listboxId}
        aria-autocomplete="list"
        aria-activedescendant={activeDescendant}
        value={query}
        placeholder={placeholder}
        onChange={(e) => setQuery(e.target.value)}
        onFocus={() => setIsFocused(true)}
        onBlur={() => {
          // Delay so a click inside the dropdown still registers
          setTimeout(() => setIsFocused(false), 150);
        }}
        onKeyDown={handleKeyDown}
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
      {showDropdown && (
        <SuggestionDropdown listboxId={listboxId} activeIndex={activeIndex} />
      )}
    </div>
  );
}
