# HUD cyberpunk et overlay Tauri — document de design

**Date** : 27 août 2026
**Statut** : design validé, en attente du plan d'implémentation
**Périmètre** : nouvelle surface de bureau pour aplan, intégrée à Hyprland

---

## 1. Objectif

Doter aplan d'un **cockpit visuel invoqué au clavier**, esthétiquement travaillé
(registre cyberpunk / electro), intégré au poste Hyprland de l'utilisateur, et dont
l'écran d'accueil est un HUD dense donnant l'état de la journée en un coup d'œil.

Deux critères ont été déclarés prioritaires, dans cet ordre :

1. **L'esthétique.** Le résultat doit être beau et distinctif, pas fonctionnel-et-gris.
2. **La performance.** Le poste est un portable ; rien ne doit tourner en permanence
   pour rien.

Toutes les décisions ci-dessous découlent de ces deux critères et d'un banc d'essai
mesuré sur la machine cible (§3).

---

## 2. Décisions

| Sujet | Décision |
|---|---|
| Véhicule de l'overlay | **Tauri v2** (GTK4 + WebKitGTK 6.0 sur Linux) |
| Contenu | Route d'accueil `/hud` + **la totalité de l'app React existante** |
| Placement | Special workspace Hyprland `aplan`, fenêtre transparente sans décorations |
| Raccourci | **`SUPER+B`** — main gauche, index. `SUPER+A` était pris ; `SUPER+O` reste l'alternative (mais tombe sous la main droite en AZERTY) |
| Cycle de vie | **Lancé à la demande**, pas résident. Coût au repos : zéro |
| Veille permanente | Un module **waybar** (tâche + chrono), sur une barre déjà lancée |
| Signaux du bureau | Nouveau crate `hud-daemon` pour l'index de conso Claude |
| Palette | **CyberNord**, l'identité déjà en place sur le poste |

### Ce qui a été écarté, et pourquoi

- **Chromium** — mesuré **opaque** sur Wayland (fond blanc, avec et sans
  `--enable-transparent-visuals`), donc incompatible avec la transparence voulue ;
  et 337 Mo de PSS pour 13 processus.
- **Quickshell (QML)** — le plus léger et le plus rapide, transparence vérifiée au
  pixel, mais impose de réécrire tout le frontend React. Son avantage annoncé
  (shaders GLSL faciles) ne compense pas le plafond esthétique supérieur de CSS :
  modes de fusion, filtres SVG, typographie, mise en page.
- **Iced** — n'a gagné sur aucun axe mesuré (plus lourd et plus gourmand que
  Quickshell, CPU au repos 6× supérieur), et surtout : **pas de rechargement à
  chaud, pas de langage de style**. Le pire point de départ pour un projet dont
  l'objectif premier est visuel.
- **GTK4** — le plus léger en mémoire (62 Mo) mais **aucune voie shader** et les
  animations CSS ont disparu depuis GTK3.

---

## 3. Le banc d'essai qui fonde ces décisions

Même bloc « Focus & temps » (320×140, respiration d'opacité sur 2,4 s, surcouche
scanline animée, chrono sur timer 1 s) implémenté à l'identique dans quatre piles,
mesuré sur la machine cible : Hyprland sur **RTX 3060 Mobile**, deux dalles
(3072×1920@60 scale 1,5 + 3840×2160@60) soit **14,2 Mpx**.

| | Quickshell | Iced | GTK4 | WebKitGTK | Chromium |
|---|---|---|---|---|---|
| PSS | 93–97 Mo | 103–135 Mo | 62–64 Mo | 261 Mo¹ | 337 Mo |
| Processus | 1 | 1 | 1 | 8 | 13 |
| Démarrage → surface | 0,11 s | 0,07 s | 0,19 s | 0,31 s¹ | 0,50 s |
| CPU animé, visible | 6,90 % | 8,25 % | 11,25 % | 3,80 % | 36,83 % |
| CPU animé, occulté | 7,45 % | 8,45 % | 10,65 % | — | 4,47 % |
| **CPU statique** | **0,10 %** | 0,60 % | **0,10 %** | — | — |
| Transparence | ✓ | non testée | — | ✓ | **✗ opaque** |

¹ Hôte de mesure en Python. L'écart Python↔Rust mesuré sur base GTK4 identique est
de **15,6 Mo** (63,0 vs 78,6). Un hôte Tauri en Rust est donc estimé à **~245 Mo**
et un démarrage sous 0,25 s.

Mesures écartées comme non fiables : les colonnes GPU et watts de la première passe.
L'échantillonneur lançait `nvidia-smi` chaque seconde, ce qui réveillait la carte et
contaminait la lecture. Une seule passe par variante, donc pas de barres d'erreur ;
variance observée ~5 % sur le PSS.

### Les deux enseignements structurants

**(a) L'animation continue est ~99 % du coût CPU.** Les mêmes variantes, animations
coupées, tombent à 0,10–0,60 %. Le choix du toolkit est du bruit à côté de la
question « anime-t-on en permanence ou non ».

