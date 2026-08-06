#!/usr/bin/env bash
# SessionStart hook: link the Claude session to an aplan task.
# Silent no-op when aplan CLI / jq is missing OR the local aplan backend is
# unreachable, so projects outside aggregated_plan are unaffected.
#
# What this hook reads is THIS SESSION'S OWN ROW (`aplan session show`), not the
# global `aplan.active_task_id` pointer. The pointer means something else: the
# human, working by hand. Deriving session state from it was the original bug —
# a re-fire mid-session announced tracking of a task the user had declined,
# because the refusal was stored nowhere. It now lives on the session row, and
# `Ne pas tracker` below persists it with `aplan session off`.
#
# Claude Code passes a JSON payload on stdin with `session_id` and a `source`:
#   "startup" | "resume" | "clear" | "compact"
# The branch is chosen by the session's recorded state, not by the source, with
# one exception: `clear` always forces the choice again, because that is what
# the user asks for by typing /clear.

set -u

command -v aplan >/dev/null 2>&1 || exit 0
command -v jq    >/dev/null 2>&1 || exit 0

# This hook makes up to three sequential `aplan` calls and each carries reqwest's
# own 10 s timeout (cli/src/client.rs:42), so a backend that accepts the connection
# and then hangs would stall SessionStart for ~30 s — well past the 10 s budget the
# hook is registered with, i.e. killed, injecting nothing at all. Three calls at
# 3 s bounds it at 9 s and, measured, at 6 s — a hang on the first call ends the
# hook, so only the second and third can both burn their budget. Either way it
# stays inside 10 s and still delivers its context, where 5 s each measured 10 s
# and sat exactly on the limit. A refused
# connection is already instant; this is only about the hang. A missing `timeout`
# must not silently disable the hook, hence the fallback: reqwest's own timeout
# still applies there.
if command -v timeout >/dev/null 2>&1; then
  aplan_bounded() { timeout 3 aplan "$@"; }
else
  aplan_bounded() { aplan "$@"; }
fi

# Unattended (cron / scheduled) sessions: no user is present, so the mandatory
# AskUserQuestion emitted below would block or burn the turn. Inject nothing —
# the scheduled job's own prompt is the only instruction it needs, and it writes
# to `memories`, not to the worklog, so the worklog rules do not apply either.
[ -n "${APLAN_UNATTENDED:-}" ] && exit 0

# Read stdin without hanging: Claude Code closes stdin so `cat` sees EOF at once;
# an empty or non-JSON payload must never crash — default the source to startup.
payload=$(cat)
source=$(printf '%s' "$payload" | jq -r '.source // "startup"' 2>/dev/null || echo startup)
[ -z "$source" ] && source=startup

# The payload is the ONLY source of the session id. There was a
# `CLAUDE_CODE_SESSION_ID` fallback here and removing it was deliberate — please do
# not helpfully re-add it. The harness exports that variable into tool
# subprocesses, so a nested `claude` whose payload carried no `session_id` would
# inherit its PARENT's id and speak about the parent session's task: the same
# wrong-actor mistake this plan exists to remove, one actor over. The fallback
# existed only because the payload's contents could not be verified statically, and
# a temporary log of real payloads has since settled it (keys: cwd,
# hook_event_name, session_id, source, transcript_path — the log lives at
# ~/.claude/hooks/.aplan-session-start-payload.log and is no longer written).
# Absence is handled by the guard below; saying nothing is the right failure.
sid=$(printf '%s' "$payload" | jq -r '.session_id // empty' 2>/dev/null || echo '')

# An empty --session is NOT harmless: by an established contract an empty session
# id falls back to the human's global pointer, which is how hooks outside any
# Claude session set the variable. Passing one here would make this hook talk
# about the human's task instead of the session's. Say nothing instead.
[ -z "$sid" ] && exit 0

# Three outcomes, two of which look alike, and picking the wrong one is silent:
#   - non-zero exit          -> backend unreachable / CLI error: inject nothing
#   - exit 0, claudeSession null -> no row yet: ask the user (below)
#   - exit 0, a row          -> obey the row
# `--json` prints the response's `data` block, so the path is `.claudeSession`
# (no `.data` wrapper), and `mode` is the GraphQL enum: uppercase TRACKING / OFF.
if ! session_json=$(aplan_bounded session show --session "$sid" --json 2>/dev/null); then
  exit 0
