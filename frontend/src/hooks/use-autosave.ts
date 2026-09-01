import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';

/** Long enough to swallow a burst of keystrokes, short enough to feel immediate. */
export const DEFAULT_AUTOSAVE_DELAY_MS = 700;

export type AutoSaveStatus = 'idle' | 'pending' | 'saving' | 'saved' | 'error';

/**
 * One write, carrying its own payload.
 *
 * Resolves true when it issued at least one write, false when there was nothing
 * to write; a rejection means the write failed.
 */
export type SaveJob = () => Promise<boolean>;

/**
 * A queued write and the entity it was captured for.
 *
 * The tag is what keeps a requeued failure honest: a cycle that fails *after* the
 * panel has moved on leaves its job in the queue, and an untagged entry would let
 * the next "Réessayer" — or close — replay the previous entity's write under the
 * new one's chrome. `null` means the caller gave no owner, which only the hook's
 * own tests do. The hook never looks an owner up; callers hand it in.
 */
interface QueuedJob {
  readonly ownerId: string | null;
  readonly job: SaveJob;
}

export interface UseAutoSaveResult {
  readonly status: AutoSaveStatus;
  readonly schedule: (job: SaveJob, ownerId?: string | null) => void;
  /** Resolves true when the cycle settled clean (including "nothing to write"). */
  readonly flushNow: (job?: SaveJob, ownerId?: string | null) => Promise<boolean>;
  /** Drains the debounce, if anything is queued for `ownerId`. Same outcome contract. */
  readonly flushQueued: (ownerId?: string | null) => Promise<boolean>;
  readonly reset: () => void;
}

/**
 * Debounced write-behind for a form that has no Save button.
 *
 * `schedule(job)` arms a debounce on that exact job; `flushNow(job)` writes right
 * away and reports whether the write succeeded, so a caller that closes the form
 * can refuse to close on a failure. The queue holds the *payload*, not a ref to
 * the freshest closure: a write must land on the state it was typed on even if
 * the panel has moved to another task by the time the timer fires.
 *
 * Overlap policy: a cycle is never re-entered — callers arriving while one is in
 * flight all coalesce onto a single trailing cycle that starts when it settles,
 * and each gets that trailing cycle's own outcome, so a burst of immediate
 * triggers costs two writes, not one per trigger.
 */
