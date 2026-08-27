#!/usr/bin/env bash
# Tests aplan-hud-toggle without touching the real compositor.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
FAILED=0

run_case() {
    local name="$1" running="$2" expect="$3"
    local stub; stub="$(mktemp -d)"
    cat > "$stub/pgrep" <<EOF
#!/usr/bin/env bash
exit $([ "$running" = "yes" ] && echo 0 || echo 1)
EOF
    cat > "$stub/hyprctl" <<'EOF'
#!/usr/bin/env bash
echo "hyprctl $*" >> "$STUB_LOG"
EOF
    cat > "$stub/aplan-hud" <<'EOF'
#!/usr/bin/env bash
echo "launched" >> "$STUB_LOG"
EOF
    chmod +x "$stub"/*
    export STUB_LOG="$stub/log"; : > "$STUB_LOG"
    PATH="$stub:$PATH" APLAN_HUD_BIN="$stub/aplan-hud" "$HERE/aplan-hud-toggle"
    # The launch path backgrounds the binary and disowns it (by design, so the
    # toggle stays cheap at rest), so the log write can land after this
    # process returns. Poll instead of a one-shot check, bounded so a real
    # failure still fails fast.
    local waited=0
    until grep -q "$expect" "$STUB_LOG" 2>/dev/null || [ "$waited" -ge 20 ]; do
        sleep 0.05
        waited=$((waited + 1))
    done
    if grep -q "$expect" "$STUB_LOG"; then
        echo "  ok   $name"
    else
        echo "  FAIL $name — expected '$expect' in:"; sed 's/^/       /' "$STUB_LOG"
        FAILED=1
    fi
    rm -rf "$stub"
}

run_case "process absent -> launches the binary"  no  "launched"
run_case "process present -> toggles the workspace" yes "togglespecialworkspace aplan"
# The windowrule places the freshly-launched window on the special workspace
# with `silent`, which does NOT show it (verified empirically against the
# running compositor: the special workspace stays empty right after launch).
# So the first invocation must also show the workspace itself, or the first
# SUPER+B press starts the app with nothing on screen.
run_case "process absent -> also shows the special workspace" no "togglespecialworkspace aplan"
exit $FAILED