**(b) Aucun toolkit natif ne s'endort quand il est recouvert.** Quickshell, Iced et
GTK4 continuent d'animer à plein régime sous une fenêtre plein écran opaque
(occultation vérifiée). Seul Chromium se met en veille (÷ 8). C'est ce constat qui a
condamné l'idée initiale d'un HUD permanent sur la couche fond : il aurait coûté 7 à
11 % d'un cœur en continu pour une animation que personne ne voit.

**La solution retenue — le HUD comme accueil d'un overlay invoqué — supprime le
problème par construction** : la surface n'existe que lorsqu'elle est regardée.

---

## 4. Architecture

Trois livrables volontairement séparés.

```
frontend/
├── src/pages/hud/          # la route /hud — écran d'accueil de l'overlay
├── src/styles/cybernord.css # tokens générés (voir §7)
└── src-tauri/              # app Tauri v2 — PROJET CARGO AUTONOME
backend/crates/
└── hud-daemon/             # binaire mince : index des transcripts Claude
```

**`frontend/src-tauri/` reste hors du workspace `backend/`.** Sinon chaque
`cargo test` de l'API tire WebKitGTK dans le graphe de compilation. Le dépôt souffre
déjà d'un crate qui casse la construction globale ; on n'en ajoute pas un second.

**`hud-daemon` respecte le découpage DDD du dépôt** : le trait d'indexation est
déclaré dans `application`, son implémentation (lecture de fichiers, parsing) vit
dans `infrastructure`, et le crate `hud-daemon` n'est qu'un binaire de câblage.

---

## 5. Flux de données — trois canaux, pas un de plus

| Donnée | Chemin | Justification |
|---|---|---|
| Focus, échéances, charge, agenda, pauses | GraphQL `:3001` + SSE, client urql existant | déjà en place, rien à écrire |
| **NEURAL BUDGET** (conso Claude) | `hud-daemon` → rollups SQLite → resolver GraphQL | la donnée devient historisée et requêtable |
| **AGENTS ACTIFS** | GraphQL sur la table `sessions` (migration 014) + fraîcheur du `.jsonl` | **aucune capture nouvelle** : la table existe déjà |
| Télémétrie système (CPU, RAM, réseau, horloge) | commande Tauri IPC (`sysinfo`) | éphémère et local ; n'a rien à faire en base |

**Une seule table nouvelle** : les rollups de conso Claude.

Aucune capture de fenêtre active ni de titre n'est prévue. Ce bloc (« CONTEXT »)
n'avait pas été retenu, et son absence évite d'introduire une base locale de données
de surveillance.

---

## 6. Intégration Hyprland

```
# fenêtre
windowrulev2 = float,       class:^(aplan-hud)$
windowrulev2 = noborder,    class:^(aplan-hud)$
windowrulev2 = workspace special:aplan, class:^(aplan-hud)$

# raccourci
bind = $mainMod, B, exec, aplan-hud-toggle
```

- La fenêtre Tauri déclare `transparent: true`, `decorations: false`, et un
  **`app_id` stable `aplan-hud`** — c'est lui que matchent les windowrules.
- `aplan-hud-toggle` bascule le special workspace si le processus tourne, et **le
  lance sinon**. Coût résident au repos : **zéro**. Ouverture estimée : ~0,25 s,
  masquée par l'animation d'ouverture du special workspace.
- Le special workspace fournit gratuitement l'animation de glissement et
  l'assombrissement du fond ; aucun protocole layer-shell n'est nécessaire pour
  l'overlay.

---

## 7. Budget d'effets visuels

Contraint par les mesures, pas par le goût.

**Autorisé**
- dégradés linéaires et coniques
- `mix-blend-mode` (`screen`, `plus-lighter`) — le vrai ressort du néon
- `box-shadow` pour la lueur, `filter: drop-shadow / hue-rotate`
- filtres SVG : `feTurbulence` et `feDisplacementMap` pour le glitch
- `<canvas>` pour sparklines et jauges

**Interdit en v1**
- **`backdrop-filter`** — coûteux dans WebKitGTK, qui n'est pas Chromium. À
  débloquer uniquement après mesure (§10.2).

**Règles**
- Les effets s'appliquent **par bloc, jamais en passe plein écran**. À 14,2 Mpx sur
  deux dalles, une passe globale sort la dGPU de son état P8 (plancher mesuré : 11 W
  sur 60 W).
- **Toute animation est conditionnée à la visibilité de la fenêtre** (§10.1).
- **Boot sequence** à l'ouverture : faux log système défilant ~1,5 s, puis
  matérialisation des blocs. Coût quasi nul, rendement maximal sur l'ambiance.

### Tokens

La palette **CyberNord** est déjà l'identité du poste :

