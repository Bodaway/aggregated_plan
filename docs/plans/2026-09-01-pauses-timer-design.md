# Pauses : prise de pause et décompte visuel — design

**Date** : 2026-09-01
**Statut** : validé, prêt pour le plan d'implémentation

## Le problème

La routine de pauses (migrations 019/020, `domain/src/rules/breaks.rs`) sait décider
*quand* une pause est due et la notifier. Elle ne sait rien faire de la pause elle-même.

Concrètement, aujourd'hui : `run_break_tick` construit une `Notification` à trois
actions — **Pris / Plus tard / Passer** — et `notify-send --wait` bloque jusqu'à la
réponse. Presser « Pris » écrit `outcome = taken` et rend la main. C'est tout.
`BreakRule::duration_seconds` — 30 s pour la pause visuelle, 5 min pour la pause
franche — ne sert qu'à calculer l'expiration de la notification. Rien ne dure, rien ne
se compte, rien ne s'affiche.

Deux conséquences. La première : la durée prescrite est décorative, et une « pause
franche de cinq minutes » se solde en pratique par trois secondes et un clic. La
seconde : `taken` ment. Il enregistre une *intention* au moment du clic, pas une pause
prise. Le taux d'adhérence mesure donc la vitesse à laquelle on fait taire une popup.

## Ce qu'on construit

Presser le bouton **ouvre une pause** : le décompte démarre, l'overlay HUD apparaît
avec un anneau qui se vide et le temps restant, et se referme à la fin. `taken` n'est
écrit qu'au bout du décompte ; couper avant écrit `abandoned`.

## Décisions de cadrage

| Question | Décision | Pourquoi |
|---|---|---|
| Où vit le visuel | L'overlay HUD Tauri existant | La surface est là, animée, avec sa palette. Rien à inventer. |
| Qui possède l'horloge | Le backend | Une seule horloge, et la pause se clôt correctement même si le HUD n'a jamais réussi à s'ouvrir. |
| Quand la pause compte | À la fin du décompte | `taken` doit vouloir dire « prise ». Une coupure devient `abandoned`. |
| Contrôles à l'écran | Un seul — « J'y retourne » | Pas de rallonge : moins de cas à tenir, et prolonger une pause n'est le problème de personne. |
| Ouverture de l'overlay | Lancé/montré à la demande | Coût au repos nul, comme aujourd'hui. ~1 s de WebKit au premier appel de la journée, accepté. |

### Ce qui a été écarté

**Le HUD possède le timer** (le backend note le départ, le front décompte et appelle
`endBreak` à zéro). Moins de machinerie côté serveur — jusqu'au premier HUD fermé en
cours de route : la ligne reste `pending` pour toujours et l'adhérence ment. Et le
backend doit de toute façon savoir clore seul, donc il finit par redupliquer la durée.

**Session en mémoire seulement** (`watch` channel + souscription SSE, aucune colonne
ajoutée). Un redémarrage de l'API perd la pause en vol. C'est exactement ce que la
migration 019 dit avoir voulu éviter : « ce qui fait qu'un report survit à un
redémarrage de l'API ».

## Architecture

### Données — migration `021_break_sessions.sql`

Deux colonnes sur `break_events` :

- `started_at TEXT` — l'instant où l'utilisateur a pressé « Prendre la pause ».
- `ends_at TEXT` — `started_at + rule.duration_seconds`, **figé à l'ouverture**.

C'est `ends_at`, et non un compteur, qui définit la fin : le backend et le HUD lisent
la même échéance absolue, donc aucun des deux ne peut dériver par rapport à l'autre.
Figer plutôt que recalculer depuis la règle est délibéré — modifier `duration_seconds`
dans l'écran de réglages pendant une pause ne doit pas rallonger la pause en cours.

Plus l'outcome `abandoned` dans le CHECK. SQLite ne sait pas élargir une contrainte en
place : reconstruction de table en 12 étapes (créer la nouvelle avec le CHECK élargi et
les deux colonnes, copier, dropper, renommer, recréer `idx_break_events_rule_due` et
`idx_break_events_outcome`). `break_events` n'a aucune FK entrante — seule sa propre FK
vers `break_rules(id) ON DELETE CASCADE` est à reconduire à l'identique.

### Domaine

`BreakOutcome::Abandoned`, qui **compte dans l'adhérence** : la notification a été vue,
l'utilisateur a répondu, la pause n'est pas allée au bout. C'est un échec mesuré, pas
du bruit d'ordonnancement — contrairement à `absorbed` et `expired`, exclus des deux
côtés.

`BreakEvent` gagne `started_at: Option<DateTime<Utc>>` et `ends_at: Option<DateTime<Utc>>`.

**Un amendement à `decide`.** Son étape 1 expire *tout* ce qui est ouvert dès que
l'instant sort des fenêtres de travail. Une pause franche démarrée à 16 h 58 se ferait
donc annuler à 17 h 00, en plein décompte. Nouvelle règle : une ligne dont la session
court encore — `started_at` posé et `ends_at` dans le futur — échappe au balayage de fin
de journée. Son propriétaire la clôra.

