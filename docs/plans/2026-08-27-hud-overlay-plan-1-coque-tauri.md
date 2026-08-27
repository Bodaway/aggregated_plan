# HUD aplan — Plan 1 : coque Tauri, spikes et intégration Hyprland

> **Pour les agents exécutants :** SOUS-SKILL REQUISE — utiliser
> `superpowers:subagent-driven-development` (recommandé) ou
> `superpowers:executing-plans` pour dérouler ce plan tâche par tâche. Les étapes
> utilisent la syntaxe case à cocher (`- [ ]`).

**But :** obtenir une fenêtre Tauri transparente, sans décorations, ouverte par
`SUPER+B` sur le special workspace `aplan`, affichant l'application React
existante — et savoir si WebKit nous signale son invisibilité.

**Architecture :** projet Cargo autonome sous `frontend/src-tauri/`, hors du
workspace `backend/`. La fenêtre pointe sur le serveur Vite en développement et sur
le bundle statique en production. Un script shell `aplan-hud-toggle` gère le cycle
de vie : lancement à la demande, bascule du special workspace ensuite.

**Pile technique :** Tauri v2 (wry / **webkit2gtk-4.1**), React 18 + Vite 5,
react-router-dom 6, pnpm 10.30.3, Hyprland, bash.

**Spec :** `docs/plans/2026-08-27-hud-overlay-tauri-design.md`

## Contraintes globales

Reprises verbatim de la spec ; elles s'appliquent implicitement à chaque tâche.

- **`app_id` Wayland stable : `aplan-hud`.** C'est lui que matchent les windowrules.
- **Raccourci : `SUPER+B`** (`$mainMod, B`). `SUPER+A` est déjà pris. Alternative
  autorisée : `SUPER+O`.
- **Fenêtre : `transparent: true`, `decorations: false`.**
- **Coût résident au repos : zéro.** L'application est lancée à la demande, pas au
  démarrage de session.
- **`backdrop-filter` interdit** tant que la tâche 2 ne l'a pas autorisé.
- **Effets par bloc, jamais en passe plein écran** (14,2 Mpx sur deux dalles).
- **`frontend/src-tauri/` ne rejoint pas le workspace `backend/`** — sinon chaque
  `cargo test` de l'API tire WebKit.
- **Palette CyberNord**, source unique : `~/.config/theme/`.
- Spécifications en français, code et commentaires en anglais.
- Pas de `Co-Authored-By` ni de `Signed-off-by` dans les commits. Ne stager que les
  fichiers de la tâche.

## Structure des fichiers

| Fichier | Responsabilité |
|---|---|
| `frontend/src-tauri/Cargo.toml` | projet Cargo autonome de la coque |
| `frontend/src-tauri/tauri.conf.json` | fenêtre, transparence, identité, URLs |
| `frontend/src-tauri/src/main.rs` | point d'entrée de la coque |
| `frontend/src/pages/hud/HudPage.tsx` | route `/hud`, coquille et séquence de boot |
| `frontend/src/pages/hud/useSurfaceVisibility.ts` | portillon d'animation |
| `frontend/src/styles/cybernord.css` | tokens générés — **jamais édité à la main** |
| `scripts/aplan-hud-toggle` | cycle de vie : lancer ou basculer |
| `scripts/hud-bench/` | harnais de mesure conservé (garde-fou perf) |
| `docs/plans/2026-08-27-hud-overlay-tauri-design.md` | spec, mise à jour par les spikes |

---

## Tâche 1 : Spike — WebKit signale-t-il son invisibilité ? *(bloquant)*

Le banc a établi qu'aucun toolkit natif n'est prévenu de son occultation. Si WebKit
ne l'est pas non plus, toute animation tournera en permanence et il faudra écouter
l'IPC Hyprland. Cette réponse conditionne la tâche 6 et tout le plan 3.

**Fichiers :**
- Créer : `scripts/hud-bench/visibility_probe.py`
- Créer : `scripts/hud-bench/probe.html`
- Créer : `scripts/hud-bench/webkit_host.py`
- Modifier : `docs/plans/2026-08-27-hud-overlay-tauri-design.md` (§10.1)

**Interfaces :**
- Consomme : rien.
- Produit : une réponse booléenne consignée dans la spec — *WebKit reçoit-il
  `visibilitychange` sur un special workspace masqué ?* La tâche 6 en dépend.

- [ ] **Étape 1 : écrire l'hôte WebKit de mesure**

`scripts/hud-bench/webkit_host.py` — GTK4 + WebKitGTK 6.0, fond transparent, charge
l'URI passée en argument.

