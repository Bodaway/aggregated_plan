# Outlook Meeting Exclusion List Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user exclude meetings from Outlook sync via a case-insensitive title-substring list configured in Settings.

**Architecture:** A pure domain predicate `is_excluded(title, patterns)` filters fetched calendar events inside `sync_outlook`; the patterns come from a new newline-separated config key `outlook.exclude_patterns`, edited via a textarea in the Settings Outlook section.

**Tech Stack:** Rust (domain/application/api), React + urql + TypeScript.

**Spec:** `docs/superpowers/specs/2026-06-09-outlook-exclusion-list-design.md`

---

## Task 1: Domain predicate `is_excluded`

**Files:**
- Create: `backend/crates/domain/src/rules/meeting.rs`
- Modify: `backend/crates/domain/src/rules/mod.rs`

- [ ] **Step 1: Write the failing test + function stub**

Create `backend/crates/domain/src/rules/meeting.rs`:

```rust
/// Returns true if `title` contains any of `patterns` (case-insensitive).
/// Empty or whitespace-only patterns are ignored (they never match).
pub fn is_excluded(title: &str, patterns: &[String]) -> bool {
    let title_lc = title.to_lowercase();
    patterns
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .any(|p| title_lc.contains(&p.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pats(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

    #[test]
    fn matches_case_insensitive_substring() {
        assert!(is_excluded("Pause Midi — équipe", &pats(&["pause midi"])));
    }
    #[test]
    fn no_match_returns_false() {
        assert!(!is_excluded("Sprint review", &pats(&["pause midi", "standup"])));
    }
    #[test]
    fn blank_patterns_are_ignored() {
        assert!(!is_excluded("Anything", &pats(&["", "   "])));
    }
    #[test]
    fn empty_list_excludes_nothing() {
        assert!(!is_excluded("Anything", &[]));
    }
    #[test]
    fn matches_any_of_several() {
        assert!(is_excluded("Daily standup", &pats(&["pause midi", "standup"])));
    }
}
```

In `backend/crates/domain/src/rules/mod.rs`, add (match the existing style — the file already has lines like `pub mod alerts;` / `pub mod urgency;`):

```rust
pub mod meeting;
```

- [ ] **Step 2: Run the tests to verify they pass**

Run: `cd backend && cargo test -p domain is_excluded`
(Tests are written to pass immediately since the implementation is included; this is a pure function with the impl provided.)
Expected: 5 tests pass. If any fail, fix the implementation.

- [ ] **Step 3: Confirm exported + clippy**

