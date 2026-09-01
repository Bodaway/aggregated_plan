import { useState, useEffect, useCallback, useRef } from 'react';
import { useTaskEdit, type FullTask } from '@/hooks/use-task-edit';
import { useAutoSave, type SaveJob } from '@/hooks/use-autosave';
import { useDelegates } from '@/hooks/use-delegates';
import { MarkdownEditor } from '@/components/markdown/MarkdownEditor';
import { WorklogSection } from '@/components/worklog/WorklogSection';
import { GryzzlyTaskPicker } from '@/components/gryzzly/GryzzlyTaskPicker';

interface TaskEditSheetProps {
  readonly taskId: string | null;
  readonly onClose: () => void;
  readonly onUpdated?: () => void;
  /** Hands the owner a way to drain the debounce *before* it re-keys `taskId`:
   *  a switch that just dropped the queued write would lose it silently. */
  readonly registerPendingFlush?: (flush: (() => Promise<boolean>) | null) => void;
}

const URGENCY_OPTIONS = [
  { value: 'LOW', label: 'Low' },
  { value: 'MEDIUM', label: 'Medium' },
  { value: 'HIGH', label: 'High' },
  { value: 'CRITICAL', label: 'Critical' },
] as const;

const IMPACT_OPTIONS = [
  { value: 'LOW', label: 'Low' },
  { value: 'MEDIUM', label: 'Medium' },
  { value: 'HIGH', label: 'High' },
  { value: 'CRITICAL', label: 'Critical' },
] as const;

const STATUS_OPTIONS = [
  { value: 'TODO', label: 'To Do' },
  { value: 'IN_PROGRESS', label: 'In Progress' },
  { value: 'DONE', label: 'Done' },
  { value: 'BLOCKED', label: 'Blocked' },
] as const;

const STATUS_STYLES: Record<string, string> = {
  TODO: 'bg-gray-100 text-gray-700 border-gray-300',
  IN_PROGRESS: 'bg-blue-50 text-blue-700 border-blue-300',
  DONE: 'bg-green-50 text-green-700 border-green-300',
  BLOCKED: 'bg-red-50 text-red-700 border-red-300',
};

/** Upstream systems that own the deadline of the tasks they feed (R76). */
const SOURCE_LABELS: Record<string, string> = {
  JIRA: 'Jira',
  EXCEL: 'Excel',
  OUTLOOK: 'Outlook',
  OBSIDIAN: 'Obsidian',
  GRYZZLY: 'Gryzzly',
};

/** GraphQL returns urgency/impact as enum strings (LOW, MEDIUM, HIGH, CRITICAL). */
function normalizeEnum(val: string): string {
  const upper = String(val).toUpperCase();
  if (['LOW', 'MEDIUM', 'HIGH', 'CRITICAL'].includes(upper)) return upper;
  return 'MEDIUM';
}

/** The editable surface of the panel, in the string shape the inputs use. */
interface FormSnapshot {
  readonly description: string;
  readonly notes: string;
  readonly estimatedHours: string;
  readonly remainingOverride: string;
  readonly estimatedOverride: string;
  readonly urgency: string;
  readonly impact: string;
  readonly status: string;
  readonly plannedDate: string;
  readonly deadline: string;
  readonly delegatedTo: string;
}

const EMPTY_FORM: FormSnapshot = {
  description: '',
  notes: '',
  estimatedHours: '',
  remainingOverride: '',
  estimatedOverride: '',
  urgency: 'MEDIUM',
  impact: 'MEDIUM',
  status: 'TODO',
  plannedDate: '',
  deadline: '',
  delegatedTo: '',
};

function snapshotOf(task: FullTask): FormSnapshot {
  return {
    description: task.description ?? '',
    notes: task.notes ?? '',
    estimatedHours: task.estimatedHours?.toString() ?? '',
    remainingOverride: task.remainingHoursOverride?.toString() ?? '',
    estimatedOverride: task.estimatedHoursOverride?.toString() ?? '',
    urgency: normalizeEnum(task.urgency),
    impact: normalizeEnum(task.impact),
    status: task.status ?? 'TODO',
    // Extract date portion from ISO datetime; deadline is a plain date, sliced
    // defensively in case the server widens it.
    plannedDate: task.plannedStart ? task.plannedStart.slice(0, 10) : '',
    deadline: task.deadline ? task.deadline.slice(0, 10) : '',
    delegatedTo: task.delegatedTo ?? '',
  };
}

function parseHours(value: string): number | null {
  return value ? parseFloat(value) : null;
}

