# Plan 3 — follow-ups after the final whole-branch review

Plan: `2026-08-06-aplan-sessions-plan-3-hooks-and-overlap.md`. Branch `feat/aplan-sessions-socle`,
head `58027dd`. **Final review verdict: ready to merge.** 1153 tests / 0 failed across 11 binaries;
CLI env A/B identical at 147/147; live database untouched at migration 14.

This file exists because the execution ledger is git-ignored, and the findings below are worth more than
the ledger's lifetime. Nothing here blocks merge — each was triaged by the final review.

## Decisions for the user

**1. Is a 12-hour idle threshold right, now that "idle" means "no aplan *write*"?**
`last_seen_at` advances only on a write, so a session that reads and converses for twelve hours is idle by
this definition and the reaper closes it mid-work, with no hook to explain it. Recovery is automatic —
`aplan session bind` revives a closed row — but the user sees nothing. Documented in § 7.3.5; the default is
deliberately unchanged because it is a judgement about how the user works, not a defect.

**2. Add `aplan overlaps [--date] [--json]`?**
All three commands skip the overlap line under `--json`, so the collision flag is **human-only** — while
`SKILL.md` tells a Claude to prefer `--json`. The final review upheld the decision not to reshape three
`--json` contracts mid-plan, but not the outcome: the skill now says "never parse the human output" and six
lines later "run the plain form to see a collision". A dedicated command (~30 lines; `ActivityOverlaps` is
already generated in the CLI and the resolver exists) sidesteps the contract question and makes the flag
reachable by machines. Nothing depends on it today and its absence misattributes nothing.

## Fix soon

| # | Finding | Why it matters |
|---|---|---|
| 1 | `SPEC_TECHNIQUE.md` — `outlook.exclude_patterns` row is missing its closing `|`, so its description spills into the `aplan.timezone` row | Pre-existing. Standalone commit; two agents were told to leave it alone and one had begun "fixing" it by accident |
| 2 | `aplan ls \| head` panics — `failed printing to stdout: Broken pipe`, missing SIGPIPE handling | Pre-existing (identical on `aplan.bak-20260804`). A panic on a normal shell idiom, and the hooks pipe `aplan ls` through `head` |
| 3 | `schema.graphql` can silently lag its source | Task 8's regeneration swept up doc comments from two earlier commits. A CI `--check` is the only durable form |
| 4 | The fake activity repo iterates `HashMap::values()` with `RandomState` (`activity_tracking.rs:244`, `:271`) while the SQLite repo is ordered | Already broke one mutation probe mid-verification. Flakes only in the fake — the wrong direction. `BTreeMap` or sort-on-read |
| 5 | 11 `"flushWorklogTime": null` mocks in `cli/tests/integration.rs` | Schema-invalid, passing only because `flush_task` swallows the deserialization error — the same swallow as the merge blocker just fixed |
| 6 | Four sites still hand-rolled the empty-id filter | **Done** in `a39f62b`; listed for completeness |
| 7 | `SPEC_TECHNIQUE.md:3142` — the enumeration of where `last_seen_at` advances omits `bind_session` and `set_session_mode` | Parked at the cap: the sentence's conclusion (writes only, never reads) is unaffected and if anything understated. Wrong list, right claim |

## Accepted, with reasons

- **Two `### 7.3` headings** in `SPEC_TECHNIQUE.md` — renumbering ripples further than the harm.
- **Three legacy tests pin a fixture artefact** (catch-all mock → unparseable response) rather than intended
  behaviour. Visible now instead of invisible, and three dedicated tests already own the failure path.
- **Removing the hooks' `CLAUDE_CODE_SESSION_ID` fallback** means a malformed payload injects nothing rather
  than raising the tracking question. An untracked session is visible and correctable; a session confidently
  attributing time to the wrong task is not. Forced to choose, take the visible failure.
- **`set_last_flush`'s `Ok(false)` ignored, and persistent failure meaning the session never closes** —
  DB-level, and it fails toward keeping the row rather than losing the time.
- **`jobs.rs:216-220` duplicates `:80-84`** (5 lines, observe/report/sleep); extraction needs the sleep
  abstracted first. No test asserts `RetryPolicy::session_reaper()`'s four constants — such a test is
  tautological.
- **Three lookup helpers** (`session_task_id` / `try_session_task_id` / `task_id_to_flush_before_closing`)
  differ on two orthogonal axes. Each is individually correct and now distinguishably documented.
- **`bind_session_flushing_previous` still uses best-effort `flush_task`** — correct: the row stays open and
  `last_flush_at` is unmoved, so `aplan --session <s> flush <prev>` recovers it in full.
- **No backup of the pre-plan-3 `aplan` binary.** `aplan.bak-20260804` predates plan 1 (no `session`
  subcommand). Rollback is `cargo build --release` from `2a14ef7`.
- **`~/.claude/hooks/.aplan-session-start-payload.log`** remains on disk (8 lines) — the user's data, and the
  evidence that settled whether the payload carries `session_id`. It does.

## What this plan fixed that was not in its scope

Three pre-existing **permanent data-loss** paths, all found because the session work made them visible:

1. **`aplan session end` closed a session without flushing.** Once the row closes, no window ever selects
   those entries again. Exposed when task 3 duplicated the block and the two copies disagreed.
2. **`aplan flush --session X <task>` parsed the flag and silently flushed the *human's* window** —
   `commands.rs` passed `session_id: None` as a literal.
3. **`session off` dropped the task binding without flushing.** Two wrong diagnoses preceded the real
   mechanism: `set_session_mode` clears `task_id`, so nothing afterwards can find the task to flush.

## What a maintainer should preserve

The final review's answer, and it was not about the code: **the comment culture.**
`SESSION_IDLE_TIMEOUT_RANGE` derives its own upper bound from chrono's arithmetic and names the silent-death
failure mode it prevents. `end_session_flushing_first` explains why it does *not* use `flush_task` when its
siblings do. The `-z "$mode"` gate states in place why the one legitimate pointer read is legitimate. Every
one of those comments is why a later reader will not undo the decision — and this plan had to reverse three
"helpful" corrections that a missing comment invited.
