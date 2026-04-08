# `aplan` CLI — Design Spec

## Goal

A keyboard-first command-line client for the Aggregated Plan cockpit that drives the existing GraphQL backend over HTTP. Optimised for the tech-lead hot path: **start a worklog, change task status, take a fast note** — without leaving the terminal. A companion Claude Code skill teaches Claude to use the CLI instead of crafting GraphQL by hand.

## Why a CLI when an MCP server already exists

The existing `aggregated-plan-mcp` binary (`backend/crates/mcp/`) talks **directly to SQLite** via the application/infrastructure crates. It's coupled to the database file and the host machine.

The CLI takes the opposite path: it speaks **HTTP/GraphQL** to the running `api` server, so it shares one source of truth with the frontend, never races on the SQLite file, and works from any shell session as long as the backend is up. Humans get a fast terminal UX; Claude (via the skill) gets a stable, parseable surface that doesn't depend on an MCP transport.

## Topology & assumptions

- **Loopback only.** Backend and CLI run on the same machine (`127.0.0.1:3001`). No auth, no TLS, no profiles.
- **Single user.** The `auth_middleware` continues to inject the default user; the CLI never sends a token.
- **Endpoint override** via `APLAN_API_URL` env var or `--api-url` flag, in case the user later moves the backend to a non-default port.

## Architecture

A new crate `backend/crates/cli/`, sibling of `api`, `mcp`, `application`, `domain`, `infrastructure`. Produces a single binary `aplan` installed via `cargo install --path backend/crates/cli`.

### Crate layout

```
backend/crates/cli/
├── Cargo.toml
├── build.rs                # graphql-client codegen against schema.graphql
├── graphql/
│   ├── schema.graphql      # exported from the api crate
│   └── *.graphql           # one operation file per query/mutation used
└── src/
    ├── main.rs             # entrypoint, dotenvy, dispatch
    ├── cli.rs              # clap derive: Cli + Commands enum
    ├── client.rs           # thin reqwest wrapper around graphql-client
    ├── lookup.rs           # task identifier resolution
    ├── output.rs           # human / JSON formatters, exit codes, colors
    └── commands.rs         # one fn per subcommand
```

### Dependencies

| Crate | Purpose |
|---|---|
| `clap` (derive) | Argument parsing |
| `reqwest` (blocking) | HTTP client; blocking is enough for a one-shot CLI |
| `graphql-client` | Compile-time-checked GraphQL operations |
| `serde`, `serde_json` | Response payloads |
| `anyhow` | Error wrapping in `commands.rs` |
| `thiserror` | Typed errors at the boundary that maps to exit codes |
| `owo-colors` | Auto-disabling ANSI colors |
| `dotenvy` | Pick up `APLAN_API_URL` from `.env` if present |

Cold start target: **< 10 ms** for the no-op help path. Hot-path commands cost at most **two** round-trips: when the target is implicit, the CLI fetches `currentActivity` first to discover the task id, then issues the mutation. When the target is explicit (UUID, key, fuzzy match, or `--task`), the lookup mutation is the only call. Both calls are loopback so combined latency is dominated by process start, not network.

### Schema export

A new subcommand on the `api` binary writes the SDL to a known path:

```bash
cargo run -p api -- export-schema > backend/crates/cli/graphql/schema.graphql
```

The exported file is committed. `cli/build.rs` invokes `graphql-client`'s codegen against it, so a backend rename or removal of a field breaks `cargo build` of the CLI before anyone ships a broken release. Refresh procedure is documented in the CLI crate's README.

## Command surface

Every command supports:

- `--json` — emit the raw GraphQL `data.*` payload (no glyphs, no colors).
- `--api-url <URL>` / `APLAN_API_URL` — endpoint override.
- `-v / --verbose` — log the request URL, operation name, and elapsed time to stderr.

### Hot path

| Command | Behaviour | Maps to |
|---|---|---|
| `aplan start <TASK>` | Start a worklog on TASK. Server already auto-stops any running activity before starting the new one. | `startActivity(taskId:)` |
| `aplan stop` | Stop the current worklog. Prints duration. | `stopActivity` |
| `aplan note <TEXT...>` | Append a markdown note to the **currently-tracked task**. `--task <T>` overrides. Variadic: `aplan note one two three` joins with spaces. | `currentActivity` + `appendTaskNotes` |
| `aplan status <STATE>` | Set task status (`todo`/`in_progress`/`done`/`blocked`) on the currently-tracked task. `--task <T>` overrides. | `updateTask(status:)` |
| `aplan triage <STATE> <TASK>` | Set tracking_state (`inbox`/`followed`/`dismissed`). TASK is **required** — these are typically inbox items, no implicit "current". | `setTrackingState` |
| `aplan done [TASK]` | Shortcut: status=Done **and** stop the running activity if it targets this task. `--keep-running` to skip the stop. | `completeTask` (+ `stopActivity` when applicable) |

