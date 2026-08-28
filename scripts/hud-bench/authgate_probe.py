#!/usr/bin/env python3
"""Does AuthGate let /hud paint inside the real Tauri window?

Method imposed by the controller: a **difference image**, never a colour
threshold. Two independent threshold-based measurements on this project
already contradicted each other and both turned out biased -- one confounded
by Hyprland dimming the special workspace (cancelled out only by comparing
HUD frames against each other, never against the bare desktop), the other
using a tolerance too strict for anti-aliased text (see
docs/plans/2026-08-27-hud-overlay-tauri-design.md §10.2 for the same lesson
in a different measurement, and
.superpowers/sdd/2026-08-27-hud-overlay-plan-1-coque-tauri/task-6-report.md
for the full adjudication this script's method reproduces).

A pixel-diff *count* alone cannot tell "boot text fading into an empty grid"
apart from "a loading spinner giving way to a sign-in screen that stays" --
both are a burst of changed pixels that then goes static. This script does
not attempt that classification: it saves every frame to disk. Read the
frame(s) right after any diff spike before concluding which branch applies --
a past run drew the wrong conclusion from the pixel count alone and had to
retract it.

Launches the HUD through scripts/aplan-hud-toggle, the exact path SUPER+B
takes in real use (the compiled release binary, not `tauri dev`), so the
measurement reflects what the user actually sees, not a synthetic stand-in.
"""
import argparse
import json
import os
import subprocess
import sys
import time

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
TOGGLE = os.path.join(REPO, "scripts", "aplan-hud-toggle")
WORKSPACE = "aplan"
BIN = os.environ.get(
    "APLAN_HUD_BIN",
    os.path.join(REPO, "frontend", "src-tauri", "target", "release", "aplan-hud"),
)


def hyprctl_json(*args):
    out = subprocess.run(["hyprctl", "-j", *args], capture_output=True, text=True, check=True)
    return json.loads(out.stdout)


def focused_monitor():
    for m in hyprctl_json("monitors"):
        if m.get("focused"):
            return m["name"]
    sys.exit("no focused monitor reported by `hyprctl -j monitors`")


def capture(monitor, out_path):
    subprocess.run(["grim", "-o", monitor, out_path], check=True)
    return np.asarray(Image.open(out_path).convert("RGB"))


def diff_px(a, b):
    if a.shape != b.shape:
        return -1  # geometry changed mid-run (e.g. monitor reconfigured) -- not comparable
    return int(np.count_nonzero(np.any(a != b, axis=-1)))


def hud_pid():
    out = subprocess.run(["pgrep", "-x", "aplan-hud"], capture_output=True, text=True)
    return out.stdout.split()[0] if out.returncode == 0 and out.stdout.strip() else None


def kill_hud():
    subprocess.run(["pkill", "-x", "aplan-hud"], check=False)
    for _ in range(20):
        if hud_pid() is None:
            return
        time.sleep(0.2)