/** Every input key the three mutations accept. Spelled out so a rename breaks the
 *  map below at compile time instead of silently leaving a field dirty forever. */
type MutationInputKey =
  | 'status'
  | 'notes'
  | 'plannedStart'
  | 'deadline'
  | 'delegatedTo'
  | 'remainingHoursOverride'
  | 'estimatedHoursOverride'
  | 'description'
  | 'estimatedHours'
  | 'urgency'
  | 'impact';

type MutationInput = Partial<Record<MutationInputKey, unknown>>;

/** Mutation input key → the form field it carries, so a partially failed save
 *  records exactly the fields that reached the server. */
const SAVED_FIELD_BY_INPUT = {
  status: 'status',
  notes: 'notes',
  plannedStart: 'plannedDate',
  deadline: 'deadline',
  delegatedTo: 'delegatedTo',
  remainingHoursOverride: 'remainingOverride',
  estimatedHoursOverride: 'estimatedOverride',
  description: 'description',
  estimatedHours: 'estimatedHours',
  urgency: 'urgency',
  impact: 'impact',
} satisfies Record<MutationInputKey, keyof FormSnapshot>;

function withSaved(
  base: FormSnapshot,
  next: FormSnapshot,
  changes: MutationInput,
): FormSnapshot {
  const patch: Record<string, string> = {};
  for (const inputKey of Object.keys(changes) as readonly MutationInputKey[]) {
    const field = SAVED_FIELD_BY_INPUT[inputKey];
    patch[field] = next[field];
  }
  return { ...base, ...patch };
}

/** How one diff reaches the server: priority has its own mutation, and a recurring
 *  instance splits between its own occurrence and the series template. */
interface Change {
  readonly kind: 'priority' | 'perInstance' | 'template';
  readonly input: MutationInput;
}

/** Everything outside the two snapshots that decides the routing — captured with
 *  them, so a queued write never re-reads a task that has since been replaced. */
interface EditFlags {
  readonly isJira: boolean;
  readonly isRecurring: boolean;
  readonly canEditDeadline: boolean;
  readonly recurrenceId: string | null;
}

/** Pure diff: what the form shows minus what the server is known to hold. */
function plan(base: FormSnapshot, next: FormSnapshot, flags: EditFlags): readonly Change[] {
  const { isJira, isRecurring, canEditDeadline, recurrenceId } = flags;

  // Per-instance fields — safe to send via updateTask for both recurring and one-shot.
  const perInstanceChanges: MutationInput = {};
  // Template fields — for recurring instances, must go through updateRecurringTask.
  const templateChanges: MutationInput = {};

  if (next.status !== base.status) {
    perInstanceChanges.status = next.status;
  }

  const newNotes = next.notes || null;
  if (newNotes !== (base.notes || null)) {
    perInstanceChanges.notes = newNotes;
  }

  // Planned date is per-instance
  if (next.plannedDate !== base.plannedDate) {
    perInstanceChanges.plannedStart = next.plannedDate ? `${next.plannedDate}T08:00:00Z` : null;
  }

  // Deadline is per-instance, like the planned date, and only ever sent for
  // personal tasks (R76). Empty string means "clear it" — an explicit null.
  if (canEditDeadline && next.deadline !== base.deadline) {
    perInstanceChanges.deadline = next.deadline || null;
  }

  const newDelegate = next.delegatedTo.trim() || null;
  if (newDelegate !== (base.delegatedTo.trim() || null)) {
    perInstanceChanges.delegatedTo = newDelegate;
  }

  if (isJira) {
    const newRemaining = parseHours(next.remainingOverride);
    if (newRemaining !== parseHours(base.remainingOverride)) {
      perInstanceChanges.remainingHoursOverride = newRemaining;
    }
    const newEstOverride = parseHours(next.estimatedOverride);
    if (newEstOverride !== parseHours(base.estimatedOverride)) {
      perInstanceChanges.estimatedHoursOverride = newEstOverride;
    }
  }

  // Template-level fields — description, urgency/impact, estimated hours
  const newDesc = next.description || null;
  if (newDesc !== (base.description || null)) {
    if (isRecurring) {
      templateChanges.description = newDesc;
    } else {
      perInstanceChanges.description = newDesc;
    }
  }

  if (!isJira) {
    const newEst = parseHours(next.estimatedHours);
    if (newEst !== parseHours(base.estimatedHours)) {
      if (isRecurring) {
        templateChanges.estimatedHours = newEst;
      } else {
        perInstanceChanges.estimatedHours = newEst;
      }
    }
  }

  const urgencyChanged = next.urgency !== base.urgency;
  const impactChanged = next.impact !== base.impact;
  if (isRecurring) {
    if (urgencyChanged) templateChanges.urgency = next.urgency;
    if (impactChanged) templateChanges.impact = next.impact;
  }

  const changes: Change[] = [];
  if (!isRecurring && (urgencyChanged || impactChanged)) {
    changes.push({ kind: 'priority', input: { urgency: next.urgency, impact: next.impact } });
  }
  if (Object.keys(perInstanceChanges).length > 0) {
    changes.push({ kind: 'perInstance', input: perInstanceChanges });
  }
  if (isRecurring && recurrenceId && Object.keys(templateChanges).length > 0) {
    changes.push({ kind: 'template', input: templateChanges });
  }
  return changes;
}

