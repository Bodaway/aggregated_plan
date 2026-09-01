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
    # `hyprctl monitors` is read back to decide which signal to send, so the
    # stub has to answer it. ${SHOWN:-no} picks the branch under test.
    cat > "$stub/hyprctl" <<EOF
#!/usr/bin/env bash
echo "hyprctl \$*" >> "\$STUB_LOG"
if [ "\$1" = "monitors" ]; then
$([ "${SHOWN:-no}" = "yes" ] \
    && echo '    echo "	special workspace: -96 (special:aplan)"' \
    || echo '    echo "	special workspace: 0 ()"')
fi
EOF
    # MUST be stubbed: the script signals the running HUD, and an unstubbed
    # pkill in a test would reach the user's real overlay.
    cat > "$stub/pkill" <<'EOF'
#!/usr/bin/env bash
echo "pkill $*" >> "$STUB_LOG"
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

test_inherited_fd() {
    local name="a still-running HUD does not block the next press on the lock"
    local stub; stub="$(mktemp -d)"
    # pgrep always says "not running" -- what matters here is that the
    # launched "aplan-hud" itself never exits, so it keeps holding whatever
    # file descriptors it inherited from the launching shell for as long as
    # the test lets it run.
    cat > "$stub/pgrep" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
    cat > "$stub/hyprctl" <<'EOF'
#!/usr/bin/env bash
echo "hyprctl $*" >> "$STUB_LOG"
EOF
    cat > "$stub/aplan-hud" <<'EOF'
#!/usr/bin/env bash
sleep 30
EOF
    chmod +x "$stub"/*
    export STUB_LOG="$stub/log"; : > "$STUB_LOG"
    local lockfile="$stub/lock"

    # Press 1: launches the long-lived stub, exactly like a real HUD binary
    # that stays resident once shown.
    PATH="$stub:$PATH" APLAN_HUD_BIN="$stub/aplan-hud" APLAN_HUD_LOCKFILE="$lockfile" \
        "$HERE/aplan-hud-toggle" >/dev/null 2>&1

    # Give the backgrounded launch a moment to actually start (and, if the
    # fd leak is present, to open the inherited lock fd) before press 2.
    sleep 0.2

    # Press 2: a `flock` guards an open file description, not a process. If
    # press 1's child inherited the lock fd without CLOEXEC, that fd is still
    # open (the child is still running), so this would block on the lock for
    # as long as the child lives. Bound the wait generously above the
    # script's own 5s bounded wait so a real hang still fails the test
    # instead of wedging the suite.
    timeout 8 env PATH="$stub:$PATH" APLAN_HUD_BIN="$stub/aplan-hud" APLAN_HUD_LOCKFILE="$lockfile" \
        "$HERE/aplan-hud-toggle" >/dev/null 2>&1
    local rc=$?

    pkill -f "$stub/aplan-hud" 2>/dev/null || true

    if [ "$rc" -eq 124 ]; then
        echo "  FAIL $name — press 2 hung on the lock inherited by press 1's child (timeout)"
        FAILED=1
    else
        echo "  ok   $name (press 2 returned, rc=$rc)"
    fi
    rm -rf "$stub"
}

test_missing_binary() {
    local name="missing binary fails loudly instead of toggling an empty overlay"
    local stub; stub="$(mktemp -d)"
    cat > "$stub/pgrep" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
    cat > "$stub/hyprctl" <<'EOF'
#!/usr/bin/env bash
echo "hyprctl $*" >> "$STUB_LOG"
EOF
    chmod +x "$stub/pgrep" "$stub/hyprctl"
    export STUB_LOG="$stub/log"; : > "$STUB_LOG"

    PATH="$stub:$PATH" APLAN_HUD_BIN="$stub/does-not-exist" APLAN_HUD_LOCKFILE="$stub/lock" \
        "$HERE/aplan-hud-toggle" >/dev/null 2>"$stub/stderr"
    local rc=$?

    if [ "$rc" -eq 0 ]; then
        echo "  FAIL $name — exited 0 with a missing binary"
        FAILED=1
    elif grep -q "togglespecialworkspace" "$STUB_LOG" 2>/dev/null; then
        echo "  FAIL $name — toggled the workspace despite the missing binary:"; sed 's/^/       /' "$STUB_LOG"
        FAILED=1
    elif [ ! -s "$stub/stderr" ]; then
        echo "  FAIL $name — failed silently, no message on stderr"
        FAILED=1
    else
        echo "  ok   $name"
    fi
    rm -rf "$stub"
}

test_signal_cases() {
    echo "signalling the HUD about its own visibility"
    SHOWN=yes run_case "workspace now shown -> SIGRTMIN" yes "pkill -x --signal RTMIN aplan-hud"
    SHOWN=no  run_case "workspace now hidden -> SIGRTMIN+1" yes "pkill -x --signal RTMIN+1 aplan-hud"
}

test_no_signal_on_launch() {
    # A HUD we just started is still linking; a signal arriving then hits the
    # default disposition and kills it (measured). The launch path must stay
    # silent -- the app seeds itself as shown, which is what this press means.
    local name="fresh launch sends no signal at all"
    local stub; stub="$(mktemp -d)"
    cat > "$stub/pgrep" <<'EOF'
#!/usr/bin/env bash
echo "pgrep $*" >> "$STUB_LOG"
exit 1
EOF
    cat > "$stub/hyprctl" <<'EOF'
#!/usr/bin/env bash
echo "hyprctl $*" >> "$STUB_LOG"
[ "$1" = "monitors" ] && echo "	special workspace: -96 (special:aplan)"
EOF
    cat > "$stub/pkill" <<'EOF'
#!/usr/bin/env bash
echo "pkill $*" >> "$STUB_LOG"
EOF
    cat > "$stub/aplan-hud" <<'EOF'
#!/usr/bin/env bash
echo "launched" >> "$STUB_LOG"
EOF
    chmod +x "$stub"/*
    export STUB_LOG="$stub/log"; : > "$STUB_LOG"
    PATH="$stub:$PATH" APLAN_HUD_BIN="$stub/aplan-hud" APLAN_HUD_LOCKFILE="$stub/lock" "$HERE/aplan-hud-toggle"
    local waited=0
    until grep -q "launched" "$STUB_LOG" 2>/dev/null || [ "$waited" -ge 20 ]; do
        sleep 0.05; waited=$((waited + 1))
    done
    if grep -q "^pkill " "$STUB_LOG"; then
        echo "  FAIL $name -- a signal was sent on the launch path; log:"; sed 's/^/       /' "$STUB_LOG"
        FAILED=1
    else
        echo "  ok   $name"
    fi
    rm -rf "$stub"
}

test_signal_cases
test_no_signal_on_launch
test_inherited_fd
test_missing_binary

# --------------------------------------------------------------------------
# `show` / `hide`, and the Hyprland signature fallback.
#
# These modes are driven by the break scheduler rather than by a finger on a
# key: it acts on the state it believes the compositor to be in, so what is
# worth asserting is how many dispatches leave the script -- exactly one when
# the state has to change, none when it already matches.
# --------------------------------------------------------------------------

# Same four stubs as run_case, written separately because the cases below vary
# the mode and the environment instead of the "is the HUD running" question,
# and because the hyprctl stub has to record the signature it was handed --
# the fallback is only observable in what hyprctl ends up being called with.
write_mode_stubs() {
    local dir="$1" shown="$2"
    # Always "running": these cases are about the dispatch decision, and the
    # launch path would add a toggle of its own to the count.
    cat > "$dir/pgrep" <<'EOF'
#!/usr/bin/env bash
echo "pgrep $*" >> "$STUB_LOG"
exit 0
EOF
    cat > "$dir/hyprctl" <<EOF
#!/usr/bin/env bash
echo "hyprctl \$* [sig=\${HYPRLAND_INSTANCE_SIGNATURE:-none}]" >> "\$STUB_LOG"
if [ "\$1" = "monitors" ]; then
$([ "$shown" = "yes" ] \
    && echo "    printf '\\tspecial workspace: -96 (special:aplan)\\n'" \
    || echo "    printf '\\tspecial workspace: 0 ()\\n'")
fi
EOF
    cat > "$dir/pkill" <<'EOF'
#!/usr/bin/env bash
echo "pkill $*" >> "$STUB_LOG"
EOF
    cat > "$dir/aplan-hud" <<'EOF'
#!/usr/bin/env bash
echo "launched" >> "$STUB_LOG"
EOF
    chmod +x "$dir"/*
}

# One entry in a fake $XDG_RUNTIME_DIR/hypr. The suite never reads the real
# one: on a machine with no Hyprland running these cases would otherwise take
# the "no instance" branch and prove nothing.
#
# The mtime is set explicitly rather than by sleeping between mkdirs: the
# script picks the newest with `-nt`, and two directories created in the same
# filesystem timestamp tick are neither newer than the other.
add_instance() {
    local run="$1" name="$2" epoch="$3"
    mkdir -p "$run/hypr/$name"
    touch -m -d "@$epoch" "$run/hypr/$name"
}

# name | mode ("" = no argument) | shown | expected number of dispatches
run_mode_case() {
    local name="$1" mode="$2" shown="$3" expected="$4"
    local stub; stub="$(mktemp -d)"
    write_mode_stubs "$stub" "$shown"
    local run="$stub/run"
    add_instance "$run" "live_instance" 1700000000
    export STUB_LOG="$stub/log"; : > "$STUB_LOG"
    # ${mode:+...} is deliberately unquoted: an empty mode must produce NO
    # argument at all, which is the keyboard-shortcut call, not an empty one.
    PATH="$stub:$PATH" XDG_RUNTIME_DIR="$run" HYPRLAND_INSTANCE_SIGNATURE="live_instance" \
        APLAN_HUD_BIN="$stub/aplan-hud" APLAN_HUD_LOCKFILE="$stub/lock" \
        "$HERE/aplan-hud-toggle" ${mode:+"$mode"}
    local rc=$?
    local dispatches; dispatches=$(grep -c "togglespecialworkspace" "$STUB_LOG")
    if [ "$rc" -ne 0 ]; then
        echo "  FAIL $name — rc=$rc, expected 0; log:"; sed 's/^/       /' "$STUB_LOG"
        FAILED=1
    elif [ "$dispatches" -ne "$expected" ]; then
        echo "  FAIL $name — $dispatches dispatch(es), expected $expected; log:"
        sed 's/^/       /' "$STUB_LOG"
        FAILED=1
    else
        echo "  ok   $name (dispatches=$dispatches)"
    fi
    rm -rf "$stub"
}

# name | mode ("" = no argument) | signature in the environment ("" = none)
# | signature expected on the wire | instance specs, as name:epoch
run_fallback_case() {
    local name="$1" mode="$2" env_sig="$3" expected_sig="$4"; shift 4
    local stub; stub="$(mktemp -d)"
    write_mode_stubs "$stub" no
    local run="$stub/run" spec
    for spec in "$@"; do
        add_instance "$run" "${spec%%:*}" "${spec##*:}"
    done
    export STUB_LOG="$stub/log"; : > "$STUB_LOG"
    PATH="$stub:$PATH" XDG_RUNTIME_DIR="$run" HYPRLAND_INSTANCE_SIGNATURE="$env_sig" \
        APLAN_HUD_BIN="$stub/aplan-hud" APLAN_HUD_LOCKFILE="$stub/lock" \
        "$HERE/aplan-hud-toggle" ${mode:+"$mode"} >/dev/null 2>&1
    local rc=$?
    # Asserted on the dispatch line, not merely on the read-back: what breaks
    # in production is the dispatch going out under a signature nothing
    # answers, which fails silently.
    if [ "$rc" -eq 0 ] && grep -q "togglespecialworkspace .*\[sig=$expected_sig\]" "$STUB_LOG"; then
        echo "  ok   $name"
    else
        echo "  FAIL $name — rc=$rc, expected a dispatch with sig=$expected_sig; log:"
        sed 's/^/       /' "$STUB_LOG"
        FAILED=1
    fi
    rm -rf "$stub"
}

test_no_instance() {
    # Two different shapes of the same situation: the glob matching nothing,
    # and the parent directory not being there either. Both mean "no live
    # compositor", and both must refuse to dispatch rather than hand hyprctl
    # a signature that names nothing.
    local shape
    for shape in empty missing; do
        local name="no Hyprland instance ($shape hypr directory) -> rc 1, no dispatch"
        local stub; stub="$(mktemp -d)"
        write_mode_stubs "$stub" no
        local run="$stub/run"
        mkdir -p "$run"
        if [ "$shape" = empty ]; then mkdir -p "$run/hypr"; fi
        export STUB_LOG="$stub/log"; : > "$STUB_LOG"
        PATH="$stub:$PATH" XDG_RUNTIME_DIR="$run" HYPRLAND_INSTANCE_SIGNATURE="dead_signature" \
            APLAN_HUD_BIN="$stub/aplan-hud" APLAN_HUD_LOCKFILE="$stub/lock" \
            "$HERE/aplan-hud-toggle" show >/dev/null 2>"$stub/stderr"
        local rc=$?
        if [ "$rc" -ne 1 ]; then
            echo "  FAIL $name — rc=$rc, expected 1"
            FAILED=1
        elif grep -q "hyprctl" "$STUB_LOG"; then
            echo "  FAIL $name — called hyprctl anyway:"; sed 's/^/       /' "$STUB_LOG"
            FAILED=1
        elif [ ! -s "$stub/stderr" ]; then
            echo "  FAIL $name — failed silently, no message on stderr"
            FAILED=1
        else
            echo "  ok   $name"
        fi
        rm -rf "$stub"
    done
}

test_unknown_argument() {
    # Exit 2, distinct from the 1 above: the caller passes a mode string, so
    # "I don't know that word" is a bug in the caller, while "no compositor"
    # is a runtime condition. A single code would blur the two in the logs.
    local name="unknown argument -> rc 2, nothing dispatched"
    local stub; stub="$(mktemp -d)"
    write_mode_stubs "$stub" no
    local run="$stub/run"
    add_instance "$run" "live_instance" 1700000000
    export STUB_LOG="$stub/log"; : > "$STUB_LOG"
    PATH="$stub:$PATH" XDG_RUNTIME_DIR="$run" HYPRLAND_INSTANCE_SIGNATURE="live_instance" \
        APLAN_HUD_BIN="$stub/aplan-hud" APLAN_HUD_LOCKFILE="$stub/lock" \
        "$HERE/aplan-hud-toggle" wobble >/dev/null 2>"$stub/stderr"
    local rc=$?
    if [ "$rc" -ne 2 ]; then
        echo "  FAIL $name — rc=$rc, expected 2"
        FAILED=1
    elif grep -q "hyprctl" "$STUB_LOG"; then
        echo "  FAIL $name — dispatched on an unknown command:"; sed 's/^/       /' "$STUB_LOG"
        FAILED=1
    elif [ ! -s "$stub/stderr" ]; then
        echo "  FAIL $name — rejected silently, no message on stderr"
        FAILED=1
    else
        echo "  ok   $name"
    fi
    rm -rf "$stub"
}

echo "show/hide are idempotent, read from the compositor"
run_mode_case "show on an already visible workspace dispatches nothing" show yes 0
run_mode_case "show on a hidden workspace dispatches once"              show no  1
run_mode_case "hide on an already hidden workspace dispatches nothing"  hide no  0
run_mode_case "hide on a visible workspace dispatches once"             hide yes 1

echo "Hyprland signature fallback"
# The aplan-api case: a long-lived service keeps the environment it started
# with, Hyprland restarts, and every dispatch would fail without a word.
run_fallback_case "dead signature -> uses the instance that is there" \
    show dead_signature live_instance live_instance:1700000000
run_fallback_case "no signature in the environment -> same fallback" \
    show "" live_instance live_instance:1700000000
# The winner is deliberately neither first nor last, in either of the two
# orders a wrong implementation would land on: it is the middle one
# alphabetically (which is glob order) AND the middle one by creation. Only
# reading mtimes picks it, which is the whole point of the `-nt` comparison.
run_fallback_case "several instances -> the newest one wins" \
    show dead_signature sig_m sig_z:1700001800 sig_m:1700003600 sig_a:1700000000
run_fallback_case "the fallback also covers the no-argument shortcut" \
    "" dead_signature live_instance live_instance:1700000000

test_no_instance
test_unknown_argument

exit $FAILED
