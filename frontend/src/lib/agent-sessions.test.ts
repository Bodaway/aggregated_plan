import { describe, it, expect } from 'vitest';
import { toActiveAgents, type ClaudeSessionRow } from './agent-sessions';

const NOW = new Date('2026-09-11T14:41:00Z').getTime();

function session(over: Partial<ClaudeSessionRow> = {}): ClaudeSessionRow {
  return {
    id: '79c1b005-72ff-4d97-9135-01bb40dc79bb',
    task: null,
    mode: 'TRACKING',
    label: '/home/mbt/appfactory/cicd-safteaction',
    lastSeenAt: '2026-09-11T14:41:00Z',
    ...over,
  };
}

describe('toActiveAgents', () => {
  it('names a session after its working directory and a slice of its id', () => {
    // `label` is the session's cwd — that is what `aplan session bind` puts
    // there. The id slice is not decoration: four sessions of the same repo
    // open at once is the ordinary case here, and the bare basename would
    // render four identical rows.
    const agents = toActiveAgents(
      [
        session({ id: 'aaaaaaaa-1111', label: '/home/mbt/appfactory/cicd-safteaction' }),
        session({ id: 'bbbbbbbb-2222', label: '/home/mbt/appfactory/cicd-safteaction' }),
      ],
      NOW,
    );

    expect(agents.map((a) => a.sessionName)).toEqual(['cicd-safteaction-aa', 'cicd-safteaction-bb']);
  });

  it('tolerates a trailing slash on the working directory', () => {
    const [agent] = toActiveAgents([session({ id: 'cc00', label: '/home/mbt/appfactory/aggregated_plan/' })], NOW);
    expect(agent.sessionName).toBe('aggregated_plan-cc');
  });

  it('falls back to the bare id when the session carries no label', () => {
    // `label` is nullable in the schema, and a session bound with an
    // explicit `--label` is not a path at all — neither may render blank.
    const [noLabel] = toActiveAgents([session({ id: 'deadbeef-0000', label: null })], NOW);
    expect(noLabel.sessionName).toBe('deadbeef');

    const [named] = toActiveAgents([session({ id: 'feedface-0000', label: 'revue de specs' })], NOW);
    expect(named.sessionName).toBe('revue de specs-fe');
  });

  it('reports how long ago the session was last seen, floored at zero', () => {
    const agents = toActiveAgents(
      [
        session({ id: 'a1', lastSeenAt: '2026-09-11T14:38:00Z' }),
        // A clock skew that puts "last seen" in the future must not render
        // a negative age.
        session({ id: 'b2', lastSeenAt: '2026-09-11T14:45:00Z' }),
      ],
      NOW,
    );

    expect(agents.find((a) => a.sessionName.endsWith('-a1'))?.lastSeenMinutes).toBe(3);
    expect(agents.find((a) => a.sessionName.endsWith('-b2'))?.lastSeenMinutes).toBe(0);
  });

  it('puts the freshest session first', () => {
    // The block caps its rows, so the order decides what gets shown at all:
    // a session that went quiet three hours ago must not push out one that
    // is working right now.
    const agents = toActiveAgents(
      [
        session({ id: 'old', lastSeenAt: '2026-09-11T11:20:00Z' }),
        session({ id: 'now', lastSeenAt: '2026-09-11T14:40:00Z' }),
        session({ id: 'mid', lastSeenAt: '2026-09-11T13:55:00Z' }),
      ],
      NOW,
    );

    expect(agents.map((a) => a.sessionName.slice(-3))).toEqual(['-no', '-mi', '-ol']);
  });

  it('carries the bound task through, null for a session that declined tracking', () => {
    // `setSessionMode(OFF)` clears `task_id` server-side, so an untracked
    // session genuinely has no task — "Unlinked", not a stale title.
    const agents = toActiveAgents(
      [
        session({ id: 'on', task: { title: 'Erreur 403 sur commentaires' }, mode: 'TRACKING' }),
        session({ id: 'off', task: null, mode: 'OFF' }),
      ],
      NOW,
    );

    expect(agents.find((a) => a.sessionName.endsWith('-on'))?.taskTitle).toBe('Erreur 403 sur commentaires');
    expect(agents.find((a) => a.sessionName.endsWith('-of'))?.taskTitle).toBeNull();
  });

  it('returns nothing for no sessions', () => {
    expect(toActiveAgents([], NOW)).toEqual([]);
  });
});
