import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';

const HUD_CSS = readFileSync(resolve(__dirname, '../hud.css'), 'utf8');

import { AgentsBlock } from './AgentsBlock';
import type { ActiveAgent } from './stub-data';

describe('AgentsBlock', () => {
  it('lists active sessions from the contract, task title shown when the session is linked', () => {
    const agents: ActiveAgent[] = [
      { sessionName: 'aggregated-plan-98', taskTitle: 'SCB-455', lastSeenMinutes: 0 },
      { sessionName: 'cicd-safteaction-3d', taskTitle: 'SAFT QRCode', lastSeenMinutes: 2 },
    ];

    render(<AgentsBlock agents={agents} />);

    expect(screen.getByTestId('agents-block')).toBeInTheDocument();
    expect(screen.getByText(/2 sessions/i)).toBeInTheDocument();

    const rows = screen.getAllByTestId('agent-row');
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent('aggregated-plan-98');
    expect(rows[0]).toHaveTextContent('SCB-455');
    expect(rows[1]).toHaveTextContent('cicd-safteaction-3d');
    expect(rows[1]).toHaveTextContent('SAFT QRCode');

    const dots = screen.getAllByTestId('agent-dot');
    expect(dots.every((d) => !d.className.includes('--idle'))).toBe(true);
  });

  it('falls back to idle duration and marks the dot idle when a session has gone quiet with no linked task', () => {
    const agents: ActiveAgent[] = [{ sessionName: 'qmkkc-1f', taskTitle: null, lastSeenMinutes: 60 }];

    render(<AgentsBlock agents={agents} />);

    expect(screen.getByText(/Idle 1h/i)).toBeInTheDocument();
    expect(screen.getByTestId('agent-dot').className).toContain('hud-agents__dot--idle');
  });

  it('reads a fresh, unlinked session as merely unlinked, not idle', () => {
    // taskTitle: null means "not linked to a task" (per the contract's own
    // comment) — it is not, by itself, staleness. A session seen a minute
    // ago must not read "Idle" just because it has no task yet.
    const agents: ActiveAgent[] = [{ sessionName: 'fresh-session', taskTitle: null, lastSeenMinutes: 1 }];

    render(<AgentsBlock agents={agents} />);

    expect(screen.getByText('Unlinked')).toBeInTheDocument();
    expect(screen.getByTestId('agent-dot').className).not.toContain('--idle');
  });

  it('marks itself as placeholder data, in the label, on every render', () => {
    // Review finding: on the real screen this block was indistinguishable
    // from a real data source — plausible session names, a plausible task
    // link. Plan 2 must remove this marker alongside stub-data.ts when it
    // supplies real resolvers, not leave it standing next to real data.
    render(<AgentsBlock />);

    expect(screen.getByTestId('stub-marker')).toHaveTextContent(/stub/i);

    const stubRule = HUD_CSS.match(/\.hud-label__stub\s*\{[^}]*\}/)?.[0] ?? '';
    expect(stubRule).toMatch(/var\(--cn-orange\)/);
  });

  it('reads a deliberate empty state when there is no active session', () => {
    render(<AgentsBlock agents={[]} />);

    expect(screen.getByText(/no active session/i)).toBeInTheDocument();
    expect(screen.queryAllByTestId('agent-row')).toHaveLength(0);

    // Regression guard, same technique as PressureBlock's empty-state test:
    // presence in the DOM is not legibility — assert the CSS actually marks
    // this as a deliberate empty state, not a stray string.
    const emptyRule = HUD_CSS.match(/\.hud-agents__empty\s*\{[^}]*\}/)?.[0] ?? '';
    expect(emptyRule).toMatch(/font-style:\s*italic/);
  });

  it('renders the plan-2 stub by default when no agents prop is given', () => {
    render(<AgentsBlock />);

    expect(screen.getByTestId('agents-block')).toBeInTheDocument();
  });
});
