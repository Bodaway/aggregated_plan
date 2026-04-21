import { useEffect, useRef, useState } from 'react';
import type { FuseResultMatch } from 'fuse.js';
import { useSearch } from '@/lib/search/SearchProvider';
import { MAX_DROPDOWN_ROWS } from '@/lib/search/fuse-config';

interface Props {
  readonly listboxId: string;
}

function renderHighlightedTitle(
  title: string,
  matchIndices: readonly (readonly [number, number])[] | undefined
) {
  if (!matchIndices || matchIndices.length === 0) return title;
  const out: React.ReactNode[] = [];
  let cursor = 0;
  for (const [start, end] of matchIndices) {
    if (start > cursor) out.push(title.slice(cursor, start));
    out.push(<strong key={`${start}-${end}`}>{title.slice(start, end + 1)}</strong>);
    cursor = end + 1;
  }
  if (cursor < title.length) out.push(title.slice(cursor));
  return <>{out}</>;
}

function titleMatchIndices(matches: readonly FuseResultMatch[] | undefined) {
  return matches?.find((m) => m.key === 'title')?.indices;
}

const SOURCE_ICON: Record<string, string> = {
  JIRA: '🧩',
  EXCEL: '📊',
  OBSIDIAN: '🗒️',
  PERSONAL: '📝',
  OUTLOOK: '📅',
};

export function SuggestionDropdown({ listboxId }: Props) {
  const { matches, openTaskInSheet, clearQuery, query } = useSearch();
  const [activeIndex, setActiveIndex] = useState(0);
  const ref = useRef<HTMLUListElement>(null);

  useEffect(() => {
    setActiveIndex(0);
  }, [matches]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (matches.length === 0) return;
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
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [matches, activeIndex, openTaskInSheet, clearQuery]);

  if (matches.length === 0) {
    return (
      <div
        role="listbox"
        id={listboxId}
        className="absolute z-30 mt-1 w-full rounded-md border border-gray-200 bg-white px-3 py-2 text-sm text-gray-500 shadow-lg"
      >
        No tasks match &ldquo;{query}&rdquo;
      </div>
    );
  }

  return (
    <ul
      ref={ref}
      role="listbox"
      id={listboxId}
      className="absolute z-30 mt-1 w-full overflow-y-auto rounded-md border border-gray-200 bg-white shadow-lg"
      style={{ maxHeight: `${MAX_DROPDOWN_ROWS * 3.25}rem` }}
      tabIndex={-1}
      onKeyDown={(e) => {
        if (e.key === 'ArrowDown') {
          e.preventDefault();
          setActiveIndex((i) => Math.min(i + 1, matches.length - 1));
        } else if (e.key === 'ArrowUp') {
          e.preventDefault();
          setActiveIndex((i) => Math.max(i - 1, 0));
        }
      }}
    >
      {matches.map((m, i) => {
        const { item } = m;
        const active = i === activeIndex;
        const meta = [item.sourceId, item.projectName, item.assignee]
          .filter(Boolean)
          .join(' · ');
        return (
          <li
            key={item.id}
            role="option"
            aria-selected={active}
            onMouseDown={() => {
              openTaskInSheet(item.id);
              clearQuery();
            }}
            onMouseEnter={() => setActiveIndex(i)}
            className={
              'flex cursor-pointer gap-2 px-3 py-2 text-sm ' +
              (active ? 'bg-blue-50' : 'hover:bg-gray-50')
            }
          >
            <span className="pt-0.5">{SOURCE_ICON[item.source] ?? '•'}</span>
            <div className="min-w-0 flex-1">
              <div className="truncate text-gray-900">
                {renderHighlightedTitle(item.title, titleMatchIndices(m.matches))}
              </div>
              {meta && (
                <div className="truncate text-xs text-gray-500">{meta}</div>
              )}
            </div>
          </li>
        );
      })}
    </ul>
  );
}
