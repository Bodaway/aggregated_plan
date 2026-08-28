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

`windowrulev2` (la syntaxe envisagée ci-dessous à l'origine) est **deprecated sur
Hyprland 0.56.2 et n'a silencieusement aucun effet** (`hyprctl configerrors` le
confirme) : la règle ne s'applique jamais et la fenêtre atterrit sur le workspace
courant, pas sur le special workspace. Syntaxe réellement chargée par le
compositeur, retranscrite depuis `~/.config/hypr/hyprland.conf:482-499` (fichier
hors dépôt, non versionné — cette section du présent document en est la seule
trace durable) :

```
# --- aplan HUD ---------------------------------------------------------
# windowrulev2 (brief's original syntax) is deprecated on this Hyprland
# build (0.56.2) and is silently rejected -- `hyprctl configerrors` flags
# it and the rule never applies. Using the block syntax already in use
# above for pcloud-to-tray etc., which is this config's live hyprlang
# window-rule syntax.
windowrule {
    name = aplan-hud
    match:class = ^(aplan-hud)$

    float = true
    border_size = 0
    no_shadow = true
    workspace = special:aplan silent
    size = monitor_w*0.9 monitor_h*0.9
    center = true
}

# raccourci
bind = $mainMod, B, exec, ~/appfactory/aggregated_plan/scripts/aplan-hud-toggle
```

Deux autres pièges de syntaxe, vérifiés empiriquement lors de l'écriture de cette
règle : une virgule avant `silent` (`workspace special:aplan, class:...`) finit
dans le **nom du workspace** au lieu de servir de séparateur, et `size 90%` est
un no-op — il faut l'expression `monitor_w*0.9 monitor_h*0.9`. Le raccourci
utilise le **chemin absolu** du script : `exec` le résout via `$PATH`, et
`aplan-hud-toggle` n'y figure pas.

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
- **`backdrop-filter`** — mesuré (§10.2) : le surcoût CPU (bornes des trois
  essais ×1,02–×1,14) n'est pas séparable du bruit inter-essais sur ce poste,
  mais reste dans tous les cas **loin du seuil de bascule (×2)**. Reste soumis
  aux mêmes règles que les autres effets : par bloc, jamais en passe plein
  écran.

**Interdit en v1**
- *(aucun)* — `backdrop-filter` était le seul candidat de cette liste ; mesuré
  (§10.2) et déplacé dans « Autorisé ».

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

### 10.1 Visibilité sur special workspace *(bloquant)* — RÉSOLU

Le banc a établi qu'aucun toolkit natif n'est prévenu de son occultation. **Une
fenêtre WebKit sur un special workspace masqué reçoit-elle un changement de
visibilité** (`document.visibilityState`, événements de fenêtre Tauri) ?

- Si oui : couper les animations sur cet événement suffit.
- Si non : le HUD doit s'abonner au socket IPC de Hyprland
  (`$XDG_RUNTIME_DIR/hypr/$HIS/.socket2.sock`) et couper ses animations lui-même,
  sans quoi 3,8 % de CPU tournent en permanence.

**Mesuré le 2026-08-27**, `hyprctl version` : Hyprland 0.56.2 (branche v0.56.2,
commit `efb50993780079460b0cbed1363e2166a2de1d9f`).

Sonde : `scripts/hud-bench/visibility_probe.py`, hôte GTK4 + WebKitGTK 6.0
(`scripts/hud-bench/webkit_host.py`) chargeant `scripts/hud-bench/probe.html`, qui
compte les `requestAnimationFrame` par tranche de 2 s et journalise chaque
`visibilitychange`. La fenêtre est déplacée sur `special:visprobe` puis basculée
visible (8 s) → masquée (8 s) via `hyprctl dispatch togglespecialworkspace`.

