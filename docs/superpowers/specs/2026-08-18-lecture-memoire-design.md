# Lecture de la mémoire par une session Claude Code — design

**Date** : 2026-08-18
**Statut** : validé en session de conception, prêt pour un plan d'implémentation
**Tâche aplan** : `35d79540` — Design : couche mémoire agent basée sur aplan

---

## 1. Le problème

La couche mémoire d'aplan écrit tous les jours et n'est jamais lue.

L'écriture tourne : le timer `aplan-consolidate.timer` lance chaque jour à 17h30 une session
Claude Code non interactive qui parcourt le worklog, dédoublonne, et écrit des mémoires. 502 des
572 entrées de worklog portent un `consolidated_at` (mesuré le 2026-08-18).

La lecture n'existe pas. Sur 148 invocations réelles des verbes mémoire mesurées dans les
transcripts (23 sessions, 353 fichiers), **138 se produisent dans le dépôt `aggregated_plan`
lui-même** — c'est-à-dire dans le dépôt où la fonctionnalité a été développée. Runs planifiés
retirés, il ne reste que du développement et du test. **Aucune session n'a jamais consulté la
mémoire pour éclairer un travail sans rapport.**

## 2. État vérifié le 2026-08-18

Trois constats de l'analyse initiale sont **périmés** et ne doivent pas guider la conception.

**La file de validation est vide.** 0 `pending`, 43 mémoires `claude_session` `active`, 1
`rejected`, 6 `manual` actives. Un recall atteint 48 mémoires sur 50, contre 9 sur 50 au moment du
constat initial. La cause structurelle « la file affame le recall » n'existe plus. Elle **se
reformera** : la consolidation écrit en `pending` par défaut, donc les mémoires du jour sont
invisibles au recall le lendemain (voir § 6).

**Le contenu a changé de nature.** Les mémoires actives ne parlent plus d'aplan : Pernod Ricard
(gates ARB/cyber/DPO, item ServiceNow), TotalEnergies (ruleset 848841, exemptions WAF), SAFT
(fenêtre de maintenance, `retry=0` sur ADF, pagination `OFFSET/LIMIT` bugguée), eActions (WAF
`azrpwafeact01` → 403), SharePoint sur Linux, micro/EasyEffects. Plus 3 mémoires `preference` qui
sont des règles de méthode.

**Deux mécanismes de ciblage sont inertes.** Les 49 mémoires actives ont `project_id NULL` et
`memory_stakeholders` est **vide** (0 ligne — la table modélise des *personnes*, « towards whom »,
et `aplan remember` n'a pas de drapeau pour l'écrire). Une seule ligne existe dans `projects` :
SAFT. Toute conception fondée sur « recall filtré par le projet ou l'entité de la session » ne
trouverait rien.

### Ce qui existe déjà et n'a jamais été câblé

`BriefVariant::Session` existe dans `domain/rules/brief.rs`, documenté « The SessionStart injection
(§ 7.2): everything, with drill-down hints ». C'est la variante que rend déjà `aplan brief`. La spec
technique décrit la commande comme « destinée au hook `SessionStart`, où elle **s'ajoute** à la
liste des tâches suivies sans la remplacer (R56) », et R55 justifie son plafond de 40 lignes ×
140 caractères par « cette sortie entre dans le contexte du modèle à chaque session ».

**Le processus 1 est donc spécifié, budgété, à moitié construit — et jamais branché.** Le hook
`~/.claude/hooks/aplan-session-start.sh` (220 lignes) n'injecte que le rattachement de tâche.

### Coûts mesurés

| Charge | Octets | ≈ tokens |
|---|---|---|
| `brief` (variante session, sortie réelle) | 1 368 | ~350 |
| `brief --json` | 3 555 | ~900 |
| 3 titres de `preference` | ~200 | ~50 |
| 3 `preference` avec leur corps `--why` | 2 175 | ~550 |
| `recall --q` à 2 résultats | 447 | ~110 |
| 10 titres de mémoire les plus récents | 1 260 | ~315 |
| les 48 titres actifs | 5 675 | ~1 400 |

### Volumes cherchables

642 tâches, 572 entrées de worklog, 26 réunions, 8 alertes, 50 mémoires. Un balayage linéaire sur
l'ensemble est instantané ; aucun index FTS supplémentaire n'est nécessaire.

## 3. Décisions arrêtées