```python
#!/usr/bin/env python3
"""Hote GTK4 + WebKitGTK — banc de mesure du HUD."""
import sys

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("WebKit", "6.0")
from gi.repository import Gdk, Gtk, WebKit  # noqa: E402

URI = sys.argv[1]
FULLSCREEN = "--fullscreen" in sys.argv


class Host(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="dev.hudbench.webkit")

    def do_activate(self):
        win = Gtk.ApplicationWindow(application=self)
        win.set_title("hudbench")
        win.set_default_size(320, 140)
        view = WebKit.WebView()
        transparent = Gdk.RGBA()
        transparent.parse("rgba(0,0,0,0)")
        view.set_background_color(transparent)
        view.load_uri(URI)
        win.set_child(view)
        if FULLSCREEN:
            win.fullscreen()
        win.present()


Host().run([sys.argv[0]])
```

- [ ] **Étape 2 : écrire la page sonde**

`scripts/hud-bench/probe.html` — journalise chaque changement de visibilité et
compte les images, ce qui révèle si le rendu continue même sans événement.

```html
<!doctype html><meta charset="utf-8"><title>visprobe</title>
<style>body{margin:0;background:#2e3440;color:#eceff4;font:14px monospace;padding:12px}</style>
<div id="out">init</div>
<script>
  let frames = 0;
  const out = document.getElementById("out");
  function loop() { frames++; requestAnimationFrame(loop); }
  requestAnimationFrame(loop);
  document.addEventListener("visibilitychange", () => {
    console.log("VISPROBE_EVENT " + document.visibilityState);
  });
  setInterval(() => {
    console.log(`VISPROBE_FRAMES ${frames} state=${document.visibilityState}`);
    out.textContent = `frames=${frames} state=${document.visibilityState}`;
    frames = 0;
  }, 2000);
</script>
```

- [ ] **Étape 3 : écrire la sonde**

`scripts/hud-bench/visibility_probe.py`

```python
#!/usr/bin/env python3
"""Le special workspace masque-t-il vraiment WebKit, de son point de vue ?"""
import os
import re
import signal
import subprocess
import time

HERE = os.path.dirname(os.path.abspath(__file__))

log = open("/tmp/visprobe.log", "w+")
proc = subprocess.Popen(
    ["python3", f"{HERE}/webkit_host.py", f"file://{HERE}/probe.html"],
    stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
time.sleep(6)

subprocess.run(["hyprctl", "dispatch", "movetoworkspacesilent", "special:visprobe"])
time.sleep(1)
subprocess.run(["hyprctl", "dispatch", "togglespecialworkspace", "visprobe"])
time.sleep(2)
print("--- special workspace VISIBLE, 8 s ---")
time.sleep(8)
subprocess.run(["hyprctl", "dispatch", "togglespecialworkspace", "visprobe"])
print("--- special workspace MASQUE, 8 s ---")
time.sleep(8)

try:
    os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
except OSError:
    proc.terminate()

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
```

- [ ] **Étape 4 : exécuter la sonde**

Exécuter : `python3 scripts/hud-bench/visibility_probe.py`

Attendu : deux séries de compteurs et un verdict explicite. Le nombre d'images en
phase masquée est le résultat qui compte.

- [ ] **Étape 5 : consigner le verdict dans la spec**

Remplacer le §10.1 de `docs/plans/2026-08-27-hud-overlay-tauri-design.md` par le
résultat mesuré, avec la date, la version de Hyprland (`hyprctl version | head -1`)
et les compteurs bruts. Écrire explicitement laquelle des deux branches s'applique :

- *WebKit s'endort* → la tâche 6 se contente d'écouter `visibilitychange`.
- *WebKit continue* → la tâche 6 doit s'abonner au socket IPC de Hyprland.

- [ ] **Étape 6 : commit**

```bash
git add scripts/hud-bench/ docs/plans/2026-08-27-hud-overlay-tauri-design.md
git commit -m "Mesurer si WebKit est notifie de son occultation sur special workspace

Le banc de design avait montre qu'aucun toolkit natif ne s'endort quand il est
recouvert. Cette sonde tranche le cas WebKit, dont depend le portillon
d'animation du HUD."
```

---

## Tâche 2 : Spike — coût de `backdrop-filter` dans WebKitGTK

Détermine si le flou d'arrière-plan entre dans le vocabulaire visuel du HUD ou
reste interdit.

**Fichiers :**
- Créer : `scripts/hud-bench/blur_probe.html`
- Créer : `scripts/hud-bench/measure.py`
- Modifier : `docs/plans/2026-08-27-hud-overlay-tauri-design.md` (§7, §10.2)

**Interfaces :**
- Consomme : `scripts/hud-bench/webkit_host.py` (tâche 1, étape 1).
- Produit : une décision consignée — `backdrop-filter` autorisé ou interdit — qui
  contraint tout le plan 3.

