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
echo "pgrep \$*" >> "\$STUB_LOG"
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
    PATH="$stub:$PATH" APLAN_HUD_BIN="$stub/aplan-hud" APLAN_HUD_LOCKFILE="$stub/lock" "$HERE/aplan-hud-toggle"
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
    # The pgrep stub previously ignored its own arguments, driven only by
    # $running -- a regression that queried the wrong process name, or
    # dropped -x, would still have passed every case above. Assert the
    # real invocation actually happened, not just its exit code.
    if grep -q "^pgrep -x aplan-hud\$" "$STUB_LOG"; then
        echo "  ok   $name (pgrep invoked with -x aplan-hud)"
    else
        echo "  FAIL $name — pgrep was not invoked with '-x aplan-hud'; log:"; sed 's/^/       /' "$STUB_LOG"
        FAILED=1
    fi
    rm -rf "$stub"
}

test_lock() {
    local name="concurrent invocations serialize on the lock"
    local stub; stub="$(mktemp -d)"
    # pgrep/hyprctl/aplan-hud don't matter for this test -- it only checks
    # that the script actually blocks on the same lock file an external
    # holder controls, so keep them minimal.
    cat > "$stub/pgrep" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
    cat > "$stub/hyprctl" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
    cat > "$stub/aplan-hud" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
    chmod +x "$stub"/*
    local lockfile="$stub/lock"

    # Hold the real lock externally for 300ms -- exactly what the script's
    # own critical section would do while a first invocation is mid-flight.
    (
        exec 9>"$lockfile"
        flock 9
        sleep 0.3
    ) &
    local holder=$!
    sleep 0.05  # give the holder a head start so it acquires the lock first

    local start end elapsed_ms
    start=$(date +%s%3N)
    PATH="$stub:$PATH" APLAN_HUD_BIN="$stub/aplan-hud" APLAN_HUD_LOCKFILE="$lockfile" "$HERE/aplan-hud-toggle" >/dev/null 2>&1
    end=$(date +%s%3N)
    wait "$holder"
    elapsed_ms=$((end - start))

    # The script must have blocked for roughly the remainder of the
    # holder's 300ms sleep. 200ms is a generous floor: it tolerates
    # scheduling jitter while still failing hard if the script never
    # touched the lock at all (it would return in a few ms).
    if [ "$elapsed_ms" -ge 200 ]; then
        echo "  ok   $name (blocked ${elapsed_ms}ms)"
    else
        echo "  FAIL $name — returned after ${elapsed_ms}ms, expected to block on the held lock"
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
# pgrep and the launch are not atomic: two rapid invocations (e.g. keyboard
# auto-repeat) could both see "absent" and both launch, breaking the
# single-instance invariant. The script must serialize on a lock.
test_lock
exit $FAILED
