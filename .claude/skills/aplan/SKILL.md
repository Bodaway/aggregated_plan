---
name: aplan
description: Use when the user wants to log work, change task status, take notes against a task, view their dashboard or priority matrix, or otherwise drive their Aggregated Plan cockpit. Invokes the local `aplan` CLI which talks to the backend at 127.0.0.1:3001.
---

# aplan — driving the Aggregated Plan CLI

You are working in the Aggregated Plan repo. The user has a CLI binary called
`aplan` on their `$PATH` that talks to a local GraphQL backend. Use it
**instead of** crafting GraphQL queries by hand or reading the SQLite file
directly. Both of those are wrong.

## Always invoke with `--json`

Every command supports `--json`, which prints the raw GraphQL `data.*` payload
to stdout. Parse that — never parse the human output, which is for the user.

```bash
aplan current --json
# → {"currentActivity":{"id":"...","task":{"id":"...","title":"Auth migration"},...}}
```

## Sessions: your own link, separate from the human's pointer

Two independent actors write to this backend, and they never share state:

- **The human**, working by hand, is `aplan.active_task_id` / `aplan.active_since` —
  one pair of global config keys. `aplan current` and the bare global pointer speak
  for the human, never for a session.
- **You**, this Claude Code session, are one row in the `sessions` table, keyed by
  `CLAUDE_CODE_SESSION_ID`. The harness exports that variable into every Bash call, so
  the global `--session` flag every command accepts defaults to it and you never pass
  it yourself. Your row carries its own task and its own flush watermark — several
  sessions can run at once, on different tasks, without touching each other or the
  human's pointer.

```bash
aplan sessions --json             # every open session + the human's pointer, read-only
aplan session show --json         # THIS session's own link: task, mode, endedAt
aplan session bind --json <task>  # link THIS session to <task>. Never touches the human's pointer.
aplan session off --json          # disable aplan logging for this session, persistently
aplan session end --json          # close this session's row — a deliberate act, not automatic
```

The SessionStart hook already made this session's choice (a bound task, or "ne pas
tracker") and told you in its injected context. Don't re-derive it from `aplan
current` — that answers for the human — and don't ask the user again.

**`aplan start` / `aplan stop` are `aplan session bind` / `aplan session end` under
another name** whenever a session id is present, which inside a Claude Code session
is always. If the user wants to switch tasks mid-session, run `aplan start <task>`
exactly as in the hot-path table below: it rebinds *this session's own* link
(flushing the task it leaves first) and leaves the human's pointer untouched.
Subsequent `aplan log` / `aplan note` / `aplan status` calls with no `--task`
retarget automatically once the bind lands. `aplan stop` is the mirror image — it
closes this session's row after flushing it, a bigger and less reversible act than
"pause the timer"; if the user only wants logging paused rather than the session
ended, use `aplan session off` instead.

## Hot-path recipes

| User intent | Command |
|---|---|
| "log a note about X" (active worklog) | `aplan note --json "X"` |
| "log a note on AP-1234" | `aplan note --json --task AP-1234 "X"` |
| "start working on AP-1234" | `aplan start --json AP-1234` |
| "what am I working on" | `aplan current --json` |
| "stop the timer" | `aplan stop --json` |
| "mark this done" | `aplan done --json` |
| "set the status to in_progress" | `aplan status --json in_progress` |
| "triage AP-1234 as followed" | `aplan triage --json followed AP-1234` |
| "that time went on the wrong task" | `aplan reattribute --json --from <wrong> --to <right> --date <day>` (preview first) |

If you are a **subagent**, `start`, `new`, `stop`, `done`, `flush` and `triage` in this
table are forbidden — see "If you are a subagent" below.

## Discovery commands (read-only, safe to ground yourself)

```bash
aplan ls --json                 # followed, non-done tasks (compact list)
aplan show --json <task>        # full detail of one task
aplan dash --json               # daily summary: tasks, meetings, alerts
aplan matrix --json             # Eisenhower priority matrix
aplan journal --json            # today's activity slots
aplan alerts --json             # unresolved alerts
```

The `<task>` argument accepts a UUID, a Jira-style key (`AP-1234`), a fuzzy
title substring (`"auth migra"`), or `@` / `current` for the currently-tracked
task. **Default to `@` for any verb that has an implicit current target** —
notes, status, done.

## Less-frequent operations

```bash
aplan new --json "Title" --deadline 2026-04-15 --urgency high --impact high
aplan rm --json <task>
aplan priority --json <task> --urgency high --impact critical
aplan priority --json <task> --reset
aplan sync --json --source jira
aplan resolve --json <alert>
aplan config get --json
aplan config set --json <KEY> <VALUE>
```

## Correcting a wrong attribution (`aplan reattribute`)

`aplan log` writes to the task it is given, so time logged against the wrong task
stays wrong — and it flows into the timesheet and on to the client invoice. This is
the verb that fixes it: it moves the worklog entries **and** re-derives the activity
slots that came from them.

```bash
# Preview — resolves both tasks, reports the hours, writes NOTHING:
aplan reattribute --json --from <wrong> --to <right> --date 2026-08-03
# Apply exactly what the preview showed:
aplan reattribute --json --from <wrong> --to <right> --date 2026-08-03 --confirm

