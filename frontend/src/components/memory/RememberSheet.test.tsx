import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { RememberSheet } from './RememberSheet';
import { TASK_SHEET_Z } from '@/lib/memory/layers';

const onSubmit = vi.fn();
const onClose = vi.fn();

beforeEach(() => {
  onSubmit.mockReset();
  onClose.mockReset();
});

function open(props: Partial<React.ComponentProps<typeof RememberSheet>> = {}) {
  return render(
    <RememberSheet open onClose={onClose} onSubmit={onSubmit} {...props} />
  );
}

describe('RememberSheet', () => {
  it('prefills the title and the body from the captured selection', () => {
    open({ initialTitle: 'Le compte -ext seul accède au ServiceNow', initialBody: 'Vu le 17/08.' });

    expect(screen.getByLabelText('Title')).toHaveValue('Le compte -ext seul accède au ServiceNow');
    expect(screen.getByLabelText('Why')).toHaveValue('Vu le 17/08.');
  });

  it('records a fact in the validation queue by default', () => {
    open({ initialTitle: 'Un fait retenu' });

    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(onSubmit).toHaveBeenCalledWith({
      kind: 'FACT',
      title: 'Un fait retenu',
      body: null,
      taskId: null,
      confirmed: false,
    });
  });

  it('skips the queue when the user validates on the spot', () => {
    open({ initialTitle: 'Un fait retenu' });

    fireEvent.click(screen.getByLabelText(/validate now/i));
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(onSubmit).toHaveBeenCalledWith(expect.objectContaining({ confirmed: true }));
  });

  it('carries the kind the user picked', () => {
    open({ initialTitle: 'Un arbitrage' });

    fireEvent.change(screen.getByLabelText('Kind'), { target: { value: 'DECISION' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(onSubmit).toHaveBeenCalledWith(expect.objectContaining({ kind: 'DECISION' }));
  });

  it('attaches the task the selection came from', () => {
    open({ initialTitle: 'Un fait', taskId: 'task-42' });

    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(onSubmit).toHaveBeenCalledWith(expect.objectContaining({ taskId: 'task-42' }));
  });

  it('cannot be saved without a title', () => {
    open({ initialTitle: '   ' });

    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });

  it('renders nothing while closed', () => {
    render(<RememberSheet open={false} onClose={onClose} onSubmit={onSubmit} />);

    expect(screen.queryByLabelText('Title')).not.toBeInTheDocument();
  });

  it('stacks above an already-open task sheet', () => {
    open({ initialTitle: 'Un fait retenu' });

    const dialog = screen.getByRole('dialog', { name: 'New memory' });

    expect(Number(dialog.style.zIndex)).toBeGreaterThan(TASK_SHEET_Z);
  });
});
