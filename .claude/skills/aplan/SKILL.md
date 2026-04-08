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

## Exit code handling

| Code | Meaning | What to do |
|---|---|---|
| `0` | success | parse `data.*` and proceed |
| `1` | generic error (network/GraphQL) | tell the user, don't retry blindly |
| `2` | not found | the task or alert doesn't exist; ask the user for a better identifier |
| `3` | ambiguous fuzzy match | re-run with a more specific query, or ask the user which match they meant |
| `4` | precondition failed | usually `aplan note` / `aplan status` with no running worklog and no `--task` — ask the user to start one or pass `--task` |

When you get exit `3`, the stderr lists up to 5 candidates with their key and
title. Use that list to ask the user which one they meant.

## Failure mode: API unreachable

If you see `error: cannot reach API at http://127.0.0.1:3001/graphql`, the
backend isn't running. Tell the user and suggest:

```
cd backend && cargo run -p api
```

Don't try to run the backend yourself.

## Things you must NOT do

- Don't `curl` the GraphQL endpoint directly — use `aplan`.
- Don't read or write `backend/aggregated_plan.db` (the SQLite file). The
  CLI is the only supported path.
- Don't invent new subcommands. If a user asks for something the CLI doesn't
  expose, say so and offer to add it (which means a code change, not a
  workaround).
- Don't parse the human output of `aplan`. Always pass `--json`.
