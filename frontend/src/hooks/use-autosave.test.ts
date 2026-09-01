import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import {
  DEFAULT_AUTOSAVE_DELAY_MS,
  useAutoSave,
  type AutoSaveStatus,
} from './use-autosave';

// ── Helpers ───────────────────────────────────────────────────────────────────
//
// The hook no longer closes over a `save` callback: each trigger hands it a job
// bound to the values (and the task) it was made for, so a write can no longer
// be re-bound to whatever task happens to be current when the timer fires.

interface Props {
  readonly options?: { readonly delayMs?: number };
}

type View = ReturnType<typeof renderAutoSave>;

function renderAutoSave(initialProps: Props = {}) {
  return renderHook(({ options }: Props) => useAutoSave(options), {
    initialProps,
  });
}

function statusOf(view: View): AutoSaveStatus {
  return view.result.current.status;
}

/** A job that issued at least one write. */
function wrote() {
  return vi.fn(async () => true);
}

/** A job that found nothing to write. */
function nothing() {
  return vi.fn(async () => false);
}

/** A job whose write failed. */
function failed() {
  return vi.fn(async (): Promise<boolean> => {
    throw new Error('boom');
  });
}

/** Move the fake clock, letting the promises each timer starts settle. */
async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

/** Drain queued microtasks without moving the clock. */
async function settle() {
  await act(async () => {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
  });
}

interface Deferred {
  readonly promise: Promise<boolean>;
  readonly resolve: (wrote: boolean) => void;
  readonly reject: (reason: unknown) => void;
}

function deferred(): Deferred {
  let resolve!: (wrote: boolean) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<boolean>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

// A rejected job must be swallowed by the hook and surfaced as a status. If it
// escapes, node reports an unhandled rejection — collect them so the assertion
// names the real problem instead of failing somewhere else in the file.
const unhandled: unknown[] = [];
const collectUnhandled = (reason: unknown) => {
  unhandled.push(reason);
};

beforeEach(() => {
  vi.useFakeTimers();
  unhandled.length = 0;
  process.on('unhandledRejection', collectUnhandled);
});

afterEach(() => {
  process.off('unhandledRejection', collectUnhandled);
  vi.useRealTimers();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('useAutoSave — defaults', () => {
  it('debounces on 700 ms unless told otherwise', () => {
    expect(DEFAULT_AUTOSAVE_DELAY_MS).toBe(700);
  });

  it('starts idle and runs nothing on mount', () => {
    const view = renderAutoSave();

    expect(statusOf(view)).toBe('idle');
  });

  it('stays idle when the clock runs and nothing was scheduled', async () => {
    const view = renderAutoSave();

    await advance(10 * DEFAULT_AUTOSAVE_DELAY_MS);

    expect(statusOf(view)).toBe('idle');
  });
});

describe('useAutoSave — scheduling', () => {
  it('goes pending on schedule() without running the job yet', () => {
    const job = wrote();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(job));

    expect(statusOf(view)).toBe('pending');
    expect(job).not.toHaveBeenCalled();
  });

  it('holds the job back until the delay has fully elapsed', async () => {
    const job = wrote();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(job));
    await advance(DEFAULT_AUTOSAVE_DELAY_MS - 1);

    expect(job).not.toHaveBeenCalled();
    expect(statusOf(view)).toBe('pending');
  });

  it('runs the job once past the delay and settles on saved', async () => {
    const job = wrote();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(job));
    await advance(DEFAULT_AUTOSAVE_DELAY_MS);

    expect(job).toHaveBeenCalledOnce();
    expect(statusOf(view)).toBe('saved');
  });

  it('honours a custom delayMs', async () => {
    const job = wrote();
    const view = renderAutoSave({ options: { delayMs: 50 } });

    act(() => view.result.current.schedule(job));
    await advance(49);
    expect(job).not.toHaveBeenCalled();

    await advance(1);
    expect(job).toHaveBeenCalledOnce();
  });

  it('reports saving while the job is in flight', async () => {
    const gate = deferred();
    const job = vi.fn(() => gate.promise);
    const view = renderAutoSave();

    act(() => view.result.current.schedule(job));
    await advance(DEFAULT_AUTOSAVE_DELAY_MS);

    expect(job).toHaveBeenCalledOnce();
    expect(statusOf(view)).toBe('saving');

    gate.resolve(true);
    await settle();

    expect(statusOf(view)).toBe('saved');
  });

  // The distinction that matters: a throttle would fire at 700 ms after the
  // FIRST call and again later; a debounce fires once, 700 ms after the LAST.
  it('debounces rather than throttles', async () => {
    const job = wrote();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(job));
    await advance(300);
    act(() => view.result.current.schedule(job));
    await advance(300);
    act(() => view.result.current.schedule(job));

    await advance(DEFAULT_AUTOSAVE_DELAY_MS - 1);
    expect(job).not.toHaveBeenCalled();

    await advance(1);
    expect(job).toHaveBeenCalledOnce();

    await advance(10 * DEFAULT_AUTOSAVE_DELAY_MS);
    expect(job).toHaveBeenCalledOnce();
  });

  it('keeps only the last scheduled job', async () => {
    const stale = wrote();
    const fresh = wrote();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(stale));
    await advance(200);
    act(() => view.result.current.schedule(fresh));
    await advance(DEFAULT_AUTOSAVE_DELAY_MS);

    expect(fresh).toHaveBeenCalledOnce();
    expect(stale).not.toHaveBeenCalled();
  });
});

