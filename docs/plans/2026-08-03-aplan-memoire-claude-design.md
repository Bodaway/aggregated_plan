# Design — aplan comme couche mémoire pour Claude

**Date** : 2026-08-03
**Statut** : v2 — révisé après revue adversariale, prêt à découper en plans d'implémentation
**Objectif utilisateur** : faire d'aplan la base d'un « Claude secrétaire » — capable de rappeler
ce qui doit être fait, et de rappeler les décisions prises et leur contexte.

> **v2** : la v1 a été soumise à une revue adversariale dont six affirmations empiriques ont été
> vérifiées et confirmées contre le dépôt et contre une session `sqlite3` FTS5. Trois d'entre elles
> invalidaient des choix de la v1. Le détail des constats retenus et écartés est en **§ 11**.

---

## 1. Contexte

aplan dispose déjà de la majeure partie d'un système de mémoire sans que ce soit nommé ainsi :

- **stockage épisodique** : `worklog_entries` horodatées, alimentées de façon atomique et
  incrémentale pendant les sessions Claude (une entrée par finding / décision / action) ;
- **état** : `tasks` (statut, échéance, urgence, impact, alertes), `activity_slots`, `meetings` ;
- **entités relationnelles** : `projects`, `signal_project_mappings`, clés Jira, `task_links` ;
- **surfaces d'accès** : CLI `aplan --json`, GraphQL, skill `aplan`, hook SessionStart.

Ce qui manque : la mémoire **sémantique** (décisions, faits, engagements, préférences), et une
**surface de récupération** exploitable par Claude.

---

## 2. État de l'art (août 2026)

### 2.1 Chiffres retenus

| Source | Résultat |
|---|---|
| mem0, rapport 2026 | LoCoMo **92,5** à 6 956 tokens/requête ; LongMemEval **94,4** ; BEAM **64,1** à 1 M de tokens → **48,6** à 10 M (−25 %) |
| mem0 vs contexte complet | ≈ 6 900 tokens/récupération contre ≈ 26 000 → **3,7×** moins |
| mem0, gains par catégorie | raisonnement temporel **+29,6 pts**, multi-hop **+23,1 pts** |
| Zep / Graphiti | LongMemEval **63,8 %** contre 49,0 % (mem0, GPT-4o) ; DMR **94,8** contre 93,4 (MemGPT) |
| BrainDB (local, SQLite) | 4 300+ souvenirs, latence **< 1 ms** (BLOBs 384-dim + BM25, fusion RRF) |
| Harvey (Dreaming, avant GA) | ≈ **6×** de complétion de tâches en test interne |

⚠️ **Les séries mem0 et Zep ne sont pas comparables** : chaque éditeur benchmarke contre la version
antérieure de l'autre (Zep annonce mem0 à 49,0 sur LongMemEval là où mem0 annonce 94,4 sur le même
benchmark). Ces chiffres ne doivent pas servir à choisir une architecture.

### 2.2 Enseignements structurants

1. **La récupération gagnante est hybride multi-signaux** : similarité vectorielle + BM25 +
   *entity matching*, normalisés puis fusionnés en un score unique. Aucun signal ne suffit seul.
2. **mem0 a retiré son graphe** au profit d'un *entity linking* intégré au scoring, pour supprimer
   le déploiement Neo4j. Conclusion de l'éditeur sur vecteur vs graphe : « both are useful; neither
   is sufficient alone ».
3. **L'apport réel de Zep n'est pas le graphe, c'est le modèle bi-temporel** : chaque fait porte
   quand il est devenu vrai, quand il a été invalidé, et par quoi il a été remplacé.
4. **Le stack local-first consensuel** est SQLite + FTS5 + `sqlite-vec` dans un seul fichier, fusionnés
   par Reciprocal Rank Fusion : `score = 1/(k + rang_fts) + 1/(k + rang_vecteur)`.
5. **Dreaming (Anthropic, mai 2026)** : consolidation mémoire *planifiée entre les sessions*, qui
   relit l'activité, en extrait des motifs et écrit de nouvelles entrées. Analogie hippocampique.
   Cadre théorique voisin : le *sleep-time compute* de Letta — sortir l'inférence du chemin critique.
6. **Le memory tool Anthropic** (`memory_20250818`, 6 commandes `view`/`create`/`str_replace`/
   `insert`/`delete`/`rename`) est **côté client** : le store est notre infra. Mais c'est un contrat
   de l'API Messages / Managed Agents, **pas de Claude Code**, et il n'offre aucune commande de
   recherche. Écarté (voir § 3.3).

### 2.3 Sources