Avant d'échantillonner la phase masquée, la sonde vérifie — via `hyprctl -j
clients` (le `workspace.name` de la fenêtre) et `hyprctl -j monitors` (le champ
`specialWorkspace` de chaque moniteur) — que la fenêtre est bien sur le special
workspace et qu'aucun moniteur ne l'affiche encore ; sans quoi elle abandonne au
lieu d'imprimer un verdict. Deux essais avec le dispatch initial du brief
(`movetoworkspacesilent special:visprobe`, sans sélecteur de fenêtre — qui agit
sur la fenêtre *active*) ont ainsi été abandonnés : la fenêtre est restée sur
`dev1` puis a atterri sur `witivio` au lieu du special workspace, un focus perdu
au moment du dispatch. Le correctif est un sélecteur de fenêtre explicite dans le
dispatch (`movetoworkspacesilent special:visprobe,class:^(dev.hudbench.webkit)$`),
après quoi les deux contrôles passent et la mesure est fiable.

Compteurs bruts de la mesure retenue (images par tranche de 2 s **depuis le
chargement de la page**, dans l'ordre) :
`[121, 120, 101, 80, 120, 120, 120, 120, 40, 0, 0]`, avec les événements
`visibilitychange` observés dans l'ordre `hidden, visible, hidden` — trois
transitions qui correspondent exactement aux trois dispatches Hyprland
(déplacement silencieux, bascule visible, bascule masquée).

**Correction (relecture finale) — la moyenne « phase visible » était mal
étiquetée.** Le chiffre de 106 images/2 s prenait les 4 premières tranches
(indices 0–3, `[121, 120, 101, 80]`), qui couvrent t≈0–8 s : c'est la période
*avant* le déplacement sur le special workspace (`movetoworkspacesilent`, t≈6 s,
puis bascule visible, t≈7 s), où la fenêtre rend normalement sur un workspace
ordinaire — pas la phase où le special workspace est effectivement montré. La
phase special-workspace-visible propre correspond aux tranches d'indices 4–7
(les 5ᵉ à 8ᵉ), toutes à **120** images/2 s. Le chiffre exact de la phase visible
est donc **120 images/2 s**, pas 106 — un écart plus net avec la phase masquée,
pas moins ; le verdict ci-dessous est inchangé (et le serait resté, en plus
favorable, avec le chiffre corrigé). Moyenne sur les 3 dernières tranches de la
phase masquée **vérifiée** : 13 images/2 s, dont les deux dernières tranches à
**0**.

**Verdict : WebKit s'endort.** Une fenêtre WebKitGTK sur un special workspace
masqué reçoit `visibilitychange` (état `hidden`) et cesse de produire des images
— le rendu tombe à 0 image/2 s en régime établi. **La branche « Si oui » s'applique :
la tâche 6 se contente d'écouter `document.visibilityState` côté frontend ; pas
besoin d'abonnement au socket IPC de Hyprland pour ce portillon d'animation.**

Le harnais de mesure du banc est réutilisable tel quel pour trancher.

### 10.2 Coût réel de `backdrop-filter` dans WebKitGTK — RÉSOLU

Détermine si le flou d'arrière-plan entre dans le vocabulaire visuel ou non.

**Mesuré le 2026-08-27.** Harnais : `scripts/hud-bench/measure.py`, qui lance une
commande, laisse 10 s de stabilisation, échantillonne les ticks CPU de l'arbre de
processus (`/proc/<pid>/stat`) sur une fenêtre de 20 s et relève le PSS
(`smaps_rollup`). Avant d'échantillonner, il interroge `hyprctl -j clients` sur le
pid lancé et exige `mapped && visible && !hidden` ; sans quoi il abandonne au lieu
d'imprimer un chiffre — une fenêtre qui n'est jamais montée à l'écran rendrait le
CPU mesuré sans rapport avec la variante testée. Même garde en fin de fenêtre, pour
détecter un crash pendant la mesure.

Sonde : `scripts/hud-bench/blur_probe.html`, une carte de 420×260 px en dérive
horizontale animée (4 s, va-et-vient) au-dessus d'un dégradé conique plein écran,
avec `backdrop-filter:blur(14px) saturate(1.4)` activé via `#blur` dans le
fragment d'URL — seule différence entre les deux modes. Hôte :
`scripts/hud-bench/webkit_host.py` (tâche 1) en plein écran, fenêtre bien visible
à l'écran (aucune manœuvre de special workspace ici, contrairement à 10.1).

Un seul échantillon est un maigre argument face au bruit CPU d'un poste de
travail en usage réel : trois essais ont été pris de chaque côté. La sortie
brute des six invocations (JSON multi-lignes tel qu'imprimé par
`measure.py`, capturée via `tee` au moment de l'exécution, pas retranscrite à la
main) est commitée telle quelle dans
`scripts/hud-bench/backdrop-filter-runs.txt` — c'est la pièce de référence ;
seul un résumé en est donné ici :

| essai | sans-blur `cpu_pct` | avec-blur `cpu_pct` | ratio |
|---|---|---|---|
| 1 | 17,35 % | 19,25 % | ×1,110 |
| 2 | 19,55 % | 20,0 % | ×1,023 |
| 3 | 17,05 % | 19,4 % | ×1,138 |

Ratio `avec-blur / sans-blur` par essai : ×1,110, ×1,023, ×1,138 — étalement
×1,02–×1,14, aucun ne s'approche du seuil de bascule ×2. Moyenne des `cpu_pct` :
sans-blur 17,98 %, avec-blur 19,55 %, soit un ratio des moyennes de ×1,087. Le
PSS ne bouge quasiment pas (~250 Mo dans les six essais) : la mémoire n'est pas
discriminante ici, seul le CPU l'est. Aucune fenêtre perdue ni processus mort
pendant les six mesures (contrôle `hyprctl` systématiquement positif).

