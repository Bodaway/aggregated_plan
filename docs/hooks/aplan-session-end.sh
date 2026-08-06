#!/usr/bin/env bash
# SessionEnd hook: materialize THIS Claude session's worklog time into closed
# activity slots, against that session's own flush window.
#
# What it reads is THIS SESSION'S OWN ROW (`aplan session show`), never the global
# `aplan.active_task_id` pointer. The pointer means something else: the human,
# working by hand. Reading it here was the original defect — every Claude
# SessionEnd flushed whatever the *human* was tracking and advanced the *human's*
# watermark, consuming a window that belonged to someone else's work.
#
# It deliberately does NOT end the session. A Claude session id survives
# `claude --resume`, so closing the row here would make the resumed session start
# against an `ended` row — a state `Session::target()` refuses by name
# (domain/src/types/session.rs:144-152) and one the SessionStart hook's branch
# table does not have. The idle-session reaper is the sole closer; the flush being
# an idempotent rebuild is what makes the reaper's later second flush harmless.
# `aplan --session <id> stop` still ends a session: a deliberate act by whoever is
# driving, not a lifecycle event.
#
# Claude Code passes a JSON payload on stdin with `session_id` and a `reason`
# ("clear" | "resume" | "logout" | "prompt_input_exit" | "other" | …). The reason
# gates nothing: every way a session ends leaves the same time to materialize.
# Nothing is printed — SessionEnd's stdout is not consumed by the harness.

set -u

command -v aplan >/dev/null 2>&1 || exit 0
command -v jq    >/dev/null 2>&1 || exit 0

# Every `aplan` call carries reqwest's own 10 s timeout (cli/src/client.rs:42) and
# this hook is registered with a 10 s budget, so a backend that accepts the
# connection and then hangs would spend the entire budget on the first call.
# Bound each call instead: two calls at 3 s bounds it at 6 s and, measured, at 3 s
# — a hang on `session show` ends the hook, so only one call can burn its budget.
# 5 s each would sit exactly on the 10 s limit. A missing `timeout` must not silently
# disable the hook, hence the fallback — reqwest's own timeout still applies there.
if command -v timeout >/dev/null 2>&1; then
  aplan_bounded() { timeout 3 aplan "$@"; }
else
  aplan_bounded() { aplan "$@"; }
fi

# Read stdin without hanging: Claude Code closes stdin so `cat` sees EOF at once;
# an empty or non-JSON payload must never crash.
payload=$(cat)

# The payload is the ONLY source of the session id. There was a
# `CLAUDE_CODE_SESSION_ID` fallback here and removing it was deliberate — please do
# not helpfully re-add it. The harness exports that variable into tool
# subprocesses, so a nested `claude` whose payload carried no `session_id` would
# inherit its PARENT's id and flush the parent's task against the parent's window:
# this defect's exact shape, one actor over. The fallback existed only because the
# payload's contents could not be verified statically, and the live payload log has
# since settled that (keys: cwd, hook_event_name, session_id, source,
# transcript_path). Absence is handled by the guard below, and a silent no-op is
# the right failure here — a flush aimed at the wrong session is not.
sid=$(printf '%s' "$payload" | jq -r '.session_id // empty' 2>/dev/null || echo '')

# An empty --session is NOT harmless: by an established contract an empty session
# id falls back to the human's global pointer, which is how hooks outside any
# Claude session set the variable (cli/tests/integration.rs:1379-1414). Passing
# one here would flush the human's task against the human's window — the very
# defect this hook was rewritten to remove. Do nothing instead.
[ -z "$sid" ] && exit 0

# Three outcomes that must not be conflated, and picking the wrong one is silent:
#   - non-zero exit                -> backend unreachable / CLI error: do nothing
#   - exit 0, no `claudeSession` key -> the output shape changed: do nothing
#   - exit 0, `claudeSession` null   -> no row for this session: nothing to flush
# `--json` prints the response's `data` block, so the path is `.claudeSession`
# (no `.data` wrapper), and `mode` is the GraphQL enum: uppercase TRACKING / OFF.
if ! session_json=$(aplan_bounded session show --session "$sid" --json 2>/dev/null); then
  exit 0
fi
printf '%s' "$session_json" | jq -e 'has("claudeSession")' >/dev/null 2>&1 || exit 0

# Which task this session's time belongs to is exactly `Session::target()`
# (domain/src/types/session.rs:144-152), and the order matters for the same reason
# it does there:
#   - ended:  whoever closed the row flushed it first, and an ended row refuses
#             implicit targets (application/…/session_tracking.rs:112-136), so no
#             entry can have accrued since.
#   - OFF:    the session opted out. Nothing is lost because `setSessionMode`
#             flushes the session's own task *before* clearing `taskId`
#             (api/…/mutation.rs:221-247) — by the time a row reads OFF here,
#             its worklog is already flushed, not merely flushable later. The
#             reaper's own pre-close flush being NOT mode-gated
#             (cli/src/lookup.rs:241-262, application/…/session_reaper.rs:43-45)
#             is a second, independent safety net for other paths, not the
#             reason an OFF session is safe.
# Anything other than TRACKING fails closed: a flush writes, so an unrecognised
# mode must never trigger one.
ended=$(printf '%s' "$session_json" | jq -r '.claudeSession.endedAt // empty' 2>/dev/null || echo '')
[ -n "$ended" ] && exit 0

mode=$(printf '%s' "$session_json" | jq -r '.claudeSession.mode // empty' 2>/dev/null || echo '')
[ "$mode" = "TRACKING" ] || exit 0

# `taskId`, not `task.id`: the raw column is what the domain rule reads, and a
# row whose task no longer resolves must not look like a row with no task — the
# flush below then fails cleanly at task resolution, before any mutation.
task_id=$(printf '%s' "$session_json" | jq -r '.claudeSession.taskId // empty' 2>/dev/null || echo '')
[ -z "$task_id" ] && exit 0

# `--session` is passed explicitly even though it defaults to
# CLAUDE_CODE_SESSION_ID (cli/src/cli.rs:85): the id above came from the payload,
# and an inherited variable quietly disagreeing with it is exactly the failure
# this makes impossible. Output is discarded; the hook's stdout is not consumed.
aplan_bounded --session "$sid" flush --json "$task_id" >/dev/null 2>&1 || exit 0
