import { useEffect, useId, useMemo, useRef, useState } from 'react';
import type { TaskPickerItem } from '@/hooks/use-activity';
import { MIN_SEARCH_LENGTH, useTaskSearch } from '@/hooks/use-task-search';

interface TaskPickerProps {
  /** Empty-state working set (FOLLOWED + non-DONE, today-first). */
  readonly tasks: readonly TaskPickerItem[];
  /** Selected task id, '' = none. */
  readonly value: string;
  readonly onChange: (taskId: string) => void;
  /** Display fallback for an arbitrary selection not in the working set (edit mode). */
  readonly selectedTask?: { readonly id: string; readonly title: string } | null;
}

/** Sentinel id for the always-present "No task" option. */
const NO_TASK_ID = '';

/**
 * Hand-rolled searchable combobox for picking an activity's task.
 *
 * - Empty / 1-char query → the working set (today-first) plus a "No task" row.
 * - Query >= {@link MIN_SEARCH_LENGTH} → debounced server-side title search.
 *
 * Mirrors the a11y / keyboard patterns of HeaderSearchBar + SuggestionDropdown.
 */
export function TaskPicker({ tasks, value, onChange, selectedTask }: TaskPickerProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const listboxId = useId();

  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);
  // Title of the last picked task, so the closed label survives when the
  // selection is neither in the working set nor in `selectedTask`.
  const [pickedTitle, setPickedTitle] = useState<string | null>(null);

  const searching = query.trim().length >= MIN_SEARCH_LENGTH;
  const { results, loading, active } = useTaskSearch(query);

  // Resolve the title to show when the field is closed.
  const selectedTitle = useMemo(() => {
    if (!value) return '';
    const fromWorkingSet = tasks.find(t => t.id === value);
    if (fromWorkingSet) return fromWorkingSet.title;
    if (selectedTask && selectedTask.id === value) return selectedTask.title;
    return pickedTitle ?? '';
  }, [value, tasks, selectedTask, pickedTitle]);

  // The list of selectable task rows (excludes the "No task" pseudo-option).
  const rows: readonly TaskPickerItem[] = searching ? results : tasks;
  // Working-set/empty modes also offer a "No task" row at index 0.
  const showNoTask = !searching;
  // Total number of navigable options = ("No task"?) + rows.
  const optionCount = (showNoTask ? 1 : 0) + rows.length;

  // Keep the active index within bounds as the option list changes.
  useEffect(() => {
    setActiveIndex(0);
  }, [query, open]);

  function commit(taskId: string, title: string | null) {
    onChange(taskId);
    setPickedTitle(title);
    setQuery('');
    setOpen(false);
    inputRef.current?.blur();
  }

  function pickByIndex(index: number) {
    if (showNoTask) {
      if (index === 0) {
        commit(NO_TASK_ID, null);
        return;
      }
      const task = rows[index - 1];
      if (task) commit(task.id, task.title);
      return;
    }
    const task = rows[index];
    if (task) commit(task.id, task.title);
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Escape') {
      setOpen(false);
      setQuery('');
      inputRef.current?.blur();
      return;
    }
    if (!open) return;

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActiveIndex(i => Math.min(i + 1, Math.max(optionCount - 1, 0)));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActiveIndex(i => Math.max(i - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      // No selectable rows (loading / empty row only) → Enter is a no-op.
      if (optionCount === 0) return;
      pickByIndex(activeIndex);
    }
  }

  const activeDescendant = open ? `${listboxId}-option-${activeIndex}` : undefined;

  return (
    <div className="relative">
      <input
        ref={inputRef}
        type="text"
        role="combobox"
        aria-expanded={open}
        aria-controls={listboxId}
        aria-autocomplete="list"
        aria-activedescendant={activeDescendant}
        value={open ? query : selectedTitle}
        placeholder="No task"
        onChange={e => {
          setQuery(e.target.value);
          if (!open) setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => {
          // Delay so a click (onMouseDown) inside the dropdown still registers.
          setTimeout(() => setOpen(false), 150);
        }}
        onKeyDown={handleKeyDown}
        className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
      />

      {open && (
        <ul
          role="listbox"
          id={listboxId}
          className="absolute z-30 mt-1 max-h-72 w-full overflow-y-auto rounded-md border border-gray-200 bg-white shadow-lg"
        >
          {showNoTask && (
            <li
              id={`${listboxId}-option-0`}
              role="option"
              aria-selected={activeIndex === 0}
              onMouseDown={() => commit(NO_TASK_ID, null)}
              className={
                'flex cursor-pointer px-3 py-2 text-sm text-gray-500 ' +
                (activeIndex === 0 ? 'bg-blue-50' : 'hover:bg-gray-50')
              }
            >
              No task
            </li>
          )}

          {searching && (loading || !active) && (
            <li role="status" aria-live="polite" className="px-3 py-2 text-sm text-gray-500">
              Loading…
            </li>
          )}

          {searching && active && !loading && results.length === 0 && (
            <li role="status" aria-live="polite" className="px-3 py-2 text-sm text-gray-500">
              No tasks match &ldquo;{query}&rdquo;
            </li>
          )}

          {rows.map((task, i) => {
            const optionIndex = showNoTask ? i + 1 : i;
            const active = optionIndex === activeIndex;
            return (
              <li
                key={task.id}
                id={`${listboxId}-option-${optionIndex}`}
                role="option"
                aria-selected={active}
                onMouseDown={() => commit(task.id, task.title)}
                className={
                  'flex cursor-pointer px-3 py-2 text-sm text-gray-900 ' +
                  (active ? 'bg-blue-50' : 'hover:bg-gray-50')
                }
              >
                <span className="truncate">{task.title}</span>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
