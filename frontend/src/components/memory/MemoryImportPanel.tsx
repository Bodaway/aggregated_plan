import { useState } from 'react';
import type { MemoryImportReport } from '@/lib/memory/types';

interface MemoryImportPanelProps {
  readonly defaultDirectory: string;
  readonly report: MemoryImportReport | null;
  readonly importing?: boolean;
  readonly error?: string | null;
  readonly onImport: (directory: string) => void;
}

/** `no_frontmatter` → `no frontmatter`; unknown reasons pass through readably. */
function readableReason(reason: string): string {
  return reason.replace(/_/g, ' ');
}

function isAlreadyUpToDate(report: MemoryImportReport): boolean {
  return (
    report.importedCount === 0 &&
    report.skippedCount > 0 &&
    report.skipped.every(s => s.reason === 'already_imported')
  );
}

export function MemoryImportPanel({
  defaultDirectory,
  report,
  importing = false,
  error = null,
  onImport,
}: MemoryImportPanelProps) {
  const [directory, setDirectory] = useState(defaultDirectory);

  return (
    <section className="bg-white border border-gray-200 rounded-lg px-4 py-3 space-y-3">
      <div>
        <h2 className="text-sm font-semibold text-gray-900">Import markdown memories</h2>
        <p className="text-xs text-gray-500 mt-0.5">
          Reads the <code>.md</code> files of a directory the backend can see, and never writes to
          it. Idempotent: a file imported once is skipped, never duplicated.
        </p>
      </div>

      <div className="flex items-end gap-2">
        <div className="flex-1">
          <label htmlFor="import-directory" className="block text-xs font-medium text-gray-700 mb-1">
            Directory
          </label>
          <input
            id="import-directory"
            value={directory}
            onChange={e => setDirectory(e.target.value)}
            className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
          />
        </div>
        <button
          type="button"
          onClick={() => onImport(directory.trim())}
          disabled={importing || directory.trim() === ''}
          className="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        >
          Import
        </button>
      </div>

      {error && <p className="text-sm text-red-600 bg-red-50 rounded-md px-3 py-2">{error}</p>}

      {report && (
        <div className="space-y-2 pt-1">
          {isAlreadyUpToDate(report) ? (
            <p className="text-sm text-gray-600">
              Nothing new — the store is already up to date with that directory.
            </p>
          ) : (
            <p className="text-sm text-gray-700">
              {report.importedCount} imported · {report.skippedCount} skipped
            </p>
          )}

          {report.imported.length > 0 && (
            <ul className="space-y-1">
              {report.imported.map(m => (
                <li key={m.id} className="text-xs text-gray-700 flex items-start gap-2">
                  <span className="text-gray-400 shrink-0">{m.kind.toLowerCase()}</span>
                  <span>{m.title}</span>
                </li>
              ))}
            </ul>
          )}

          {report.skipped.length > 0 && (
            <ul className="space-y-1">
              {report.skipped.map(s => (
                <li key={s.fileName} className="text-xs text-gray-500 flex items-start gap-2">
                  <span className="font-mono">{s.fileName}</span>
                  <span className="text-gray-400">{readableReason(s.reason)}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </section>
  );
}