**Correction (relecture finale) — la précision ×1,09 est surinterprétée.** Les
trois essais *sans-blur* s'étalent eux-mêmes sur 17,05–19,55 % (écart de 2,50
point), plus large que l'effet revendiqué (1,57 point entre les deux moyennes) :
le *sans-blur* de l'essai 2 (19,55 %) dépasse même l'*avec-blur* de l'essai 1
(19,25 %). Un ratio moyen à trois essais, sur un poste en usage réel, n'isole
donc pas de façon fiable une constante à ×1,09 — il est **indissociable du bruit
inter-essais**.

**Verdict : quel que soit le surcoût exact, il est loin du seuil de bascule
(×2)** — c'est l'affirmation qui porte la décision, et elle tient aussi bien sur
chacun des trois essais que sur leur moyenne. Règle du brief appliquée telle
quelle : `backdrop-filter` quitte la liste « interdit » et rejoint « autorisé »
(§7). Il reste soumis aux mêmes règles que les autres effets — par bloc, jamais
en passe plein écran — la mesure ci-dessus porte sur un seul bloc animé, pas sur
une passe globale.

### 10.3 Stabilité de l'`app_id` — RÉSOLU

Vérifier que Tauri v2 pose bien `aplan-hud` comme app_id Wayland. Constat annexe du
banc : **Chromium en mode `--app=` ignore `--class`** et dérive son app_id de l'URL —
piège classique dont Tauri doit être exempt pour que les windowrules tiennent.

**Vérifié empiriquement à la tâche 4** (`task-4-report.md`, étape 6) : binaire
lancé directement, `hyprctl -j clients` interrogé ~4 s après, sortie brute
filtrée sur `class`/`title` contenant `aplan` :

```
class = 'aplan-hud' | xwayland = False
```

confirmé par le bloc JSON complet de l'entrée (`"class": "aplan-hud"`,
`"initialClass": "aplan-hud"`, `"xwayland": false`). **Résultat : `aplan-hud` est
bien posé comme app_id Wayland natif et stable**, sans le piège Chromium
`--app=` évoqué ci-dessus. C'est cette valeur que matchent les windowrules de la
§6 (`match:class = ^(aplan-hud)$`), en usage réel depuis la tâche 5.

### 10.4 Format du socket Hyprland — RÉSOLU

Si 10.1 impose l'écoute IPC : le format d'événements de `socket2` **n'est pas une API
stable** et bouge entre versions de Hyprland. À isoler derrière un trait dans
`infrastructure`, jamais laissé fuiter vers le domaine.

**Sans objet.** §10.1 a conclu que WebKit s'endort de lui-même sur le special
workspace masqué (`visibilitychange` puis chute à 0 image/2 s) : le portillon
d'animation s'appuie sur `document.visibilityState` côté frontend, sans
abonnement au socket Hyprland. La question du format de `socket2` ne se pose
donc pas pour ce plan.

### 10.5 `AuthGate` laisse-t-il monter `/hud` ? *(bloquant)* — RÉSOLU (intermittent)