Le reste de `decide` n'a pas besoin d'être touché : son étape 2 passe déjà sur les
lignes `pending` sans `deferred_until` (« Fired and unanswered: the notifier owns its
fate, not the tick »), ce qu'est exactement une pause en cours.

### Application — le déroulé

Le tick qui a fait sonner la notification **garde la main du début à la fin**, comme il
le fait déjà pour l'attente de `notify-send --wait`. Aucune régression de cadence : le
tick suivant démarre en retard, et l'ancrage horloge de `decide` fait qu'un tick tardif
ne perd aucune échéance. Et aucune pause ne peut sonner pendant une pause, ce qui est
la bonne propriété.

1. La notification sonne. L'action `taken` s'intitule désormais **« Prendre la pause »** ;
   la clé d'action ne change pas.
2. Pression → écriture de `started_at` / `ends_at`, l'outcome **reste `pending`**, et la
   surface est montrée.

   > **Amendement (implémentation).** `started_at` est l'instant de la **pression du
   > bouton**, relu sur l'horloge à ce moment-là, et non le `now` du tick. Le tick a
   > commencé avant que la notification ne parte, et `notify-send --action` implique
   > `--wait` : cet instant peut avoir des minutes quand le bouton est pressé. S'y ancrer
   > servirait une pause raccourcie d'exactement l'hésitation de l'utilisateur — et, sur
   > une pause visuelle de 30 s répondue tardivement, une pause déjà terminée à l'instant
   > où elle s'ouvre.
3. Attente de l'échéance, en relisant la ligne chaque seconde. Deux issues :
   - `ends_at` atteint → `taken`, `responded_at = now`, surface cachée ;
   - la ligne est passée à `abandoned` par la mutation GraphQL → l'attente cesse,
     surface cachée.
4. `Plus tard`, `Passer`, notification ignorée ou non délivrée : strictement inchangés.

Une notification « Pause terminée » d'urgence basse accompagne la fermeture. L'overlay
qui disparaît est un signal muet, et le principe d'une pause est qu'on ne regarde pas
l'écran.

**Le trait `SurfaceController`** (`application/services`), deux méthodes `show()` /
`hide()`, avec une implémentation nulle quand il n'y a pas de session graphique. C'est
le montage `Notifier` / `NullNotifier` déjà en place, pour la même raison : la couche
application ne lance pas de processus.

**Reprise après redémarrage.** L'API tombe pendant une pause : la ligne reste `pending`
avec `started_at` posé. Le tick suivant ne clôt **que les sessions dont l'échéance est
déjà passée**, en `taken` — le décompte était lancé et il est allé à son terme tout seul ;
l'appeler autrement serait inventer un abandon que rien n'atteste. Une session encore
vivante est **laissée à son propriétaire** et repassera ici à un tick ultérieur. Reste un
seul cas incohérent, la ligne au `ends_at` NULL que `start_session` ne produit jamais :
elle ne peut être ni attendue ni datée, elle est close en `abandoned`.

> **Amendement (implémentation).** Ce paragraphe annonçait `abandoned` pour *toute*
> session orpheline non échue, sur la prémisse « le scheduler est une boucle unique, donc
> si un tick la voit, c'est qu'aucun tick ne l'attend ». La prémisse vaut par *processus*,
> pas par *base* : un `cargo run -p api` de développement à côté du service installé fait
> deux processus sur un seul fichier — ce qui arrive sur cette machine —, et le premier
> tick du second abandonnait une pause parfaitement saine en plein décompte, overlay
> retiré de l'écran. Deux nuisances disparaissent avec : un recul d'horloge ne fabrique
> plus de faux abandon, et un redémarrage en pleine pause ne salit plus le taux
> d'adhérence, `abandoned` comptant contre lui. Attendre coûte au plus un tick.

### Infrastructure

`SqliteBreakEventRepository` gagne :

- la lecture/écriture des deux colonnes dans `map_event`, `create` et les `UPDATE` ;
- `find_active(user_id)` — la ligne `pending` avec `started_at` posé, s'il y en a une ;
- `start_session(id, started_at, ends_at)` ;
- `abandon_if_running(user_id, id, responded_at)` — le compare-and-swap qui sert
  `endBreak`, et qui renvoie s'il est bien celui qui a clos la ligne.

> **Amendement (implémentation).** Cette troisième méthode manquait au design, et elle est
> portée par `user_id` **en plus** de l'id d'événement : la comparaison-échange porte sur
> `id AND user_id AND outcome = 'pending' AND started_at IS NOT NULL`, de sorte qu'un
> identifiant d'événement seul ne suffise jamais à clore la pause d'autrui. C'est aussi ce
> qui met toute la décision *dans l'écriture* — lire la ligne puis la mettre à jour serait
> un check-then-act, et l'écriture du tick, arrivant entre les deux, serait écrasée.