- [ ] **Étape 1 : écrire le harnais de mesure**

`scripts/hud-bench/measure.py` — lance une commande, attend que sa surface
apparaisse, échantillonne le CPU de l'arbre de processus et relève le PSS.

```python
#!/usr/bin/env python3
"""Mesure le cout d'une variante de HUD. CPU lu dans /proc (conso reelle sur
l'intervalle, pas la moyenne de vie de `ps`), memoire en PSS (les bibliotheques
partagees sont reparties au prorata, seule facon de comparer Qt, GTK et WebKit)."""
import argparse
import json
import os
import signal
import subprocess
import sys
import time

CLK_TCK = os.sysconf("SC_CLK_TCK")


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
        sys.exit("aucune commande fournie")

    proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL, start_new_session=True)
    try:
        time.sleep(a.settle)
        pids = descendants(proc.pid)
        t0, c0 = time.monotonic(), cpu_ticks(pids)
        time.sleep(a.phase)
        t1, c1 = time.monotonic(), cpu_ticks(pids)
        pids = descendants(proc.pid)
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
```

- [ ] **Étape 2 : écrire la page témoin**

`scripts/hud-bench/blur_probe.html` — deux modes pilotés par le fragment d'URL, pour
que la seule différence mesurée soit le filtre.

```html
<!doctype html><meta charset="utf-8"><title>blurprobe</title>
<style>
  body{margin:0;height:100vh;overflow:hidden;
       background:conic-gradient(#2e3440,#3b4252,#08f7fe,#ff2e63,#2e3440);}
  .card{position:absolute;left:60px;top:60px;width:420px;height:260px;
        border:1px solid #08f7fe;border-radius:10px;background:rgba(59,66,82,.45);
        animation:drift 4s ease-in-out infinite alternate;}
  .blur{backdrop-filter:blur(14px) saturate(1.4);}
  @keyframes drift{to{transform:translateX(340px)}}
</style>
<div class="card" id="c"></div>
<script>
  if (location.hash === "#blur") document.getElementById("c").classList.add("blur");
</script>
```

- [ ] **Étape 3 : mesurer les deux modes**

Exécuter :

```bash
python3 scripts/hud-bench/measure.py --label sans-blur -- \
  python3 scripts/hud-bench/webkit_host.py \
  "file://$PWD/scripts/hud-bench/blur_probe.html" --fullscreen

python3 scripts/hud-bench/measure.py --label avec-blur -- \
  python3 scripts/hud-bench/webkit_host.py \
  "file://$PWD/scripts/hud-bench/blur_probe.html#blur" --fullscreen
```

Attendu : deux objets JSON comportant `cpu_pct` et `pss_mb`.

- [ ] **Étape 4 : arbitrer**

Règle de décision, à appliquer telle quelle : si le `cpu_pct` du mode `#blur`
dépasse **2 ×** celui du mode sans filtre, `backdrop-filter` reste **interdit** et le
§7 de la spec est inchangé. Sinon, le déplacer dans la liste « autorisé » en notant
le surcoût mesuré à côté.

- [ ] **Étape 5 : consigner et commiter**

```bash
git add scripts/hud-bench/ docs/plans/2026-08-27-hud-overlay-tauri-design.md
git commit -m "Chiffrer le cout de backdrop-filter dans WebKitGTK

WebKitGTK n'est pas Chromium et son compositing est plus faible. Le filtre
etait interdit par precaution ; il l'est desormais sur mesure, ou autorise."
```

---

## Tâche 3 : Tokens CyberNord générés vers le frontend

Une seule source de vérité pour la palette : changer le thème du poste doit
repeindre le HUD.

