-- Two changes, one migration, both needed by the terminated-project work.
--
--   1. `gryzzly_tasks.project_status` — lets a task on a CLOSED project be told apart
--      from a task DELETED in Gryzzly. Before this column both rendered identically
--      as `stale`, because `sync_gryzzly` fetched active projects only and the rest
--      were soft-pruned out of sight.
--   2. `sync_status.status` must admit `not_configured`.

-- ── 1. project_status ───────────────────────────────────────────────────────────
--
-- Values come from the Gryzzly API verbatim: `active` or `done`. Deliberately NO
-- CHECK: 013 and 015 are this repo's record of what enumerating someone else's
-- vocabulary in a CHECK costs, and the API is free to add a status tomorrow.
--
-- NULL for every pre-existing row, and NULL reads as "unknown, treat as active".
-- Rows imported by scripts/gryzzly/import_catalog.py predate the column and must
-- not suddenly render as terminated.
ALTER TABLE gryzzly_tasks ADD COLUMN project_status TEXT;

-- ── 2. sync_status.status must admit `not_configured` ───────────────────────────
--
-- `update_sync_error(..., "Not configured")` recorded an unconfigured connector as
-- `status = error` with the state carried as prose, so the UI painted a red Error
-- dot for something merely unconfigured — indistinguishable from a real failure.
-- `SyncSourceStatus` gains a fifth variant and this CHECK has to follow.
--
-- THIRD instance of this bug class in this schema: `alerts.alert_type` (013),
-- `sync_status.source` (015), and now `sync_status.status`. The pair of tests in
-- `database::connection::migration_tests` now enumerates BOTH columns' enums.
--
-- SQLite cannot ALTER a CHECK, so this is the documented rebuild
-- (https://sqlite.org/lang_altertable.html#otherxform), identical in shape to 015:
--
--   * steps 2 and 11 (BEGIN / COMMIT) belong to sqlx's per-migration transaction;
--   * steps 1 and 12 (`PRAGMA foreign_keys` off/on) are absent — a documented no-op
--     inside a transaction, and `sync_status` is a child table only (nothing has a
--     foreign key TO it), so neither the DROP nor the RENAME can break one;
--   * step 3 (inventory): one table, no explicit index (only the PRIMARY KEY and
--     UNIQUE autoindexes, which the new table recreates itself), no trigger, no
--     view — steps 8 and 9 are empty;
--   * step 10 (`PRAGMA foreign_key_check`) cannot fail a migration from inside SQL,
--     so it stays asserted in the test suite.
--
-- The `source` CHECK keeps 015's six values. Column lists are written out on both
-- sides of the INSERT rather than `SELECT *`.
CREATE TABLE new_sync_status (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source TEXT NOT NULL
        CHECK (source IN ('jira', 'outlook', 'excel', 'obsidian', 'personal', 'gryzzly')),
    last_sync_at TEXT,
    status TEXT NOT NULL DEFAULT 'idle'
        CHECK (status IN ('idle', 'syncing', 'success', 'error', 'not_configured')),
    error_message TEXT,
    UNIQUE(user_id, source)
);

INSERT INTO new_sync_status
    (id, user_id, source, last_sync_at, status, error_message)
SELECT
     id, user_id, source, last_sync_at, status, error_message
FROM sync_status;

DROP TABLE sync_status;

ALTER TABLE new_sync_status RENAME TO sync_status;
