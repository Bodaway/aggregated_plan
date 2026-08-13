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
aplan log "finding here"      # timestamped worklog entry (drives the time)
aplan log --at 2026-08-06T14:30 "…"  # …for a past day, see below
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

## How the time is derived

`aplan start`/`stop`/`done` open no timer: the time comes from the timestamps of the
worklog entries, materialized into closed activity slots by `aplan flush <task>` (and by
`stop`, `done` and the `SessionEnd` hook).

**The 45-minute rule.** A gap of more than forty-five minutes between two consecutive
entries is time that was *not* spent on the task. The entries of a half-day are
therefore cut into as many slots as there were continuous stretches of work: entries
forty-five minutes apart or less stay in the same slot, a longer pause starts a new one,
and the idle stretch in between is charged to nobody. A slot never straddles the
morning/afternoon boundary, and a stretch reduced to a single entry lasts one minute.

Forty-five and not fifteen because an entry is an *event marker*, not an activity
sample: at one entry per finding, decision or action, forty minutes can pass during a
code read or a build wait without the work stopping.

Practical consequence: a day whose entries are spread thin is worth less than the span
from its first entry to its last. That is the point — but it means `aplan journal` and
the activity report show the stretches, not the span.

## Writing up a past day

`aplan log` stamps the entry `now`, which is right while the work is happening and wrong
when a day is written up afterwards: a batch of entries about Thursday, all posted on
Monday, leaves Thursday billing nothing and puts a near-zero slot on Monday. `--at`
places the entry in time instead.

```bash
aplan log --at 2026-08-06T14:30 "cause racine identifiée"   # that day, that hour
aplan log --at "2026-08-06 14:30" "…"                       # same, quoted form
aplan log --at 14:30 "…"                                    # today at 14:30
aplan log --at 2026-08-06 "…"                               # that day at noon
```

`--at` is read in **your** timezone (`aplan.timezone`) and sent unconverted; the server
does the conversion, so the entry lands on the local day you typed.

**A bare date means noon**, and the reason is the 45-minute rule above run backwards: an
entry is evidence for the three quarters of an hour *before* it, clipped to the working
windows. Noon carries 11:15–12:00, fully inside the morning. Midnight would fall in no
window at all, and even the workday's own 08:00 start casts its shadow over 07:15–08:00,
entirely before the window opens — both would bill zero.

`--at` also **rebuilds that day's activity slots**, and it has to: `aplan flush` works
out which half-days to rebuild from the entries inside its own window, which starts when
the session did, so a backdated entry is invisible to it and its day would keep showing
no hours. The rebuild is reported on its own line:

```text
✎ Audit config Claude Code: worklog entry added (2026-08-06 14:30)
↻ 2026-08-06 afternoon: 1 slot(s) rebuilt
```

If that rebuild fails the command still **succeeds**, because the entry is written and
re-running `log` would duplicate it. The warning names the repair, which is idempotent:

```bash
aplan slots rebuild --task <task> --date 2026-08-06
```