**Fichiers :**
- Modifier : `~/.config/theme/apply-theme.sh` (ajout d'une section)
- Créer : `frontend/src/styles/cybernord.css` (généré)
- Créer : `frontend/src/styles/tokens.test.ts`
- Modifier : `frontend/src/main.tsx` (import du CSS)
- Modifier : `frontend/tailwind.config.ts`

**Interfaces :**
- Consomme : les variables du fichier de palette (`BG`, `FG`, `DIM`, `BG_SURFACE`,
  `BLUE`, `GREEN`, `YELLOW`, `RED`, `PURPLE`, `TEAL`, `ORANGE`, `FONT`).
- Produit : les custom properties `--cn-bg`, `--cn-fg`, `--cn-dim`, `--cn-surface`,
  `--cn-blue`, `--cn-green`, `--cn-yellow`, `--cn-red`, `--cn-purple`, `--cn-teal`,
  `--cn-orange`, `--cn-font` sur `:root`, plus les utilitaires Tailwind
  `bg-cn-surface`, `text-cn-fg`, `border-cn-teal`, `font-cn`. Les plans 3 et 4 les
  consomment.

- [ ] **Étape 1 : écrire le test**

`frontend/src/styles/tokens.test.ts`

```typescript
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const CSS = readFileSync(resolve(__dirname, 'cybernord.css'), 'utf8');

const REQUIRED = [
  '--cn-bg', '--cn-fg', '--cn-dim', '--cn-surface', '--cn-blue', '--cn-green',
  '--cn-yellow', '--cn-red', '--cn-purple', '--cn-teal', '--cn-orange', '--cn-font',
] as const;

describe('tokens CyberNord', () => {
  it('déclare toutes les custom properties sur :root', () => {
    for (const token of REQUIRED) {
      expect(CSS).toContain(`${token}:`);
    }
  });

  it('porte un avertissement de non-édition', () => {
    expect(CSS).toMatch(/généré par apply-theme\.sh/i);
  });

  it('utilise des couleurs hexadécimales à 6 chiffres', () => {
    const colors = CSS.match(/--cn-(?!font)[a-z]+:\s*([^;]+);/g) ?? [];
    expect(colors.length).toBeGreaterThanOrEqual(11);
    for (const decl of colors) {
      expect(decl).toMatch(/#[0-9a-fA-F]{6}/);
    }
  });
});
```

- [ ] **Étape 2 : lancer le test pour le voir échouer**

Exécuter : `cd frontend && pnpm vitest run src/styles/tokens.test.ts`
Attendu : ÉCHEC — `ENOENT: no such file or directory ... cybernord.css`.

- [ ] **Étape 3 : étendre le générateur de thème**

Créer d'abord le répertoire cible :

```bash
mkdir -p frontend/src/styles
```

Puis ajouter cette section à `~/.config/theme/apply-theme.sh`, avant sa ligne
d'écho de fin :

```bash
# --------------------------------------------------------------------------- #
# N. aplan HUD — CSS custom properties
# --------------------------------------------------------------------------- #
APLAN_CSS="$HOME/appfactory/aggregated_plan/frontend/src/styles/cybernord.css"
if [[ -d "$(dirname "$APLAN_CSS")" ]]; then
    cat > "$APLAN_CSS" <<EOF
/* Fichier généré par apply-theme.sh — NE PAS ÉDITER À LA MAIN.
   Thème : ${THEME_NAME}. Régénérer avec : ~/.config/theme/apply-theme.sh */
:root {
  --cn-bg:      #${BG};
  --cn-surface: #${BG_SURFACE};
  --cn-dim:     #${DIM};
  --cn-fg:      #${FG};
  --cn-blue:    #${BLUE};
  --cn-green:   #${GREEN};
  --cn-yellow:  #${YELLOW};
  --cn-red:     #${RED};
  --cn-purple:  #${PURPLE};
  --cn-teal:    #${TEAL};
  --cn-orange:  #${ORANGE};
  --cn-font:    "${FONT}", monospace;
}
EOF
    echo "  [ok] aplan hud tokens"
fi
```

- [ ] **Étape 4 : générer et vérifier**

Exécuter :

```bash
~/.config/theme/apply-theme.sh ~/.config/theme/$(cat ~/.config/theme/.current)
cd frontend && pnpm vitest run src/styles/tokens.test.ts
```

Attendu : SUCCÈS, trois tests verts.

- [ ] **Étape 5 : brancher les tokens dans l'application**

Dans `frontend/src/main.tsx`, ajouter l'import **avant** l'import Tailwind existant :

```typescript
import '@/styles/cybernord.css';
```

Dans `frontend/tailwind.config.ts`, remplacer `theme: { extend: {} }` par :

```typescript
  theme: {
    extend: {
      colors: {
        cn: {
          bg: 'var(--cn-bg)',
          surface: 'var(--cn-surface)',
          dim: 'var(--cn-dim)',
          fg: 'var(--cn-fg)',
          blue: 'var(--cn-blue)',
          green: 'var(--cn-green)',
          yellow: 'var(--cn-yellow)',
          red: 'var(--cn-red)',
          purple: 'var(--cn-purple)',
          teal: 'var(--cn-teal)',
          orange: 'var(--cn-orange)',
        },
      },
      fontFamily: {
        cn: ['var(--cn-font)'],
      },
    },
  },
```

- [ ] **Étape 6 : vérifier qu'aucune régression n'apparaît**

Exécuter : `cd frontend && pnpm type-check && pnpm test`
Attendu : SUCCÈS sur la suite existante.

- [ ] **Étape 7 : commit**

```bash
git add frontend/src/styles/ frontend/src/main.tsx frontend/tailwind.config.ts
git commit -m "Generer les tokens CyberNord depuis le theme du poste

Le HUD doit partager la palette du bureau plutot que de la dupliquer :
apply-theme.sh emet desormais un fichier de custom properties CSS a cote de
colors.conf, et Tailwind les expose sous le prefixe cn-."
```

Note : `apply-theme.sh` vit hors du dépôt (`~/.config/theme/`). Le commit ne le
contient donc pas — le corps du message le mentionne, et la modification doit être
reportée si ce dotfile est versionné ailleurs.

---

## Tâche 4 : Coque Tauri v2 avec fenêtre transparente

**Fichiers :**
- Créer : `frontend/src-tauri/Cargo.toml`
- Créer : `frontend/src-tauri/tauri.conf.json`
- Créer : `frontend/src-tauri/build.rs`
- Créer : `frontend/src-tauri/src/main.rs`
- Créer : `frontend/src-tauri/.gitignore`
- Modifier : `frontend/package.json` (scripts)

**Interfaces :**
- Consomme : le serveur Vite sur `http://localhost:3000` et le bundle `frontend/dist`.
- Produit : un binaire `aplan-hud` ouvrant `/hud`, dont l'`app_id` Wayland est
  **vérifié à l'étape 6**. La tâche 5 s'appuie sur la valeur constatée.

- [ ] **Étape 1 : installer l'outillage Tauri**

```bash
cd frontend
pnpm add -D @tauri-apps/cli@^2
pnpm add @tauri-apps/api@^2
```

Vérifier les dépendances système (déjà installées sur le poste) :

```bash
pacman -Qq webkit2gtk-4.1 gtk4 librsvg 2>/dev/null
```

- [ ] **Étape 2 : écrire la configuration Tauri**

`frontend/src-tauri/tauri.conf.json`

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "aplan-hud",
  "version": "0.1.0",
  "identifier": "dev.aplan.hud",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:3000",
    "beforeDevCommand": "pnpm dev",
    "beforeBuildCommand": "pnpm build"
  },
  "app": {
    "windows": [
      {
        "label": "hud",
        "title": "aplan",
        "url": "/hud",
        "width": 1600,
        "height": 1000,
        "transparent": true,
        "decorations": false,
        "resizable": true,
        "center": true
      }
    ],
    "security": { "csp": null }
  },
  "bundle": { "active": false }
}
```

- [ ] **Étape 3 : écrire le crate Rust**

`frontend/src-tauri/Cargo.toml`

```toml
[package]
name = "aplan-hud"
version = "0.1.0"
edition = "2021"
description = "Coque Tauri du HUD aplan"