// ── Status must not claim a save that never happened ──────────────────────────

describe('useAutoSave — a job that wrote nothing', () => {
  it('leaves the status idle rather than saved', async () => {
    const job = nothing();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(job));
    await advance(DEFAULT_AUTOSAVE_DELAY_MS);

    expect(job).toHaveBeenCalledOnce();
    expect(statusOf(view)).toBe('idle');
  });

  it('clears a previous saved status', async () => {
    // The ✓ belongs to a write that happened. A later no-op edit must not keep
    // borrowing it.
    const view = renderAutoSave();

    act(() => view.result.current.schedule(wrote()));
    await advance(DEFAULT_AUTOSAVE_DELAY_MS);
    expect(statusOf(view)).toBe('saved');

    act(() => view.result.current.schedule(nothing()));
    await advance(DEFAULT_AUTOSAVE_DELAY_MS);

    expect(statusOf(view)).toBe('idle');
  });

  it('still counts as a clean cycle for the caller', async () => {
    const view = renderAutoSave();

    let outcome: unknown;
    await act(async () => {
      outcome = await view.result.current.flushNow(nothing());
    });

    expect(outcome).toBe(true);
  });
});

describe('useAutoSave — flushNow', () => {
  it('runs the job it is given and drops the queued one', async () => {
    const queued = wrote();
    const explicit = wrote();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(queued));
    await act(async () => {
      await view.result.current.flushNow(explicit);
    });

    expect(explicit).toHaveBeenCalledOnce();
    expect(queued).not.toHaveBeenCalled();

    // The armed timer must be gone, not merely early.
    await advance(10 * DEFAULT_AUTOSAVE_DELAY_MS);
    expect(queued).not.toHaveBeenCalled();
    expect(explicit).toHaveBeenCalledOnce();
  });

  it('runs the queued job when called with no argument', async () => {
    const queued = wrote();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(queued));
    await act(async () => {
      await view.result.current.flushNow();
    });

    expect(queued).toHaveBeenCalledOnce();
  });

  it('empties the queue, so a later flushQueued has nothing left to run', async () => {
    const queued = wrote();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(queued));
    await act(async () => {
      await view.result.current.flushNow();
    });
    await act(async () => {
      await view.result.current.flushQueued();
    });

    expect(queued).toHaveBeenCalledOnce();
  });

  // `handleClose` calls flushNow() with no argument. With nothing queued but a
  // cycle already in flight (a select or date wrote immediately a moment ago),
  // answering `true` would close the panel over a write that is still failing.
  it('reports the in-flight cycle outcome when it has nothing of its own to run', async () => {
    const gate = deferred();
    const view = renderAutoSave();

    let closeOutcome: unknown;
    await act(async () => {
      const inFlight = view.result.current.flushNow(vi.fn(() => gate.promise));
      const closing = view.result.current.flushNow();
      gate.reject(new Error('boom'));
      [, closeOutcome] = await Promise.all([inFlight, closing]);
    });

    expect(closeOutcome).toBe(false);
    expect(statusOf(view)).toBe('error');
  });

  it('resolves true after a clean cycle and false after a failing one', async () => {
    const view = renderAutoSave();

    let clean: unknown;
    let broken: unknown;
    await act(async () => {
      clean = await view.result.current.flushNow(wrote());
    });
    await act(async () => {
      broken = await view.result.current.flushNow(failed());
    });

    expect(clean).toBe(true);
    expect(broken).toBe(false);
    expect(statusOf(view)).toBe('error');
  });

  it('resolves true again once a retry succeeds', async () => {
    const view = renderAutoSave();

    let retried: unknown;
    await act(async () => {
      await view.result.current.flushNow(failed());
    });
    await act(async () => {
      retried = await view.result.current.flushNow(wrote());
    });

    expect(retried).toBe(true);
    expect(statusOf(view)).toBe('saved');
  });

  it('resolves only once the job has completed', async () => {
    const gate = deferred();
    const job = vi.fn(() => gate.promise);
    const view = renderAutoSave();

    let settled = false;
    let flushed!: Promise<void>;
    // The status flip to 'saving' happens as flushNow() is entered, so start it
    // inside act(); it is deliberately not awaited here.
    await act(async () => {
      flushed = view.result.current.flushNow(job).then(() => {
        settled = true;
      });
      await Promise.resolve();
    });

    expect(job).toHaveBeenCalledOnce();
    expect(settled).toBe(false);

    gate.resolve(true);
    await act(async () => {
      await flushed;
    });

    expect(settled).toBe(true);
    expect(statusOf(view)).toBe('saved');
  });

  // Callers decide whether to close (or switch task) on what THEIR flush did. A
  // coalesced caller handed the in-flight cycle's verdict would act on someone
  // else's success.
  it('resolves the coalesced flush with the trailing cycle outcome', async () => {
    const gate = deferred();
    const first = vi.fn(() => gate.promise);
    const second = failed();
    const view = renderAutoSave();

    let firstOutcome: unknown;
    let secondOutcome: unknown;
    await act(async () => {
      const inFlight = view.result.current.flushNow(first);
      const trailing = view.result.current.flushNow(second);
      gate.resolve(true);
      [firstOutcome, secondOutcome] = await Promise.all([inFlight, trailing]);
    });

    expect(first).toHaveBeenCalledOnce();
    expect(second).toHaveBeenCalledOnce();
    expect(firstOutcome).toBe(true);
    expect(secondOutcome).toBe(false);
    expect(statusOf(view)).toBe('error');
  });
});