fi
# If the key is absent the output shape changed under us; a silent no-op is the
# safe failure, mis-reading it as "no row" and re-asking is not.
printf '%s' "$session_json" | jq -e 'has("claudeSession")' >/dev/null 2>&1 || exit 0

mode=$(printf '%s' "$session_json"       | jq -r '.claudeSession.mode // empty'       2>/dev/null || echo '')
sess_title=$(printf '%s' "$session_json" | jq -r '.claudeSession.task.title // empty' 2>/dev/null || echo '')
sess_task_id=$(printf '%s' "$session_json" | jq -r '.claudeSession.task.id // empty'  2>/dev/null || echo '')
# The idle-session reaper closes ANY open row past the idle timeout, tracking
# or not, and idleness there means "no aplan write" — reading/discussing for
# hours with no `aplan log` counts. `claude --resume` on a row it closed
# lands here still `mode = TRACKING` with a resolvable title: without this
# check the branch below would confirm a task this row is no longer able to
# log against (a closed row fails `aplan log` with exit 4), stating something
# untrue in the one place this plan exists to make trustworthy.
ended=$(printf '%s' "$session_json" | jq -r '.claudeSession.endedAt // empty' 2>/dev/null || echo '')

read -r -d '' base_rules <<EOF || true
This Claude Code session has its own aplan link: the session row keyed by session id ${sid}. That row — not the global active-task pointer — is what says which task this session logs against; the pointer is the human working by hand, and nothing you do here may move it. Use the \`aplan\` CLI for all task interactions (see the \`aplan\` skill for the full set of recipes).

Worklog logging — atomic, incremental, readable:
- Log work as it happens, not in a batch at the end. One \`aplan log "<entry>"\` per finding, decision, or completed action — like a journal entry posted in real time.
- Each \`aplan log\` entry is a timestamped worklog record (1–2 sentences, stands alone) — one per finding/decision/action; these entries also drive automatic time tracking. Don't batch; three findings = three \`aplan log\` calls.
- Trigger an entry on: discovery confirmed (e.g. root cause identified), decision taken, code change pushed, test result, blocker hit. Not on: "I'm about to look at X", "I read file Y".
- Match the language of the task title (French task → French note, English task → English note).
- If the user signals they want to switch tasks mid-session, run \`aplan start <task>\` first: inside a session that rebinds this session's link (flushing the task it leaves) and leaves the human's pointer alone. Subsequent \`aplan log\` calls retarget automatically.
- If \`aplan log\` returns exit 4 (no active worklog), tell the user and stop trying to log silently.
EOF

# Reusable formatter for the "Top followed tasks" block (up to 20 lines).
task_lines_block() {
  ls_json=$(aplan_bounded ls --json 2>/dev/null || echo '')
  printf '%s' "$ls_json" | jq -r '
    .tasks.edges[]?.node
    | "- \((.sourceId // (.id|.[0:8]))) — \(.title) [\(.status)]"
  ' 2>/dev/null | head -n 20
}

# `clear` forces the choice again even for a known session: the user typed
# /clear and wants it explicit. Every other source obeys the recorded state,
# which is the whole point of this rewrite.
if [ "$source" != "clear" ] && [ "$mode" = "OFF" ]; then
  context="aplan logging is DISABLED for this Claude Code session — the choice is recorded on this session's row (session ${sid}), so it survives restarts, /resume and compaction.

Do NOT ask the user about it again, and for the REST OF THIS SESSION never call \`aplan log\`, \`aplan start\`, \`aplan stop\` or \`aplan flush\`. Say nothing about aplan unless the user raises it; just proceed with whatever the user asks."

elif [ "$source" != "clear" ] && [ -z "$ended" ] && [ "$mode" = "TRACKING" ] && [ -n "$sess_title" ]; then
  context="This Claude Code session is tracking aplan task: \"${sess_title}\".

${base_rules}

Confirm in one short line that you're tracking this task, then proceed with whatever the user asks."

else
  # No row yet, a row that says TRACKING without a resolvable task, or an
  # explicit /clear: make the user choose, and persist whatever they choose.
  task_lines=$(task_lines_block)

  cont_title=""
  cont_target=""
  cont_key=""
  cont_bound=no
  if [ -z "$ended" ] && [ "$mode" = "TRACKING" ] && [ -n "$sess_title" ]; then
    # /clear on a session that already has a link: offer its own task, and the
    # human's pointer never enters the picture.
    cont_title=$sess_title
    cont_target=$sess_task_id
    cont_bound=yes
  elif [ -n "$ended" ] && [ "$mode" = "TRACKING" ] && [ -n "$sess_title" ]; then
    # The reaper closed this row while it was still tracking, and reviving it
    # works: `session bind` clears `ended_at` (`session_repo.rs`'s upsert
    # overwrites it). Offer the same task, but never as a bare confirm —
    # `cont_bound=no` forces the Continuer action below to actually run the
    # bind, since a confirm alone would leave the session linked to a row
    # that still cannot log.
    cont_title=$sess_title
    cont_target=$sess_task_id
  elif [ -z "$mode" ]; then
    # The ONE legitimate use of the human's pointer: NO session row exists at all
    # (hence the `-z "$mode"` gate), and the human has just opened a Claude on what
    # they were doing by hand. Offering it is genuinely useful; moving it would be
    # the original bug. The gate is what keeps that "one use" honest: without it,
    # `/clear` on a session whose row says OFF fell through to here and offered the
    # human's task as "(Recommended)" to a session that had explicitly opted out —
    # a nudge rather than the old bug, since nothing moved the pointer, but still a
    # breach of the rule that for a KNOWN session the pointer is never consulted.
    # A known row with an unresolvable task takes the same path now: no Option 1.
    current_json=$(aplan_bounded current --json 2>/dev/null || echo '')
    cont_title=$(printf '%s' "$current_json" | jq -r '.currentActivity.task.title // empty'    2>/dev/null || echo '')
    cont_target=$(printf '%s' "$current_json" | jq -r '.currentActivity.task.id // empty'       2>/dev/null || echo '')
    cont_key=$(printf '%s' "$current_json"   | jq -r '.currentActivity.task.sourceId // ""'    2>/dev/null || echo '')
  fi

  if [ -n "$cont_title" ]; then
    short_title=$cont_title
    if [ ${#cont_title} -gt 40 ]; then
      short_title="${cont_title:0:40}…"
    fi
    if [ -n "$cont_key" ]; then
      key_suffix=" (${cont_key})"
    else
      key_suffix=""
    fi
    options_block="  - Option 1 — label \"Continuer : ${short_title}\", marked \"(Recommended)\", description \"${cont_title}${key_suffix}\".
  - Option 2 — label \"Choisir une autre tâche\".
  - Option 3 — label \"Créer une nouvelle tâche\".
  - Option 4 — label \"Ne pas tracker\"."
    if [ "$cont_bound" = yes ]; then
      continue_action="- Continuer → this session is already linked to it; run NO aplan command, just confirm in one short line which task you are tracking."
    else
      continue_action="- Continuer → run \`aplan session bind --session ${sid} ${cont_target}\`. That links THIS session only; it must not and does not move the user's own active-task pointer."
    fi
  else
    options_block="  - Option 1 — label \"Choisir une tâche\".
  - Option 2 — label \"Créer une nouvelle tâche\".
  - Option 3 — label \"Ne pas tracker\"."
    continue_action=""
  fi

  context="MANDATORY FIRST ACTION for this Claude Code session — before doing ANY other work, before even answering the user's first question:

Call the AskUserQuestion tool with a SINGLE question \"Quelle tâche aplan tracker pour cette session ?\" and these options:
${options_block}

Then map the chosen answer to actions:
${continue_action:+${continue_action}
}- Choisir (une autre tâche) → show the \"Top followed tasks\" list below and run \`aplan session bind --session ${sid} <task>\` on the user's pick (UUID prefix, Jira key, or fuzzy title all work). A second AskUserQuestion listing the top tasks is fine; the user can free-type a different one via the Other field.
- Créer une nouvelle tâche → if no title was given, ask for one, then run \`aplan new \"<title>\"\` followed by \`aplan session bind --session ${sid} \"<title>\"\`.
- Ne pas tracker → run \`aplan session off --session ${sid}\` so the refusal is recorded on this session's row and you are not asked again, and for the REST OF THIS SESSION never call \`aplan log\`/\`start\`/\`stop\`/\`flush\`.

Every one of those commands acts on this session's link only. None of them touches the user's own active-task pointer.

Top followed tasks (status ≠ done):
${task_lines:-(no followed tasks)}

Once a task is chosen (any option except \"Ne pas tracker\"), the following worklog rules apply:

${base_rules}"
fi

jq -nc --arg ctx "$context" '{hookSpecificOutput:{hookEventName:"SessionStart", additionalContext:$ctx}}'