| # | Décision | Motif |
|---|---|---|
| D1 | **Deux processus**, pas un déclencheur unique : une poussée au démarrage, une traction pendant la session | Aucune option isolée n'était acceptable |
| D2 | Charge du démarrage : **titres des `preference` + brief**, ≈ 400 tokens | Le corps `--why` doublerait le coût ; `aplan recall <id>` le sert à la demande |
| D3 | Processus 2 = **une skill** adossée au CLI, pas un outil MCP | Voir § 5.1 |
| D4 | La skill cherche **partout dans aplan**, pas seulement dans les mémoires | Demande explicite |
| D5 | Construire **`aplan search --q` d'abord**, la skill par-dessus | Évite de coupler la skill au schéma SQLite |
| D6 | La consolidation écrit les `fact` en **`--confirm`** ; la file ne garde que `decision` et `commitment` | Le garde-fou a rejeté 1 mémoire sur 44 (2 %) au prix de semaines d'invisibilité |
| D7 | Devant une contradiction, la session **signale et propose** (ignorer pour cette fois / remplacer) ; elle n'invalide jamais d'elle-même | L'écriture reste un acte validé par l'humain |

## 4. Processus 1 — l'injection au démarrage

### 4.1 Section `preferences` dans le brief

Les `preference` entrent dans le **brief**, pas dans le hook. Les mettre dans le hook créerait un
second rendu hors du domaine, donc un second endroit où le plafond de R55 peut être contourné.

- `domain/rules/brief.rs` : une section `preferences: BriefSection<MemoryEntry>` dans la structure
  `Brief`, rendue **en premier** — une règle de méthode prime sur une échéance.
- Sélection : les mémoires `kind = preference` actives, les plus récentes d'abord, filtrées par R45
  comme les autres sections.
- Titres seuls, jamais le corps. Chaque ligne porte sa référence courte réutilisable (`[m:xxx]`),
  comme l'exige déjà R56 — c'est ce qui permet `aplan recall m:xxx` pour obtenir le pourquoi.
- L'ordre de sacrifice de R55 est étendu **en fin de chaîne** : les décisions cèdent avant les
  engagements, qui cèdent avant les échéances, qui cèdent avant les préférences. Les préférences
  sont à la fois les plus utiles et les moins chères (3 lignes, ~50 tokens) : elles sont donc les
  dernières coupées, ce qui est cohérent avec leur rendu en tête.
- R56 est amendée pour décrire la nouvelle composition.

### 4.2 Câblage du hook

Le hook construit déjà une chaîne `$context` et l'émet via
`jq -nc '{hookSpecificOutput:{hookEventName:"SessionStart", additionalContext:$ctx}}'`. La sortie de
`aplan brief` s'y ajoute.

**Meilleur effort, impérativement.** Si l'API sur :3001 est éteinte, `aplan brief` échoue — et un
hook `SessionStart` qui échoue coûte le rattachement de tâche, c'est-à-dire tout le dispositif de
suivi. Échec silencieux : le contexte part sans sa strate mémoire. Précédent explicite : `show` et
`journal`, qui n'échouent pas quand leur strate optionnelle manque.

Le pied de page du brief (`Détail : aplan recall m:xxx · Recherche : aplan recall --q "…"`) devient
la porte d'entrée du processus 2 ; il est étendu pour mentionner `aplan search`.

## 5. Processus 2 — la traction

### 5.1 Pourquoi une skill et pas un outil MCP

`crates/mcp` **n'a jamais compilé**. Le commentaire du `Cargo.toml` du workspace est explicite : la
crate est entrée en WIP (`0b0c559`), `AggregatedPlanServer` n'implémente ni `ServerHandler` ni
`Service<RoleServer>`, la macro rejette son champ `tool_box`, et aucune version 1.x de rmcp ne la
compile (1.0.0, 1.1.0, 1.2.0 → les mêmes neuf erreurs). Elle est exclue du workspace. Ses 914 lignes
exposent des outils de **tâches** écrits avant la couche mémoire : il n'y a pas une ligne de mémoire
dedans.

Mais l'argument décisif n'est pas le coût, c'est la **nature de la panne**. Le CLI était appelable
depuis n'importe où depuis le début, et il ne l'a jamais été hors du dépôt aplan. La panne n'est pas
une panne de capacité, c'est une panne de **saillance**. Or un outil MCP n'est appelé que si le
modèle juge la situation pertinente — exactement comme une skill n'est chargée que si sa description
matche. Payer cher le processus 2 n'achète donc pas la fiabilité recherchée : celle-ci vient du
processus 1.

MCP reste le seul chemin si la mémoire doit être lue depuis autre chose que Claude Code. La skill
peut plus tard devenir une enveloppe fine au-dessus d'outils MCP, sans rien jeter.

### 5.2 `aplan search --q` — la recherche transverse

