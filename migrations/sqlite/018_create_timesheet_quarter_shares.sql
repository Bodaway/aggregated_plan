-- Quarter-day arbitration: one row per (draft, quarter, lane).
--
-- A share is a billing DECISION, so it gets a table rather than a JSON column.
-- `blocks_json` and `unresolved_json` are documented as opaque display payloads that
-- readers tolerate missing — the right contract for a timeline, the wrong one for hours
-- that reach a client invoice.
CREATE TABLE IF NOT EXISTS timesheet_quarter_shares (
    id                 TEXT PRIMARY KEY,
    draft_id           TEXT NOT NULL REFERENCES timesheet_drafts(id) ON DELETE CASCADE,
    -- 0..3 = the four quarter-days: morning first half, morning second half,
    -- afternoon first half, afternoon second half.
    quarter_index      INTEGER NOT NULL CHECK (quarter_index BETWEEN 0 AND 3),
    -- ON DELETE SET NULL, never CASCADE: deleting a task must not erase hours already
    -- declared against it. `lane_key` and `label` survive so the row stays readable.
    task_id            TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    -- 'task:<uuid>' | 'meeting:<source_ref>' | 'unattributed'
    lane_key           TEXT NOT NULL,
    label              TEXT NOT NULL,
    gryzzly_project_id TEXT,
    -- The weight the hours were derived from. Stored beside them so a reader can always
    -- tell a well-evidenced share from an even split.
    presence_minutes   INTEGER NOT NULL DEFAULT 0,
    hours              REAL NOT NULL,
    -- A share the user set by hand. A re-reconstruct preserves it and rebalances the
    -- rest of its quarter around it.
    is_pinned          INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL,
    UNIQUE (draft_id, quarter_index, lane_key)
);

CREATE INDEX IF NOT EXISTS idx_tqs_draft ON timesheet_quarter_shares(draft_id, quarter_index);

-- The concurrent evidence view: per-lane intervals for the day, display only.
-- Tolerant parse, same contract as blocks_json — a day persisted before this column
-- existed renders as "reconstruct to see the evidence", never as a failed query.
ALTER TABLE timesheet_drafts ADD COLUMN lanes_json TEXT;