| Rôle | Valeur |
|---|---|
| fond | `#2e3440` |
| surface | `#3b4252` |
| atténué | `#4c566a` |
| texte | `#eceff4` |
| cyan électrique | `#08f7fe` |
| rose | `#ff2e63` |
| violet | `#d08fff` |
| vert néon | `#00ff9c` |
| orange | `#ff6e27` |
| police | JetBrainsMono Nerd Font |

Le script `apply-theme.sh` du poste, qui génère déjà `~/.config/hypr/colors.conf`,
est **étendu pour émettre en parallèle un fichier de custom properties CSS**. Une
seule source de vérité : changer le thème du poste repeint le HUD.

---

## 8. Les six blocs du HUD

1. **Focus & temps** — tâche active, chrono, les quatre quarts de la journée et leur
   remplissage, prochaine pause et compte à rebours.
2. **Pression** — échéances du jour et J-N, alertes de surcharge, charge planifiée
   contre capacité.
3. **Agenda** — prochaine réunion avec compte à rebours, timeline de la journée.
4. **Télémétrie d'ambiance** — horloge et date en gros, CPU, RAM, réseau.
5. **NEURAL BUDGET** — burn rate Claude sur la fenêtre glissante de 5 h, jauge contre
   un plafond déclaré, sparkline tokens/jour, répartition par modèle, top projets.
6. **AGENTS ACTIFS** — sessions Claude Code vivantes et la tâche aplan de chacune.

**Contrainte de composition** : six blocs, c'est dense. La maquette doit établir une
**hiérarchie forte** — un bloc dominant, deux secondaires, trois en périphérie — et
non six cartes de même poids visuel.

---

## 9. L'index de conso Claude

Source : `~/.claude/projects/**/*.jsonl`. État constaté sur la machine :
**462 fichiers, 434 Mo**. Chaque message assistant porte
`usage{input_tokens, cache_creation_input_tokens, cache_read_input_tokens,
output_tokens, output_tokens_details.thinking_tokens}`, plus `model`, `timestamp`
et `sessionId` ; le nom du dossier encode le chemin du projet.

**Index incrémental obligatoire** : offset de lecture et `mtime` mémorisés par
fichier, jamais de re-scan complet.

**Limite assumée** : le quota d'abonnement lui-même (les chiffres de `/usage`) n'est
pas exposé par une API publique — il vient d'un endpoint interne interrogé avec le
token OAuth du CLI. Le HUD affiche donc un burn **mesuré localement** contre un
**plafond déclaré à la main**, calibré une fois contre `/usage`. La jauge ne ment que
sur son dénominateur, et ce point doit être visible dans l'interface.

---

## 10. Risques ouverts — spikes à mener avant de coder

### 10.1 Visibilité sur special workspace *(bloquant)*

Le banc a établi qu'aucun toolkit natif n'est prévenu de son occultation. **Une
fenêtre WebKit sur un special workspace masqué reçoit-elle un changement de
visibilité** (`document.visibilityState`, événements de fenêtre Tauri) ?

- Si oui : couper les animations sur cet événement suffit.
- Si non : le HUD doit s'abonner au socket IPC de Hyprland
  (`$XDG_RUNTIME_DIR/hypr/$HIS/.socket2.sock`) et couper ses animations lui-même,
  sans quoi 3,8 % de CPU tournent en permanence.

Le harnais de mesure du banc est réutilisable tel quel pour trancher.

### 10.2 Coût réel de `backdrop-filter` dans WebKitGTK

Détermine si le flou d'arrière-plan entre dans le vocabulaire visuel ou non.

### 10.3 Stabilité de l'`app_id`

Vérifier que Tauri v2 pose bien `aplan-hud` comme app_id Wayland. Constat annexe du
banc : **Chromium en mode `--app=` ignore `--class`** et dérive son app_id de l'URL —
piège classique dont Tauri doit être exempt pour que les windowrules tiennent.

### 10.4 Format du socket Hyprland

Si 10.1 impose l'écoute IPC : le format d'événements de `socket2` **n'est pas une API
stable** et bouge entre versions de Hyprland. À isoler derrière un trait dans
`infrastructure`, jamais laissé fuiter vers le domaine.

---

## 11. Tests

- **Vitest + React Testing Library** sur les composants du HUD.
- **Playwright** sur la route `/hud` (déjà configuré dans le dépôt).
- **Tests Rust** sur le parseur de transcripts : logique pure, testable sans I/O.
- **Garde-fou de performance** : le harnais de mesure du banc est versionné et
  rejoué, pour détecter une régression de CPU ou de mémoire au fil des ajouts de
  blocs.

---

## 12. Hors périmètre v1

- Capture de fenêtre active ou de titre, et toute alimentation automatique de la
  reconstruction de timesheet par les signaux du bureau.
- Bloc NOW PLAYING et spectre audio (nécessiterait `cava`, non installé).
- Bloc ARCHIVE (ticker sur les mémoires).
- Interactivité du module waybar : affichage seul.
- Toute tentative de lire le quota d'abonnement Claude par un endpoint non public.