# NE PAS rattacher ce crate au workspace backend/ : il tirerait WebKit dans
# chaque `cargo test` de l'API. La cle [workspace] vide l'en isole.
[workspace]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }

[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
strip = true
```

`frontend/src-tauri/build.rs`

```rust
fn main() {
    tauri_build::build()
}
```

`frontend/src-tauri/src/main.rs`

```rust
// Minimal shell. On Wayland the app_id is derived by GTK from the executable
// name, which `productName = "aplan-hud"` in tauri.conf.json already sets.
// Step 6 verifies that empirically rather than trusting it — if the compositor
// reports something else, the windowrule follows the measurement, not this file.
fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to start the aplan HUD shell");
}
```

`frontend/src-tauri/.gitignore`

```
/target
/gen
```

- [ ] **Étape 4 : ajouter les scripts npm**

Dans `frontend/package.json`, ajouter à `scripts` :

```json
    "tauri": "tauri",
    "hud:dev": "tauri dev",
    "hud:build": "tauri build --no-bundle"
```

- [ ] **Étape 5 : compiler**

Exécuter : `cd frontend && pnpm hud:build`
Attendu : SUCCÈS ; binaire à `frontend/src-tauri/target/release/aplan-hud`.

Première compilation longue — Tauri tire un large graphe de dépendances.

- [ ] **Étape 6 : vérifier l'`app_id` — le point de vérité**

C'est le spike 10.3 de la spec.

```bash
frontend/src-tauri/target/release/aplan-hud &
sleep 4
hyprctl -j clients | python3 -c "
import sys, json
for c in json.load(sys.stdin):
    if 'aplan' in (c.get('class') or '').lower() or c.get('title') == 'aplan':
        print('class =', repr(c.get('class')), '| xwayland =', c.get('xwayland'))"
pkill -x aplan-hud
```

Attendu : `class = 'aplan-hud'` et `xwayland = False`.

**Si la classe diffère** (par exemple `dev.aplan.hud`) : ne pas tordre le code —
relever la valeur réelle, **l'employer telle quelle à la tâche 5**, et corriger la
contrainte globale de ce plan ainsi que le §6 de la spec. La windowrule matche la
réalité, pas l'inverse.

- [ ] **Étape 7 : commit**

```bash
git add frontend/src-tauri/ frontend/package.json frontend/pnpm-lock.yaml
git commit -m "Ajouter la coque Tauri v2 du HUD