// ── flushQueued: the task-switch path ─────────────────────────────────────────

describe('useAutoSave — flushQueued', () => {
  it('runs the queued job before its timer was due', async () => {
    const queued = wrote();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(queued));
    await advance(100);
    await act(async () => {
      await view.result.current.flushQueued();
    });

    expect(queued).toHaveBeenCalledOnce();

    await advance(10 * DEFAULT_AUTOSAVE_DELAY_MS);
    expect(queued).toHaveBeenCalledOnce();
  });

  it('resolves true without running anything when the queue is empty', async () => {
    const view = renderAutoSave();

    let outcome: unknown;
    await act(async () => {
      outcome = await view.result.current.flushQueued();
    });

    expect(outcome).toBe(true);
    expect(statusOf(view)).toBe('idle');
  });

  it('does not touch the status when the queue is empty', async () => {
    // A switch away from a task with nothing pending must not wipe the ⚠ of the
    // write that failed a moment ago, nor invent a ✓.
    const view = renderAutoSave();

    await act(async () => {
      await view.result.current.flushNow(failed());
    });
    expect(statusOf(view)).toBe('error');

    await act(async () => {
      await view.result.current.flushQueued();
    });

    expect(statusOf(view)).toBe('error');
  });

  it('reports the queued job failure to the caller', async () => {
    const view = renderAutoSave();

    act(() => view.result.current.schedule(failed()));

    let outcome: unknown;
    await act(async () => {
      outcome = await view.result.current.flushQueued();
    });

    expect(outcome).toBe(false);
    expect(statusOf(view)).toBe('error');
  });
});

