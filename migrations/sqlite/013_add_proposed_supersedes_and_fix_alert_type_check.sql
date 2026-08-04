-- Two independent corrections, one migration.
--
--   1. `memories.proposed_supersedes` — the structured form of a supersession that
--      has been PROPOSED but not applied.
--   2. `alerts.alert_type` — the CHECK written in 001 never learned about the
--      fourth `AlertType` variant, so the end-of-day timesheet job aborted on
--      every run.

-- ── 1. Structured supersession proposals ────────────────────────────────────────
--
-- `superseded_by` records a supersession that HAPPENED; this column records one the
-- consolidation run merely PROPOSES. They must stay apart: the hard recall filter
-- reads `invalidated_at`, and a proposal is precisely a claim that nobody has
-- validated yet — it may never hide a memory that is still true. Before this
-- column the proposal lived as prose inside the candidate's `body`, which no
-- surface could read and no verb could act on.
--
-- ON DELETE SET NULL, for the same reason as `superseded_by` in 012: deleting a
-- memory (which `apply_merge` does to the discarded row) must not take another
-- memory with it, and a null claim beats a dangling id.
--
-- The claim is only meaningful while the candidate is `pending`. The domain clears
-- it on accept / reject / merge / supersede, so `status <> 'pending'` implies this
-- column is NULL — a stale proposal can therefore never outlive the triage that
-- answered it. That invariant is enforced in `domain::rules::memory_lifecycle` and
-- `Memory::new`, not by a CHECK: `memories` deliberately carries none (012), not
-- even on `kind` or `status`.
ALTER TABLE memories ADD COLUMN proposed_supersedes TEXT REFERENCES memories(id) ON DELETE SET NULL;

-- Partial index: only the handful of candidates carrying a claim are worth
-- indexing, and this is the column SQLite has to scan to apply `ON DELETE SET
-- NULL` every time a `memories` row is deleted.
CREATE INDEX idx_memories_proposed_supersedes
    ON memories(proposed_supersedes) WHERE proposed_supersedes IS NOT NULL;

-- ── 2. alerts.alert_type must admit `timesheet_ready` ──────────────────────────
--
-- `domain::AlertType` has four variants and `alert_type_to_str` maps
-- `TimesheetReady` onto `timesheet_ready` (infrastructure/src/database/
-- conversions.rs), but 001 constrained the column to three. The end-of-day
-- timesheet reconstruction therefore failed on EVERY run since the timesheet
-- feature merged — `(code: 275) CHECK constraint failed` — and the whole job
-- aborted, not just the alert.
--
-- SQLite cannot ALTER a CHECK constraint, so this is the documented table rebuild
-- (https://sqlite.org/lang_altertable.html#otherxform):
--
--   * steps 2 and 11 (BEGIN / COMMIT) belong to sqlx, which runs each migration in
--     a transaction — so a failure anywhere below leaves the old table intact;
--   * steps 1 and 12 (`PRAGMA foreign_keys` off/on) are deliberately absent: that
--     pragma is a documented no-op inside a transaction, and it is not needed here
--     because `alerts` is a child table only. Nothing references it, so neither
--     the DROP nor the RENAME can touch another table's foreign key;
--   * step 3 (inventory) found exactly one table and one explicit index for
--     `alerts` in `sqlite_master` — no trigger, no view, so step 9 and half of
--     step 8 are empty;
--   * step 10 (`PRAGMA foreign_key_check`) cannot fail a migration from inside SQL
--     — it returns rows rather than raising — so it is asserted in the test suite
--     instead (`database::connection::migration_tests`).
--
-- The column list on both sides of the INSERT is written out rather than relying
-- on `SELECT *`: a positional copy is what silently transposes two columns of the
-- same type the day one is inserted in the middle.
CREATE TABLE new_alerts (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    alert_type TEXT NOT NULL
        CHECK (alert_type IN ('deadline', 'overload', 'conflict', 'timesheet_ready')),
    severity TEXT NOT NULL
        CHECK (severity IN ('critical', 'warning', 'information')),
    message TEXT NOT NULL,
    related_items TEXT NOT NULL DEFAULT '[]',
    date TEXT NOT NULL,
    resolved INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO new_alerts
    (id, user_id, alert_type, severity, message, related_items, date, resolved, created_at)
SELECT
     id, user_id, alert_type, severity, message, related_items, date, resolved, created_at
FROM alerts;

DROP TABLE alerts;

ALTER TABLE new_alerts RENAME TO alerts;

-- Dropping the table dropped its index with it; 001's index is recreated verbatim.
CREATE INDEX idx_alerts_user_resolved ON alerts(user_id, resolved);
