import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { HeaderSearchBar } from './HeaderSearchBar';
import { SearchProvider } from '@/lib/search/SearchProvider';

vi.mock('@/hooks/use-searchable-tasks', () => ({
  useSearchableTasks: () => ({ tasks: [], loading: false, error: null, refetch: () => {} }),
}));
vi.mock('@/components/task/TaskEditSheet', () => ({ TaskEditSheet: () => null }));

function renderBar() {
  return render(
    <SearchProvider>
      <HeaderSearchBar />
    </SearchProvider>
  );
}

describe('HeaderSearchBar', () => {
  it('renders the input with placeholder', () => {
    renderBar();
    expect(screen.getByRole('combobox')).toBeInTheDocument();
  });

  it('focuses the input when "/" is pressed on the document body', () => {
    renderBar();
    const input = screen.getByRole('combobox') as HTMLInputElement;
    expect(document.activeElement).not.toBe(input);
    fireEvent.keyDown(window, { key: '/' });
    expect(document.activeElement).toBe(input);
  });

  it('focuses the input when Cmd+K is pressed', () => {
    renderBar();
    const input = screen.getByRole('combobox') as HTMLInputElement;
    fireEvent.keyDown(window, { key: 'k', metaKey: true });
    expect(document.activeElement).toBe(input);
  });

  it('ignores "/" while a textarea is focused', () => {
    const { container } = renderBar();
    const textarea = document.createElement('textarea');
    container.appendChild(textarea);
    textarea.focus();
    fireEvent.keyDown(window, { key: '/' });
    expect(document.activeElement).toBe(textarea);
  });

  it('Escape clears the query and blurs the input', () => {
    renderBar();
    const input = screen.getByRole('combobox') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'auth' } });
    expect(input.value).toBe('auth');
    input.focus();
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(input.value).toBe('');
    expect(document.activeElement).not.toBe(input);
  });
});
