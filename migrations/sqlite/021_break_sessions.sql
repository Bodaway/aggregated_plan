-- Taking a break becomes an event with a duration instead of a click.
--
-- Until now `taken` recorded an *intention*: the instant the popup was dismissed. The
-- prescribed `duration_seconds` was decorative, so the adherence rate measured how fast
-- the user silenced a notification. Pressing the button now opens a session — the two
-- columns below — and `taken` is only written once it reaches its end.
--
-- `ends_at` is stored rather than derived from `started_at + rule.duration_seconds`
-- because it must be *frozen*: retuning the rule in the settings screen while a break is
-- running must not lengthen the break in progress, and the backend and the HUD have to
-- read one absolute deadline rather than two counters that can drift apart.
--
-- Cutting a break short needs an outcome of its own, `abandoned`, and SQLite cannot
-- widen a CHECK in place — so this is the documented table rebuild
-- (https://sqlite.org/lang_altertable.html#otherxform):
--
--   * steps 2 and 11 (BEGIN / COMMIT) belong to sqlx, which runs each migration in a
--     transaction, so a failure anywhere below leaves the old table intact;
--   * steps 1 and 12 (`PRAGMA foreign_keys` off/on) are deliberately absent: the pragma
--     is a documented no-op inside a transaction, and `break_events` is a child table
--     only — nothing references it, so neither the DROP nor the RENAME can strand
--     another table's foreign key. Its own FK to `break_rules(id)` is reconducted
--     verbatim, cascade included, because dropping a rule must still take its history;
--   * step 3 (inventory) found one table and two explicit indexes for `break_events` in
--     `sqlite_master` — no trigger, no view, so step 9 and half of step 8 are empty;
--   * step 10 (`PRAGMA foreign_key_check`) cannot fail a migration from inside SQL — it
--     returns rows rather than raising — so it is asserted in the test suite instead.
--
-- The column list is written out on both sides of the INSERT rather than relying on
-- `SELECT *`: a positional copy is what silently transposes two columns of the same type
-- the day one is inserted in the middle.
CREATE TABLE new_break_events (
    id                       TEXT PRIMARY KEY,
    user_id                  TEXT NOT NULL,
    rule_id                  TEXT NOT NULL REFERENCES break_rules(id) ON DELETE CASCADE,
    due_at                   TEXT NOT NULL,
    fired_at                 TEXT,
    deferred_until           TEXT,
    defer_reason             TEXT CHECK (defer_reason IS NULL OR defer_reason IN ('meeting','snooze')),
    suppressed_by_meeting_id TEXT,
    outcome                  TEXT NOT NULL
        CHECK (outcome IN ('pending','taken','snoozed','skipped','ignored','absorbed','expired','abandoned')),
    responded_at             TEXT,
    -- The instant the user pressed "Prendre la pause".
    started_at               TEXT,
    -- `started_at + rule.duration_seconds`, frozen when the session opens.
    ends_at                  TEXT,
    created_at               TEXT NOT NULL
);

INSERT INTO new_break_events
    (id, user_id, rule_id, due_at, fired_at, deferred_until, defer_reason,
     suppressed_by_meeting_id, outcome, responded_at, created_at)
SELECT
     id, user_id, rule_id, due_at, fired_at, deferred_until, defer_reason,
     suppressed_by_meeting_id, outcome, responded_at, created_at
FROM break_events;

DROP TABLE break_events;

ALTER TABLE new_break_events RENAME TO break_events;

-- Dropping the table dropped its indexes with it; 019's two are recreated verbatim.
CREATE INDEX idx_break_events_rule_due ON break_events(user_id, rule_id, due_at);
CREATE INDEX idx_break_events_outcome  ON break_events(user_id, outcome);