aplan reattribute --json --from <wrong> --to <right> --since 2026-08-01 --until 2026-08-03
aplan reattribute --json --from <wrong> --to <right> --entry 7c1 --entry 9ab
```

**Always show the user the preview and get their agreement before adding
`--confirm`.** This rewrites billing-relevant history: `--from`/`--to` accept fuzzy
titles, so a mistyped token resolves to a task nobody named, and `--confirm` on a
wrong resolution moves the wrong day of work.

Read the preview's `source`/`destination` `hoursBefore`/`hoursAfter` and
`slotsDiscarded`/`slotsRebuilt` back to the user. If the pair's total changes, say so
and why: a partial move re-spans both tasks' half-days, and a half-day carrying slots
the worklog does not account for is rebuilt from what the entries now say — one slot
per continuous stretch of work, cut wherever two consecutive entries are more than
forty-five minutes apart.

Exit codes: `2` unknown task or entry, `3` ambiguous reference (re-run with more
characters — never guess), `4` a refusal that wrote nothing (same source and
destination, an entry belonging to another task, an empty selection, a window at the
1 000-entry page cap: narrow it and correct in several passes).

After a `--confirm`, tell the user to re-run `aplan timesheet --date <day>`: the
draft was reconstructed before the correction.

## Memory consolidation (scheduled sessions only)

If you are a **scheduled** session running the 17:30 memory consolidation, your
instructions are `docs/prompts/consolidation-memoire.md` — read it and follow it,
it is more specific than this skill. Three verbs:

```bash
aplan consolidate pending --json      # worklog entries never consolidated, oldest
                                      # first. Read-only, so run it FIRST as the
                                      # reachability probe: if it fails, do nothing
                                      # at all and mark nothing.
