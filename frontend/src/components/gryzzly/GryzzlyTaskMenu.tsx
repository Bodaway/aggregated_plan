import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useAssignGryzzlyTask } from '@/hooks/use-assign-gryzzly-task';
import { AssignedGryzzlyTask } from '@/lib/gryzzly-picker-options';
import { GryzzlyTaskOptionList } from './GryzzlyTaskOptionList';
import { TerminatedBadge } from './TerminatedBadge';

const DROPDOWN_MIN_WIDTH = 288;
const DROPDOWN_MAX_HEIGHT = 256;

interface GryzzlyTaskMenuProps {
  readonly taskId: string;
  readonly assigned: AssignedGryzzlyTask | null;
}

interface Coords {
  readonly top: number;
  readonly left: number;
  readonly width: number;
}

/** Anchors the dropdown under the chip, flipping above when the bottom of the
 *  viewport is closer than the list is tall, and never overflowing to the right.
 *  A chip made wide by a long label drags the list wider with it — the options
 *  carry the same long project/task names as the trigger. */
function anchorTo(trigger: HTMLElement): Coords {
  const rect = trigger.getBoundingClientRect();
  const spaceBelow = window.innerHeight - rect.bottom;
  const flipUp = spaceBelow < DROPDOWN_MAX_HEIGHT && rect.top > spaceBelow;
  const width = Math.min(Math.max(DROPDOWN_MIN_WIDTH, rect.width), window.innerWidth - 8);

  return {
    top: flipUp ? Math.max(4, rect.top - DROPDOWN_MAX_HEIGHT - 4) : rect.bottom + 4,
    left: Math.max(4, Math.min(rect.left, window.innerWidth - width - 4)),
    width,
  };
}

/** Chip-sized Gryzzly task dropdown for task cards, sized like StatusMenu so the
 *  bottom row of a card reads as one strip of controls.
 *
 * The list is portalled to the body rather than positioned inside the card: the
 * dashboard's day columns are `overflow-hidden` scrollers, which would clip an
 * absolutely-positioned menu to a couple of rows. Being portalled costs the
 * event containment care below — a portal still bubbles React events up to the
 * card, whose own onClick opens the edit sheet. */
export function GryzzlyTaskMenu({ taskId, assigned }: GryzzlyTaskMenuProps) {
  const [open, setOpen] = useState(false);
  const [coords, setCoords] = useState<Coords>({ top: 0, left: 0, width: DROPDOWN_MIN_WIDTH });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const { assign, clear } = useAssignGryzzlyTask(taskId);

  const handleSelect = useCallback(
    async (gryzzlyTaskId: string) => {
      setOpen(false);
      await assign(gryzzlyTaskId);
    },
    [assign],
  );

  const handleClear = useCallback(async () => {
    setOpen(false);
    await clear();
  }, [clear]);

  // Position before paint so the list never flashes at the wrong spot.
  useLayoutEffect(() => {
    if (open && triggerRef.current) setCoords(anchorTo(triggerRef.current));
  }, [open]);

  // Close on Escape. Follow the chip on scroll and resize rather than closing:
  // the day column scrolls under a fixed dropdown, and closing on scroll made
  // the menu unusable — focusing the search box scrolls its own container, which
  // fires `scroll` and would shut the menu the instant it opened. Only a chip
  // scrolled clean out of the viewport closes it, since there is nothing left to
  // anchor to.
  useEffect(() => {
    if (!open) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    const reanchor = () => {
      const trigger = triggerRef.current;
      if (!trigger) return;
      const rect = trigger.getBoundingClientRect();
      const offscreen = rect.bottom < 0 || rect.top > window.innerHeight;
      if (offscreen) setOpen(false);
      else setCoords(anchorTo(trigger));
    };
    document.addEventListener('keydown', onKeyDown);
    window.addEventListener('scroll', reanchor, true);
    window.addEventListener('resize', reanchor);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('scroll', reanchor, true);
      window.removeEventListener('resize', reanchor);
    };
  }, [open]);

  // Close on outside click. Containment is tested against both nodes because the
  // dropdown is not a DOM descendant of the trigger once portalled.
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      const target = e.target as Node;
      if (triggerRef.current?.contains(target)) return;
      if (dropdownRef.current?.contains(target)) return;
      setOpen(false);
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  const label = assigned ? assigned.name ?? '(unknown)' : 'Gryzzly';
  const chipStyle = assigned
    ? assigned.stale
      ? 'bg-amber-100 text-amber-800'
      : 'bg-indigo-100 text-indigo-700'
    : 'bg-gray-100 text-gray-500';
  const title = assigned
    ? `Gryzzly: ${assigned.name ?? '(unknown)'}${assigned.projectName ? ` — ${assigned.projectName}` : ''}`
    : 'Assign a Gryzzly task';

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        title={title}
        // The chip sits inside a draggable card: stop the pointer here so
        // dnd-kit's sensor never claims the gesture, and stop the click so the
        // card does not open the edit sheet underneath.
        onPointerDown={(e) => e.stopPropagation()}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((v) => !v);
        }}
        // Sized to its label, capped at roughly the width a day-column card
        // leaves next to the status menu — enough for a full "TMA - Green Center
        // Saft", while a short name like "MCO" stays a short chip rather than a
        // long empty coloured bar. min-w-0 lets it shrink and truncate instead of
        // overflowing inside the much narrower unplanned sidebar; the full name
        // stays available as the title tooltip.
        className={`inline-flex min-w-0 max-w-[270px] items-center gap-1 px-1.5 py-0.5 rounded text-xs font-medium transition-colors hover:brightness-95 ${chipStyle}`}
      >
        <svg className="w-3 h-3 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        <span className="truncate">{label}</span>
        {assigned?.projectStatus === 'done' && <TerminatedBadge small />}
        <svg className="w-3 h-3 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {open &&
        createPortal(
          <div
            ref={dropdownRef}
            style={{ position: 'fixed', top: coords.top, left: coords.left, width: coords.width }}
            className="z-50"
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => e.stopPropagation()}
          >
            <GryzzlyTaskOptionList
              assigned={assigned}
              onSelect={(id) => void handleSelect(id)}
              onClear={() => void handleClear()}
              className="max-h-64 overflow-y-auto rounded-md border border-gray-200 bg-white shadow-lg"
            />
          </div>,
          document.body,
        )}
    </>
  );
}
