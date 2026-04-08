# `aplan` — Aggregated Plan command-line cockpit

A keyboard-first CLI client for the Aggregated Plan backend. Talks HTTP to
`http://127.0.0.1:3001/graphql` (override via `APLAN_API_URL` or `--api-url`).

## Install

```bash
cd backend
cargo install --path crates/cli
```

This puts an `aplan` binary on your `$PATH` (typically `~/.cargo/bin/aplan`).

## Hot path

```bash
aplan start AP-1234           # start a worklog on a Jira-keyed task
aplan note "thoughts here"    # append to the currently-tracked task
aplan status in_progress      # change status of the current task
aplan done                    # mark done + stop the timer
aplan stop                    # stop the timer without changing status
aplan triage followed AP-1234 # set tracking state on an inbox item
```

## Discovery

```bash
aplan current                 # what am I working on?
aplan ls                      # followed, not-done tasks
aplan show AP-1234            # full detail of a task
aplan dash                    # daily dashboard summary
aplan matrix                  # Eisenhower priority matrix
aplan journal                 # today's activity slots
aplan alerts                  # unresolved alerts
```

## Less-frequent

```bash
aplan new "Title" --deadline 2026-04-15 --urgency high --impact high
aplan rm <task>
aplan priority <task> --urgency high --impact critical
aplan priority <task> --reset
aplan sync --source jira
aplan resolve <alert>
aplan config get
aplan config set general.working_hours 8
```

## Task identifier resolution

Wherever a command takes a TASK argument the same resolver runs:

1. Empty / `@` / `current` → the currently-tracked activity's task. Exits 4
   if no worklog is running or the running slot has no task.
2. UUID → used directly.
3. Jira-style key (`^[A-Z][A-Z0-9]*-\d+$`, e.g. `AP-1234`, `INFRA-42`) →
   exact match on `tasks.source_id`.
4. Anything else → fuzzy match against task titles via `titleContains`. One
   hit wins; zero hits exits 2; multiple hits exits 3 with up to 5 candidates
   printed and a suggestion to be more specific.

## Output

Default: terse human output, one line per action.
`--json`: emits the raw GraphQL `data.*` payload — used by the Claude skill.

## Exit codes

- `0` success
- `1` generic error (network, GraphQL, parse)
- `2` not found
- `3` ambiguous lookup (more than one fuzzy match)
- `4` precondition failed (e.g. `aplan note` with no current task)

## Refreshing the GraphQL schema

After backend changes, re-export the SDL:

```bash
cd backend
cargo run -p api -- export-schema > crates/cli/graphql/schema.graphql
```

The CLI build will fail if any operation no longer matches the schema.