function formatSeconds(seconds: number | null): string {
  if (seconds === null || seconds === undefined) return '-';
  const hours = seconds / 3600;
  if (hours < 1) return `${Math.round(seconds / 60)}m`;
  return `${hours.toFixed(1)}h`;
}

export function TaskEditSheet({ taskId, onClose, onUpdated, registerPendingFlush }: TaskEditSheetProps) {
  const { task, loading, updateTask, updatePriority, skipOccurrence, updateRecurringTask } = useTaskEdit(taskId);
  const { delegates } = useDelegates();
  const isOpen = taskId !== null;
  const isJira = task?.source === 'JIRA' || task?.source === 'EXCEL';
  const isRecurring = task?.isRecurring ?? false;
  // R76: sync.rs reassigns `deadline` unconditionally on every pass, so a manual
  // edit on a synced task would be destroyed at the next cycle. Personal tasks only.
  const canEditDeadline = task?.source === 'PERSONAL';

  // Local form state
  const [description, setDescription] = useState('');
  const [notes, setNotes] = useState('');
  const [estimatedHours, setEstimatedHours] = useState('');
  const [remainingOverride, setRemainingOverride] = useState('');
  const [estimatedOverride, setEstimatedOverride] = useState('');
  const [urgency, setUrgency] = useState('MEDIUM');
  const [impact, setImpact] = useState('MEDIUM');
  const [status, setStatus] = useState('TODO');
  const [plannedDate, setPlannedDate] = useState('');
  const [deadline, setDeadline] = useState('');
  const [delegatedTo, setDelegatedTo] = useState('');
  const [titleCopied, setTitleCopied] = useState(false);
  const [skipFailed, setSkipFailed] = useState(false);

  // A trigger fires in the same tick as the setState that caused it, so the diff
  // reads this synchronous mirror of the form instead of the pending state.
  const formRef = useRef<FormSnapshot>(EMPTY_FORM);
  // What the server is known to hold. Every mutation triggers a network-only
  // refetch, so `task` lags behind for a moment and diffing against it would
  // resend fields — or, worse, re-hydrate over what is being typed.
  const lastSavedRef = useRef<FormSnapshot | null>(null);
  const hydratedForIdRef = useRef<string | null>(null);

  // Declared before the hydration effect so it always runs first in a commit:
  // reopening the same task has to hydrate again, a refetch must not.
  useEffect(() => {
    hydratedForIdRef.current = null;
  }, [taskId]);

  const dispatch = useCallback(async (
    id: string,
    base: FormSnapshot,
    next: FormSnapshot,
    flags: EditFlags,
    onSaved?: (saved: FormSnapshot) => void,
  ): Promise<boolean> => {
    const changes = plan(base, next, flags);
    if (changes.length === 0) return false;

    // Up to three mutations, and the second can fail after the first landed, so the
    // baseline advances per successful call: what landed is never resent, what
    // failed is never recorded as saved.
    let saved = base;
    let wrote = false;
    // A throw aborts the remaining mutations on purpose: firing them anyway would
    // half-apply a change whose shape the user cannot see, and the queued retry
    // replays whatever is still dirty.
    try {
      for (const change of changes) {
        if (change.kind === 'priority') {
          await updatePriority(id, next.urgency, next.impact);
        } else if (change.kind === 'perInstance') {
          await updateTask(id, change.input);
        } else if (flags.recurrenceId) {
          await updateRecurringTask(flags.recurrenceId, change.input);
        } else {
          // `plan` never emits a template change without a series id.
          continue;
        }
        wrote = true;
        saved = withSaved(saved, next, change.input);
        // Reported before the next await can throw, so a retry of this same job
        // resends only what failed (R77).
        onSaved?.(saved);
        // This can resolve after the panel has moved on: patching another task's
        // baseline with these values would make its next diff resend them, and on
        // a recurring task that rewrites the whole series template.
        if (hydratedForIdRef.current === id) lastSavedRef.current = saved;
      }
    } finally {
      // At least one mutation landed — refresh the searchable list for that part
      // even when a later one threw.
      if (wrote) onUpdated?.();
    }
    return wrote;
  }, [updateTask, updatePriority, updateRecurringTask, onUpdated]);

  const { status: autoSaveStatus, schedule, flushNow, flushQueued, reset } = useAutoSave();

  // Freeze everything a write needs at the moment the user triggers it — the only
  // moment `taskId`, `task` and the refs unambiguously describe the same task. The
  // job that comes out reads nothing live, so a switch to another task can no
  // longer redirect a pending write onto whatever the panel now shows.
  const capture = useCallback((patch: Partial<FormSnapshot>): SaveJob | undefined => {
    formRef.current = { ...formRef.current, ...patch };
    const id = taskId;
    const base = lastSavedRef.current;
    if (!id || !base || hydratedForIdRef.current !== id) return undefined;
    // `task` carries the routing (Jira? recurring? which series?). urql v4 nulls
    // `data` on a re-keyed query and the client uses the document cache, so it
    // cannot be another task's today — the id check guards a future graphcache.
    if (!task || task.id !== id) return undefined;
    const next = formRef.current;
    const flags: EditFlags = { isJira, isRecurring, canEditDeadline, recurrenceId: task.recurrenceId };
    // The job's own cursor over what it has written, and its fallback baseline once
    // the panel has moved on. Otherwise the live baseline wins at job start: it
    // advances only on a *successful* write, so re-reading it drops fields the
    // server already holds at the same value — no duplicate when a second edit
    // lands inside the first's round trip — while a field whose captured value
    // differs still diffs dirty, which is what keeps the retry replaying it (R77).
    let sent = base;
    return () => dispatch(
      id,
      hydratedForIdRef.current === id ? (lastSavedRef.current ?? sent) : sent,
      next,
      flags,
      (saved) => { sent = saved; },
    );
  }, [taskId, task, isJira, isRecurring, canEditDeadline, dispatch]);

  // A finished choice (select, date) writes straight away; anything typed
  // debounces, or every keystroke would be a mutation.
  const commit = useCallback((patch: Partial<FormSnapshot>) => {
    const job = capture(patch);
    if (job) void flushNow(job, taskId);
  }, [capture, flushNow, taskId]);

  const draft = useCallback((patch: Partial<FormSnapshot>) => {
    const job = capture(patch);
    if (job) schedule(job, taskId);
  }, [capture, schedule, taskId]);

  const editStatus = useCallback((value: string) => { setStatus(value); commit({ status: value }); }, [commit]);
  const editUrgency = useCallback((value: string) => { setUrgency(value); commit({ urgency: value }); }, [commit]);
  const editImpact = useCallback((value: string) => { setImpact(value); commit({ impact: value }); }, [commit]);
  const editPlannedDate = useCallback((value: string) => { setPlannedDate(value); commit({ plannedDate: value }); }, [commit]);
  const editDeadline = useCallback((value: string) => { setDeadline(value); commit({ deadline: value }); }, [commit]);
  const editDelegatedTo = useCallback((value: string) => { setDelegatedTo(value); draft({ delegatedTo: value }); }, [draft]);
  const editDescription = useCallback((value: string) => { setDescription(value); draft({ description: value }); }, [draft]);
  const editNotes = useCallback((value: string) => { setNotes(value); draft({ notes: value }); }, [draft]);
  const editEstimatedHours = useCallback((value: string) => { setEstimatedHours(value); draft({ estimatedHours: value }); }, [draft]);
  const editRemainingOverride = useCallback((value: string) => { setRemainingOverride(value); draft({ remainingOverride: value }); }, [draft]);
  const editEstimatedOverride = useCallback((value: string) => { setEstimatedOverride(value); draft({ estimatedOverride: value }); }, [draft]);

  // Hydrate on task identity only: a refetch hands us a new `task` object for the
  // same task, and re-running here would overwrite the ongoing edit.
  useEffect(() => {
    if (!task || task.id !== taskId || hydratedForIdRef.current === task.id) return;
    // Defence in depth: the owner drains the queue before re-keying `taskId`, so
    // this is normally a no-op — but any other path that re-keys us would leave a
    // queued write whose baseline is about to be overwritten below. Untagged on
    // purpose: the entry belongs to the task being replaced, not to this one.
    // The drain is async, so the `reset` below can settle first — the hook makes
    // the reset win over a late requeue rather than us delaying it, which would
    // cancel anything typed in the meantime.
    void flushQueued();
    const snapshot = snapshotOf(task);
    setDescription(snapshot.description);
    setNotes(snapshot.notes);
    setEstimatedHours(snapshot.estimatedHours);
    setRemainingOverride(snapshot.remainingOverride);
    setEstimatedOverride(snapshot.estimatedOverride);
    setUrgency(snapshot.urgency);
    setImpact(snapshot.impact);
    setStatus(snapshot.status);
    setPlannedDate(snapshot.plannedDate);
    setDeadline(snapshot.deadline);
    setDelegatedTo(snapshot.delegatedTo);
    formRef.current = snapshot;
    lastSavedRef.current = snapshot;
    hydratedForIdRef.current = task.id;
    reset();
  }, [task, taskId, reset, flushQueued]);

  // Closing on a failed write would take the edit with it, so a failure keeps the
  // panel open on its ⚠ / Réessayer footer instead.
  const handleClose = useCallback(async () => {
    if (await flushQueued(taskId)) onClose();
  }, [flushQueued, taskId, onClose]);

  // The sheet outlives a task switch, so the owner needs the drain handle for as
  // long as it is mounted.
  useEffect(() => {
    registerPendingFlush?.(flushQueued);
    return () => registerPendingFlush?.(null);
  }, [registerPendingFlush, flushQueued]);

  // Copy-title confirmation: reset when the panel switches task, and on unmount.
  const copyResetRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    setTitleCopied(false);
    setSkipFailed(false);
    return () => {
      if (copyResetRef.current) clearTimeout(copyResetRef.current);
    };
  }, [taskId]);

  const handleCopyTitle = useCallback(async () => {
    const title = task?.title;
    if (!title) return;
    try {
      await navigator.clipboard.writeText(title);
    } catch {
      // Clipboard unavailable (insecure context) or permission denied — stay silent.
      return;
    }
    setTitleCopied(true);
    if (copyResetRef.current) clearTimeout(copyResetRef.current);
    copyResetRef.current = setTimeout(() => setTitleCopied(false), 1500);
  }, [task?.title]);

  const handleSkipOccurrence = useCallback(async () => {
    setSkipFailed(false);
    if (!taskId) return;
    // A deliberate close, like Fermer: a queued write must land first, and a failed
    // one keeps the panel open instead of taking the edit down with it.
    if (!(await flushQueued(taskId))) return;
    try {
      await skipOccurrence(taskId);
    } catch {
      // The occurrence is still there — keep the panel open and say so rather
      // than closing on a write that never landed.
      setSkipFailed(true);
      return;
    }
    onUpdated?.();
    onClose();
  }, [taskId, flushQueued, skipOccurrence, onUpdated, onClose]);

  // Close on Escape
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') void handleClose();
    };
    if (isOpen) {
      document.addEventListener('keydown', handleKeyDown);
      return () => document.removeEventListener('keydown', handleKeyDown);
    }
  }, [isOpen, handleClose]);

  return (
    <>
      {/* Backdrop */}
      {isOpen && (
        <div
          data-testid="task-sheet-backdrop"
          className="fixed inset-0 bg-black/20 z-40 transition-opacity"
          onClick={() => void handleClose()}
        />
      )}

      {/* Sheet panel */}
      <div
        className={`fixed top-0 right-0 h-full w-full max-w-2xl bg-white shadow-xl z-50 transform transition-transform duration-200 ease-in-out ${
          isOpen ? 'translate-x-0' : 'translate-x-full'
        }`}
      >
        {isOpen && (
          <div className="flex flex-col h-full">
            {/* Header */}
            <div className="flex items-center justify-between px-5 py-4 border-b border-gray-200">
              <div className="flex items-center gap-2 min-w-0">
                {task?.sourceId && (
                  <span className="text-xs font-mono font-medium text-blue-600 flex-shrink-0">
                    {task.sourceId}
                  </span>
                )}
                <h2 className="text-base font-semibold text-gray-900 truncate">
                  {task?.title ?? 'Loading...'}
                </h2>
                {task?.title && (
                  <button
                    type="button"
                    onClick={handleCopyTitle}
                    data-testid="task-sheet-copy-title"
                    aria-label={titleCopied ? 'Copied' : 'Copy title'}
                    title={titleCopied ? 'Copied' : 'Copy title'}
                    className={`p-1.5 flex-shrink-0 rounded-md transition-colors ${
                      titleCopied
                        ? 'text-green-600'
                        : 'text-gray-400 hover:text-gray-600 hover:bg-gray-100'
                    }`}
                  >
                    {titleCopied ? (
                      <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5} aria-hidden="true">
                        <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
                      </svg>
                    ) : (
                      <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5} aria-hidden="true">
                        <path strokeLinecap="round" strokeLinejoin="round" d="M15.666 3.888A2.25 2.25 0 0 0 13.5 2.25h-3c-1.03 0-1.9.693-2.166 1.638m7.332 0c.055.194.084.4.084.612v0a.75.75 0 0 1-.75.75H9a.75.75 0 0 1-.75-.75v0c0-.212.03-.418.084-.612m7.332 0c.646.049 1.288.11 1.927.184 1.1.128 1.907 1.077 1.907 2.185V19.5a2.25 2.25 0 0 1-2.25 2.25H6.75A2.25 2.25 0 0 1 4.5 19.5V6.257c0-1.108.806-2.057 1.907-2.185a48.208 48.208 0 0 1 1.927-.184" />
                      </svg>
                    )}
                  </button>
                )}
              </div>
              <button
                data-testid="task-sheet-header-close"
                onClick={() => void handleClose()}
                className="p-1.5 text-gray-400 hover:text-gray-600 rounded-md hover:bg-gray-100 transition-colors"
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>

            {/* Content */}
            <div className="flex-1 overflow-y-auto px-5 py-4 space-y-5">
              {loading && !task ? (
                <div className="flex items-center justify-center py-12">
                  <div className="w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
                </div>
              ) : task ? (
                <>
                  {/* Recurring task banner */}
                  {isRecurring && (
                    <div className="flex items-start gap-2 rounded-md bg-violet-50 border border-violet-200 px-3 py-2 text-sm text-violet-800">
                      <svg className="mt-0.5 w-4 h-4 flex-shrink-0 text-violet-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5} aria-hidden="true">
                        <path strokeLinecap="round" strokeLinejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0 3.181 3.183a8.25 8.25 0 0 0 13.803-3.7M4.031 9.865a8.25 8.25 0 0 1 13.803-3.7l3.181 3.182m0-4.991v4.99" />
                      </svg>
                      <span>
                        Cette tâche fait partie d&apos;une série. Le statut et les dates s&apos;appliquent à cette occurrence ; les autres champs s&apos;appliquent à toute la série.
                      </span>
                    </div>
                  )}

                  {/* Info section */}
                  <div className="space-y-2">
                    <div className="flex items-center gap-2 text-sm text-gray-600">
                      <span className="font-medium w-20">Status:</span>
                      <select
                        value={status}
                        onChange={(e) => editStatus(e.target.value)}
                        className={`rounded-md border px-2 py-0.5 text-xs font-medium focus:outline-none focus:ring-2 focus:ring-blue-500 ${STATUS_STYLES[status] ?? 'bg-gray-100 text-gray-700 border-gray-300'}`}
                      >
                        {STATUS_OPTIONS.map(o => (
                          <option key={o.value} value={o.value}>{o.label}</option>
                        ))}
                      </select>
                      {task.jiraStatus && (
                        <span className="px-2 py-0.5 bg-blue-50 text-blue-700 rounded text-xs font-medium border border-blue-200">
                          {task.jiraStatus}
                        </span>
                      )}
                    </div>
                    {task.assignee && (
                      <div className="flex items-center gap-2 text-sm text-gray-600">
                        <span className="font-medium w-20">Assignee:</span>
                        <span>{task.assignee}</span>
                      </div>
                    )}
                    {!canEditDeadline && task.deadline && (
                      <div className="flex items-start gap-2 text-sm text-gray-600">
                        <span className="font-medium w-20 flex-shrink-0">Échéance:</span>
                        <span>
                          {task.deadline}
                          <span className="ml-1.5 text-xs text-gray-400">
                            — définie par {SOURCE_LABELS[task.source] ?? task.source}, réécrite à chaque synchronisation
                          </span>
                        </span>
                      </div>
                    )}
                    {task.project?.name && (
                      <div className="flex items-center gap-2 text-sm text-gray-600">
                        <span className="font-medium w-20">Project:</span>
                        <span>{task.project.name}</span>
                      </div>
                    )}
                  </div>

                  {/* Jira time tracking (read-only display) */}
                  {isJira && (task.jiraOriginalEstimateSeconds !== null || task.jiraTimeSpentSeconds !== null || task.jiraRemainingSeconds !== null) && (
                    <div className="bg-blue-50 rounded-lg p-3 space-y-1.5">
                      <h4 className="text-xs font-semibold text-blue-800 uppercase tracking-wider">Jira Time Tracking</h4>
                      <div className="grid grid-cols-3 gap-2 text-center">
                        <div>
                          <p className="text-xs text-blue-600">Estimate</p>
                          <p className="text-sm font-medium text-blue-900">{formatSeconds(task.jiraOriginalEstimateSeconds)}</p>
                        </div>
                        <div>
                          <p className="text-xs text-blue-600">Logged</p>
                          <p className="text-sm font-medium text-blue-900">{formatSeconds(task.jiraTimeSpentSeconds)}</p>
                        </div>
                        <div>
                          <p className="text-xs text-blue-600">Remaining</p>
                          <p className="text-sm font-medium text-blue-900">{formatSeconds(task.jiraRemainingSeconds)}</p>
                        </div>
                      </div>
                    </div>
                  )}

                  {/* Editable fields */}
                  <div className="space-y-4">
                    <div>
                      <label className="block text-xs font-medium text-gray-700 mb-1">Planned Date</label>
                      <input
                        type="date"
                        value={plannedDate}
                        onChange={(e) => editPlannedDate(e.target.value)}
                        className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                      />
                      {plannedDate && (
                        <button
                          type="button"
                          onClick={() => editPlannedDate('')}
                          className="mt-1 text-xs text-gray-400 hover:text-red-500 transition-colors"
                        >
                          Clear planned date
                        </button>
                      )}
                    </div>

                    {canEditDeadline && (
                      <div>
                        <label htmlFor="task-deadline" className="block text-xs font-medium text-gray-700 mb-1">
                          Échéance
                        </label>
                        <input
                          id="task-deadline"
                          type="date"
                          value={deadline}
                          onChange={(e) => editDeadline(e.target.value)}
                          className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                        />
                        {deadline && (
                          <button
                            type="button"
                            onClick={() => editDeadline('')}
                            className="mt-1 text-xs text-gray-400 hover:text-red-500 transition-colors"
                          >
                            Effacer l&apos;échéance
                          </button>
                        )}
                      </div>
                    )}

                    <div>
                      <label htmlFor="task-delegated-to" className="block text-xs font-medium text-gray-700 mb-1">
                        Delegated to
                      </label>
                      <input
                        id="task-delegated-to"
                        type="text"
                        list="delegate-suggestions"
                        value={delegatedTo}
                        onChange={(e) => editDelegatedTo(e.target.value)}
                        placeholder="Nobody — type a name to delegate"
                        className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                      />
                      <datalist id="delegate-suggestions">
                        {delegates.map((name) => (
                          <option key={name} value={name} />
                        ))}
                      </datalist>
                    </div>

                    <div>
                      <label className="block text-xs font-medium text-gray-700 mb-1">
                        Gryzzly task
                      </label>
                      <GryzzlyTaskPicker
                        taskId={task.id}
                        assigned={task.gryzzlyTask ?? null}
                      />
                    </div>

                    <h4 className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Priority</h4>

                    <div className="grid grid-cols-2 gap-3">
                      <div>
                        <label className="block text-xs font-medium text-gray-700 mb-1">Urgency</label>
                        <select
                          value={urgency}
                          onChange={(e) => editUrgency(e.target.value)}
                          className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                        >
                          {URGENCY_OPTIONS.map(o => (
                            <option key={o.value} value={o.value}>{o.label}</option>
                          ))}
                        </select>
                      </div>
                      <div>
                        <label className="block text-xs font-medium text-gray-700 mb-1">Impact</label>
                        <select
                          value={impact}
                          onChange={(e) => editImpact(e.target.value)}
                          className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                        >
                          {IMPACT_OPTIONS.map(o => (
                            <option key={o.value} value={o.value}>{o.label}</option>
                          ))}
                        </select>
                      </div>
                    </div>

                    <h4 className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Time Estimates</h4>

                    {isJira ? (
                      <div className="grid grid-cols-2 gap-3">
                        <div>
                          <label className="block text-xs font-medium text-gray-700 mb-1">
                            Remaining (h) <span className="text-gray-400">override</span>
                          </label>
                          <input
                            type="number"
                            step="0.5"
                            min="0"
                            value={remainingOverride}
                            onChange={(e) => editRemainingOverride(e.target.value)}
                            placeholder={task.jiraRemainingSeconds !== null ? formatSeconds(task.jiraRemainingSeconds) : '-'}
                            className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                          />
                        </div>
                        <div>
                          <label className="block text-xs font-medium text-gray-700 mb-1">
                            Estimate (h) <span className="text-gray-400">override</span>
                          </label>
                          <input
                            type="number"
                            step="0.5"
                            min="0"
                            value={estimatedOverride}
                            onChange={(e) => editEstimatedOverride(e.target.value)}
                            placeholder={task.jiraOriginalEstimateSeconds !== null ? formatSeconds(task.jiraOriginalEstimateSeconds) : '-'}
                            className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                          />
                        </div>
                      </div>
                    ) : (
                      <div>
                        <label className="block text-xs font-medium text-gray-700 mb-1">Estimated hours</label>
                        <input
                          type="number"
                          step="0.5"
                          min="0"
                          value={estimatedHours}
                          onChange={(e) => editEstimatedHours(e.target.value)}
                          placeholder="e.g. 4"
                          className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                        />
                      </div>
                    )}

                    <div>
                      <div className="flex items-center justify-between mb-1">
                        <label className="block text-xs font-medium text-gray-700">Description</label>
                        {isJira && (
                          <span className="text-[10px] text-amber-600">
                            synced from Jira — local edits will be overwritten
                          </span>
                        )}
                      </div>
                      <textarea
                        value={description}
                        onChange={(e) => editDescription(e.target.value)}
                        rows={8}
                        className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                        placeholder="Add a description..."
                      />
                    </div>

                    <div>
                      <label className="block text-xs font-medium text-gray-700 mb-1">
                        Notes <span className="text-gray-400">(markdown · local only)</span>
                      </label>
                      <MarkdownEditor
                        value={notes}
                        onChange={editNotes}
                        placeholder="Working notes, decisions, links… (preserved across Jira syncs)"
                      />
                    </div>

                    {task?.id && (
                      <div className="border-t border-gray-200 pt-4">
                        <WorklogSection
                          taskId={task.id}
                          recurrenceId={task.recurrenceId ?? undefined}
                          isRecurring={isRecurring}
                        />
                      </div>
                    )}
                  </div>
                </>
              ) : null}
            </div>

            {/* Footer */}
            <div className="px-5 py-3 border-t border-gray-200 flex items-center justify-between gap-2">
              {/* Left side: skip button for recurring instances */}
              <div className="flex items-center gap-2">
                {isRecurring && (
                  <button
                    type="button"
                    data-testid="task-sheet-skip"
                    onClick={() => void handleSkipOccurrence()}
                    className="px-3 py-1.5 text-sm font-medium text-amber-700 border border-amber-300 bg-amber-50 rounded-md hover:bg-amber-100 transition-colors"
                  >
                    Ignorer cette occurrence
                  </button>
                )}
                {skipFailed && (
                  <span data-testid="task-sheet-skip-error" className="text-xs font-medium text-red-700">
                    ⚠ L&apos;occurrence n&apos;a pas pu être ignorée
                  </span>
                )}
              </div>

              {/* Right side: autosave status + close. There is no Save button, so a
                  failed write has to shout — silence would be invisible data loss. */}
              <div className="flex items-center gap-3">
                <div
                  data-testid="task-sheet-autosave-status"
                  aria-live="polite"
                  className="min-h-[1.25rem] flex items-center gap-1.5 text-xs"
                >
                  {autoSaveStatus === 'pending' && <span className="text-gray-400">Modification…</span>}
                  {autoSaveStatus === 'saving' && <span className="text-gray-500">Enregistrement…</span>}
                  {autoSaveStatus === 'saved' && <span className="text-green-600">✓ Enregistré</span>}
                  {autoSaveStatus === 'error' && (
                    <>
                      <span className="font-medium text-red-700">⚠ Échec de l&apos;enregistrement</span>
                      <button
                        type="button"
                        data-testid="task-sheet-autosave-retry"
                        onClick={() => void flushQueued(taskId)}
                        className="px-2 py-0.5 font-medium text-red-700 border border-red-300 bg-red-50 rounded-md hover:bg-red-100 transition-colors"
                      >
                        Réessayer
                      </button>
                    </>
                  )}
                </div>
                <button
                  data-testid="task-sheet-cancel"
                  onClick={() => void handleClose()}
                  className="px-3 py-1.5 text-sm font-medium text-gray-700 border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
                >
                  Fermer
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </>
  );
}