- [Memory tool — Claude Platform Docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool)
- [State of AI Agent Memory 2026 — mem0](https://mem0.ai/blog/state-of-ai-agent-memory-2026)
- [Zep: A Temporal Knowledge Graph Architecture for Agent Memory (arXiv 2501.13956)](https://arxiv.org/abs/2501.13956)
- [Mem0 vs Zep (Graphiti) — Vectorize](https://vectorize.io/articles/mem0-vs-zep)
- [Why SQLite+FTS5 beats Vector DBs for AI Agent Memory](https://earezki.com/ai-news/2026-04-08-why-sqlitefts5-beats-vector-dbs-for-ai-agent-memory/)
- [vstash: Local-First Hybrid Retrieval with Adaptive Fusion (arXiv 2604.15484)](https://arxiv.org/pdf/2604.15484)
- [Anthropic will let its managed agents dream — The New Stack](https://thenewstack.io/anthropic-managed-agents-dreaming-outcomes/)
- [Claude Code & Agent Memory: Best Practices for 2026 — orchestrator.dev](https://orchestrator.dev/blog/2026-04-06--claude-code-agent-memory-2026/)
- [AI Agent Memory Layer: Episodic, Semantic, Relational — Datapace](https://datapace.ai/blog/ai-agent-memory-layer-architecture-guide-2026)

---

## 3. Décisions de cadrage

### 3.1 Retenues

| Décision | Justification |
|---|---|
| **Surface = Claude Code local + rappels proactifs** | aplan reste bindé sur `127.0.0.1`. Pas de MCP distant, pas de tunnel. La secrétaire peut néanmoins parler la première (cron + notification). |
| **Capture hybride : Claude propose, l'utilisateur valide** | Le tout-automatique est écarté : la base contient déjà ~550 tâches majoritairement issues de doublons de test, et le *memory sprawl* est l'anti-pattern n° 1 documenté. |
| **Store canonique en base aplan + injection par hook** | Une seule source de vérité, requêtable et sauvegardée avec la DB ; injection automatique en contexte sans dépendre des heuristiques du harness. |
| **Bi-temporel dès le schéma** | À l'échelle d'un usage personnel, le goulot n'est pas la récupération mais l'**invalidation**. Rappeler une décision annulée est pire que ne rien rappeler. |
| **Graphe : non** | L'éditeur qui l'a le plus poussé l'a déprécié. Les entités sont déjà en relationnel dans aplan — l'*entity linking* s'obtient par jointure. |
| **Vecteurs : différés derrière un trait** | Tous les benchmarks cités opèrent à 1–10 M de tokens. À l'échelle visée (quelques centaines de souvenirs durables/an), BM25 + entités + récence couvre l'essentiel : le vocabulaire est stable et nominal (*Cartier*, *Saft*, *Pernod*, `AP-1234`), cas idéal du lexical. Le vecteur ne gagne que sur la paraphrase, au prix d'un modèle d'embedding local et du chargement d'une extension SQLite. Décision réversible à coût nul. |
| **Consolidation hors backend** | Le backend Rust n'a aujourd'hui aucun code LLM. Y faire entrer un client, une clé API et du prompt engineering est disproportionné. Une **session Claude Code planifiée** consomme le modèle déjà payé, garde la frontière DDD intacte, et laisse le prompt d'extraction itérable sans recompilation. |

### 3.2 Alternatives d'architecture écartées

- **A — entité domaine seule, sans injection** : bonne structure, mauvais canal. Claude ne consulte
  la mémoire que s'il y pense, donc souvent pas.
- **C — aplan génère les fichiers `memory/`** : bon canal, mauvaise structure. Fichiers plats → pas
  de requête, pas d'historique de révision, plafond d'auto-chargement, et **deux sources de vérité**
  dès que Claude écrit lui-même dans `memory/`.
- **D = A + C** : retenu dans son principe, mais avec la cible d'injection corrigée (voir § 7).

### 3.3 B — aplan comme backend du memory tool Anthropic

Écarté : contrat de l'API Messages / Managed Agents, pas de Claude Code ; travail au bénéfice
d'agents non exploités. Le contrat est en outre plus pauvre qu'une requête — manipulation de
fichiers, aucune recherche. À revisiter uniquement si un agent aplan est un jour construit sur l'API.

---

## 4. Architecture retenue (« D+ »)

```
                       ┌─ Chemin 1 : écriture directe (déterministe) ──────┐
  session Claude ─────►│ aplan remember --kind decision|commitment "…"     │──┐
                       │ → memories(status='pending'), sans passer par le  │  │
                       │   worklog : aucune tâche requise                  │  │
                       └───────────────────────────────────────────────────┘  │
                                                                              ├──► inbox ──► memories
                       ┌─ Chemin 2 : consolidation 17h30 (probabiliste) ───┐  │   (pending)     (active)
  worklog dont         │ session Claude Code planifiée : propose           │──┘                    │
  consolidated_at ────►│ fact|preference, rattrape les décisions non        │    3 issues :        │
  IS NULL              │ enregistrées, PROPOSE LES SUPERSESSIONS            │  new/merge/supersede │
                       └───────────────────────────────────────────────────┘                      │
                                                                                                   ▼
   hook SessionStart ◄── aplan brief (≤ 40 lignes, avec IDs) ◄── MemoryRetriever ◄─────────────────┘
                         (s'AJOUTE à la liste de tâches,        (FTS5 + entités + récence,
                          ne la remplace pas)                    filtre invalidated_at IS NULL)

   aplan recall <id> / --q "…"     ◄── récupération profonde à la demande
   aplan memory supersede <old> --by <new>  ◄── le seul écrivain de invalidated_at
```

| Couche | Contenu | Mécanisme |
|---|---|---|
| Écriture chaude | `aplan remember --kind decision\|commitment\|fact\|preference` | écriture directe dans `memories`, `status='pending'` |
| Consolidation | job 17 h 30 : relit les entrées `consolidated_at IS NULL`, propose des souvenirs **et des supersessions** | session Claude Code planifiée (`CronCreate` / skill `schedule`) |
| File de validation | `aplan inbox` : accepter / fusionner / **superséder** / rejeter | CLI (écran React ultérieur) |
| Invalidation | `aplan memory supersede <old> --by <new>` | **seul chemin d'écriture de `invalidated_at`** |
| Store canonique | entité `memory` bi-temporelle, liée à `project` / `task` | migration `012_create_memories.sql` |
| Récupération | `trait MemoryRetriever` → FTS5/BM25 + jointure d'entités + récence | application + `domain/src/rules/recall.rs` |
| Injection auto | `aplan brief` → sortie du hook SessionStart | hook existant, **enrichi** |
| Rappel profond | `aplan recall <id>` / `aplan recall --q "…"` | *just-in-time retrieval* |

---

## 5. Modèle de données

### 5.1 La frontière qui structure tout

| Table | Question à laquelle elle répond | Statut |
|---|---|---|
| `tasks` | « qu'est-ce que je **dois faire** ? » — état, échéance, alertes | existant, inchangé |
| `worklog_entries` | « qu'est-ce qui **s'est passé**, quand ? » (épisodique) | existant, + une colonne |
| `memories` | « qu'est-ce que je **dois savoir** ? » — décisions, engagements, faits, préférences | **nouveau** |

**Cas limite arbitré** : *« j'ai promis à Pierre de répondre sur l'architecture avant vendredi »*.
La partie actionnable (échéance, statut, alerte de retard) est une **`task`** — la machinerie existe
déjà. Le **souvenir** enregistre le fait qu'un engagement a été pris : envers qui, en quels termes,
à quelle date, et pointe vers la tâche. Sans cet arbitrage, statut / échéance / alertes seraient
dupliqués dans `memories`, produisant deux systèmes de rappel divergents.

### 5.2 Schéma

```sql
-- migrations/sqlite/012_create_memories.sql
CREATE TABLE memories (
  id             TEXT PRIMARY KEY,
  user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  kind           TEXT NOT NULL,     -- decision | commitment | fact | preference
  title          TEXT NOT NULL,     -- une phrase : ce qu'on retient
  body           TEXT,              -- le contexte : pourquoi, alternatives écartées

  -- bi-temporel (apport de Zep, sans le graphe)
  occurred_at    TEXT NOT NULL,     -- quand ça a été décidé / promis
  recorded_at    TEXT NOT NULL,     -- quand aplan l'a su
  invalidated_at TEXT,              -- NULL = encore vrai ; écrit UNIQUEMENT par `memory supersede`
  superseded_by  TEXT REFERENCES memories(id) ON DELETE SET NULL,

  -- provenance
  source         TEXT NOT NULL,     -- claude_session | manual | dreaming
  source_ref     TEXT,              -- id d'entrée worklog, id de session. PAS de FK (voir § 5.3)
  status         TEXT NOT NULL,     -- pending | active | rejected

  -- rattachement (= entity linking, gratuit par jointure)
  -- SET NULL et non CASCADE : supprimer une tâche ne doit pas effacer
  -- le souvenir de la décision qui l'a créée
  project_id     TEXT REFERENCES projects(id) ON DELETE SET NULL,
  task_id        TEXT REFERENCES tasks(id)    ON DELETE SET NULL
);

CREATE TABLE memory_stakeholders (       -- « envers qui », « avec qui »
  memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
  person    TEXT NOT NULL,
  PRIMARY KEY (memory_id, person)
);

-- Table FTS5 AUTONOME (pas `content=`) : voir § 5.3 pour la justification
CREATE VIRTUAL TABLE memories_fts USING fts5(
  memory_id UNINDEXED,
  title,
  body,
  tokenize = 'unicode61 remove_diacritics 2'
);

-- Filigrane de consolidation : marqueur PAR ENTRÉE, pas curseur global (§ 6.2)
ALTER TABLE worklog_entries ADD COLUMN consolidated_at TEXT;
```

**La colonne `kind` sur `worklog_entries` de la v1 est supprimée du périmètre** — voir § 6.1.

### 5.3 Choix défendus

- **`invalidated_at` + `superseded_by` plutôt qu'un `DELETE`** — permet de répondre à la fois à
  « qu'avait-on décidé » et « pourquoi a-t-on changé ». Une décision annulée est une décision avec
  une fin de validité et un successeur, pas une ligne effacée. **Ces colonnes n'ont de valeur que
  parce que § 6.3 définit un écrivain explicite** ; sans lui, le filtre dur de § 7.1 serait décoratif.
- **`status = 'rejected'` conservé comme pierre tombale** — sinon la consolidation re-propose chaque
  soir un candidat déjà rejeté. Le tombstone fait converger la boucle.
- **Table FTS5 autonome, pas `content='memories'`.** Vérifié : avec une table à contenu externe et
  sans triggers, après un `INSERT` dans `memories`, `MATCH 'wave'` renvoie **0 ligne** tandis que
  `SELECT count(*) FROM memories_fts` renvoie **1** — donc le contrôle d'intégrité le plus naturel
  masque la panne, et toute récupération est silencieusement vide. Une table autonome écrite dans la
  **même transaction** que l'insertion (responsabilité du repo) supprime le besoin de triggers, et
  évite en outre de coupler `content_rowid='rowid'` à une table dont la clé primaire est un `TEXT` —
  ces rowids sont renumérotables par `VACUUM`.
- **`tokenize = 'unicode61 remove_diacritics 2'` explicite.** Le défaut plie déjà les accents, mais
  le tokenizer doit être écrit dans la migration plutôt que subi. `porter` est exclu : c'est un
  stemmer anglais, il dégraderait du texte français.
- **`ON DELETE SET NULL` sur `project_id` et `task_id`.** Supprimer une tâche ne doit pas effacer le
  souvenir de la décision qui l'a produite. Corollaire : `source_ref` ne porte **pas** de FK, car les
  `worklog_entries` disparaissent en CASCADE avec leur tâche — la provenance peut donc devenir
  pendante, et c'est accepté explicitement (une chaîne de provenance morte vaut mieux qu'un souvenir
  supprimé).
- **Pas de colonne `confidence`** — remplacée par la porte de validation humaine. Un score que
  personne ne lit ne sert à rien.
- **Pas de `kind = 'procedure'`** — le procédural (conventions, workflows) est déjà couvert par
  `CLAUDE.md` et les skills ; l'ajouter créerait une troisième source de vérité sur les conventions.
- **Les pools de test doivent activer `foreign_keys(true)`.** Le pool réel l'active
  (`infrastructure/src/database/connection.rs`), mais un `SqlitePool::connect("sqlite::memory:")` nu
  ne l'active pas : sans cela, une violation de FK reste verte en TDD et n'apparaît qu'en runtime.

---

## 6. Chemins d'écriture

### 6.1 Deux chemins, précisions différentes

**Chemin 1 — écriture directe (déterministe, gratuit).**

```bash
aplan remember --json --kind decision "Wave 0 limitée au périmètre AI Microsoft" \
  --why "Pierre veut un livrable avant septembre" --project pernod
aplan remember --json --kind commitment "Répondre à Pierre sur l'archi" --to Pierre --task 509a006c
```

Écrit directement dans `memories` avec `status='pending'`. `--confirm` écrit `active` sans passer par
la file (pour les saisies faites par l'utilisateur lui-même).

**La v1 faisait passer ce chemin par `aplan log --kind decision`. C'était impossible** :
`worklog_entries.task_id` est `TEXT NOT NULL REFERENCES tasks(id)` (vérifié,
`006_create_worklog_entries.sql:6`). Conséquences de ce choix, toutes fatales :

1. une discussion d'architecture sans tâche associée ne pouvait produire **aucun** candidat — or
   c'est précisément le type de session qui produit des décisions ;
2. l'option « Ne pas tracker » du hook de démarrage interdit `aplan log` pour toute la session,
   supprimant le chemin 1 entièrement ;
3. quand une tâche *était* suivie, la décision héritait de son projet — souvent une fixture du type
   `Test uppercase kind` — et le bonus d'entité de § 7.1 aurait alors scoré sur le mauvais projet.

`aplan remember` n'a aucune de ces contraintes : `project_id` et `task_id` sont nullables et
explicites. **La colonne `kind` sur `worklog_entries` disparaît donc du périmètre v1** : elle n'était
qu'un moyen d'atteindre le chemin 1, et le chemin 1 n'en a plus besoin.

**Chemin 2 — consolidation (probabiliste).** Relit les `worklog_entries` dont `consolidated_at IS
NULL` et propose : les faits et préférences que personne ne pense à enregistrer, les décisions prises
sans `remember`, et — nouveauté v2 — **les supersessions** (§ 6.3).

**Propriété recherchée : dégradation gracieuse.** Si le prompt d'extraction est médiocre — et il le
sera au début — l'intégralité des décisions typées est conservée. Le composant incertain n'est
jamais sur le chemin critique.

### 6.2 Consolidation — horaire et idempotence

- **Horaire : 17 h 30** (le poste est éteint à 22 h).
- **Marqueur par entrée, pas curseur global.** Le job traite les `worklog_entries` dont
  `consolidated_at IS NULL`. Sans filigrane, toute journée où le poste est éteint à 17 h 30 (client,
  congé) perdrait définitivement ses candidats.
- **`sync_status` ne peut pas porter ce filigrane** (contrairement à ce qu'annonçait la v1) : son
  schéma impose `CHECK (source IN ('jira','outlook','excel','obsidian'))` (vérifié,
  `001_initial.sql:120`). `source='consolidation'` est rejeté à l'`INSERT`, SQLite ne sait pas
  `ALTER` une contrainte `CHECK` (reconstruction de table en 12 étapes), et ajouter une variante à
  `domain::Source` fuiterait dans `dashboard.syncStatuses` et `sync_all`.
- **Un curseur horodaté serait également faux** : toute entrée insérée tardivement avec un
  `logged_at` antérieur au curseur serait sautée définitivement. Le marqueur par entrée est immunisé
  contre l'ordonnancement.
- **Le marqueur est posé APRÈS les écritures réussies.** Un doublon est récupérable via les
  tombstones ; une entrée sautée ne l'est jamais.
- La date du dernier passage est stockée dans la table **`configuration`** (clé/valeur, sans
  contrainte `CHECK`) et **affichée dans le brief** — pour qu'une consolidation morte depuis trois
  semaines soit visible au lieu d'être silencieuse.
- **Garde de disponibilité** : la CLI est un client GraphQL. Si `cargo run -p api` n'est pas lancé,
  chaque appel est un échec silencieux. Le job doit donc tester la joignabilité de l'API **avant**
  toute chose et, si elle est absente, ne rien faire et ne poser aucun marqueur — l'exécution
  suivante rattrapera.
- Corollaire : les entrées loggées après 17 h 30 sont reprises au run suivant, sans perte. L'horaire
  n'est donc pas critique.

**Deux prérequis hors dépôt, à valider par l'utilisateur avant le lot 5 :**

1. **`~/.claude/hooks/aplan-session-start.sh` impose `AskUserQuestion` comme première action
   obligatoire.** Une session planifiée non interactive va donc bloquer ou brûler son tour. Le hook
   doit détecter les sessions non interactives et sauter la question.
2. **Le backend est un `cargo run -p api` lancé à la main.** Toute la proactivité (brief 08 h 30,
   consolidation 17 h 30) en dépend. Une unité `systemd --user` est le correctif de fond ; sans elle,
   la garde de disponibilité ci-dessus se contente de rendre la panne visible.

### 6.3 Inbox

```bash
aplan inbox --json                          # les candidats pending
aplan inbox accept <id> [--kind …]          # nouveau fait → status='active'
aplan inbox merge <id> --into <id>          # même fait, meilleure formulation
aplan inbox supersede <id> --replaces <id>  # le fait a CHANGÉ → invalide l'ancien
aplan inbox reject <id>                     # tombstone, ne sera plus re-proposé
aplan memory supersede <old> --by <new>     # hors file : révision d'un fait déjà actif
```

### Supersede ≠ merge — la distinction porte tout le bi-temporel

C'était le trou de la v1 : **aucune commande n'écrivait `invalidated_at`**, ce qui rendait le filtre
dur de § 7.1 purement décoratif et laissait le pire mode d'échec du design entièrement découvert.

- **`merge`** = « c'est le même fait, mieux écrit ». Une seule ligne survit. **Écrase l'historique.**
- **`supersede`** = « le fait a changé ». L'ancienne ligne reçoit `invalidated_at` et `superseded_by` ;
  les deux lignes survivent. **Conserve l'historique** — c'est-à-dire exactement ce que le modèle
  bi-temporel existe pour conserver.

Confondre les deux ferait disparaître la réponse à « pourquoi a-t-on changé d'avis », qui est la
moitié de la valeur d'un secrétaire.

**Trois écrivains de `invalidated_at`, tous passant par une validation humaine :**

1. `aplan inbox supersede` — quand le candidat contredit un souvenir actif ;
2. `aplan memory supersede` — révision explicite hors file ;
3. la **consolidation**, qui *propose* la supersession : pour chaque candidat de type `decision`, elle
   reçoit les décisions actives du même projet et, si le candidat en contredit une, le soumet avec
   l'`id` de l'ancienne. La supersession suit donc la même règle hybride que tout le reste — Claude
   propose, l'utilisateur tranche.

Deux garde-fous contre la boucle infinie de propositions :

1. **à la proposition** — la consolidation reçoit les mémoires actives du projet *et* les tombstones
   rejetés, avec instruction de ne proposer que du nouveau ;
2. **à l'acceptation** — contrôle de quasi-doublon via FTS5, qui propose `merge` **ou** `supersede`
   selon que le candidat reformule ou contredit, et jamais un ajout muet.

### 6.4 Amorçage

Corpus de départ existant dans `~/.claude/projects/-home-mbt-appfactory-aggregated-plan/memory/` :

| Fichier | `kind` | Rattachement |
|---|---|---|
| `feedback_aplan_note_cadence.md` | `preference` | — |
| `project_mcp_crate_broken_rmcp.md` | `fact` | projet aplan |
| `project_db_task_fixture_pollution.md` | `fact` | projet aplan |
| `project_timesheet_reconstruction.md` | `fact` | projet aplan |

Import one-shot via `aplan memory import <dir>`. **À partir de là, ce dossier reste géré par le
harness ; aplan n'écrit pas dedans** (voir § 7).

---

## 7. Récupération et injection

### 7.1 Scoring

`trait MemoryRetriever` en application ; règle de scoring **pure** en domaine
(`domain/src/rules/recall.rs`), donc testable sans I/O :

- **filtre dur** : `invalidated_at IS NULL AND status = 'active'`, sauf `--history` explicite —
  garde-fou contre le rappel périmé, non négociable ;
- **pertinence** : `-bm25(memories_fts)` — **voir la note de signe ci-dessous** ;
- **bonus d'entité** : correspondance `project_id` / `task_id` / `stakeholders` avec le contexte
  courant ;
- **décroissance de récence** sur `occurred_at` ;
- **poids par `kind`** : `decision` et `commitment` devant `fact` sur une question de type
  « qu'avait-on décidé ».

### Trois pièges FTS5 vérifiés, à traiter comme des exigences

**1. `bm25()` retourne des valeurs négatives, et plus c'est négatif, meilleur c'est.** Mesuré :
`-1.0e-06`. Traiter « BM25 normalisé » comme un score croissant **inverse le classement** — les
meilleurs résultats sortent derniers. La formule est donc `pertinence = -bm25(memories_fts)`, et un
test doit fixer ce signe (deux souvenirs, celui qui matche le mieux en tête).

**2. L'entrée utilisateur ne doit jamais atteindre `MATCH` telle quelle.** Mesuré :

| Requête brute | Résultat |
|---|---|
| `MATCH 'AP-1234'` | `no such column: 1234` |
| `MATCH 'Cartier: certificat'` | `no such column: Cartier` |

Les deux-points et le tiret font partie de la syntaxe de requête FTS5. Ce sont les identifiants
Jira et les libellés « Client : sujet » — c'est-à-dire le vocabulaire quotidien — qui font donc
**planter** la recherche. Il faut une **fonction pure de construction de requête, en `domain`** :
découpage sur les non-alphanumériques, mise entre guillemets de chaque unité (les guillemets FTS5
produisent une requête de phrase, donc `"AP-1234"` matche bien `AP` suivi de `1234`), rejet des
unités vides. Table de cas de test : `AP-1234`, `Cartier : certificat`, `wave 0`, chaîne vide,
guillemet seul, `*`, `NOT`, `OR`.

**3. Le tokenizer ne fait aucune lemmatisation — les pluriels français échouent.** Mesuré : un
souvenir contenant « engagement pris envers Pierre » interrogé avec `engagements` renvoie **0
ligne** ; avec `engagement*`, **1**. C'est un mode d'échec quotidien pour un secrétaire francophone
(« mes engagements », « les décisions »). Correctif : la fonction ci-dessus **suffixe `*`** aux
unités purement alphabétiques de ≥ 4 caractères (le seuil évite l'explosion de faux positifs sur les
mots courts). Les accents, eux, sont bien pliés par `unicode61` — cette inquiétude était infondée.
Si l'expansion par préfixe s'avère insuffisante, le repli documenté est le tokenizer `trigram`
(coût : index nettement plus gros, et pas de requêtes par préfixe).

**Pas de Reciprocal Rank Fusion en v1.** Le RRF fusionne *plusieurs listes classées* ; en v1 il n'y
en a qu'une (BM25). Une somme pondérée de signaux normalisés suffit et se débogue. Le RRF est
réservé à l'arrivée du vecteur comme seconde liste.

### 7.2 Cible d'injection : le hook SessionStart, pas `memory/`

Projeter vers `~/.claude/projects/<slug>/memory/MEMORY.md` serait une erreur : ce fichier a déjà un
écrivain — le mécanisme d'auto-mémoire du harness. Deux écrivains sur un fichier généré = divergence
garantie, exactement le défaut reproché à l'approche C.

**Le hook SessionStart est la bonne cible** : injection directe en contexte, aucun fichier partagé,
aucune dépendance aux heuristiques du harness, contrôle total par aplan.

**Le brief s'AJOUTE à la liste de tâches, il ne la remplace pas.** La v1 annonçait un remplacement :
c'était une erreur. La liste des tâches suivies alimente le sélecteur « Choisir une autre tâche » du
hook de démarrage ; l'ensemble « échéances + engagements » n'est pas l'ensemble « tâches suivies », et
la supprimer priverait le sélecteur de ses candidats. Le brief **dédoublonne et raccourcit** cette
liste (les fixtures du type `Test uppercase kind` ×3, `Test recurring enum` ×3 en sont filtrées) et
**ajoute** les sections mémoire.

```
## Brief — lundi 3 août
Échéances (3) : Cartier certificat J-42 · Pernod assessment J-5 · …
Engagements ouverts (2) : Pierre — archi AI Microsoft périmètre wave 0 [m:a3f]
Décisions actives (projet courant) : [m:7c1] Wave 0 limitée à … (12/06)
À trier : 4 candidats mémoire → `aplan inbox`
⚠ Dernière consolidation : il y a 19 jours
Détail : `aplan recall m:7c1` · Recherche : `aplan recall --q "…"`
```

Les IDs permettent le forage à la demande (*just-in-time retrieval*). **Plafond : 40 lignes**, budget
vérifié en test. La ligne d'avertissement n'apparaît qu'au-delà de 3 jours sans consolidation — c'est
ce qui rend visible une panne silencieuse du backend (§ 6.2).

### 7.3 Proactivité

| Quand | Quoi |
|---|---|
| **08 h 30** | `aplan brief --morning` → notification bureau (échéances du jour, engagements ouverts, candidats à trier) |
| **17 h 30** | session Claude Code planifiée : consolidation (filigrane) → notification avec le nombre de candidats |
| **à chaque session** | hook SessionStart → brief injecté en contexte |

---

## 8. Lots de livraison

**Ordre révisé : l'ancien lot 3 (`brief`) passe après l'écriture.** La v1 le présentait comme le
« point de rentabilité », à tort : à ce stade `memories` ne contient que les 4 lignes importées, donc
« Engagements ouverts » et « Décisions actives » sont vides **par construction**. Un brief sur une
table vide n'est qu'une liste de tâches reformatée. Le brief n'a de valeur qu'une fois qu'il y a de
la matière à afficher.

| Lot | Contenu | Valeur livrée |
|---|---|---|
| **0** | ~~**Spike FTS5**~~ — **FAIT, concluant** (§ 11.4) | risque bloquant levé, aucun repli nécessaire |
| **1** | Migration `012` + domaine + **construction de requête FTS5 (fonction pure)** + règles de scoring **avec le signe de bm25 fixé par un test** (TDD) + repo (écriture FTS dans la même transaction, `foreign_keys(true)` dans les pools de test) + GraphQL + CLI `remember` / `recall` | le store existe et se cherche |
| **2** | Import des 4 souvenirs actuels | corpus réel pour tester la récupération |
| **3** | `aplan inbox` (accept / merge / **supersede** / reject) + `aplan memory supersede` | **l'invalidation existe** — sans ce lot, le bi-temporel est décoratif |
| **4** | `aplan brief` + enrichissement du hook SessionStart (ajout, pas remplacement) | **point de rentabilité : valeur quotidienne** |
| **5** | Prérequis (hook non interactif, `systemd --user` pour l'API) puis consolidation 17 h 30 (marqueur `consolidated_at`, garde de joignabilité) + notification 08 h 30 | capture passive |
| **6** *(plus tard)* | écran inbox React · relance « toujours vrai ? » sur les décisions jamais revisitées · vecteurs + RRF **si** échecs de rappel constatés | — |

À partir du lot 4, le gain est quotidien même si le lot 5 attend.

---

## 9. Risques

| Risque | Gravité | Traitement |
|---|---|---|
| Rappel d'une décision périmée | **élevé** (pire mode d'échec) | filtre dur + **écrivains explicites de `invalidated_at`** (§ 6.3). C'était le trou de la v1 |
| Index FTS vide et invisible | **élevé** | table FTS5 autonome, écrite dans la même transaction ; test qui interroge par `MATCH`, jamais par `count(*)` |
| Recherche qui plante sur le vocabulaire quotidien | **élevé** | fonction pure de construction de requête + table de cas (`AP-1234`, `Client : sujet`, `*`, `NOT`, vide) |
| Classement inversé par le signe de `bm25()` | moyen | `-bm25(...)`, signe fixé par un test d'ordre |
| Pluriels français non trouvés | moyen | expansion par préfixe ≥ 4 caractères ; repli `trigram` |
| ~~FTS5 absent du SQLite embarqué par `sqlx 0.8`~~ | **levé** | lot 0 exécuté : FTS5 et le tokenizer retenu fonctionnent sur la cible réelle (§ 11.4) |
| Consolidation morte silencieusement (API arrêtée) | moyen | garde de joignabilité + âge du dernier passage affiché dans le brief + `systemd --user` |
| Session planifiée bloquée par le hook interactif | moyen | prérequis du lot 5 : détection des sessions non interactives |
| Qualité du prompt d'extraction de la consolidation | moyen | composant le plus incertain, mais hors chemin critique grâce au chemin 1 déterministe |
| Boucle de re-proposition des candidats rejetés | moyen | tombstones `status = 'rejected'` + contexte fourni au job |
| FK non vérifiées en test | moyen | `foreign_keys(true)` dans les pools de test, sinon TDD reste vert sur une violation |
| Prose de `body` qui vieillit (échéance citée en dur) | faible | interdiction d'écrire des dates d'échéance dans un souvenir — l'échéance vit dans la `task` |
| Budget de tokens du hook SessionStart | faible | plafond 40 lignes, vérifié en test |
| Divergence avec le dossier `memory/` du harness | faible | aplan n'écrit pas dedans (§ 7.2) |

---

## 10. Conventions à respecter à l'exécution

- **TDD** : règles de domaine et scoring avant tout code de production (Red → Green → Refactor).
- **Couches DDD strictes** : scoring pur en `domain`, traits en `application`, FTS5 et I/O en
  `infrastructure`, resolvers en `api`. Aucune dépendance LLM dans le backend.
- **Spec** : `SPEC_FONCTIONNELLE.md` et `SPEC_TECHNIQUE.md` mis à jour dans le même commit (en français).
- **Tests** : `cargo test -p domain -p application -p infrastructure -p api` (scopé — le crate `mcp`
  ne compile pas à HEAD).
- **Requêtes** : `sqlx::query` en runtime, pas de macro compile-time ; erreurs mappées vers
  `RepositoryError::Database`.

---

## 11. Revue adversariale (v1 → v2)

### 11.1 Constats retenus

Six affirmations empiriques ont été vérifiées indépendamment avant révision. Toutes confirmées.

| # | Constat | Vérification | Effet sur le design |
|---|---|---|---|
| 1 | Aucune commande n'écrivait `invalidated_at` | lecture du doc v1 | § 6.3 : `supersede` en file et hors file + proposition par la consolidation |
| 2 | `worklog_entries.task_id` est `NOT NULL` | `006_create_worklog_entries.sql:6` | § 6.1 : abandon de `aplan log --kind`, remplacé par `aplan remember` ; colonne `kind` retirée du périmètre |
| 3 | FTS5 à contenu externe reste muet sans triggers | `MATCH` → 0 ligne, `count(*)` → 1 | § 5.2 : table FTS5 autonome, écrite dans la même transaction |
| 4 | `sync_status.source` est sous `CHECK` fermé | `001_initial.sql:120` | § 6.2 : marqueur `consolidated_at` par entrée + `configuration` pour la date de passage |
| 5 | `bm25()` est négatif (`-1.0e-06`) | session `sqlite3` | § 7.1 : `-bm25(...)`, signe fixé par un test |
| 6 | Entrée brute dans `MATCH` → `no such column` ; pluriels français → 0 ligne | `'AP-1234'`, `'Cartier: certificat'`, `engagements` vs `engagement*` | § 7.1 : fonction pure de construction de requête + expansion par préfixe |

Également retenus : `ON DELETE SET NULL` plutôt que défaut, `foreign_keys(true)` dans les pools de
test, réordonnancement des lots (le brief après l'écriture), le brief qui s'ajoute à la liste de
tâches au lieu de la remplacer, la garde de joignabilité de l'API, le marqueur posé après écriture,
et les deux prérequis hors dépôt du lot 5.

### 11.2 Constats écartés, avec la raison

- **« Remplacer `memory_stakeholders` par une colonne JSON, comme `meetings.participants` ».**
  Écarté. « Quels engagements ai-je pris envers Pierre ? » est une requête de premier ordre pour un
  secrétaire, et elle mérite une jointure indexable plutôt qu'un `LIKE` sur du JSON. Le précédent
  invoqué est d'ailleurs listé dans `CLAUDE.md` parmi les *gotchas*, pas parmi les patterns à imiter.
  Coût de la table : deux colonnes.
- **« Supprimer la table de benchmarks de § 2.1, inactionnable pour un implémenteur ».** Écarté en
  partie. Elle ne pilote effectivement aucune décision — c'est dit explicitement dans § 2.1 — mais
  elle évite de refaire la veille dans six mois, et elle documente *pourquoi* le graphe et les
  vecteurs ont été écartés. Conservée, avec son avertissement de non-comparabilité.
- **« Trois axes de cycle de vie (`status`, `invalidated_at`, `superseded_by`) dont aucun n'est
  écrit ».** Écarté sur la conclusion, retenu sur le symptôme. `status` gouverne le cycle de la
  **file de validation** ; `invalidated_at` / `superseded_by` gouvernent le cycle de la **vérité**.
  Ce sont deux axes distincts, pas une redondance. Le correctif est d'ajouter l'écrivain manquant
  (fait, § 6.3), pas de supprimer une colonne.

### 11.3 Ce qui a survécu à l'attaque, et qu'il ne faut donc pas retoucher

- **La séparation `tasks` / `worklog_entries` / `memories`** — tentative de collapse explicite, échouée :
  dupliquer échéance et statut dans `memories` produit deux moteurs d'alerte divergents. Le risque de
  péremption est dans la prose de `body`, et se traite en interdisant les dates d'échéance dans un
  souvenir, pas en restructurant.
- **Le report des vecteurs** — correct à quelques centaines de lignes. Les échecs de recherche
  identifiés sont des bugs de tokenizer et d'échappement, pas un plafond de rappel : les vecteurs ne
  les auraient pas corrigés.
- **La consolidation hors du backend Rust** — l'absence de client LLM dans `infrastructure` est la
  bonne frontière.
- **La dégradation gracieuse chemin 1 / chemin 2** — réelle, une fois le constat n° 2 corrigé.
- **Ne pas écrire dans `~/.claude/.../memory/`** — deux écrivains sur `MEMORY.md` divergeraient.
- **Le lot 0 comme spike** — bon marché, et concluant (§ 11.4).

### 11.4 Lot 0 — résultat du spike (exécuté le 2026-08-03)

Exécuté contre le SQLite **réellement embarqué par `sqlx 0.8`**, et non contre le `sqlite3` du
système. Les six constats de la revue sont reproduits sur la cible réelle, plus deux cas
d'échappement supplémentaires.

| Vérification | Résultat |
|---|---|
| `CREATE VIRTUAL TABLE … USING fts5(…)` | **OK — FTS5 est présent** |
| `tokenize = 'unicode61 remove_diacritics 2'` | OK |
| Accents pliés (doc « limitée », requête `limitee`) | 1 ligne → l'inquiétude sur les accents est bien infondée |
| `MATCH 'engagements'` / `MATCH 'engagement*'` | **0** / 1 |
| `bm25()` | **−0.000001** |
| `MATCH 'AP-1234'` | `no such column: 1234` |
| `MATCH 'Cartier: certificat'` | `no such column: Cartier` |
| `MATCH '*'` | `unknown special query` |
| `MATCH 'NOT'` | `fts5: syntax error near "NOT"` |
| Contenu externe sans triggers | `MATCH` → **0**, `count(*)` → 1 |

**Conséquences** : aucun repli `LIKE` nécessaire ; le tokenizer de § 5.2 est retenu tel quel ; la
table de cas du query-builder (§ 7.1) doit inclure `*` et `NOT` en plus des cas déjà listés. Le spike
était jetable et a été supprimé — ses assertions sont à reprendre comme tests permanents dans le
lot 1 (garde d'environnement : FTS5 disponible, `bm25()` négatif).
