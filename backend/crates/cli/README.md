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

## Semantic memory

```bash
aplan brief                                   # session brief, capped at 40 lines
aplan remember "Wave 0 limited to MS AI" --kind decision --why "Pierre asked"
aplan recall --q "AP-1234"                    # search; raw input is safe
aplan inbox                                   # the validation queue
aplan inbox accept <id>                       # or merge --into / supersede --replaces / reject
aplan memory supersede <old> --by <new>       # revise an already-active memory
```

## Consolidation (driven by a scheduled Claude session)

The 17:30 consolidation is **not** a backend job — the backend holds no model. A
scheduled Claude Code session drives these three verbs; its instruction set lives
in `docs/prompts/consolidation-memoire.md`, outside the binary so it can be
iterated without recompiling.

```bash
aplan consolidate pending --json      # entries never consolidated, oldest first.
                                      # Read-only, so it doubles as the reachability
                                      # probe: if it fails, do nothing and mark nothing.
aplan consolidate mark <id>… --json   # LAST, after the memories are persisted
aplan consolidate record-run --json   # so `aplan brief` can see the job is alive
```

The order matters: marking before writing trades a recoverable failure (a duplicate
candidate, which a rejection turns into a tombstone) for an unrecoverable one (an
entry marked that never produced anything).

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
- `4` precondition failed — `aplan note` with no current task; and on the memory
  verbs, a state the store refuses to leave: a candidate already active or
  rejected, a merge target that is not active, an already-invalidated memory, a
  supersession cycle, a query with nothing searchable in it.

`1` versus `4` is load-bearing for automated callers: `4` means "skip this one",
`1` means "the call never landed — retry the whole run and write no watermark".

## Refreshing the GraphQL schema

After backend changes, re-export the SDL:

```bash
cd backend
cargo run -p api -- export-schema > crates/cli/graphql/schema.graphql
```

The CLI build will fail if any operation no longer matches the schema.