`HudToggleSurface` implémente `SurfaceController` en appelant `aplan-hud-toggle show`
et `... hide`. Le nom du programme est résolu sur le `PATH`, surchargeable par
`APLAN_HUD_TOGGLE`.

### Le script `aplan-hud-toggle`

Deux sous-commandes **`show`** et **`hide`**, idempotentes : elles lisent l'état réel du
compositeur (`hyprctl monitors`, déjà fait par le script) et ne dispatchent que si
l'état diffère. L'appel sans argument — le raccourci clavier — ne change pas.

**Le repli de signature.** `HYPRLAND_INSTANCE_SIGNATURE` est bien dans l'environnement
systemd utilisateur (uwsm le finalise), mais l'API est un service au long cours et la
signature change à chaque redémarrage de Hyprland : le service garderait l'ancienne,
`hyprctl` échouerait, et l'écran de pause cesserait d'apparaître sans un mot dans les
logs. Le script re-dérive donc : si `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE`
n'existe pas, il prend l'instance la plus récente du dossier.

### API GraphQL

```graphql
type ActiveBreak {
  eventId: ID!
  kind: BreakKindGql!
  label: String!
  body: String!
  startedAt: DateTime!
  endsAt: DateTime!
}

extend type Query    { activeBreak: ActiveBreak }
extend type Mutation { endBreak(eventId: ID!): Boolean! }
```

`endBreak` est **idempotente** : si la ligne n'est plus `pending`, elle renvoie `false`
sans rien toucher. Le décompte peut s'être terminé dans la seconde où le bouton était
pressé, et l'écriture du tick doit gagner. Le test n'est pas fait ici mais dans le `WHERE`
de `abandon_if_running` (voir l'amendement en *Infrastructure*), qui exige aussi le
`user_id`.

### Frontend

`HudPage` rend `<BreakScreen>` **à la place** de la grille quand une pause court, et
**saute la séquence de boot** dans ce cas : 1,5 s de rideau devant un décompte de 30 s
n'a aucun sens.

`BreakScreen` : le libellé de la règle, son `body` (« Regarde au loin 20 s, relâche les
épaules »), un anneau SVG en `stroke-dashoffset` qui se vide, le restant en `m:ss`, et
le bouton « J'y retourne ». Le restant se calcule **depuis `endsAt` et l'horloge, jamais
par décrémentation** — un compteur décrémenté dérive dès que le webview est ralenti.
Rendu dans les tokens de `hud.css` : on suit le langage visuel existant, on n'en invente
pas un second.

`useActiveBreak` interroge toutes les 2 s et **s'éteint quand la surface est cachée** —
le HUD ne doit pas sonder l'API toute la journée derrière un workspace masqué. C'est le
backend qui montre l'overlay, donc l'événement `surface-visibility` arrive en premier et
le poll démarre par un refetch immédiat.

## Tests

TDD, rouge → vert, à chaque couche.

**Domaine**
- `abandoned` fait l'aller-retour par sa chaîne de stockage et compte dans l'adhérence.
- Une session en cours (`started_at` posé, `ends_at` futur) survit au balayage de
  `decide` hors fenêtre de travail ; une ligne ouverte sans session est toujours
  expirée.

**Application**
- La pression sur `taken` ouvre la session : `started_at` / `ends_at` écrits, outcome
  toujours `pending`, surface montrée.
- L'échéance atteinte écrit `taken` avec `responded_at`, et cache la surface.
- Un `abandoned` posé pendant l'attente l'interrompt et cache la surface.
- Une session orpheline **échue** (`started_at` posé, `ends_at` passé) est close en
  `taken` par le tick suivant ; une session orpheline **encore vivante** est laissée
  intacte ; une ligne au `ends_at` NULL est close en `abandoned` (voir l'amendement).
- `Plus tard` / `Passer` n'ouvrent aucune session.

**Infrastructure**
- Aller-retour des deux colonnes ; `find_active` ne renvoie que la ligne en session.
- `aplan-hud-toggle.test.sh` : `show` sur un workspace déjà visible ne dispatche rien,
  `hide` sur un workspace déjà caché non plus, et le repli de signature choisit
  l'instance présente dans `$XDG_RUNTIME_DIR/hypr`.

**API**
- `activeBreak` renvoie `null` hors pause et la ligne en cours pendant.
- `endBreak` écrit `abandoned` ; un second appel renvoie `false` sans réécrire.

**Frontend**
- `BreakScreen` calcule le restant depuis `endsAt` avec une horloge figée.
- Le bouton appelle la mutation avec le bon `eventId`.
- `HudPage` bascule sur l'écran de pause et saute le boot.
- Le poll s'éteint quand la surface disparaît.

## Documentation

Mise à jour dans le même commit, en français :

- `SPEC_FONCTIONNELLE.md` — la prise de pause, le décompte, et ce que `taken` /
  `abandoned` veulent désormais dire.
- `SPEC_TECHNIQUE.md` — migration 021, `SurfaceController`, `activeBreak` / `endBreak`,
  les sous-commandes du script.
- `CLAUDE.md` — le paragraphe *Break routine*.