Projet Cargo volontairement autonome : l'integrer au workspace backend
ferait tirer WebKit a chaque cargo test de l'API. Fenetre transparente et
sans decorations, ouverte sur la route /hud."
```

---

## Tâche 5 : Intégration Hyprland — windowrules, bascule, raccourci

**Fichiers :**
- Créer : `scripts/aplan-hud-toggle`
- Créer : `scripts/aplan-hud-toggle.test.sh`
- Modifier : `~/.config/hypr/hyprland.conf`

**Interfaces :**
- Consomme : l'`app_id` constaté à la tâche 4 étape 6 et le binaire
  `frontend/src-tauri/target/release/aplan-hud`.
- Produit : la commande `aplan-hud-toggle`, appelée par le keybind. Aucune tâche
  ultérieure n'en dépend.

- [ ] **Étape 1 : écrire le test**

`scripts/aplan-hud-toggle.test.sh` — isole le script de `hyprctl` et `pgrep` par des
doublures posées sur le `PATH`.

```bash
#!/usr/bin/env bash
# Teste aplan-hud-toggle sans toucher au compositeur reel.
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
    if grep -q "$expect" "$STUB_LOG"; then
        echo "  ok   $name"
    else
        echo "  FAIL $name — attendu '$expect' dans :"; sed 's/^/       /' "$STUB_LOG"
        FAILED=1
    fi
    rm -rf "$stub"
}

run_case "processus absent -> lance le binaire"      no  "launched"
run_case "processus present -> bascule le workspace" yes "togglespecialworkspace aplan"
exit $FAILED
```

- [ ] **Étape 2 : lancer le test pour le voir échouer**

Exécuter : `chmod +x scripts/aplan-hud-toggle.test.sh && ./scripts/aplan-hud-toggle.test.sh`
Attendu : ÉCHEC — `No such file or directory`, le script sous test n'existe pas.

- [ ] **Étape 3 : écrire le script**

`scripts/aplan-hud-toggle`

```bash
#!/usr/bin/env bash
# Ouvre le HUD aplan, ou le masque s'il est deja la.
#
# Le HUD n'est pas resident : la premiere invocation lance le binaire, les
# suivantes se contentent de basculer le special workspace. Cout au repos : zero.
set -euo pipefail

WORKSPACE="aplan"
BIN="${APLAN_HUD_BIN:-$HOME/appfactory/aggregated_plan/frontend/src-tauri/target/release/aplan-hud}"

if pgrep -x aplan-hud >/dev/null 2>&1; then
    hyprctl dispatch togglespecialworkspace "$WORKSPACE"
else
    "$BIN" &
    disown
fi
```

- [ ] **Étape 4 : lancer le test pour le voir passer**

Exécuter : `chmod +x scripts/aplan-hud-toggle && ./scripts/aplan-hud-toggle.test.sh`
Attendu : SUCCÈS, deux cas `ok`.

- [ ] **Étape 5 : déclarer les règles Hyprland**

Ajouter à `~/.config/hypr/hyprland.conf` — en substituant `aplan-hud` par la classe
réellement constatée si elle diffère :

```
# --- aplan HUD -------------------------------------------------------------
windowrulev2 = float,                          class:^(aplan-hud)$
windowrulev2 = noborder,                       class:^(aplan-hud)$
windowrulev2 = noshadow,                       class:^(aplan-hud)$
windowrulev2 = workspace special:aplan silent, class:^(aplan-hud)$
windowrulev2 = size 90% 90%,                   class:^(aplan-hud)$
windowrulev2 = center,                         class:^(aplan-hud)$

bind = $mainMod, B, exec, ~/appfactory/aggregated_plan/scripts/aplan-hud-toggle
```

- [ ] **Étape 6 : recharger et vérifier à la main**

Exécuter : `hyprctl reload`

Presser `SUPER+B` trois fois et confirmer, dans l'ordre :
1. la fenêtre apparaît, sans bordure ni barre de titre, centrée à 90 % ;
2. elle disparaît ;
3. elle réapparaît **sans relancer de processus** — `pgrep -c aplan-hud` doit
   rester à `1`.

- [ ] **Étape 7 : commit**

```bash
git add scripts/aplan-hud-toggle scripts/aplan-hud-toggle.test.sh
git commit -m "Ouvrir le HUD sur SUPER+B via un special workspace