describe('useAutoSave — failure', () => {
  it('surfaces a rejected job as an error status', async () => {
    const job = failed();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(job));
    await advance(DEFAULT_AUTOSAVE_DELAY_MS);

    expect(job).toHaveBeenCalledOnce();
    expect(statusOf(view)).toBe('error');
  });

  it('does not let the rejection escape the hook', async () => {
    const view = renderAutoSave();

    act(() => view.result.current.schedule(failed()));
    await advance(DEFAULT_AUTOSAVE_DELAY_MS);
    await settle();

    expect(unhandled).toEqual([]);

    // flushNow() is awaited by callers (close, task switch): it must resolve,
    // not reject, or every one of them needs its own try/catch.
    let outcome = 'never settled';
    await act(async () => {
      outcome = await view.result.current
        .flushNow(failed())
        .then(() => 'resolved', () => 'rejected');
    });
    expect(outcome).toBe('resolved');
    expect(statusOf(view)).toBe('error');
  });

  it('retries on a fresh schedule after a failure', async () => {
    const view = renderAutoSave();

    act(() => view.result.current.schedule(failed()));
    await advance(DEFAULT_AUTOSAVE_DELAY_MS);
    expect(statusOf(view)).toBe('error');

    act(() => view.result.current.schedule(wrote()));
    expect(statusOf(view)).toBe('pending');

    await advance(DEFAULT_AUTOSAVE_DELAY_MS);
    expect(statusOf(view)).toBe('saved');
  });
});

describe('useAutoSave — reset', () => {
  it('cancels an armed job and returns to idle', async () => {
    const job = wrote();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(job));
    act(() => view.result.current.reset());

    expect(statusOf(view)).toBe('idle');

    await advance(10 * DEFAULT_AUTOSAVE_DELAY_MS);
    expect(job).not.toHaveBeenCalled();
    expect(statusOf(view)).toBe('idle');
  });

  it('drops the queued job, so a later flushQueued does not resurrect it', async () => {
    // Hydrating a new task calls reset(); the outgoing task's write must already
    // have been flushed by then, and must not fire later against the new one.
    const job = wrote();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(job));
    act(() => view.result.current.reset());

    let outcome: unknown;
    await act(async () => {
      outcome = await view.result.current.flushQueued();
    });

    expect(job).not.toHaveBeenCalled();
    expect(outcome).toBe(true);
  });

  it('clears a saved status', async () => {
    const view = renderAutoSave();

    act(() => view.result.current.schedule(wrote()));
    await advance(DEFAULT_AUTOSAVE_DELAY_MS);
    expect(statusOf(view)).toBe('saved');

    act(() => view.result.current.reset());
    expect(statusOf(view)).toBe('idle');
  });

  it('clears an error status', async () => {
    const view = renderAutoSave();

    act(() => view.result.current.schedule(failed()));
    await advance(DEFAULT_AUTOSAVE_DELAY_MS);
    expect(statusOf(view)).toBe('error');

    act(() => view.result.current.reset());
    expect(statusOf(view)).toBe('idle');
  });
});

describe('useAutoSave — unmount', () => {
  it('drops an armed job instead of firing it after teardown', async () => {
    const job = wrote();
    const view = renderAutoSave();

    act(() => view.result.current.schedule(job));
    view.unmount();

    await advance(10 * DEFAULT_AUTOSAVE_DELAY_MS);
    expect(job).not.toHaveBeenCalled();
  });
});

describe('useAutoSave — identity', () => {
  it('keeps every returned function referentially stable across re-renders', () => {
    const view = renderAutoSave();
    const first = view.result.current;

    view.rerender({});
    view.rerender({ options: { delayMs: 900 } });
    const second = view.result.current;

    expect(second.schedule).toBe(first.schedule);
    expect(second.flushNow).toBe(first.flushNow);
    expect(second.flushQueued).toBe(first.flushQueued);
    expect(second.reset).toBe(first.reset);
  });
});
