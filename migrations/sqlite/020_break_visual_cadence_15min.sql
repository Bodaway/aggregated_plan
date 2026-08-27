-- The seeded visual break moves from 20 minutes to 15.
--
-- 20/30/60 interleave: over an hour the dues fall at :20, :30, :40, :00 — gaps of 10,
-- 10 then 20 minutes, a rhythm with no beat to it. 15/30/60 coincide instead, at :30
-- and at :00, where the collision rule (R68) absorbs the overlap and leaves one
-- notification. The user gets an even quarter-hour cadence rather than a ragged one.
--
-- A separate migration rather than an edit to 019: 019 is already applied on the live
-- database and sqlx validates migration checksums, so changing it in place would fail
-- startup rather than retune anything.
--
-- Scoped to the seeded id and guarded on the seeded value of 20, so a user who has
-- deliberately retuned this rule in the settings screen keeps their own number.
UPDATE break_rules
SET interval_minutes = 15,
    updated_at = '2026-08-27T12:00:00+00:00'
WHERE id = '11111111-1111-4111-8111-000000000001'
  AND cadence = 'interval'
  AND interval_minutes = 20;