SUPER+A etait deja pris ; B reste sous l'index gauche en AZERTY. Le script
lance le binaire a la premiere invocation puis se contente de basculer le
workspace, ce qui garde un cout resident nul au repos."
```

---

## Tâche 6 : Route `/hud`, séquence de boot et portillon d'animation

**Fichiers :**
- Créer : `frontend/src/pages/hud/HudPage.tsx`
- Créer : `frontend/src/pages/hud/useSurfaceVisibility.ts`
- Créer : `frontend/src/pages/hud/useSurfaceVisibility.test.ts`
- Créer : `frontend/src/pages/hud/HudPage.test.tsx`
- Modifier : `frontend/src/App.tsx`

**Interfaces :**
- Consomme : les tokens `--cn-*` (tâche 3) et le verdict de visibilité (tâche 1).
- Produit : `useSurfaceVisibility(): boolean` — `true` quand la surface est
  regardée. **Le plan 3 y branche chaque animation de bloc.** Et `<HudPage />`,
  coquille d'accueil que le plan 3 remplira de ses six blocs.

- [ ] **Étape 1 : écrire le test du portillon**

`frontend/src/pages/hud/useSurfaceVisibility.test.ts`

```typescript
import { act, renderHook } from '@testing-library/react';
import { useSurfaceVisibility } from './useSurfaceVisibility';

function setVisibility(state: DocumentVisibilityState) {
  Object.defineProperty(document, 'visibilityState', {
    configurable: true,
    get: () => state,
  });
  document.dispatchEvent(new Event('visibilitychange'));
}

describe('useSurfaceVisibility', () => {
  afterEach(() => setVisibility('visible'));

  it('démarre visible quand le document est visible', () => {
    setVisibility('visible');
    const { result } = renderHook(() => useSurfaceVisibility());
    expect(result.current).toBe(true);
  });

  it('bascule à false quand le document devient masqué', () => {
    const { result } = renderHook(() => useSurfaceVisibility());
    act(() => setVisibility('hidden'));
    expect(result.current).toBe(false);
  });

  it('revient à true au retour du document', () => {
    const { result } = renderHook(() => useSurfaceVisibility());
    act(() => setVisibility('hidden'));
    act(() => setVisibility('visible'));
    expect(result.current).toBe(true);
  });

  it('se désabonne au démontage', () => {
    const remove = vi.spyOn(document, 'removeEventListener');
    const { unmount } = renderHook(() => useSurfaceVisibility());
    unmount();
    expect(remove).toHaveBeenCalledWith('visibilitychange', expect.any(Function));
  });
});
```

- [ ] **Étape 2 : lancer le test pour le voir échouer**

Exécuter : `cd frontend && pnpm vitest run src/pages/hud/useSurfaceVisibility.test.ts`
Attendu : ÉCHEC — module introuvable.

- [ ] **Étape 3 : écrire le portillon**

`frontend/src/pages/hud/useSurfaceVisibility.ts`

```typescript
import { useEffect, useState } from 'react';

/**
 * True while the HUD surface is actually being looked at.
 *
 * Every animation in the HUD must be gated on this. The design benchmark showed
 * that continuous animation accounts for ~99% of CPU cost, and that no native
 * toolkit stops on its own when covered — so we stop explicitly.
 */
export function useSurfaceVisibility(): boolean {
  const [visible, setVisible] = useState(() => document.visibilityState === 'visible');

  useEffect(() => {
    const onChange = () => setVisible(document.visibilityState === 'visible');
    document.addEventListener('visibilitychange', onChange);
    return () => document.removeEventListener('visibilitychange', onChange);
  }, []);

  return visible;
}
```

**Si la tâche 1 a conclu que WebKit ne s'endort pas**, ce hook ne suffit pas seul :
ouvrir alors une tâche dédiée en tête du plan 3 pour relayer les événements du
socket Hyprland vers ce même hook via une commande Tauri. Ne rien écrire de
spéculatif ici — la tâche 1 tranche, et son verdict est inscrit au §10.1 de la spec.

- [ ] **Étape 4 : lancer le test pour le voir passer**

Exécuter : `cd frontend && pnpm vitest run src/pages/hud/useSurfaceVisibility.test.ts`
Attendu : SUCCÈS, quatre tests verts.

- [ ] **Étape 5 : écrire le test de la page**

`frontend/src/pages/hud/HudPage.test.tsx`

```tsx
import { render, screen, act } from '@testing-library/react';
import { HudPage } from './HudPage';

