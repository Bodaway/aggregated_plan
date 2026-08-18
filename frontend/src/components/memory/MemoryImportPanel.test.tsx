import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MemoryImportPanel } from './MemoryImportPanel';
import type { Memory, MemoryImportReport } from '@/lib/memory/types';

const DIR = '/home/mbt/.claude/projects/-home-mbt-appfactory-aggregated-plan/memory';

const imported: Memory = {
  id: 'imp-1',
  kind: 'PREFERENCE',
  title: 'aplan note cadence : une entrée par constat',
  body: null,
  occurredAt: '2026-08-03T10:00:00Z',
  recordedAt: '2026-08-03T10:00:00Z',
  invalidatedAt: null,
  supersededBy: null,
  proposedSupersedes: null,
  status: 'ACTIVE',
  taskId: null,
  projectId: null,
  stakeholders: [],
};

const onImport = vi.fn();

beforeEach(() => onImport.mockReset());

describe('MemoryImportPanel', () => {
  it('offers the harness memory directory as the default', () => {
    render(<MemoryImportPanel defaultDirectory={DIR} report={null} onImport={onImport} />);

    expect(screen.getByLabelText(/directory/i)).toHaveValue(DIR);
  });

  it('imports whatever directory the field holds', () => {
    render(<MemoryImportPanel defaultDirectory={DIR} report={null} onImport={onImport} />);

    fireEvent.change(screen.getByLabelText(/directory/i), { target: { value: '/tmp/notes' } });
    fireEvent.click(screen.getByRole('button', { name: 'Import' }));

    expect(onImport).toHaveBeenCalledWith('/tmp/notes');
  });

  it('lists what it imported', () => {
    const report: MemoryImportReport = {
      imported: [imported],
      importedCount: 1,
      skipped: [],
      skippedCount: 0,
    };

    render(<MemoryImportPanel defaultDirectory={DIR} report={report} onImport={onImport} />);

    expect(screen.getByText(imported.title)).toBeInTheDocument();
  });

  it('names every skipped file with the reason it was skipped', () => {
    const report: MemoryImportReport = {
      imported: [],
      importedCount: 0,
      skipped: [
        { fileName: 'MEMORY.md', reason: 'no_frontmatter' },
        { fileName: 'feedback-aplan-note-cadence.md', reason: 'already_imported' },
      ],
      skippedCount: 2,
    };

    render(<MemoryImportPanel defaultDirectory={DIR} report={report} onImport={onImport} />);

    expect(screen.getByText('MEMORY.md')).toBeInTheDocument();
    expect(screen.getByText(/no frontmatter/i)).toBeInTheDocument();
    expect(screen.getByText('feedback-aplan-note-cadence.md')).toBeInTheDocument();
    expect(screen.getByText(/already imported/i)).toBeInTheDocument();
  });

  it('says the store is already up to date when a re-run imported nothing', () => {
    const report: MemoryImportReport = {
      imported: [],
      importedCount: 0,
      skipped: [{ fileName: 'a.md', reason: 'already_imported' }],
      skippedCount: 1,
    };

    render(<MemoryImportPanel defaultDirectory={DIR} report={report} onImport={onImport} />);

    expect(screen.getByText(/already up to date/i)).toBeInTheDocument();
  });
});