Use `slots rebuild` by hand for a day whose entries were backdated some other way (the
web UI's timestamp editor, `updateWorklogEntry`). It takes no `--confirm`: it only ever
rewrites one task's own half-days from entries that are still there, so running it twice
leaves the same day. Then re-run `aplan timesheet --date <day>` — the draft was
reconstructed before the rebuild.

One thing `--at` does not invent: **hours still come from the spread of the entries.**
Seven entries backdated to the same minute are worth one minute, exactly as they would
be live. Writing up a past day honestly means giving each entry the time it happened at.

## Correcting an attribution

`aplan log` writes to the task it is given, so a day logged against the wrong task
stays wrong — and that time reaches the timesheet and the client invoice.
`aplan reattribute` moves the entries **and** the activity slots derived from them.

```bash
aplan reattribute --from <wrong> --to <right> --date 2026-08-03            # preview
aplan reattribute --from <wrong> --to <right> --date 2026-08-03 --confirm  # apply
aplan reattribute --from <wrong> --to <right> --since 2026-08-01 --until 2026-08-03
aplan reattribute --from <wrong> --to <right> --entry 7c1 --entry 9ab      # single entries
```

**It previews by default.** Without `--confirm` nothing is written: it resolves both
tasks, prints their titles and the before/after hours, and stops. `--confirm` then
applies exactly what was shown — the two run through one code path, so the preview
cannot drift from the write.

`--from`/`--to` take every form a TASK argument takes (see below). `--entry` takes a
full UUID or an id prefix; an ambiguous prefix exits 3 rather than moving the wrong
hour of work.

Slots are **re-derived**, not re-pointed: they are a projection of worklog timestamps
(one slot per continuous stretch of work — see the 45-minute rule above),
so the correction drops the projection of the two tasks in the half-days a moved entry
falls in, and rebuilds it from what the entries now say. Consequences worth knowing:

- A third task working the same half-day is never touched, and a morning is left
  alone when only the afternoon moved.
- A **partial** move re-spans both sides, so the pair's total can change.
- A half-day whose slots the worklog does not account for — several partial flushes
  of the same half-day, a flush whose entries were later edited, or a flush that
  predates the 45-minute rule — is rebuilt from the entries. The output says so; check
  the totals in the preview before confirming.
- A running (open) slot is never deleted.

Refusals: same source and destination, an entry that belongs to another task, an
empty selection and a selection at the 1 000-entry page cap all exit 4 and write
nothing. After applying, re-run `aplan timesheet --date <day>`: the draft was
reconstructed before the correction.

## Semantic memory

```bash
aplan brief                                   # session brief, capped at 40 lines
aplan remember "Wave 0 limited to MS AI" --kind decision --why "Pierre asked"
aplan remember "Wave 0 extended" --kind decision --contradicts m:7c1
                                              # ^ proposes a supersession; invalidates NOTHING
aplan recall --q "AP-1234"                    # search; raw input is safe
aplan inbox                                   # the validation queue, conflicts shown inline
aplan inbox accept <id>                       # or merge --into / reject
aplan inbox supersede <id>                    # --replaces defaults to what the candidate proposes
aplan memory supersede <old> --by <new>       # revise an already-active memory
```

Every id above takes the short reference the brief prints (`m:7c1`, `[m:7c1]`, `7c1`)
as well as a full UUID. `--contradicts` is refused next to `--confirm`: a proposal is
a question for the validation queue, and a confirmed memory never enters it.

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

## Sessions — who is writing

Two natures of actor share one cockpit. **The global pointer is you, working by hand**:
`aplan.active_task_id`, one task at a time, unchanged. **A session is a Claude**: one row per
Claude Code session, each with its own task, so several sessions can log against different tasks
at once without overwriting each other's pointer.

```
aplan sessions                        # the open sessions: task, label, since, last write, mode
                                      # plus one display-only line for your own pointer
aplan session show                    # what this session is linked to — what the hook reads
aplan session bind <task>             # link this session (does not move your pointer)
aplan session off                     # disable aplan logging for this session, persistently
aplan session end                     # close this session
```

`--session <ID>` is global and defaults to `CLAUDE_CODE_SESSION_ID`, which the harness exports
into every Bash call — so a Claude never passes it, and a plain terminal never has it.

Target resolution for `log`, `note`, `status`, `done`:

1. `--task` wins.
2. Otherwise the session's task. **If the session is in `off` mode, the command refuses with
   exit 4** rather than falling back. That refusal is the point: silently falling back to the
   global pointer is how a Claude ends up reporting work on a task you declined.
3. Otherwise the global pointer — you.

`remember` is the deliberate exception: it never refuses. `--task` wins, else a tracking session
attaches the memory to its task, else the memory is created unattached and the command succeeds —
including for an `off` session. Memories sit outside the worklog rules, and an unattached memory
misattributes nothing, where a misattributed worklog entry is billable time on the wrong task.

`aplan start` and `aplan stop` still act on the global pointer, not on the session.

## Output

Default: terse human output, one line per action.
`--json`: emits the raw GraphQL `data.*` payload — used by the Claude skill.
`aplan current --json` also carries `actor`: the session id, or `manual`.

## Exit codes

- `0` success
- `1` generic error (network, GraphQL, parse)
- `2` not found
- `3` ambiguous lookup (more than one fuzzy match)
- `4` precondition failed — `aplan note` with no current task; on the memory
  verbs, a state the store refuses to leave: a candidate already active or
  rejected, a merge target that is not active, an already-invalidated memory, a
  supersession cycle, a query with nothing searchable in it; and on
  `aplan reattribute`, a selection it refuses to act on: same source and
  destination, an entry belonging to another task, nothing selected, nothing
  matched, or a window at the page cap. On the worklog verbs with a session, one of three
  refusals: the session is not tracked (`aplan session off` was answered), it has no task bound,
  or it has ended. An unknown session id exits `2`, not `4` — it is a lookup failure, not a state.

`1` versus `4` is load-bearing for automated callers: `4` means "skip this one",
`1` means "the call never landed — retry the whole run and write no watermark".

## Refreshing the GraphQL schema

After backend changes, re-export the SDL:

```bash
cd backend
cargo run -p api -- export-schema > crates/cli/graphql/schema.graphql
```

The CLI build will fail if any operation no longer matches the schema.
