import React, { useState } from 'react';
import type { ImportResult } from '@aggregated-plan/shared-types';
import { triggerSharePointImport } from '../infrastructure/api-client';

type ImportButtonProps = {
  readonly onImportComplete: () => void;
};

export const ImportButton = ({ onImportComplete }: ImportButtonProps): React.JSX.Element => {
  const [isImporting, setIsImporting] = useState(false);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleImport = async (): Promise<void> => {
    setIsImporting(true);
    setResult(null);
    setError(null);

    try {
      const importResult = await triggerSharePointImport();
      setResult(importResult);
      onImportComplete();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Import failed');
    } finally {
      setIsImporting(false);
    }
  };

  return (
    <div style={{ display: 'inline-flex', flexDirection: 'column', gap: '8px' }}>
      <button
        onClick={() => void handleImport()}
        disabled={isImporting}
        style={{ padding: '8px 16px', cursor: isImporting ? 'not-allowed' : 'pointer' }}
      >
        {isImporting ? 'Importing...' : 'Import from SharePoint'}
      </button>

      {error && (
        <div style={{ color: 'red', fontSize: '14px' }}>
          Error: {error}
        </div>
      )}

      {result && (
        <div style={{ fontSize: '14px', border: '1px solid #ccc', padding: '8px', borderRadius: '4px' }}>
          <div><strong>{result.totalRowsParsed} rows parsed</strong></div>
          {result.parseErrors.length > 0 && (
            <div style={{ color: 'orange' }}>
              {result.parseErrors.length} parse error(s)
            </div>
          )}
          <ul style={{ margin: '4px 0', paddingLeft: '20px' }}>
            {result.projects.map((p) => (
              <li key={p.projectName}>
                {p.projectName} — <em>{p.action}</em>
                {p.tasksCreated > 0 && ` (${p.tasksCreated} tasks)`}
                {p.milestonesCreated > 0 && ` (${p.milestonesCreated} milestones)`}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
};
