import React from 'react';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';
import { ImportButton } from '../import-button';

const mockTriggerSharePointImport = jest.fn();
jest.mock('../../infrastructure/api-client', () => ({
  triggerSharePointImport: (...args: unknown[]) => mockTriggerSharePointImport(...args),
}));

describe('ImportButton', () => {
  beforeEach(() => {
    mockTriggerSharePointImport.mockReset();
  });

  it('renders the import button', () => {
    render(<ImportButton onImportComplete={jest.fn()} />);
    expect(screen.getByText('Import from SharePoint')).toBeInTheDocument();
  });

  it('shows loading state during import', async () => {
    mockTriggerSharePointImport.mockReturnValue(new Promise(() => {}));
    render(<ImportButton onImportComplete={jest.fn()} />);

    fireEvent.click(screen.getByText('Import from SharePoint'));
    expect(screen.getByText('Importing...')).toBeInTheDocument();
  });

  it('displays import result summary on success', async () => {
    mockTriggerSharePointImport.mockResolvedValue({
      totalRowsParsed: 10,
      parseErrors: [],
      projects: [
        { projectName: 'Project A', action: 'created', tasksCreated: 3, milestonesCreated: 1 },
        { projectName: 'Project B', action: 'updated', tasksCreated: 0, milestonesCreated: 0 },
      ],
    });

    const onComplete = jest.fn();
    render(<ImportButton onImportComplete={onComplete} />);

    fireEvent.click(screen.getByText('Import from SharePoint'));

    await waitFor(() => {
      expect(screen.getByText(/10 rows parsed/)).toBeInTheDocument();
    });

    expect(screen.getByText(/Project A/)).toBeInTheDocument();
    expect(screen.getByText(/created/)).toBeInTheDocument();
    expect(onComplete).toHaveBeenCalled();
  });

  it('displays error message on failure', async () => {
    mockTriggerSharePointImport.mockRejectedValue(new Error('Network error'));

    render(<ImportButton onImportComplete={jest.fn()} />);

    fireEvent.click(screen.getByText('Import from SharePoint'));

    await waitFor(() => {
      expect(screen.getByText(/Network error/)).toBeInTheDocument();
    });
  });
});
