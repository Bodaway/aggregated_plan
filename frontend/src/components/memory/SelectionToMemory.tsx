import { useEffect, useState } from 'react';
import { RememberSheet } from './RememberSheet';
import { CAPTURE_CHIP_Z } from '@/lib/memory/layers';
import { splitSelection } from '@/lib/memory/selection';
import type { RememberInput } from '@/lib/memory/types';

interface SelectionToMemoryProps {
  readonly onRemember: (input: RememberInput) => void | Promise<void>;
  readonly saving?: boolean;
  readonly error?: string | null;
}

/** Below this, a selection is a mis-click, not something worth remembering. */
const MIN_SELECTION_LENGTH = 4;

/** Chip footprint, used to keep it inside the viewport. Approximate on purpose:
 *  measuring would need a layout pass, and being 10px off is invisible. */
const CHIP_WIDTH = 116;
const CHIP_HEIGHT = 30;
const GAP = 8;

interface Capture {
  readonly text: string;
  readonly taskId: string | null;
  readonly top: number;
  readonly left: number;
}

/** The task whose card the selection sits in, when it sits in one. */
function findTaskId(node: Node | null): string | null {
  const element = node instanceof Element ? node : (node?.parentElement ?? null);
  return element?.closest('[data-task-id]')?.getAttribute('data-task-id') ?? null;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

/**
 * Where to park the chip for a selection, or `null` when the selection has no
 * on-screen geometry.
 *
 * A range whose nodes were replaced under it — every dashboard refetch does
 * that — still stringifies, but its rect collapses to 0×0 at the origin.
 * Treating that as a position is what used to park the chip in the top-left
 * corner of the screen, far from anything the user had selected. No geometry
 * means the selection is not visible, so there is nothing to offer.
 */
function chipPosition(selection: Selection | null): { top: number; left: number } | null {
  if (!selection || selection.rangeCount === 0) return null;
  const rect = selection.getRangeAt(0).getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return null;

  // Off-screen selections happen too: an inner container scrolls after the
  // selection was made. The chip is `fixed`, so it would land below the fold.
  const onScreen =
    rect.bottom > 0 && rect.top < window.innerHeight && rect.right > 0 && rect.left < window.innerWidth;
  if (!onScreen) return null;

  const maxLeft = Math.max(GAP, window.innerWidth - CHIP_WIDTH - GAP);
  const maxTop = Math.max(GAP, window.innerHeight - CHIP_HEIGHT - GAP);
  const below = rect.bottom + GAP;
  const fitsBelow = below + CHIP_HEIGHT <= window.innerHeight;
  return {
    left: clamp(rect.left, GAP, maxLeft),
    top: clamp(fitsBelow ? below : rect.top - CHIP_HEIGHT - GAP, GAP, maxTop),
  };
}

export function SelectionToMemory({ onRemember, saving = false, error = null }: SelectionToMemoryProps) {
  const [capture, setCapture] = useState<Capture | null>(null);
  const [sheetOpen, setSheetOpen] = useState(false);

  useEffect(() => {
    // While the sheet is open the pointer is in the form, and reading the
    // selection again would wipe the very capture being edited.
    if (sheetOpen) return undefined;

    const selectedText = () => (window.getSelection()?.toString() ?? '').trim();

    /** Offer a capture — on the events that fire AFTER the browser has settled
     *  the selection (`keydown` fires before it, and read stale values). */
    const offer = () => {
      const selection = window.getSelection();
      const text = selection?.toString() ?? '';
      if (text.trim().length < MIN_SELECTION_LENGTH) {
        setCapture(null);
        return;
      }
      const position = chipPosition(selection);
      if (!position) {
        setCapture(null);
        return;
      }
      setCapture({
        text,
        taskId: findTaskId(selection?.anchorNode ?? null),
        ...position,
      });
    };

    /** Clear a capture whose selection is gone — a re-render can drop it with no
     *  click and no keystroke, leaving the chip stranded. Never creates one:
     *  `selectionchange` fires throughout a drag-select, and a chip that
     *  followed the cursor would flicker. */
    const dropIfGone = () => {
      if (selectedText().length < MIN_SELECTION_LENGTH) setCapture(null);
    };

    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'm') {
        e.preventDefault();
        setSheetOpen(prev => prev || capture !== null);
      }
    };

    document.addEventListener('mouseup', offer);
    document.addEventListener('keyup', offer);
    document.addEventListener('keydown', onKeyDown);
    document.addEventListener('selectionchange', dropIfGone);
    return () => {
      document.removeEventListener('mouseup', offer);
      document.removeEventListener('keyup', offer);
      document.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('selectionchange', dropIfGone);
    };
  }, [sheetOpen, capture]);

  const split = capture ? splitSelection(capture.text) : null;

  const submit = (input: RememberInput) => {
    void onRemember(input);
    setSheetOpen(false);
    setCapture(null);
  };

  return (
    <>
      {capture && !sheetOpen && (
        <button
          type="button"
          aria-label="Capture selection as memory"
          onClick={() => setSheetOpen(true)}
          style={{ top: capture.top, left: capture.left, zIndex: CAPTURE_CHIP_Z }}
          className="fixed flex items-center gap-1.5 px-2.5 py-1.5 rounded-md bg-gray-900 text-white text-xs font-medium shadow-lg hover:bg-gray-800 transition-colors"
        >
          <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
          </svg>
          Memory
          <span className="text-gray-400">⌃M</span>
        </button>
      )}

      <RememberSheet
        open={sheetOpen}
        initialTitle={split?.title ?? ''}
        initialBody={split?.body ?? null}
        taskId={capture?.taskId ?? null}
        saving={saving}
        error={error}
        onClose={() => setSheetOpen(false)}
        onSubmit={submit}
      />
    </>
  );
}
