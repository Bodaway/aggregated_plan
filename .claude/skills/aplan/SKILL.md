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
aplan session off --json          # disable aplan logging for this session, persistently —
                                   # also forgets the bound task; resuming means naming one again
aplan session end --json          # close this session's row now — SessionEnd never does this on
                                   # its own; the idle reaper is the only automatic closer
```

The SessionStart hook's injected context is authoritative for this session, and it
does one of two things: either it reports a choice already recorded on this
session's row (a bound task, or "ne pas tracker" — obey it, don't ask the user
again), or, when no choice is recorded yet, it hands you a **mandatory**
`AskUserQuestion` to make one — obey that instruction just as strictly; suppressing
it is how a session ends up with neither a task nor a recorded "ne pas tracker",
silently. Either way, never re-derive tracking state from `aplan current`, which
answers for the human, not for this session.

**`aplan start` / `aplan stop` are `aplan session bind` / `aplan session end` under
another name** whenever a session id is present, which inside a Claude Code session
is always. If the user wants to switch tasks mid-session, run `aplan start <task>`
exactly as in the hot-path table below: it rebinds *this session's own* link
(flushing the task it leaves first) and leaves the human's pointer untouched.
Subsequent `aplan log` / `aplan note` / `aplan status` calls with no `--task`
retarget automatically once the bind lands. `aplan stop` is the mirror image — it
closes this session's row after flushing it, a bigger and less reversible act than
"pause the timer"; if the user only wants logging paused rather than the session
ended, use `aplan session off` instead — though "paused" undersells it: `off` also
forgets the bound task, so resuming means naming one again with `aplan session
bind`, not just flipping back to tracking.

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

If you are a **subagent**, `start`, `stop`, `done` and `triage` in this table are
forbidden — see "If you are a subagent" below, which also covers `aplan flush` and
the writing `session` subcommands, none of which appear above.

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
task — which is the **human's** `aplan.active_task_id`, always, on every verb.
**For a verb with an implicit target — `log`, `note`, `status`, `done` — omit
`--task` entirely** and let it resolve through the session (see "Sessions"
above): that is what attributes the write to this session rather than to the
human. Pass `--task @` (or `--task current` — the same token, mechanically) only
when you deliberately mean the human's own pointer, and know the write then lands
unattributed to any session.

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
| `4` | precondition failed | `aplan log` / `aplan note` / `aplan status` with no running worklog, no `--task`, and no session bound to a task — ask the user to start one or pass `--task`. Also this session refusing outright: it's `off`, has no task bound, or has ended — `aplan session bind <task>` fixes all three, since a bind is a request to work and reopens an ended row. `aplan session show|bind|off|end` with no session id at all (no `--session`, no `CLAUDE_CODE_SESSION_ID`) is the same code, for the same reason: nothing to act on. On the memory verbs `4` means a state the store refuses to leave (candidate already active or rejected, merge target not active, memory already invalidated, supersession cycle, nothing searchable in the query): **skip that item, never `--force`** |

`1` and `4` must not be treated alike. `4` is a normal outcome to skip; `1` means
the call never landed, so anything that depends on it has to be retried whole.

When you get exit `3`, the stderr lists up to 5 candidates with their key and
title. Use that list to ask the user which one they meant.

**An unknown session id on `log`/`note`/`status` is not an error.** Those three
resolve their implicit target in order — `--task`, then the session, then the
human's pointer — and an id with no matching row carries no decision to honour, so
it falls through to the pointer exactly as an absent `--session` would: if the
pointer names a task, exit `0` and the write lands there with no `session_id`
attached; if it does not, the ordinary `4` (no active worklog) fires instead — the
fallthrough is not itself an error, but it still needs somewhere to land. Either way
this is different from the three `4`s above, which fire only for a session
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

## If you are a subagent: never touch the parent session's row, never materialise its time

**A subagent must never write to a session's own link, under any name.** That
currently means `aplan start`, `aplan stop`, `aplan done`, `aplan flush`, and any
*writing* `aplan session` subcommand — `bind`, `off`, `end`. Read freely — `ls`,
`show`, `current`, `dash`, `brief`, `recall`, `sessions`, and `aplan session show`
are all safe. `aplan new` and `aplan triage` are also off-limits, for an unrelated
reason given at the end of this section.

The reason, not just the rule: nothing in the environment tells `aplan` a subagent
apart from its parent. The only variable set for a subagent is
`CLAUDE_CODE_CHILD_SESSION=1`, and it is **also set in the main thread**, so it
cannot be used to distinguish the two — every `aplan` call a subagent makes carries
the same `CLAUDE_CODE_SESSION_ID` as its parent, because that is what the harness
exports, and resolves to the **parent session's own row**, not a row of its own.
`aplan start` on a task the subagent picked out of `aplan ls` silently rebinds the
parent's tracking to it. `aplan stop` closes the parent's session outright (after
flushing it — see "Sessions" above). `aplan done` completes, and flushes, whatever
task the parent's session happens to be bound to, whether or not the subagent meant
to touch it. `aplan flush` materialises the parent's worklog time into slots ahead of
when the parent's own session would have. `aplan session bind` / `aplan session end`
are `start` / `stop` under another name — the same two hazards, spelled differently.
`aplan session off` has no bare-verb equivalent, but declaring "ne pas tracker" for
the parent — and clearing its bound task while doing so — is exactly as much the
parent's decision to make as the other four.

This has already happened in this repo: an agent read this skill, ran `aplan start` on
a task it found interesting, and redirected roughly 4h35 of another session's time onto
it. Nothing failed, nothing warned, and the wrong attribution reached the timesheet.
Undoing it needs `aplan reattribute` and a human who noticed.

`aplan note` / `aplan log` are correct for a subagent to run, not a tolerated
exception: they write to a task, not to the session's row, and work a subagent does
genuinely belongs to the parent session that spawned it — that attribution is the
point, not a bug waiting to be closed. If a subagent needs to log against a specific
task instead, pass `--task <id>` explicitly rather than relying on whichever task the
parent session happens to be bound to. If the work genuinely requires starting,
stopping, flushing or completing a task, say so and let the parent session (or the
user) do it.

`aplan new` and `aplan triage` are forbidden for a different reason: neither touches
the session or the pointer at all. Creating a task, or changing what surfaces in the
human's queue, is a judgement call — the parent's or the user's to make, not a
subagent's to decide on its own.

## Things you must NOT do

- Don't `curl` the GraphQL endpoint directly — use `aplan`.
- Don't read or write `backend/aggregated_plan.db` (the SQLite file). The
  CLI is the only supported path.
- Don't invent new subcommands. If a user asks for something the CLI doesn't
  expose, say so and offer to add it (which means a code change, not a
  workaround).
- Don't parse the human output of `aplan`. Always pass `--json`.