**Le trou comblé** : seul le magasin de mémoires est cherchable en texte aujourd'hui. `ls` ne filtre
que par `status` et `triage`, `show` exige de connaître déjà la tâche, `journal` ne prend qu'une
date, `alerts` qu'un état.

**Périmètre** : tâches (titre, description), entrées de worklog (contenu), réunions (titre),
mémoires. Les alertes sont hors périmètre — 8 lignes, déjà servies par `aplan alerts`.

**Pas de classement unique.** Mélanger un score BM25 de mémoire avec une correspondance de titre de
tâche produit un ordre qui ne veut rien dire. La sortie est **groupée par entité**, chaque groupe
avec son propre ordre : mémoires par pertinence (le scoring `recall` existant, réutilisé tel quel),
tâches et worklog par récence. Le lecteur est un agent : il sait lire quatre petites listes, il ne
sait pas se méfier d'un classement bancal.

**Le pliage des accents vit dans le domaine.** `memories_fts` plie les diacritiques
(`tokenize = 'unicode61 remove_diacritics 2'` — « memoire » et « mémoire » ramènent les mêmes
lignes), alors qu'un `LIKE` SQLite ne les plie pas ; une recherche mi-FTS mi-`LIKE` se comporterait
différemment selon l'entité visée. Plutôt qu'ajouter un index FTS sur les tâches et le worklog — ce
qui obligerait à toucher tous les chemins d'écriture, comme le fait `memories_fts` —,
l'infrastructure charge les lignes candidates et le domaine filtre :

- `domain/src/rules/search.rs` : `normalize(&str) -> String` (minuscules + pliage des diacritiques)
  et `matches(haystack, terms) -> bool`. Fonctions pures, testables sans I/O, même sémantique que
  FTS5 côté mémoire.

**Sortie plafonnée par défaut** : 5 résultats par groupe, `--limit` pour élargir, `--json` comme
partout ailleurs. C'est un agent qui appelle ; une commande qui crache 642 tâches ne sera plus
jamais appelée.

**Écueil à ne pas reproduire** : la requête GraphQL `tasks` sort en `first:50` DESC alors que la
base compte 642 tâches. `search` n'en dépend pas — elle interroge le dépôt directement, sans plafond
caché, et n'applique que le sien.

**Chemin d'implémentation**, calqué sur `recall` de bout en bout :

| Couche | Fichier |
|---|---|
| Domaine | `crates/domain/src/rules/search.rs` (+ `rules/mod.rs`) |
| Application | `crates/application/src/repositories/` (trait), `use_cases/` |
| Infrastructure | `crates/infrastructure/src/database/` (requêtes) |
| API | `crates/api/src/graphql/query.rs`, `graphql/types/` |
| CLI | `crates/cli/src/cli.rs`, `queries.rs`, `search_cmd.rs`, `main.rs` |

### 5.3 La skill

Une skill **distincte** de la skill `aplan` existante : les déclencheurs n'ont rien à voir. `aplan`
se déclenche sur des verbes d'écriture (« logue ça », « passe la tâche en cours »). Celle-ci se
déclenche sur des signaux de **récupération** :

- un nom de client ou de système déjà rencontré (Pernod Ricard, TotalEnergies, SAFT, Cartier,
  eActions, SharePoint, Gryzzly) ;
- les formulations « est-ce qu'on avait déjà… », « comment on avait fait pour… », « qu'est-ce que je
  sais de… », « on en était où sur… ».

Ce qu'elle porte et qu'un outil ne pourrait pas porter, c'est une **procédure** :

1. chercher **avant** de répondre quand le sujet touche un client ou un système connu, pas après ;
2. lire chaque résultat comme un indice **daté**, pas comme une vérité ;
3. **protocole de contradiction (D7)** : quand ce que la session observe contredit une mémoire, elle
   signale la mémoire concernée (avec sa référence courte et sa date) et propose deux issues —
   « on ignore pour cette fois » ou « on remplace » — puis attend. Elle n'invalide, ne supersède et
   n'écrit jamais d'elle-même.

## 6. Ajustement de la consolidation

`docs/prompts/consolidation-memoire.md` passe les `fact` en `--confirm` ; la file ne garde que
`decision` et `commitment`.

Motif chiffré : sur les 44 mémoires écrites par Claude, 43 ont été validées et 1 rejetée. La file
attrape 2 %, au prix de plusieurs semaines d'invisibilité pour 40 entrées. Et `pending` n'est pas
« périmé » : une mémoire périmée était vraie et ne l'est plus, ce que couvre `invalidated_at` ; une
mémoire `pending` est une extraction non relue. Le principe « recaller une décision périmée est le
pire échec » ne condamne donc pas mécaniquement le `pending` — mais le garde-fou est conservé là où
l'enjeu est un engagement, c'est-à-dire sur `decision` et `commitment`.

