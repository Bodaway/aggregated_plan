-- Daily reconstructed timesheet: one header row per (user, local date) + per-project lines.
-- Independent of activity_slots (which has no project). Never auto-submitted to Gryzzly.
CREATE TABLE timesheet_drafts (
    id             TEXT PRIMARY KEY,
    user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    date           TEXT NOT NULL,               -- LOCAL calendar date (YYYY-MM-DD)
    status         TEXT NOT NULL DEFAULT 'draft', -- draft | validated | submitted | day_off
    target_hours   REAL NOT NULL,
    total_hours    REAL NOT NULL,
    day_confidence TEXT NOT NULL,               -- high | medium | low
    blocks_json    TEXT,                        -- serialized timeline (AttributedBlock[])
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    UNIQUE(user_id, date)
);

CREATE TABLE timesheet_draft_lines (
    id                 TEXT PRIMARY KEY,
    draft_id           TEXT NOT NULL REFERENCES timesheet_drafts(id) ON DELETE CASCADE,
    gryzzly_project_id TEXT,                    -- NULL = "unattributed" bucket
    project_name       TEXT,
    hours              REAL NOT NULL,
    is_pinned          INTEGER NOT NULL DEFAULT 0,
    confidence         TEXT NOT NULL,           -- high | medium | low
    source_refs_json   TEXT,
    created_at         TEXT NOT NULL
);
CREATE INDEX idx_tsl_draft ON timesheet_draft_lines(draft_id);
