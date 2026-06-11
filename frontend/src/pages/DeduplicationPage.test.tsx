import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

const executeMutation = vi.fn(() => Promise.resolve({ error: undefined }));
const reexecuteQuery = vi.fn();
const SUGGESTION = {
  id: 'ephemeral-sug-id',
  taskA: { id: 'task-a-id', title: 'Mettre a jour la pipeline', source: 'JIRA', assignee: 'Michel', project: null },
  taskB: { id: 'task-b-id', title: 'SCB-457 A3 Node 24', source: 'PERSONAL', assignee: 'Michel', project: null },
  confidenceScore: 1, titleSimilarity: 1, assigneeMatch: true, projectMatch: true,
};
vi.mock('urql', () => ({
  useQuery: () => [{ data: { deduplicationSuggestions: [SUGGESTION] }, fetching: false, error: undefined }, reexecuteQuery],
  useMutation: () => [{}, executeMutation],
}));

import { DeduplicationPage } from './DeduplicationPage';

describe('DeduplicationPage mutation contract', () => {
  beforeEach(() => {
    executeMutation.mockClear();
    reexecuteQuery.mockClear();
  });

  it('calls confirmDeduplication with taskIdPrimary, taskIdSecondary, accept:true when Merge is clicked', async () => {
    render(<DeduplicationPage />);
    fireEvent.click(screen.getByRole('button', { name: /merge/i }));
    await waitFor(() => expect(executeMutation).toHaveBeenCalled());
    expect(executeMutation).toHaveBeenCalledWith({
      taskIdPrimary: 'task-a-id',
      taskIdSecondary: 'task-b-id',
      accept: true,
    });
  });

  it('calls confirmDeduplication with taskIdPrimary, taskIdSecondary, accept:false when Not a Duplicate is clicked', async () => {
    render(<DeduplicationPage />);
    fireEvent.click(screen.getByRole('button', { name: /not a duplicate/i }));
    await waitFor(() => expect(executeMutation).toHaveBeenCalled());
    expect(executeMutation).toHaveBeenCalledWith({
      taskIdPrimary: 'task-a-id',
      taskIdSecondary: 'task-b-id',
      accept: false,
    });
  });
});