describe('HudPage', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('affiche la séquence de boot en premier', () => {
    render(<HudPage />);
    expect(screen.getByTestId('boot-sequence')).toBeInTheDocument();
    expect(screen.queryByTestId('hud-grid')).not.toBeInTheDocument();
  });

  it('cède la place à la grille après la séquence', () => {
    render(<HudPage />);
    act(() => void vi.advanceTimersByTime(1600));
    expect(screen.queryByTestId('boot-sequence')).not.toBeInTheDocument();
    expect(screen.getByTestId('hud-grid')).toBeInTheDocument();
  });

  it('peint un fond transparent, la fenêtre étant transparente', () => {
    const { container } = render(<HudPage />);
    const root = container.firstElementChild as HTMLElement;
    expect(root.className).toContain('bg-transparent');
  });
});
```

- [ ] **Étape 6 : lancer le test pour le voir échouer**

Exécuter : `cd frontend && pnpm vitest run src/pages/hud/HudPage.test.tsx`
Attendu : ÉCHEC — module introuvable.

- [ ] **Étape 7 : écrire la page**

`frontend/src/pages/hud/HudPage.tsx`

```tsx
import { useEffect, useState } from 'react';
import { useSurfaceVisibility } from './useSurfaceVisibility';

const BOOT_LINES = [
  'aplan cockpit v0.1.0',
  'link 127.0.0.1:3001 ......... ok',
  'palette cybernord .......... ok',
  'session bus ................ ok',
] as const;

const BOOT_MS = 1500;

export function HudPage() {
  const [booting, setBooting] = useState(true);
  const visible = useSurfaceVisibility();

  useEffect(() => {
    const t = setTimeout(() => setBooting(false), BOOT_MS);
    return () => clearTimeout(t);
  }, []);

  return (
    <div
      className="h-screen w-screen bg-transparent font-cn text-cn-fg"
      data-surface-visible={visible}
    >
      {booting ? (
        <pre data-testid="boot-sequence" className="p-8 text-sm text-cn-teal">
          {BOOT_LINES.join('\n')}
        </pre>
      ) : (
        <div data-testid="hud-grid" className="grid h-full grid-cols-12 gap-3 p-6">
          {/* Les six blocs arrivent au plan 3. */}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Étape 8 : lancer les tests pour les voir passer**

Exécuter : `cd frontend && pnpm vitest run src/pages/hud/`
Attendu : SUCCÈS, sept tests verts.

- [ ] **Étape 9 : déclarer la route**

Dans `frontend/src/App.tsx`, ajouter l'import après les autres imports de pages :

```typescript
import { HudPage } from '@/pages/hud/HudPage';
```

Puis, **avant** la route `/` de redirection, ajouter — délibérément **sans**
`PageLayout`, le HUD étant plein écran et transparent :

```tsx
          <Route path="/hud" element={<HudPage />} />
```

- [ ] **Étape 10 : vérifier la chaîne complète**

Exécuter : `cd frontend && pnpm type-check && pnpm test`
Attendu : SUCCÈS, aucune régression.

Puis en conditions réelles :

```bash
cd frontend && pnpm hud:build
scripts/aplan-hud-toggle
sleep 5
grim -o "$(hyprctl -j monitors | python3 -c 'import sys,json;print([m["name"] for m in json.load(sys.stdin) if m["focused"]][0])')" /tmp/hud-final.png
python3 -c "
from PIL import Image
im = Image.open('/tmp/hud-final.png').convert('RGB')
print('coin :', im.getpixel((40, 40)))"
```

Attendu : la séquence de boot défile puis laisse la grille vide, et le coin d'écran
rend la couleur du fond d'écran — **pas** `(255, 255, 255)`. C'est la vérification
définitive de la transparence, celle que Chromium avait échouée au banc de design.

- [ ] **Étape 11 : commit**

```bash
git add frontend/src/pages/hud/ frontend/src/App.tsx
git commit -m "Ajouter la route /hud, sa sequence de boot et le portillon d'animation

Le banc de design a montre que l'animation continue represente ~99 % du cout
CPU et qu'aucun toolkit ne s'arrete de lui-meme quand il est recouvert.
useSurfaceVisibility est le point unique ou chaque bloc du HUD viendra
suspendre ses animations."
```

---

## Ce que ce plan ne couvre pas

- **Plan 2** — crate `hud-daemon`, index incrémental des transcripts Claude, table
  de rollups, resolvers GraphQL pour NEURAL BUDGET et AGENTS ACTIFS.
- **Plan 3** — maquette visuelle validée, puis les six blocs du HUD, leur hiérarchie
  de composition et leur vocabulaire d'effets. Plus, si la tâche 1 l'impose, le
  relais du socket Hyprland vers `useSurfaceVisibility`.
- **Plan 4** — module waybar (tâche + chrono) pour la veille permanente.

## Définition de terminé

`SUPER+B` ouvre une fenêtre transparente et sans décorations sur le special
workspace `aplan`, affichant une séquence de boot puis une grille vide aux couleurs
CyberNord. Une seconde pression la masque, une troisième la rappelle sans relancer
de processus. `pnpm test` et `pnpm type-check` passent. La spec porte les verdicts
mesurés des deux spikes.