Le fichier vit hors du binaire précisément pour être itérable sans recompiler.

## 7. Mesure et critère de succès

**C'est le vrai livrable.** La ligne de base est nette : 148 invocations des verbes mémoire, 138
dans le dépôt aplan, **zéro** lecture métier hors dépôt.

Le critère de succès n'est pas « la skill existe », c'est : **le compteur d'invocations hors dépôt
`aggregated_plan` décolle sous quinze jours.** S'il reste à zéro, la skill a échoué et MCP redevient
la question — pour le prix d'un fichier markdown, pas d'une crate réanimée.

L'instrument est le script de comptage par projet (`docs/prompts/reprise-lecture-memoire.md`, § « Instruments de mesure »), à relancer à l'identique.

## 8. Tests

- `domain/rules/search.rs` : normalisation, pliage des accents, correspondance multi-termes,
  résultat vide. Sans I/O, comme `recall.rs`.
- Section `preferences` du brief : présence, ordre (préférences en premier), **plafond de R55 sur
  une entrée pathologique** (40 lignes, 140 caractères), et ordre de sacrifice de la troncature.
- `search` en infrastructure : SQLite en mémoire, jeu de lignes couvrant les quatre entités,
  vérification qu'aucun plafond caché ne s'applique.
- Le hook et la skill ne se testent pas unitairement — ils se **mesurent** (§ 7).

## 9. Hors périmètre

- Réanimer `crates/mcp` : reconsidéré seulement si la mesure du § 7 échoue.
- Rendre les `pending` lisibles par le recall (option écartée au profit de D6).
- Rattacher les mémoires à des projets ou à des entités : inerte aujourd'hui (§ 2), et sans valeur
  tant que `projects` ne compte qu'une ligne.
- Un index FTS sur les tâches et le worklog : inutile aux volumes actuels (§ 2).

## 10. Frictions connues, corrigibles au passage

Relevées lors de l'analyse initiale, indépendantes du présent design :

- `recall --project` n'est **pas** un filtre mais un bonus d'entité dans le score (1.309 → 1.609
  sur la mémoire rattachée) : `RecallQuery` ne porte aucun champ projet, seul `RecallContext`.
  L'aide CLI dit « Restrict the search context to a project », ce qui se lit comme un filtre.
- Un `--project` introuvable renvoie `error: no task matches <token>` — le mot « task » alors que la
  résolution porte sur les projets (`crates/cli/src/memory_cmd.rs:58`).
- La charge JSON de `recall --q` n'expose pas `supersededBy` (9 champs) là où `recall <id>` en
  expose 16 : une session voit `invalidatedAt` sans savoir par quoi la mémoire a été remplacée.

## 11. Garde-fous à respecter pendant l'implémentation

- **Ne jamais déplacer le pointeur `aplan.active_task_id`** : c'est l'humain travaillant à la main.
  Une session ne touche que sa propre ligne (`aplan session bind`).
- **Ne pas lancer `aplan consolidate mark` / `record-run`** hors d'un passage de consolidation :
  marquer une entrée la rend invisible au passage suivant, sans retour.
- Le magasin contient de **vraies** données. Toute sonde d'écriture doit être nettoyée ; le tri des
  candidats existants est une décision de l'utilisateur.
- Toute modification du comportement documenté met à jour `SPEC_FONCTIONNELLE.md` et
  `SPEC_TECHNIQUE.md` dans le même commit (R55, R56 sont directement concernées).

## 12. Ordre de construction

Les cinq chantiers sont indépendamment livrables, mais leur ordre n'est pas libre : D5 impose que
`aplan search` précède la skill, faute de quoi la skill devrait se coupler au schéma SQLite.

| Ordre | Chantier | Dépend de |
|---|---|---|
| 1 | Section `preferences` du brief (§ 4.1) + amendement R55/R56 | — |
| 2 | Câblage du hook `SessionStart` (§ 4.2) | 1 |
| 3 | `docs/prompts/consolidation-memoire.md` : `fact` en `--confirm` (§ 6) | — |
| 4 | `aplan search --q` (§ 5.2) | — |
| 5 | La skill (§ 5.3) | 4 |
| 6 | Relance de la mesure à J+15 (§ 7) | 2, 5 |

Les chantiers 1-2 et 3 livrent seuls de la valeur : ils suffisent à ce qu'une session voie enfin
quelque chose de la mémoire. Les chantiers 4-5 ajoutent la traction.