### Read commands

| Command | Output |
|---|---|
| `aplan current` | Running activity slot + its task title, one line. Empty + exit 0 if nothing running. |
| `aplan ls [--status X] [--triage Y] [--project P]` | Task list. Default filter: `triage=followed` and `status≠done`. |
| `aplan show <TASK>` | Full task detail, including notes. |
| `aplan dash [--date YYYY-MM-DD]` | Daily dashboard summary (tasks, meetings, alerts). |
| `aplan matrix` | Eisenhower matrix grouped by quadrant. |
| `aplan journal [--date]` | Activity slots for the day. |
| `aplan alerts [--all]` | Unresolved alerts (or all with `--all`). |

### Less-frequent

| Command | Notes |
|---|---|
| `aplan new <TITLE> [--project P] [--deadline D] [--impact I] [--urgency U] [--hours H]` | Create personal task. |
| `aplan rm <TASK>` | Delete task. |
| `aplan priority <TASK> [--urgency U] [--impact I]` | Override priority levels. |
| `aplan priority <TASK> --reset` | Reset urgency to deadline-derived. |
| `aplan sync [--source jira\|outlook\|excel]` | Force sync; no flag = all sources. |
| `aplan resolve <ALERT>` | Resolve alert. |
| `aplan config get [KEY]` | Print config (single key or all). |
| `aplan config set <KEY> <VALUE>` | Set a config key. |

### Out of scope for v1

- Interactive pickers (ambiguous fuzzy matches print a numbered list and exit non-zero — re-run with a tighter query)
- Tab completion (clap can generate it later via `clap_complete`)
- Login / auth (loopback only)
- State file / last-list aliases / offline buffer
- SSE subscriptions (CLI is one-shot)
- Multi-profile config

## Task lookup resolver

A single function `resolve_task(token: Option<&str>, client: &Client) -> Result<TaskRef>` is shared by every command that takes a TASK argument. Order:

1. **Empty / `@` / `current`** → fetch `currentActivity`. Exit 4 (`PreconditionFailed`) if nothing's running, **or** if the running slot has no `taskId` (started without a task). Otherwise use that task.
2. **UUID** (parses via `Uuid::parse_str`) → use directly.
3. **External-key shape** (`^[A-Z][A-Z0-9]*-\d+$`, e.g. `AP-123`, `INFRA-42`) → look up by `externalKey`. This is a heuristic: it matches Jira-style keys and any other source that adopts the same convention. Sources whose `external_key` doesn't follow this shape (e.g. some Personal/Obsidian items) will fall through to step 4, which is the desired behaviour.
4. **Anything else** → fuzzy match against task titles via `titleContains`.
   - 1 hit → use it.
   - 0 hits → exit 2 (`NotFound`).
   - >1 hits → exit 3 (`Ambiguous`), print up to 5 candidates with their key + title, suggest a more specific query.

Step 3 and 4 require server-side filtering. The current `tasks(filter:)` GraphQL query exposes neither.

### Backend change required

Extend `TaskFilter` (in `application/repositories/task_repository.rs` and the GraphQL `TaskFilterInput`) with:

```graphql
input TaskFilterInput {
  # ...existing fields...
  externalKey: String        # exact match on tasks.external_key
  titleContains: String      # case-insensitive substring on tasks.title
}
```

Resolver, repository SQL, and unit tests get one new branch each. Both filters are also useful for the frontend search bar and the MCP server, so the cost is amortised.

The CLI uses these filters to:

- `externalKey: "AP-123"` → exact lookup for the Jira-key path.
- `titleContains: "auth migra"` → fuzzy candidates for the title path. The CLI then ranks candidates client-side (e.g. by token-set similarity) to pick "1 hit" vs "many hits".

## Output, errors, exit codes

### Default (human) output

Terse, one line per action. ANSI colors auto-disabled when stdout is not a TTY (handled by `owo-colors::if_supports_color`).

```
$ aplan start AP-1234
▶ started: AP-1234 — Auth migration (morning slot)

$ aplan note "lock contention spikes at 30s timeout"
✎ AP-1234: note appended

$ aplan status in_progress
↻ AP-1234: todo → in_progress

$ aplan done
✓ AP-1234 done — timer stopped (1h 47m logged)
```

Read commands print compact tables with simple alignment, no Unicode borders.

### `--json` mode

