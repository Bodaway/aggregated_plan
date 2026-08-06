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

# Where the session id comes from is the one thing about this payload that could
# not be verified from the repository, so take it from the payload and fall back
# to the environment rather than betting on either.
sid=$(printf '%s' "$payload" | jq -r '.session_id // empty' 2>/dev/null || echo '')
[ -z "$sid" ] && sid="${CLAUDE_CODE_SESSION_ID:-}"

# TEMPORARY (plan 3, task 4): record the raw payload so the next real session
# settles empirically whether SessionStart carries `session_id`. One compact line
# per invocation. Remove this block once the answer is in the log. It must never
# fail the hook, hence the discarded stderr and the `|| true`; and it must never
# touch $HOME unguarded, because under `set -u` an unset HOME would abort the
# script here — after reading stdin and before emitting anything.
if [ -n "${HOME:-}" ]; then
  {
    log_line=$(printf '%s' "$payload" | jq -c . 2>/dev/null || printf '%s' "$payload" | tr -d '\n')
    printf '%s\n' "$log_line" >>"$HOME/.claude/hooks/.aplan-session-start-payload.log"
  } 2>/dev/null || true
fi

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
if ! session_json=$(aplan session show --session "$sid" --json 2>/dev/null); then
  exit 0
fi
# If the key is absent the output shape changed under us; a silent no-op is the
# safe failure, mis-reading it as "no row" and re-asking is not.
printf '%s' "$session_json" | jq -e 'has("claudeSession")' >/dev/null 2>&1 || exit 0

mode=$(printf '%s' "$session_json"       | jq -r '.claudeSession.mode // empty'       2>/dev/null || echo '')
sess_title=$(printf '%s' "$session_json" | jq -r '.claudeSession.task.title // empty' 2>/dev/null || echo '')
sess_task_id=$(printf '%s' "$session_json" | jq -r '.claudeSession.task.id // empty'  2>/dev/null || echo '')

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
  ls_json=$(aplan ls --json 2>/dev/null || echo '')
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

elif [ "$source" != "clear" ] && [ "$mode" = "TRACKING" ] && [ -n "$sess_title" ]; then
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
  if [ "$mode" = "TRACKING" ] && [ -n "$sess_title" ]; then
    # /clear on a session that already has a link: offer its own task, and the
    # human's pointer never enters the picture.
    cont_title=$sess_title
    cont_target=$sess_task_id
    cont_bound=yes
  else
    # The ONE legitimate use of the human's pointer: no session row exists, and
    # the human has just opened a Claude on what they were doing by hand.
    # Offering it is genuinely useful; moving it would be the original bug.
    current_json=$(aplan current --json 2>/dev/null || echo '')
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