export function useAutoSave(options?: { readonly delayMs?: number }): UseAutoSaveResult {
  const [status, setStatus] = useState<AutoSaveStatus>('idle');

  // `schedule`/`flushNow` have to stay referentially stable because callers wrap
  // field setters with them, so a changed delay travels through a ref.
  const delayMs = options?.delayMs ?? DEFAULT_AUTOSAVE_DELAY_MS;
  const delayRef = useRef(delayMs);

  // Committed values only: a render React throws away must not leave its delay
  // behind in a ref that an armed timer would then use.
  useLayoutEffect(() => {
    delayRef.current = delayMs;
  }, [delayMs]);

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingRef = useRef<QueuedJob | null>(null);
  const inFlightRef = useRef<Promise<boolean> | null>(null);
  const trailingRef = useRef<Promise<boolean> | null>(null);
  const trailingJobRef = useRef<QueuedJob | null>(null);

  // Bumped by every reset. A cycle that fails across one must not requeue into a
  // form that has been re-keyed in the meantime: the reset wins. Ordering it this
  // way rather than delaying the reset until the drain settles matters — a delayed
  // reset would cancel an edit made in between.
  const generationRef = useRef(0);

  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  /**
   * Hands over the queued entry, if it still belongs to the caller.
   *
   * Only a *definite* mismatch — two known, different owners — refuses, so an
   * untagged call can still drain (the hydration path deliberately drains the
   * outgoing entity's entry). Either way the queue comes out empty.
   */
  const takeQueued = useCallback((ownerId?: string | null): QueuedJob | null => {
    const entry = pendingRef.current;
    pendingRef.current = null;
    if (entry === null) return null;
    if (entry.ownerId !== null && typeof ownerId === 'string' && ownerId !== entry.ownerId) {
      return null;
    }
    return entry;
  }, []);

  const runCycle = useCallback(async (entry: QueuedJob): Promise<boolean> => {
    const generation = generationRef.current;
    setStatus('saving');
    try {
      const wrote = await entry.job();
      // A job with nothing to send must not claim "✓ Enregistré": there is no new
      // state on the server to confirm, so the footer goes quiet instead of lying.
      setStatus(wrote ? 'saved' : 'idle');
      return true;
    } catch {
      // A failed write goes back in the queue, still tagged: "Réessayer" and a
      // close both flush the queue, and that is the only copy of the edit that
      // never reached the server. A job queued since supersedes it — snapshots
      // are cumulative.
      if (pendingRef.current === null && generationRef.current === generation) {
        pendingRef.current = entry;
      }
      // Without a Save button, a rejection has nowhere to surface but the status,
      // and it must not escape as an unhandled rejection.
      setStatus('error');
      return false;
    }
  }, []);

  const startCycle = useCallback((entry: QueuedJob): Promise<boolean> => {
    const cycle = runCycle(entry).finally(() => {
      inFlightRef.current = null;
    });
    inFlightRef.current = cycle;
    return cycle;
  }, [runCycle]);

  const run = useCallback((entry: QueuedJob): Promise<boolean> => {
    const inFlight = inFlightRef.current;
    if (inFlight === null) return startCycle(entry);

    // The last job wins: every snapshot is cumulative, so the newest one already
    // carries what the ones it replaced were going to write.
    trailingJobRef.current = entry;
    if (trailingRef.current === null) {
      trailingRef.current = inFlight.then(() => {
        trailingRef.current = null;
        const trailing = trailingJobRef.current;
        trailingJobRef.current = null;
        return trailing !== null ? startCycle(trailing) : true;
      });
    }
    return trailingRef.current;
  }, [startCycle]);

  const flushNow = useCallback((job?: SaveJob, ownerId?: string | null): Promise<boolean> => {
    clearTimer();
    // Dropping the queued job is correct: the caller's snapshot is cumulative, so
    // an immediate job already contains whatever the debounced one would send.
    const queued = takeQueued(ownerId);
    if (job) return run({ ownerId: ownerId ?? null, job });
    if (queued) return run(queued);
    // Nothing of our own to write, but a write may already be in the air: a caller
    // about to close or switch away has to wait for its verdict, or it would act
    // on a success that has not happened yet. Deliberate, not incidental — an
    // immediate write that fails must still keep the panel open.
    const settling = trailingRef.current ?? inFlightRef.current;
    // Truly nothing pending: no cycle, and no status of its own — the footer keeps
    // describing the last real write.
    return settling ?? Promise.resolve(true);
  }, [clearTimer, run, takeQueued]);

  const flushQueued = useCallback(
    (ownerId?: string | null): Promise<boolean> => flushNow(undefined, ownerId),
    [flushNow],
  );

  const schedule = useCallback((job: SaveJob, ownerId?: string | null) => {
    clearTimer();
    pendingRef.current = { ownerId: ownerId ?? null, job };
    setStatus('pending');
    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      const queued = pendingRef.current;
      pendingRef.current = null;
      if (queued) void run(queued);
    }, delayRef.current);
  }, [clearTimer, run]);

  /** Drops a pending write and forgets the last outcome; an in-flight one still lands. */
  const reset = useCallback(() => {
    clearTimer();
    generationRef.current += 1;
    pendingRef.current = null;
    setStatus('idle');
  }, [clearTimer]);

  // React cannot await an unmount, so we only disarm — the caller flushes on close.
  useEffect(() => clearTimer, [clearTimer]);

  return { status, schedule, flushNow, flushQueued, reset };
}