Prints the raw GraphQL `data.*` payload for the operation. One stable shape per command, no extra wrapping. Glyphs and colors suppressed. This is the form Claude consumes via the skill.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Generic error (network, GraphQL, parse) |
| `2` | Not found (task lookup, alert id, config key) |
| `3` | Ambiguous (fuzzy lookup matched >1 task) |
| `4` | Precondition failed (e.g. `aplan note` with no current task and no `--task`) |

### Error messages are actionable

```
$ aplan note "..."
error: no worklog is currently running
hint: pass --task <jira-key> to target a specific task,
      or start one with `aplan start <task>`

$ aplan start AP-1234
error: cannot reach API at http://127.0.0.1:3001/graphql
hint: is the backend running? try `cargo run -p api`
```

## Claude Code skill

A skill at `.claude/skills/aplan/SKILL.md`, versioned with the repo so it ships to anyone who clones it.

### Frontmatter

```yaml
---
name: aplan
description: Use when the user wants to log work, change task status,
  take notes against a task, view their dashboard or priority matrix,
  or otherwise drive their Aggregated Plan cockpit. Invokes the local
  `aplan` CLI which talks to the backend at 127.0.0.1:3001.
---
```

### Body sections

1. **Always invoke with `--json`** so output is parseable. Parse `data.*` from the response.
2. **Hot-path recipes** — exact commands for the most common asks:
   - "log a note about X" → `aplan note --json "X"` (or `aplan note --json --task AP-123 "X"` if user named one)
   - "start working on AP-123" → `aplan start --json AP-123`
   - "mark this done" → `aplan done --json`
   - "what am I working on" → `aplan current --json`
3. **Discovery commands** Claude can use to ground itself before acting: `aplan ls --json`, `aplan show --json <task>`, `aplan dash --json`, `aplan matrix --json`.
4. **Exit code handling** — what 2/3/4 mean and how to recover (re-run with a more specific query, ask the user which match, etc.).
5. **Failure mode** — if the API is unreachable (exit 1 with the "cannot reach" message), tell the user instead of guessing.
6. **What NOT to do** — don't shell out to `curl` against the GraphQL endpoint directly; don't try to read the SQLite file; the CLI is the only supported path.

The skill is one screen long. It's not a tutorial — it's instructions for an LLM that already understands shell and JSON.

## Testing strategy

### Unit tests (in `cli` crate)

- `lookup.rs`: UUID detection, Jira-key regex, fuzzy ranker, candidate truncation.
- `output.rs`: human formatter snapshots, JSON passthrough, exit code mapping.
- `cli.rs`: clap parser snapshots for every subcommand.

All pure functions, no I/O.

### Integration tests

`wiremock` spins up a mock GraphQL server. Each test:

1. Stubs one or more operations with canned responses.
2. Invokes the CLI binary via `assert_cmd`.
3. Asserts request body, stdout, exit code.

Covers every subcommand at least once: hot path, read path, error path. No real backend required in CI.

### Smoke test

One `#[ignore]`-gated test pointing at a live `cargo run -p api`, used for manual sanity checks before release.

### Backend tests for the new filters

`externalKey` and `titleContains` filters get unit tests in the application layer (filter struct + use case) and infrastructure layer (SQLite `WHERE` clauses).

## Specs to update

Per `CLAUDE.md`, behaviour-affecting changes update the French specs in the same commit:

- **`SPEC_TECHNIQUE.md`** — new section describing the `cli` crate, the `aplan` binary, the schema-export procedure, and the new `TaskFilterInput` fields.
- **`SPEC_FONCTIONNELLE.md`** — short paragraph in the "interfaces" section noting the CLI as a third client (alongside frontend and MCP) and listing the hot-path commands.

## Implementation order (preview)

The `writing-plans` skill will turn this spec into a step-by-step plan. The expected ordering is:

1. **Backend filter additions** (`externalKey`, `titleContains`) — small, isolated, unblocks the CLI lookup.
2. **Schema export** subcommand on `api`.
3. **`cli` crate scaffold** — Cargo.toml, build.rs, empty main, one no-op subcommand to validate the codegen pipeline end-to-end.
4. **Client + lookup resolver** — pure-function plumbing with full unit tests.
5. **Hot path commands** — `start`, `stop`, `note`, `status`, `done`, `triage`, `current`. Each lands with its integration test.
6. **Read commands** — `ls`, `show`, `dash`, `matrix`, `journal`, `alerts`.
7. **Less-frequent commands** — `new`, `rm`, `priority`, `sync`, `resolve`, `config`.
8. **Output polish** — colors, exit codes, error hints.
9. **Claude skill** — `.claude/skills/aplan/SKILL.md`.
10. **Spec updates** — `SPEC_TECHNIQUE.md`, `SPEC_FONCTIONNELLE.md`.
