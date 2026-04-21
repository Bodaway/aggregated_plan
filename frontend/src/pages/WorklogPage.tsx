import { useMemo, useState, useCallback } from 'react';
import { useWorklog, type WorklogEntry } from '@/hooks/use-worklog';
import { WorklogEntryCard } from '@/components/worklog/WorklogEntryCard';

type Preset = 'today' | '7d' | 'week' | 'month' | 'custom';

function startOfDay(d: Date): Date {
  const n = new Date(d);
  n.setHours(0, 0, 0, 0);
  return n;
}

function addDays(d: Date, n: number): Date {
  const r = new Date(d);
  r.setDate(r.getDate() + n);
  return r;
}

function startOfWeek(d: Date): Date {
  const n = startOfDay(d);
  const day = n.getDay();
  const diff = day === 0 ? -6 : 1 - day;
  return addDays(n, diff);
}

function rangeForPreset(
  p: Preset,
  customFrom?: string,
  customTo?: string
): { from?: string; to?: string } {
  const today = startOfDay(new Date());
  switch (p) {
    case 'today':
      return { from: today.toISOString(), to: addDays(today, 1).toISOString() };
    case '7d':
      return { from: addDays(today, -6).toISOString(), to: addDays(today, 1).toISOString() };
    case 'week':
      return {
        from: startOfWeek(today).toISOString(),
        to: addDays(startOfWeek(today), 7).toISOString(),
      };
    case 'month': {
      const first = new Date(today.getFullYear(), today.getMonth(), 1);
      const nextFirst = new Date(today.getFullYear(), today.getMonth() + 1, 1);
      return { from: first.toISOString(), to: nextFirst.toISOString() };
    }
    case 'custom': {
      const from = customFrom ? new Date(customFrom).toISOString() : undefined;
      const to = customTo ? addDays(new Date(customTo), 1).toISOString() : undefined;
      return { from, to };
    }
  }
}

function formatDayHeader(dayKey: string): string {
  const d = new Date(dayKey);
  return d.toLocaleDateString(undefined, {
    weekday: 'long',
    day: '2-digit',
    month: 'short',
    year: 'numeric',
  });
}

function groupByDay(entries: WorklogEntry[]): Array<{ dayKey: string; items: WorklogEntry[] }> {
  const map = new Map<string, WorklogEntry[]>();
  for (const e of entries) {
    const d = new Date(e.loggedAt);
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
    const arr = map.get(key) ?? [];
    arr.push(e);
    map.set(key, arr);
  }
  return Array.from(map.entries())
    .sort((a, b) => (a[0] < b[0] ? 1 : -1))
    .map(([dayKey, items]) => ({
      dayKey,
      items: items.sort((a, b) => (a.loggedAt < b.loggedAt ? 1 : -1)),
    }));
}

const PRESETS: ReadonlyArray<{ value: Preset; label: string }> = [
  { value: 'today', label: 'Today' },
  { value: '7d', label: 'Last 7 days' },
  { value: 'week', label: 'This week' },
  { value: 'month', label: 'This month' },
  { value: 'custom', label: 'Custom…' },
];

export function WorklogPage() {
  const [preset, setPreset] = useState<Preset>('7d');
  const [customFrom, setCustomFrom] = useState('');
  const [customTo, setCustomTo] = useState('');

  const filter = useMemo(() => {
    const { from, to } = rangeForPreset(preset, customFrom, customTo);
    return { from, to, limit: 500 };
  }, [preset, customFrom, customTo]);

  const { entries, loading, error, updateEntry, deleteEntry } = useWorklog(filter);
  const grouped = useMemo(() => groupByDay(entries), [entries]);

  const openTask = useCallback((taskId: string) => {
    window.dispatchEvent(new CustomEvent('task:open', { detail: { taskId } }));
  }, []);

  return (
    <div className="max-w-4xl space-y-4">
      <div className="flex flex-wrap items-center gap-2 rounded-md border border-gray-200 bg-white p-3">
        {PRESETS.map((p) => (
          <button
            key={p.value}
            type="button"
            onClick={() => setPreset(p.value)}
            className={`rounded-full px-3 py-1 text-xs font-medium ${
              preset === p.value
                ? 'bg-blue-600 text-white'
                : 'bg-gray-100 text-gray-700 hover:bg-gray-200'
            }`}
          >
            {p.label}
          </button>
        ))}
        {preset === 'custom' && (
          <div className="ml-2 flex items-center gap-2">
            <input
              type="date"
              value={customFrom}
              onChange={(e) => setCustomFrom(e.target.value)}
              className="rounded-md border border-gray-300 px-2 py-1 text-xs"
            />
            <span className="text-xs text-gray-500">to</span>
            <input
              type="date"
              value={customTo}
              onChange={(e) => setCustomTo(e.target.value)}
              className="rounded-md border border-gray-300 px-2 py-1 text-xs"
            />
          </div>
        )}
      </div>

      {error && (
        <div className="rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          {error.message}
        </div>
      )}

      {loading && entries.length === 0 ? (
        <p className="text-sm text-gray-500">Loading…</p>
      ) : grouped.length === 0 ? (
        <p className="rounded-md border border-gray-200 bg-white p-6 text-center text-sm text-gray-500">
          No entries for this range.
        </p>
      ) : (
        <div className="space-y-6">
          {grouped.map(({ dayKey, items }) => (
            <section key={dayKey}>
              <h2 className="mb-2 text-sm font-semibold text-gray-700">
                {formatDayHeader(dayKey)} — {items.length}{' '}
                {items.length === 1 ? 'entry' : 'entries'}
              </h2>
              <ul className="space-y-2">
                {items.map((e) => (
                  <li key={e.id}>
                    <WorklogEntryCard
                      entry={e}
                      showTaskChip
                      onTaskClick={openTask}
                      onSave={(patch) =>
                        updateEntry({ id: e.id, ...patch }).then(() => undefined)
                      }
                      onDelete={() => deleteEntry(e.id).then(() => undefined)}
                    />
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}
