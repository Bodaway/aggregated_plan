-- 014_create_sessions.sql
-- One row per Claude Code session.
--
-- The global `aplan.active_task_id` / `aplan.active_since` pair keeps its meaning
-- untouched: it is the human, working by hand, one task at a time. These rows are
-- the other actors — one per Claude session, each with its own task — so two
-- sessions can work on two tasks without overwriting one another's pointer.
CREATE TABLE sessions (
    id            TEXT PRIMARY KEY,                              -- CLAUDE_CODE_SESSION_ID
    user_id       TEXT NOT NULL REFERENCES users(id),
    task_id       TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    mode          TEXT NOT NULL CHECK (mode IN ('tracking','off')),
    label         TEXT,                                          -- the hook's `cwd`, for display
    started_at    TEXT NOT NULL,
    last_seen_at  TEXT NOT NULL,
    last_flush_at TEXT,
    ended_at      TEXT
);

-- `aplan sessions` reads the open ones; the idle-session reaper (plan 3) reads the
-- same index from the other end.
CREATE INDEX idx_sessions_user_open ON sessions(user_id, ended_at);

-- Authorship. NULL means the human: the global pointer has no session row, and it
-- never will.
ALTER TABLE worklog_entries ADD COLUMN session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL;
ALTER TABLE activity_slots  ADD COLUMN session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL;

-- Provenance: 'worklog' is a slot the flush projection owns and a rebuild may
-- replace, 'manual' is anything else (a live timer, a hand-made slot).
--
-- Deliberately left NULL for the rows already in the table, and deliberately without
-- a CHECK: the enum is enforced in Rust (`SlotSource`), because fixing a CHECK on an
-- existing SQLite table costs the full table rebuild that migration 013 had to do.
-- The API's one-shot classification pass fills these rows from the data itself, and
-- until it has run — or if it ever misses one — a NULL reads as 'manual', so the
-- unknown is protected rather than rebuilt away.
ALTER TABLE activity_slots ADD COLUMN source TEXT;
