# Outlook Meeting Exclusion List

**Date:** 2026-06-09
**Status:** Design — approved (verbal); spec for the record
**Branch:** `feat/microsoft-auth-gate` (same Outlook-sync area)

## Problem

Recurring noise meetings (e.g. "pause midi" — 7 of the 10 originally-synced events) clutter the
plan. The user wants to exclude chosen meetings from Outlook sync via a configurable list.

## Decisions (confirmed with user)

- Match **by title text**, **case-insensitive `contains`** (an entry "pause midi" skips
  "Pause Midi", "Pause midi — équipe", etc.).
- Applies to **all instances** (naturally covers recurring series; also any one-off with that title).
- List stored as a **newline-separated** string (one entry per line) in config key
  `outlook.exclude_patterns`.

## Behavior

During Outlook sync, an event whose subject contains any non-blank list entry (case-insensitive)
is **skipped**: not mapped, not upserted. Because skipped events are absent from the sync's
`current_ids` set, the existing `delete_stale` step **removes any previously-synced matches** on the
next sync — so adding a pattern retroactively purges already-stored matches. An empty list (no
non-blank entries) excludes nothing.

## Components

1. **Domain (pure, tested):** `domain::rules` gains
   `pub fn is_excluded(title: &str, patterns: &[String]) -> bool` — returns true if `title`
   lowercased contains any `patterns[i]` lowercased, skipping entries that are empty/whitespace.
   Unit-tested (match, case-insensitivity, blank-pattern-ignored, no-match, empty-list).
2. **Config key:** `outlook.exclude_patterns` — newline-separated. Not secret (round-trips through
   the `configuration` GraphQL query unredacted).
3. **Sync wiring (`application::use_cases::sync`):** the `Source::Outlook` arm of `sync_source`
   reads `outlook.exclude_patterns`, parses into `Vec<String>` (split on `\n`, trim, drop blanks),
   and passes it to `sync_outlook`. `sync_outlook` gains an `exclude_patterns: &[String]` parameter
   and filters fetched events with `is_excluded` before building `Meeting`s (so `meeting_count`,
   upsert, and `current_ids` all reflect the filtered set).
4. **Frontend (Settings, Outlook section):** a textarea "Excluded meeting titles (one per line)"
   bound to `outlook.exclude_patterns`, saved with the existing Outlook section save (alongside
   `outlook.calendar_days`). Helper text: "Case-insensitive; matches if the title contains the text."

## Data flow

```
force_sync (OUTLOOK)
  → sync_source: read outlook.exclude_patterns → patterns: Vec<String>
  → sync_outlook(client, meeting_repo, sync_repo, user_id, date_range, &patterns)
      → fetch_calendar(...) → events
      → events.filter(|e| !is_excluded(&e.title, patterns)) → meetings
      → upsert_batch(meetings); delete_stale(user_id, current_ids)  // purges excluded matches
```

## Error handling / edge cases

- Missing key → empty patterns → no exclusions (unchanged behavior).
- Blank / whitespace-only lines → ignored (an empty pattern must NOT match-all).
- Matching is on the raw Outlook subject as fetched.

## Testing

- **Domain unit tests** for `is_excluded` (the core logic).
- **Application:** an existing/extended `sync_outlook` test (with a stub Outlook client returning a
  couple of events) asserts an excluded-title event is not upserted while others are.
- **Manual:** add "pause midi", trigger Outlook sync, confirm the lunch entries disappear and other
  meetings remain.

## Out of scope (YAGNI)

- Recurring-series-only matching / `seriesMasterId` (title match is sufficient and needs no Graph
  field changes).
- Regex / per-field (organizer, location) matching.
- A pick-from-detected-series UI.