def hud_shown_on(monitor):
    """True only if THIS monitor is actually displaying the special
    workspace -- matching visibility_probe.py's caution: the toggle acts on
    the active window/workspace, and it has missed its target before.

    `hyprctl dispatch togglespecialworkspace` takes the bare name (`aplan`),
    but `specialWorkspace.name` in `hyprctl -j monitors` reports it prefixed
    (`special:aplan`) -- comparing against the bare name here always came
    back false, which is what made the first two runs of this script abort
    ("never showed") even though `hyprctl` itself was reporting it shown.
    """
    mon = next((m for m in hyprctl_json("monitors") if m["name"] == monitor), None)
    return bool(mon and (mon.get("specialWorkspace") or {}).get("name") == f"special:{WORKSPACE}")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out-dir", default="/tmp/authgate_probe")
    ap.add_argument("--duration", type=float, default=6.0, help="seconds of HUD frames to capture")
    ap.add_argument("--interval", type=float, default=0.3, help="seconds between HUD captures (brief: ~300ms)")
    a = ap.parse_args()

    os.makedirs(a.out_dir, exist_ok=True)
    monitor = focused_monitor()
    print(f"monitor: {monitor}")

    if not os.path.isfile(BIN):
        sys.exit(f"{BIN} is missing -- ask the controller to run `pnpm hud:build` first")

    # Clean starting state: no aplan-hud process, workspace not shown anywhere.
    kill_hud()

    # --- Phase 1: noise floor -- bare desktop, no HUD, two captures ---
    n0 = capture(monitor, os.path.join(a.out_dir, "noise_0.png"))
    time.sleep(0.3)
    n1 = capture(monitor, os.path.join(a.out_dir, "noise_1.png"))
    noise_floor = diff_px(n0, n1)
    print(f"noise floor (bare desktop, 2 captures): {noise_floor} px")

    # --- Phase 2: launch + reveal, capture early and often ---
    # scripts/aplan-hud-toggle launches the binary (silent, on the hidden
    # special workspace) AND reveals it in one call -- exactly what SUPER+B
    # does. Timestamps below are measured from this call, not from whenever
    # the reveal-confirmation loop happens to return: the boot sequence lasts
    # only 1500ms from mount, and the window is born hidden -- a late reveal
    # (or a late t0) makes it look like nothing rendered when it already
    # finished off-screen.
    t_launch = time.monotonic()
    subprocess.run([TOGGLE], check=True)

    # The first launch of a run spawns the binary cold (WebKitGTK renderer
    # included) -- scripts/aplan-hud-toggle's own comment notes this is the
    # only invocation that pays that cost. A 1.5s poll window was too short
    # on a cold start and produced a false "never revealed" abort; give it
    # up to 10s, which still fails fast on a genuine problem.
    revealed = False
    for _ in range(100):
        if hud_shown_on(monitor):
            revealed = True
            break
        time.sleep(0.1)
    if not revealed:
        kill_hud()
        sys.exit(
            "special workspace never showed on the focused monitor after the "
            "toggle -- aborting, a capture series here would be meaningless"
        )

    frames = []
    n = max(1, int(a.duration / a.interval))
    for i in range(n):
        path = os.path.join(a.out_dir, f"hud_{i:03d}.png")
        img = capture(monitor, path)
        t_ms = round((time.monotonic() - t_launch) * 1000)
        frames.append((t_ms, path, img))
        time.sleep(a.interval)

    print(f"first capture at t={frames[0][0]}ms since toggle call "
          f"(boot window is 0-1500ms from mount)")

    diffs = []
    for (t_a, _, img_a), (t_b, path_b, img_b) in zip(frames, frames[1:]):
        d = diff_px(img_a, img_b)
        diffs.append({"t_a_ms": t_a, "t_b_ms": t_b, "diff_px": d})
        print(f"t={t_a:5d}ms -> t={t_b:5d}ms : {d:>7d} px differ  ({os.path.basename(path_b)})")

    # --- Cleanup: never leave this wedged on the user's live desktop ---
    subprocess.run(["hyprctl", "dispatch", "togglespecialworkspace", WORKSPACE], check=False)
    time.sleep(0.3)
    kill_hud()
    print(
        f"cleanup: aplan-hud process gone={hud_pid() is None}, "
        f"special workspace still shown on {monitor}={hud_shown_on(monitor)}"
    )

    result = {
        "noise_floor_px": noise_floor,
        "frames": [{"t_ms": t, "path": p} for t, p, _ in frames],
        "pairwise_diffs": diffs,
    }
    print(json.dumps(result, indent=2))
    print(
        f"\nFrames saved to {a.out_dir} -- inspect the frame(s) right after any "
        "large diff spike to see WHAT appeared (sign-in box vs. boot text vs. "
        "empty grid). A diff count alone cannot tell these apart -- that is "
        "exactly the mistake an earlier measurement on this project made and "
        "had to retract."
    )


if __name__ == "__main__":
    main()
