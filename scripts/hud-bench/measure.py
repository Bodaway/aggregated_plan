#!/usr/bin/env python3
"""Measure the cost of a HUD variant. CPU read from /proc (actual consumption
over the interval, not `ps`'s lifetime average); memory in PSS (shared
libraries are apportioned pro rata, the only way to compare Qt, GTK and
WebKit)."""
import argparse
import json
import os
import signal
import subprocess
import sys
import time

CLK_TCK = os.sysconf("SC_CLK_TCK")


def window_visible(pid):
    """Ask Hyprland whether the process's window is actually on screen.

    A live process tree is not proof the surface rendered: WebKitGTK can fail
    to map a window while the launcher process stays alive, which would make
    the CPU figure describe an idle process, not the variant under test. This
    mirrors the pid-matching approach in visibility_probe.py — matching on
    class too could hit a stale same-class window left over from a crash.
    """
    try:
        out = subprocess.run(["hyprctl", "-j", "clients"],
                              capture_output=True, text=True, check=True)
        clients = json.loads(out.stdout)
    except (OSError, subprocess.CalledProcessError, ValueError):
        return None  # hyprctl unavailable or failed: can't confirm either way
    matches = [c for c in clients if c.get("pid") == pid]
    if len(matches) != 1:
        return False
    c = matches[0]
    return bool(c.get("mapped") and c.get("visible") and not c.get("hidden"))


def descendants(root):
    children = {}
    for pid in os.listdir("/proc"):
        if not pid.isdigit():
            continue
        try:
            parts = open(f"/proc/{pid}/stat").read().rsplit(") ", 1)[1].split()
            children.setdefault(int(parts[1]), []).append(int(pid))
        except (OSError, IndexError):
            continue
    out, stack = [], [root]
    while stack:
        p = stack.pop()
        out.append(p)
        stack.extend(children.get(p, []))
    return out


def cpu_ticks(pids):
    total = 0
    for p in pids:
        try:
            parts = open(f"/proc/{p}/stat").read().rsplit(") ", 1)[1].split()
            total += int(parts[11]) + int(parts[12])
        except (OSError, IndexError, ValueError):
            continue
    return total


def pss_kb(pids):
    total = 0
    for p in pids:
        try:
            for line in open(f"/proc/{p}/smaps_rollup"):
                if line.startswith("Pss:"):
                    total += int(line.split()[1])
                    break
        except (OSError, ValueError):
            continue
    return total


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--label", required=True)
    ap.add_argument("--settle", type=float, default=10.0)
    ap.add_argument("--phase", type=float, default=20.0)
    ap.add_argument("cmd", nargs=argparse.REMAINDER)
    a = ap.parse_args()
    cmd = a.cmd[1:] if a.cmd and a.cmd[0] == "--" else a.cmd
    if not cmd:
        sys.exit("no command given")

    proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL, start_new_session=True)
    try:
        time.sleep(a.settle)
        pids = descendants(proc.pid)
        visible = window_visible(proc.pid)
        if proc.poll() is not None or len(pids) <= 1 or visible is not True:
            # Either the tree collapsed to just the launcher (or the launcher
            # itself exited), or Hyprland confirms the window never mapped, or
            # Hyprland itself couldn't be asked (`visible is None`). Treating
            # "unconfirmed" the same as "confirmed absent" matters: `is False`
            # would silently disarm this guard exactly when `hyprctl` breaks,
            # letting an unverified surface through as if it had been checked.
            # Either way the surface is not confirmed on screen, so a CPU
            # figure here would describe an idle process, not the variant
            # under test.
            sys.exit(
                f"surface not confirmed on screen after settle ({a.settle}s): "
                f"alive={proc.poll() is None} pids={pids} "
                f"hyprctl_visible={visible} — no trustworthy measurement"
            )
        t0, c0 = time.monotonic(), cpu_ticks(pids)
        time.sleep(a.phase)
        t1, c1 = time.monotonic(), cpu_ticks(pids)
        pids = descendants(proc.pid)
        if proc.poll() is not None or window_visible(proc.pid) is not True:
            # Same reasoning as the settle check above: a `hyprctl` failure
            # here (None) must abort too, not be waved through as if the
            # window had been reconfirmed visible.
            sys.exit(
                "process tree died or window disappeared during the measured "
                "phase — figures above would describe a crash, not the "
                "variant under test"
            )
        print(json.dumps({
            "label": a.label,
            "procs": len(pids),
            "cpu_pct": round(((c1 - c0) / CLK_TCK) / (t1 - t0) * 100, 2),
            "pss_mb": round(pss_kb(pids) / 1024, 1),
        }, indent=2))
    finally:
        try:
            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        except OSError:
            proc.terminate()


if __name__ == "__main__":
    main()
