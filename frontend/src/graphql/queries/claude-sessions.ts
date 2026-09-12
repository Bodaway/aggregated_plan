// The sessions still open — `ended_at IS NULL`, whatever their mode. A
// session that declined tracking (`mode: OFF`) is still a live agent and
// still belongs on the HUD; the server clears its `task_id`, so it renders
// as unlinked rather than carrying a stale title.
export const OPEN_CLAUDE_SESSIONS_QUERY = `
  query OpenClaudeSessions {
    openClaudeSessions {
      id
      mode
      label
      lastSeenAt
      task { id title }
    }
  }
`;
