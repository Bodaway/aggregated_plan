-- Break routine: several superposed cadences, one row each, plus one row per due slot.
--
-- The cadences overlap by construction (20/30/60 all coincide at minute 60), so
-- `priority` is not cosmetic: the engine fires at most one notification per tick and
-- marks the rest absorbed. Without it the user takes three pop-ups every hour and
-- turns the whole thing off within two days.
CREATE TABLE IF NOT EXISTS break_rules (
    id               TEXT PRIMARY KEY,
    user_id          TEXT NOT NULL,
    kind             TEXT NOT NULL CHECK (kind IN ('visual','posture','long','strength')),
    label            TEXT NOT NULL,
    body             TEXT NOT NULL,
    cadence          TEXT NOT NULL CHECK (cadence IN ('interval','daily')),
    interval_minutes INTEGER CHECK (interval_minutes IS NULL OR interval_minutes > 0),
    -- 'HH:MM', read in aplan.timezone by the application.
    at_time          TEXT,
    duration_seconds INTEGER NOT NULL CHECK (duration_seconds > 0),
    priority         INTEGER NOT NULL DEFAULT 0,
    enabled          INTEGER NOT NULL DEFAULT 1,
    urgency          TEXT NOT NULL DEFAULT 'normal' CHECK (urgency IN ('low','normal','critical')),
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    -- The exclusivity of the two cadence shapes is an invariant we do not entrust to
    -- application code alone: a rule with both set has no defined due time at all.
    CHECK ((cadence = 'interval' AND interval_minutes IS NOT NULL AND at_time IS NULL)
        OR (cadence = 'daily'    AND at_time         IS NOT NULL AND interval_minutes IS NULL))
);

CREATE INDEX IF NOT EXISTS idx_break_rules_user_enabled ON break_rules(user_id, enabled);

-- One row per due slot. This is what makes a deferral survive an API restart, and
-- what makes adherence measurable afterwards.
CREATE TABLE IF NOT EXISTS break_events (
    id                       TEXT PRIMARY KEY,
    user_id                  TEXT NOT NULL,
    rule_id                  TEXT NOT NULL REFERENCES break_rules(id) ON DELETE CASCADE,
    due_at                   TEXT NOT NULL,
    fired_at                 TEXT,
    deferred_until           TEXT,
    defer_reason             TEXT CHECK (defer_reason IS NULL OR defer_reason IN ('meeting','snooze')),
    suppressed_by_meeting_id TEXT,
    outcome                  TEXT NOT NULL
        CHECK (outcome IN ('pending','taken','snoozed','skipped','ignored','absorbed','expired')),
    responded_at             TEXT,
    created_at               TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_break_events_rule_due ON break_events(user_id, rule_id, due_at);
CREATE INDEX IF NOT EXISTS idx_break_events_outcome  ON break_events(user_id, outcome);

-- Seeded routine, straight from the ergonomics evidence. The user edits it afterwards
-- in the settings screen; the copy is French because the user reads it in a popup.
--
-- Seeded against the fixed local user id (api::state::DEFAULT_USER_ID_STR) rather than
-- `SELECT ... FROM users`: no migration ever inserts into `users`, so a row-driven seed
-- would silently produce nothing on a fresh database.
INSERT INTO break_rules (id, user_id, kind, label, body, cadence, interval_minutes, at_time,
                         duration_seconds, priority, enabled, urgency, created_at, updated_at)
VALUES
  ('11111111-1111-4111-8111-000000000001', '00000000-0000-0000-0000-000000000001', 'visual',
   'Pause visuelle', 'Regarde au loin 20 s, relâche les épaules.',
   'interval', 20, NULL, 30, 1, 1, 'low',
   '2026-08-27T00:00:00+00:00', '2026-08-27T00:00:00+00:00'),
  ('11111111-1111-4111-8111-000000000002', '00000000-0000-0000-0000-000000000001', 'posture',
   'Change de posture', 'Lève-toi, bouge, marche un instant.',
   'interval', 30, NULL, 120, 2, 1, 'normal',
   '2026-08-27T00:00:00+00:00', '2026-08-27T00:00:00+00:00'),
  ('11111111-1111-4111-8111-000000000003', '00000000-0000-0000-0000-000000000001', 'long',
   'Pause franche', 'Cinq minutes hors écran.',
   'interval', 60, NULL, 300, 3, 1, 'normal',
   '2026-08-27T00:00:00+00:00', '2026-08-27T00:00:00+00:00'),
  ('11111111-1111-4111-8111-000000000004', '00000000-0000-0000-0000-000000000001', 'strength',
   'Renfo épaule', 'Deux minutes d''élastique : rotations externes, rétractions scapulaires.',
   'daily', NULL, '14:00', 120, 4, 1, 'normal',
   '2026-08-27T00:00:00+00:00', '2026-08-27T00:00:00+00:00');