Run: `cd backend && cargo test -p domain && cargo clippy -p domain 2>&1 | tail -5`
Expected: all domain tests pass; clippy clean. Confirm `domain::rules::meeting::is_excluded` resolves (the plan's later tasks import it).

- [ ] **Step 4: Commit**

```bash
git add backend/crates/domain/src/rules/meeting.rs backend/crates/domain/src/rules/mod.rs
git commit -m "feat(domain): is_excluded predicate for meeting title exclusion"
```

---

## Task 2: Filter excluded events in `sync_outlook`

**Files:**
- Modify: `backend/crates/application/src/use_cases/sync.rs`

- [ ] **Step 1: Add the `exclude_patterns` parameter + filter to `sync_outlook`**

Find the `sync_outlook` signature (around line 210):

```rust
pub async fn sync_outlook(
    outlook_client: &dyn OutlookClient,
    meeting_repo: &dyn MeetingRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    date_range: (NaiveDate, NaiveDate),
) -> Result<SyncResult, AppError> {
```

Add a parameter:

```rust
pub async fn sync_outlook(
    outlook_client: &dyn OutlookClient,
    meeting_repo: &dyn MeetingRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    date_range: (NaiveDate, NaiveDate),
    exclude_patterns: &[String],
) -> Result<SyncResult, AppError> {
```

In the body, after `let events = outlook_client.fetch_calendar(...)...?;` and before the `let meetings: Vec<Meeting> = events.into_iter().map(...)` mapping, filter:

```rust
    // Skip events whose title matches the user's exclusion list (case-insensitive contains).
    let events: Vec<_> = events
        .into_iter()
        .filter(|e| !domain::rules::meeting::is_excluded(&e.title, exclude_patterns))
        .collect();
```

(The existing `.map()` then runs over the filtered `events`, so `meeting_count`, `upsert_batch`, and `current_ids` all reflect the filtered set — and `delete_stale` purges previously-synced matches.)

- [ ] **Step 2: Pass patterns from `sync_source`**

In the `Source::Outlook` arm of `sync_source` (around line 657, just after the `days`/`calendar_days` read added previously), read and parse the exclusion list, then pass it:

```rust
        Source::Outlook => {
            if let Some(client) = outlook_client {
                let today = Utc::now().date_naive();
                let days: i64 = config_repo
                    .get(user_id, "outlook.calendar_days")
                    .await?
                    .and_then(|v| v.trim().parse::<i64>().ok())
                    .filter(|d| *d > 0)
                    .unwrap_or(14);
                let end = today + chrono::Duration::days(days);
                let exclude_patterns: Vec<String> = config_repo
                    .get(user_id, "outlook.exclude_patterns")
                    .await?
                    .map(|raw| {
                        raw.lines()
                            .map(|l| l.trim().to_string())
                            .filter(|l| !l.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                sync_outlook(client, meeting_repo, sync_repo, user_id, (today, end), &exclude_patterns).await?;
            } else {
                update_sync_error(sync_repo, user_id, Source::Outlook, "Not configured").await?;
            }
        }
```

(Match the exact existing structure of that arm; only add the `exclude_patterns` read and the extra argument.)

- [ ] **Step 3: Fix other `sync_outlook` callers + tests**

Run: `cd backend && rg -n "sync_outlook\(" crates/` to find all call sites (including tests). Update each to pass the new argument: production/other callers pass the real list; tests pass `&[]` unless asserting exclusion. Add or extend a `sync_outlook` test so it covers exclusion — if there is an existing `sync_outlook` test with a stub `OutlookClient`, add a case where one returned event's title matches a pattern and assert it is NOT among the upserted meetings (and a non-matching one IS). If no such test exists, add a minimal one using the existing test stubs in `sync.rs`'s `#[cfg(test)] mod tests`.

- [ ] **Step 4: Build + test**

Run: `cd backend && cargo test -p application sync && cargo clippy -p application 2>&1 | tail -5`
Expected: all sync tests pass (including the new exclusion assertion); clippy clean.

- [ ] **Step 5: Verify the api crate still builds (caller of force_sync → sync_source unchanged signature)**

Run: `cd backend && cargo build -p api`
Expected: compiles (only `sync_outlook`'s signature changed; `sync_source`/`force_sync` signatures are unchanged).

- [ ] **Step 6: Commit**

```bash
git add backend/crates/application/src/use_cases/sync.rs
git commit -m "feat(sync): exclude meetings by title via outlook.exclude_patterns"
```

---

## Task 3: Settings textarea for the exclusion list

**Files:**
- Modify: `frontend/src/pages/SettingsPage.tsx`

- [ ] **Step 1: Add a CONFIG key constant**

In `SettingsPage.tsx`, in the `CONFIG_KEYS` object, add:

```ts
  OUTLOOK_EXCLUDE_PATTERNS: 'outlook.exclude_patterns',
```

- [ ] **Step 2: Add the textarea to the Outlook section**

In the Microsoft Graph (Outlook) settings section, below the "Calendar Range (days)" input and above that section's Save button, add a labeled textarea bound to the config value. Match the file's existing form-control styling; if there is no reusable textarea component, use a plain styled `<textarea>`:

```tsx
          <div className="space-y-1">
            <label className="text-sm font-medium">Excluded meeting titles (one per line)</label>
            <textarea
              className="w-full rounded border px-3 py-2 text-sm"
              rows={4}
              value={getConfigValue(CONFIG_KEYS.OUTLOOK_EXCLUDE_PATTERNS)}
              onChange={e => setConfigValue(CONFIG_KEYS.OUTLOOK_EXCLUDE_PATTERNS, e.target.value)}
              placeholder={'pause midi\nDaily standup'}
            />
            <p className="text-xs text-muted-foreground">Case-insensitive; a meeting is skipped if its title contains any line.</p>
          </div>
```

- [ ] **Step 3: Save the new key with the Outlook section**

Find the Outlook section's `saveConfigKeys([...])` call (currently `[CONFIG_KEYS.OUTLOOK_CALENDAR_DAYS]`) and add the new key:

```tsx
                saveConfigKeys([
                  CONFIG_KEYS.OUTLOOK_CALENDAR_DAYS,
                  CONFIG_KEYS.OUTLOOK_EXCLUDE_PATTERNS,
                ])
```

- [ ] **Step 4: Build**

Run: `cd frontend && pnpm build`
Expected: TypeScript compiles, no errors. (The `"********"` skip-on-save guard already present does not affect this non-secret key.)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/pages/SettingsPage.tsx
git commit -m "feat(frontend): Outlook excluded-meeting-titles setting"
```

---

## Task 4: Spec (French) + verification

**Files:** `SPEC_TECHNIQUE.md`, `SPEC_FONCTIONNELLE.md`

- [ ] **Step 1: Document in SPEC_TECHNIQUE.md (French)**

In the configuration-keys area, add `outlook.exclude_patterns` (liste de titres, un par ligne; exclusion par sous-chaîne insensible à la casse appliquée dans `sync_outlook` via `domain::rules::meeting::is_excluded`; les correspondances déjà synchronisées sont purgées par `delete_stale`).

- [ ] **Step 2: Document in SPEC_FONCTIONNELLE.md (French)**

Add a short note (near the Outlook sync feature): l'utilisateur peut exclure des réunions de la synchronisation en listant des titres (une entrée par ligne) dans les paramètres Outlook; utile pour les réunions récurrentes (ex. « pause midi »).

- [ ] **Step 3: Commit**

```bash
git add SPEC_TECHNIQUE.md SPEC_FONCTIONNELLE.md
git commit -m "docs(spec): document outlook.exclude_patterns exclusion list"
```

- [ ] **Step 4: Manual verification**

With backend + frontend running and signed in: Settings → Outlook → add `pause midi` → Save → trigger Outlook sync → confirm the "pause midi" meetings are gone and other meetings remain.

---

## Self-review notes

- **Spec coverage:** domain predicate (Task 1), sync filter + config read (Task 2), Settings UI (Task 3), specs (Task 4). All spec sections mapped.
- **Type consistency:** `is_excluded(title: &str, patterns: &[String]) -> bool` defined in Task 1 and called identically in Task 2; config key `outlook.exclude_patterns` identical across Tasks 2-4; newline-split parsing matches the spec.
- **No silent over-match:** blank patterns filtered in BOTH the domain fn and the `sync_source` parse step (defense in depth) so an empty list never excludes everything.
- **Out of scope:** recurring-series/seriesMasterId matching, regex, other fields.
