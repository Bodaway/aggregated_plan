-- Semantic memory store: "what must I know?" (decisions, commitments, facts, preferences).
-- Bi-temporal: occurred_at = when it became true, invalidated_at = when it stopped being true.
-- `invalidated_at` / `superseded_by` are written ONLY by the supersede commands (later lot).
CREATE TABLE memories (
  id             TEXT PRIMARY KEY,
  user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  kind           TEXT NOT NULL,     -- decision | commitment | fact | preference
  title          TEXT NOT NULL,     -- one sentence: what we retain
  body           TEXT,              -- the context: why, alternatives dropped

  -- bi-temporal
  occurred_at    TEXT NOT NULL,     -- when it was decided / promised
  recorded_at    TEXT NOT NULL,     -- when aplan learned about it
  invalidated_at TEXT,              -- NULL = still true
  superseded_by  TEXT REFERENCES memories(id) ON DELETE SET NULL,

  -- provenance
  source         TEXT NOT NULL,     -- claude_session | manual | dreaming
  source_ref     TEXT,              -- worklog entry id, session id. NO FK: worklog rows
                                    -- cascade away with their task, a dangling provenance
                                    -- chain is preferred over a deleted memory.
  status         TEXT NOT NULL,     -- pending | active | rejected

  -- entity linking, obtained for free by join
  -- SET NULL and not CASCADE: deleting a task must not erase the memory of the
  -- decision that created it
  project_id     TEXT REFERENCES projects(id) ON DELETE SET NULL,
  task_id        TEXT REFERENCES tasks(id)    ON DELETE SET NULL
);

-- Covers the `list` predicate (user_id + status + optional project_id) and the
-- newest-first ordering the queue and history lists rely on.
CREATE INDEX idx_memories_user_status ON memories(user_id, status, project_id);
CREATE INDEX idx_memories_occurred_at ON memories(user_id, occurred_at DESC);

CREATE TABLE memory_stakeholders (       -- "towards whom", "with whom"
  memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
  person    TEXT NOT NULL,
  PRIMARY KEY (memory_id, person)
);
-- "Which commitments did I make to Pierre?" is a first-order question, and the
-- PRIMARY KEY only indexes (memory_id, person) — the reverse lookup needs its own.
CREATE INDEX idx_memory_stakeholders_person ON memory_stakeholders(person);

-- STANDALONE FTS5 table (no `content=`): an external-content table without triggers
-- returns 0 rows on MATCH while count(*) still reports 1, so the most natural
-- integrity check hides the breakage. The repository writes this row in the SAME
-- transaction as the `memories` row.
--
-- ⚠ ORPHAN HAZARD: a virtual table takes no foreign key, so `memory_id` here is
-- NOT constrained and deleting a `memories` row does NOT cascade to this one.
-- Every write path must maintain it by hand, inside the caller's transaction:
--   * insert  -> INSERT here            (`create`)
--   * retitle -> DELETE + INSERT here   (`update`, else the memory stays findable
--                                        only under its OLD wording)
--   * delete  -> DELETE here FIRST      (`apply_merge` on the discarded row)
-- A missed DELETE leaves an orphan that keeps answering MATCH for a memory that
-- no longer exists; the join then drops it and the search silently under-returns.
CREATE VIRTUAL TABLE memories_fts USING fts5(
  memory_id UNINDEXED,
  title,
  body,
  tokenize = 'unicode61 remove_diacritics 2'
);

-- Consolidation watermark: PER-ENTRY marker, not a global cursor.
ALTER TABLE worklog_entries ADD COLUMN consolidated_at TEXT;
