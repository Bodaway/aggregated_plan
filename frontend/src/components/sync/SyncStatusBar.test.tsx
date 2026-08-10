import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { SyncStatusBar } from './SyncStatusBar';

interface Row {
  source: string;
  status: string;
  lastSyncAt: string | null;
  errorMessage: string | null;
}

const row = (over: Partial<Row> = {}): Row => ({
  source: 'GRYZZLY',
  status: 'SUCCESS',
  lastSyncAt: '2026-08-10T15:55:45Z',
  errorMessage: null,
  ...over,
});

describe('SyncStatusBar', () => {
  it('renders nothing when there are no statuses', () => {
    const { container } = render(<SyncStatusBar statuses={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  // An unconfigured connector is not a failure. It used to arrive as
  // status=error + errorMessage="Not configured" and paint a red Error dot.
  it('labels a not-configured source without calling it an error', () => {
    render(<SyncStatusBar statuses={[row({ status: 'NOT_CONFIGURED' })]} />);
    // Regex, not an exact string: the component wraps every label in parentheses,
    // so the text content is "(Non configuré)". An exact-match negative assertion
    // would pass vacuously here and test nothing.
    expect(screen.getByText(/Non configuré/)).toBeInTheDocument();
    expect(screen.queryByText(/Error/)).not.toBeInTheDocument();
  });

  // The expiry date and the instruction are the whole value of the message, and a
  // title tooltip hides them until you happen to hover.
  it('shows an error message inline, not only in a tooltip', () => {
    render(
      <SyncStatusBar
        statuses={[
          row({
            status: 'ERROR',
            errorMessage:
              'the Gryzzly session cookie expired on 2026-08-17 14:51:50 UTC — log in again on app.gryzzly.io (it lasts 7 days)',
          }),
        ]}
      />,
    );
    expect(screen.getByText(/expired on 2026-08-17/)).toBeInTheDocument();
  });

  it('offers a Gryzzly reconnect link when the session needs attention', () => {
    render(<SyncStatusBar statuses={[row({ status: 'NOT_CONFIGURED' })]} />);
    const link = screen.getByRole('link', { name: /reconnecter/i });
    expect(link).toHaveAttribute('href', 'https://app.gryzzly.io');
  });

  it('offers no reconnect link for a healthy Gryzzly', () => {
    render(<SyncStatusBar statuses={[row()]} />);
    expect(screen.queryByRole('link', { name: /reconnecter/i })).not.toBeInTheDocument();
  });

  // The link is Gryzzly-specific: it exists because that cookie expires weekly.
  it('offers no reconnect link for another source in error', () => {
    render(
      <SyncStatusBar statuses={[row({ source: 'JIRA', status: 'ERROR', errorMessage: 'boom' })]} />,
    );
    expect(screen.getByText('boom')).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: /reconnecter/i })).not.toBeInTheDocument();
  });
});
