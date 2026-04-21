import { useMemo } from 'react';
import { useWorklog } from '@/hooks/use-worklog';
import { AddWorklogEntryForm } from './AddWorklogEntryForm';
import { WorklogEntryCard } from './WorklogEntryCard';

interface Props {
  readonly taskId: string;
}

export function WorklogSection({ taskId }: Props) {
  const filter = useMemo(() => ({ taskIds: [taskId], limit: 50 }), [taskId]);
  const { entries, loading, error, addEntry, updateEntry, deleteEntry } = useWorklog(filter);

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold uppercase tracking-wider text-gray-700">
          Worklog
        </h3>
        <span className="text-xs text-gray-400">
          {entries.length} entr{entries.length === 1 ? 'y' : 'ies'}
        </span>
      </div>

      <AddWorklogEntryForm
        onSubmit={(body) => addEntry({ taskId, body }).then(() => undefined)}
      />

      {error && (
        <div className="rounded-md border border-red-200 bg-red-50 p-2 text-xs text-red-700">
          {error.message}
        </div>
      )}

      {loading && entries.length === 0 ? (
        <p className="text-xs text-gray-400">Loading…</p>
      ) : entries.length === 0 ? (
        <p className="py-4 text-center text-xs text-gray-400">No entries yet.</p>
      ) : (
        <ul className="space-y-2">
          {entries.map((e) => (
            <li key={e.id}>
              <WorklogEntryCard
                entry={e}
                onSave={(patch) => updateEntry({ id: e.id, ...patch }).then(() => undefined)}
                onDelete={() => deleteEntry(e.id).then(() => undefined)}
              />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