aplan consolidate mark <id>… --json   # LAST, only after the memories are written
aplan consolidate record-run --json   # record the run, even if nothing was proposed
```

In an ordinary session, don't run these. Marking entries outside a consolidation
pass makes them invisible to the next one, and that is not recoverable.

## Exit code handling

| Code | Meaning | What to do |
|---|---|---|
| `0` | success | parse `data.*` and proceed |
| `1` | generic error (network/GraphQL) | tell the user, don't retry blindly |
| `2` | not found | the task or alert doesn't exist; ask the user for a better identifier. Also `aplan session show` on a session id aplan has never heard of — the one place an unknown session id **is** an error (with `--json` it's still exit `0` and `{"claudeSession":null}`; see "Sessions" above) |
| `3` | ambiguous fuzzy match | re-run with a more specific query, or ask the user which match they meant |
| `4` | precondition failed | `aplan log` / `aplan note` / `aplan status` with no running worklog, no `--task`, and no session bound to a task — ask the user to start one or pass `--task`. Also this session refusing outright: it's `off`, has no task bound, or has ended — `aplan session bind <task>` fixes all three, since a bind is a request to work and reopens an ended row. On the memory verbs `4` means a state the store refuses to leave (candidate already active or rejected, merge target not active, memory already invalidated, supersession cycle, nothing searchable in the query): **skip that item, never `--force`** |

`1` and `4` must not be treated alike. `4` is a normal outcome to skip; `1` means
the call never landed, so anything that depends on it has to be retried whole.

When you get exit `3`, the stderr lists up to 5 candidates with their key and
title. Use that list to ask the user which one they meant.

**An unknown session id on `log`/`note`/`status` is not an error.** Those three
resolve their implicit target in order — `--task`, then the session, then the
human's pointer — and an id with no matching row carries no decision to honour, so
it falls through to the pointer exactly as an absent `--session` would: exit `0`,
and the write lands on the human's task with no `session_id` attached, not on the
session. That is different from the three `4`s above, which fire only for a session
id aplan *does* recognise. `SessionUnknown` (exit `2`) exists only for `aplan
session show`, where the user asked about that id directly and a silent fallback
would hide a typo instead of reporting it.

**`aplan remember` never refuses on any of this.** `--task` wins if given, else a
tracking session's task attaches, else the memory is stored unattached — even for
a session that ran `aplan session off`. That is deliberate, not an inconsistency to
fix: memories sit outside the worklog rules, and an unattached memory misattributes
nothing, where a misattributed worklog entry is billable time landing on the wrong
task.

## Failure mode: API unreachable

If you see `error: cannot reach API at http://127.0.0.1:3001/graphql`, the
backend isn't running. Tell the user and suggest:

```
cd backend && cargo run -p api
```

Don't try to run the backend yourself.

## If you are a subagent: never move the pointer, never materialise time

**A subagent must never run `aplan start`, `aplan new`, `aplan stop`, `aplan done`,
`aplan flush` or `aplan triage`.** Read freely — `ls`, `show`, `current`, `dash`,
`brief`, `recall` are all safe — but do not write anything that touches the
active-task pointer or turns worklog entries into activity slots.

The reason, not just the rule: `aplan.active_task_id` is **one** value, shared by
every process that talks to this backend. It belongs to the parent session, which set
it to the task the user is actually working on. When a subagent calls `aplan start` on
a task it picked out of `aplan ls`, it silently repoints that single value — and from
then on every `aplan log` in the session, and the `SessionEnd` flush, attribute the
parent's work to the subagent's task. `aplan stop` / `done` / `flush` do the mirror
image: they materialise slots and clear the pointer under the parent's feet.

This has already happened in this repo: an agent read this skill, ran `aplan start` on
a task it found interesting, and redirected roughly 4h35 of another session's time onto
it. Nothing failed, nothing warned, and the wrong attribution reached the timesheet.
Undoing it needs `aplan reattribute` and a human who noticed.

`aplan note` / `aplan log` are the exception: they write to a task, not to the pointer,
and Claude's own journalling depends on them. If a subagent needs to log against a
specific task, pass `--task <id>` explicitly rather than relying on — or changing —
whatever the pointer happens to be. If the work genuinely requires starting, stopping
or triaging a task, say so and let the parent session (or the user) do it.

## Things you must NOT do

- Don't `curl` the GraphQL endpoint directly — use `aplan`.
- Don't read or write `backend/aggregated_plan.db` (the SQLite file). The
  CLI is the only supported path.
- Don't invent new subcommands. If a user asks for something the CLI doesn't
  expose, say so and offer to add it (which means a code change, not a
  workaround).
- Don't parse the human output of `aplan`. Always pass `--json`.