`AuthGate` (`main.tsx`, au-dessus de `App`) statue sur `session.authenticated`,
une valeur purement serveur : `query.rs:547-562` renvoie `true` ssi
`microsoft.refresh_token` est non vide dans la table `configuration` pour
l'utilisateur par défaut — un état global, sans cookie, indépendant de
l'origine de la requête. Pendant le chargement (`fetching`), la porte rend un
`<div>` centré `"Loading…"`. En échec, elle rend un écran de connexion
centré (« Aggregated Plan » / « Sign in with your Microsoft account to
continue. » / bouton bleu « Sign in with Microsoft ») — **le même écran**
qu'une session réellement expirée ou qu'un échec de la requête GraphQL
elle-même (`use-session.ts:21` retombe sur `authenticated:false` dès que
`result.data` est absent, quelle qu'en soit la cause).

**Un chiffre du brief était déjà rétracté.** La référence « ~22 000 pixels =
séquence de boot qui cède la place à la grille » vient de l'adjudication
*initiale* de la tâche 6 du plan 1. La revue finale de branche du même plan
(`progress.md:372-393`, « CORRECTION MAJEURE ») avait déjà retracté cette
lecture : en recadrant cette même capture, c'est le bouton « Sign in with
Microsoft » qui *apparaissait*, pas le boot qui disparaissait — confirmé par
`curl` (le backend répond `authenticated:true`, sain) contre un vrai onglet
Chrome (`TypeError: Failed to fetch`), la cause étant
`backend/crates/api/src/main.rs:204` qui fixe
`access-control-allow-origin: http://localhost:3000` quelle que soit
l'`Origin` reçue. Cette rétractation n'avait jamais été reportée dans une
spec — l'erreur a failli se reproduire ici via le brief de cette tâche.

**Mesuré le 2026-08-28.** Méthode imposée : image de différence, jamais de
seuil de couleur — et, cette fois, **le seuil de couleur s'est révélé être un
piège à deux reprises pendant la mesure elle-même** (voir plus bas), pas
seulement dans l'historique du plan 1.

Sonde : `scripts/hud-bench/authgate_probe.py`. Lance le HUD via
`scripts/aplan-hud-toggle` (le chemin réel de SUPER+B), capture le moniteur
focus toutes les ~300-500 ms depuis l'appel du toggle, diffuse les images
consécutives entre elles (jamais contre le bureau nu). Plancher de bruit
mesuré (bureau seul, 2 captures) : **0 pixel** sur les essais retenus (un
premier essai à 154 px reflétait un résidu de processus non nettoyé, voir
auto-revue). Trois lancements indépendants, chacun démarrant d'un état propre
(processus tué, workspace masqué vérifié avant relance) :

| lancement | écran de connexion apparaît | transition vers le vrai HUD | délai observé |
|---|---|---|---|
| 1 (11 s observées) | oui, dès t≈0,3-2 s | **oui** — séquence de boot réelle (`aplan cockpit v0.1.0`, checks `link`/`palette`/`session bus`) puis grille vide | ≈6,7-8,3 s |
| 2 (12,5 s observées) | oui, dès t≈0,3-2 s | **oui** — même séquence de boot, byte pour byte identique | ≈7,2-8,6 s |
| 3 (29 s observées) | oui, dès t≈0,3-2 s | **non** — reste sur l'écran de connexion jusqu'à la fin de l'observation | jamais (dans la fenêtre observée) |

Deux pièges de seuil de couleur rencontrés en mesurant, documentés pour que
la sonde ne les répète pas : (1) la couleur exacte du bouton varie entre
`#2563eb` (repos) et une teinte voisine (`:hover`, selon la position
accidentelle du curseur physique) — un seuil calé sur une seule des deux
valeurs donne des faux négatifs ; (2) un seuil large mais mal calé confond le
bleu du bouton avec le bleu ambiant des liens hypertexte du terminal visible
par transparence (ce bureau n'est pas au repos comme celui du plan 1 — des
sessions Claude Code y tournent en direct). Les deux ont été corrigés en
calibrant la couleur réellement rendue (histogramme de fréquence sur l'image
elle-même) avant de compter, puis **vérifiés par inspection visuelle directe
de chaque image retenue** — pas seulement par le chiffre — après qu'une
première lecture (image par image, sans recalibrer) a produit une fausse
lecture (« l'image 18 montre encore l'écran de connexion ») contredite par la
suite par le comptage de pixels puis par une relecture visuelle correcte de
la même image. Cet aller-retour est resté dans les brouillons de mesure, pas
dans ce paragraphe.

**Verdict : comportement intermittent, ni « passe de façon fiable » ni
« bloque de façon fiable ».** Taux observé : 2/3 lancements atteignent le
vrai HUD (boot puis grille vide), mais seulement après un délai de 6,7-8,6 s
inexpliqué ; 1/3 reste bloqué sur l'écran de connexion sur toute la fenêtre
observée (29 s). Le mécanisme du délai n'a pas pu être établi : aucun
timer/`setInterval` dans `AuthGate.tsx`/`use-session.ts`/`urql-client.ts` ne
l'explique, `journalctl --user -u aplan-api` ne journalise pas les requêtes
individuelles au niveau de log actuel (piste non poursuivie plutôt que de
redémarrer le service en production pour l'instrumenter), et aucun outil de
devtools n'est disponible dans ce build release (même limite que la tâche 6
du plan 1). Ce qui reste établi avec certitude : le CORS reste cassé
(`curl -H "Origin: tauri://localhost"` contre le backend réel, aujourd'hui,
renvoie toujours `access-control-allow-origin: http://localhost:3000`) et le
compte est authentifié côté serveur (`microsoft.refresh_token` non vide,
`curl` sans CORS obtient `authenticated:true`) — mais ces deux faits, à eux
seuls, prédisent un blocage *déterministe*, pas le comportement intermittent
observé. Une variable non identifiée fait la différence entre les 2 lancements
qui finissent par réussir et celui qui reste bloqué.

**Conséquence pour la suite du plan 3 : ni la branche « procéder tel quel »
ni la branche « s'arrêter, remonter un choix CORS-vs-porte à trancher » ne
s'applique proprement.** Un correctif visant le CORS n'est pas justifié par
la donnée actuelle (le blocage n'est pas systématique), et « procéder tel
quel » ferait construire six blocs sur une porte qui, un lancement sur trois
dans cet échantillon, ne s'ouvre jamais. Remonté au contrôleur tel quel,
conformément à la règle du brief sur les changements d'authentification.

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
