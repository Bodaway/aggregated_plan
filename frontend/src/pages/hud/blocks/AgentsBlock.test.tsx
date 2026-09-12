import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

const HUD_CSS = readFileSync(resolve(__dirname, '../hud.css'), 'utf8');

const agentSessionsMock = vi.fn();
vi.mock('../useAgentSessions', () => ({ useAgentSessions: () => agentSessionsMock() }));

import { AgentsBlock } from './AgentsBlock';
import type { ActiveAgent } from '@/lib/agent-sessions';

function mockAgents(agents: readonly ActiveAgent[]) {
  agentSessionsMock.mockReturnValue(agents);
}

describe('AgentsBlock', () => {
  beforeEach(() => {
    agentSessionsMock.mockReset();
  });

  it('lists live sessions, task title shown when the session is linked', () => {
    mockAgents([
      { sessionName: 'aggregated-plan-98', taskTitle: 'SCB-455', lastSeenMinutes: 0 },
      { sessionName: 'cicd-safteaction-3d', taskTitle: 'SAFT QRCode', lastSeenMinutes: 2 },
    ]);

    render(<AgentsBlock />);

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

  it('reports how long a session has been silent, and marks its dot', () => {
    // "Quiet", not "Idle": `lastSeenAt` is the last `aplan` call, not the
    // last thought. Caught on the real HUD labelling a session that was
    // actively working "Idle 6h 34min" — the panel may only claim what it
    // can measure.
    mockAgents([{ sessionName: 'qmkkc-1f', taskTitle: null, lastSeenMinutes: 60 }]);

    render(<AgentsBlock />);

    expect(screen.getByText(/Quiet 1h/i)).toBeInTheDocument();
    expect(screen.queryByText(/Idle/i)).not.toBeInTheDocument();
    expect(screen.getByTestId('agent-dot').className).toContain('hud-agents__dot--idle');
  });

  it('reads a fresh, unlinked session as merely unlinked, not quiet', () => {
    // taskTitle: null means "not linked to a task" — it is not, by itself,
    // silence. A session seen a minute ago must not read "Quiet" just
    // because it has no task yet.
    mockAgents([{ sessionName: 'fresh-session', taskTitle: null, lastSeenMinutes: 1 }]);

    render(<AgentsBlock />);

    expect(screen.getByText('Unlinked')).toBeInTheDocument();
    expect(screen.getByTestId('agent-dot').className).not.toContain('--idle');
  });

  it('holds a session quiet for ten minutes as still working', () => {
    // The threshold moved from 5 to 15 minutes when the block left the stub:
    // `lastSeenAt` is stamped by `aplan` calls, not by an agent thinking, so
    // five minutes of silence is an ordinary gap in a live session.
    mockAgents([{ sessionName: 'thinking-hard', taskTitle: null, lastSeenMinutes: 10 }]);

    render(<AgentsBlock />);

    expect(screen.getByText('Unlinked')).toBeInTheDocument();
    expect(screen.getByTestId('agent-dot').className).not.toContain('--idle');
  });

  it('caps the rendered rows while the label keeps the true total', () => {
    // Not hypothetical: sessions stay open until the reaper closes them, so
    // a working day really does accumulate more than fit the panel. The hook
    // sorts freshest first, so a full panel drops the quietest.
    mockAgents(
      Array.from({ length: 8 }, (_, i) => ({
        sessionName: `session-${i}`,
        taskTitle: null,
        lastSeenMinutes: i,
      })),
    );

    render(<AgentsBlock />);

    expect(screen.getByText(/8 sessions/i)).toBeInTheDocument();
    expect(screen.getAllByTestId('agent-row')).toHaveLength(5);
  });

  it('no longer marks itself as placeholder data', () => {
    // The STUB badge existed because the block rendered fabricated sessions
    // that read as real on screen. It is reading `openClaudeSessions` now,
    // and a warning left next to real data is worse than none.
    mockAgents([{ sessionName: 'real-session', taskTitle: 'A real task', lastSeenMinutes: 0 }]);

    render(<AgentsBlock />);

    expect(screen.queryByTestId('stub-marker')).not.toBeInTheDocument();
  });

  it('reads a deliberate empty state when there is no active session', () => {
    mockAgents([]);

    render(<AgentsBlock />);

    expect(screen.getByText(/no active session/i)).toBeInTheDocument();
    expect(screen.queryAllByTestId('agent-row')).toHaveLength(0);

    // Presence in the DOM is not legibility — assert the CSS actually marks
    // this as a deliberate empty state, not a stray string.
    const emptyRule = HUD_CSS.match(/\.hud-agents__empty\s*\{[^}]*\}/)?.[0] ?? '';
    expect(emptyRule).toMatch(/font-style:\s*italic/);
  });
});
