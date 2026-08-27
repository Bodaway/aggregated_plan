#!/usr/bin/env python3
"""Le special workspace masque-t-il vraiment WebKit, de son point de vue ?"""
import json
import os
import re
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
CLASS = "dev.hudbench.webkit"
WORKSPACE = "special:visprobe"


def hyprctl_json(*args):
    out = subprocess.run(["hyprctl", "-j", *args], capture_output=True, text=True, check=True)
    return json.loads(out.stdout)


def find_client(pid, proc):
    # Match on pid only. Matching on `class` too (as an earlier version did) can
    # return a stale same-class orphan instead of the freshly-spawned window —
    # e.g. one left behind by a Ctrl+C or crash that bypassed abort()'s cleanup —
    # which would silently make the workspace check below trust the wrong window.
    matches = [c for c in hyprctl_json("clients") if c.get("pid") == pid]
    if len(matches) > 1:
        abort(
            f"{len(matches)} windows share pid {pid} in `hyprctl -j clients` — "
            "ambiguous, refusing to guess which one is the probe",
            proc,
        )
    return matches[0] if matches else None


def kill_proc(proc):
    try:
        os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
    except OSError:
        proc.terminate()


def abort(msg, proc):
    # An abort must never leave the WebKit host running: a previous manual test
    # left one alive, which then skewed a later measurement by keeping the
    # "dev.hudbench.webkit" D-Bus name and accumulating extra windows.
    kill_proc(proc)
    print(f"ABORT : {msg}")
    sys.exit(1)


log = open("/tmp/visprobe.log", "w+")
proc = subprocess.Popen(
    ["python3", f"{HERE}/webkit_host.py", f"file://{HERE}/probe.html"],
    stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
time.sleep(6)

if find_client(proc.pid, proc) is None:
    abort("WebKit window not found in `hyprctl -j clients` after 6 s", proc)

# The brief dispatched `movetoworkspacesilent special:visprobe` alone, which acts
# on the ACTIVE window. On this machine that form missed its target twice in a
# row (window stayed on 'dev1', then landed on 'witivio' instead of the special
# workspace) — confirmed by the probe itself: hyprctl dispatch accepts an
# explicit window selector as a 2nd argument, which removes the "active window"
# assumption entirely.
subprocess.run([
    "hyprctl", "dispatch", "movetoworkspacesilent",
    f"{WORKSPACE},class:^({CLASS})$",
])
time.sleep(1)
subprocess.run(["hyprctl", "dispatch", "togglespecialworkspace", "visprobe"])
time.sleep(2)
print("--- special workspace VISIBLE, 8 s ---")
time.sleep(8)
subprocess.run(["hyprctl", "dispatch", "togglespecialworkspace", "visprobe"])
time.sleep(1)

# Ruling (controller): movetoworkspacesilent acts on the active window. If it
# missed its target, the "hidden" phase was never actually hidden and the
# verdict would be a false negative. So before sampling that phase, verify:
#   1. the probed window really is assigned to the special workspace;
#   2. no monitor is still displaying that special workspace.
client = find_client(proc.pid, proc)
observed_ws = (client or {}).get("workspace", {}).get("name")
if client is None or observed_ws != WORKSPACE:
    abort(
        f"WebKit window is not assigned to {WORKSPACE!r} "
        f"(observed: {observed_ws!r}) — movetoworkspacesilent missed its "
        "target, the 'hidden' phase is not trustworthy, verdict abandoned",
        proc,
    )

monitors = hyprctl_json("monitors")
still_showing = [
    m["name"] for m in monitors
    if (m.get("specialWorkspace") or {}).get("name") == WORKSPACE
]
if still_showing:
    abort(
        f"special workspace {WORKSPACE!r} is still shown on {still_showing} "
        "after togglespecialworkspace — the 'hidden' phase is not "
        "trustworthy, verdict abandoned",
        proc,
    )

print("--- special workspace MASQUE (verifie via clients+monitors), 8 s ---")
time.sleep(8)

kill_proc(proc)

log.seek(0)
text = log.read()
events = re.findall(r"VISPROBE_EVENT (\w+)", text)
frames = [int(n) for n in re.findall(r"VISPROBE_FRAMES (\d+)", text)]
visible = frames[:4] or [0]
hidden = frames[-3:] or [0]
avg_v = sum(visible) / len(visible)
avg_h = sum(hidden) / len(hidden)
print(f"evenements visibilitychange : {events or 'AUCUN'}")
print(f"images par tranche de 2 s   : {frames}")
print(f"moyenne visible={avg_v:.0f}  moyenne masque={avg_h:.0f}")
print("VERDICT :", "WebKit S'ENDORT" if avg_h < avg_v * 0.25 else "WebKit CONTINUE A RENDRE")
