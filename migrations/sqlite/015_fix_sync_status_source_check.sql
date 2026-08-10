-- `sync_status.source` — the CHECK written in 001 knows four sources; `domain::Source`
-- has six.
--
-- `source_to_str` (infrastructure/src/database/conversions.rs) maps `Source::Gryzzly`
-- onto `gryzzly` and `Source::Personal` onto `personal`, neither of which 001 admits.
-- The consequence for `gryzzly` was total: `sync_gryzzly` marks
-- `sync_status(gryzzly) -> syncing` as its very FIRST step, so every
-- `aplan sync --source gryzzly` died on `(code: 275) CHECK constraint failed` before
-- reaching the connector. That is why the source had never once run — the missing
-- API key was only the second lock on the same door.
--
-- Exactly the class of bug 013 fixed on `alerts.alert_type`: a CHECK enumerating
-- variants, left behind when a variant was added. 009 introduced the Gryzzly source
-- and its catalog table but never widened this constraint.
--
-- SQLite cannot ALTER a CHECK constraint, so this is the documented table rebuild
-- (https://sqlite.org/lang_altertable.html#otherxform), following 013 step for step:
--
--   * steps 2 and 11 (BEGIN / COMMIT) belong to sqlx, which runs each migration in a
--     transaction — a failure below leaves the old table intact;
--   * steps 1 and 12 (`PRAGMA foreign_keys` off/on) are deliberately absent: the
--     pragma is a documented no-op inside a transaction, and `sync_status` is a child
--     table only — `SELECT name FROM sqlite_master WHERE sql LIKE '%REFERENCES
--     sync_status%'` is empty, so neither the DROP nor the RENAME can break another
--     table's foreign key;
--   * step 3 (inventory) found one table and NO explicit index for `sync_status` —
--     only `sqlite_autoindex_sync_status_1` and `_2`, which PRIMARY KEY and UNIQUE
--     recreate by themselves. No trigger, no view, so steps 8 and 9 are empty and
--     there is nothing to recreate by hand;
--   * step 10 (`PRAGMA foreign_key_check`) cannot fail a migration from inside SQL,
--     so it stays asserted in `database::connection::migration_tests`.
--
-- The column list on both sides of the INSERT is written out rather than `SELECT *`:
-- a positional copy is what silently transposes two columns of the same type.
CREATE TABLE new_sync_status (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source TEXT NOT NULL
        CHECK (source IN ('jira', 'outlook', 'excel', 'obsidian', 'personal', 'gryzzly')),
    last_sync_at TEXT,
    status TEXT NOT NULL DEFAULT 'idle'
        CHECK (status IN ('idle', 'syncing', 'success', 'error')),
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
