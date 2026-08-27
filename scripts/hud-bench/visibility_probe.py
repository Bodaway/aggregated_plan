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


def find_client(pid):
    for c in hyprctl_json("clients"):
        if c.get("pid") == pid or c.get("class") == CLASS:
            return c
    return None


def kill_proc(proc):
    try:
        os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
    except OSError:
        proc.terminate()


def abort(msg, proc):
    # Un abandon ne doit jamais laisser trainer l'hote WebKit : un precedent
    # test manuel en a laisse un vivant, qui a ensuite fausse une mesure en
    # gardant le nom D-Bus "dev.hudbench.webkit" et en cumulant les fenetres.
    kill_proc(proc)
    print(f"ABORT : {msg}")
    sys.exit(1)


log = open("/tmp/visprobe.log", "w+")
proc = subprocess.Popen(
    ["python3", f"{HERE}/webkit_host.py", f"file://{HERE}/probe.html"],
    stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
time.sleep(6)

if find_client(proc.pid) is None:
    abort("fenetre WebKit introuvable dans `hyprctl -j clients` apres 6 s", proc)

# Le brief dispatchait `movetoworkspacesilent special:visprobe` seul, qui agit sur
# la fenetre ACTIVE. Sur cette machine cette forme a rate sa cible deux fois de
# suite (fenetre restee sur 'dev1', puis deplacee sur 'witivio' au lieu du special
# workspace) — corrobore par la sonde: hyprctl dispatch accepte un selecteur de
# fenetre explicite en 2e argument, qui elimine l'hypothese "fenetre active".
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

# Ruling (controleur) : movetoworkspacesilent agit sur la fenetre active. S'il a
# rate sa cible, la phase "masquee" n'a jamais ete masquee et le verdict serait
# un faux negatif. On verifie donc avant d'echantillonner cette phase :
#   1. la fenetre sondee est bien assignee au special workspace ;
#   2. aucun moniteur n'affiche plus ce special workspace.
client = find_client(proc.pid)
observed_ws = (client or {}).get("workspace", {}).get("name")
if client is None or observed_ws != WORKSPACE:
    abort(
        f"la fenetre WebKit n'est pas assignee a {WORKSPACE!r} "
        f"(observe : {observed_ws!r}) — movetoworkspacesilent a rate sa cible, "
        "la phase 'masquee' n'est pas fiable, verdict abandonne",
        proc,
    )

monitors = hyprctl_json("monitors")
still_showing = [
    m["name"] for m in monitors
    if (m.get("specialWorkspace") or {}).get("name") == WORKSPACE
]
if still_showing:
    abort(
        f"le special workspace {WORKSPACE!r} est encore affiche sur {still_showing} "
        "apres togglespecialworkspace — la phase 'masquee' n'est pas fiable, "
        "verdict abandonne",
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
