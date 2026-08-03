# Design — aplan comme couche mémoire pour Claude

**Date** : 2026-08-03
**Statut** : validé, prêt à découper en plans d'implémentation
**Objectif utilisateur** : faire d'aplan la base d'un « Claude secrétaire » — capable de rappeler
ce qui doit être fait, et de rappeler les décisions prises et leur contexte.

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
                       ┌─ Chemin 1 : typage à la volée (déterministe) ─────┐
  session Claude ─────►│ aplan log --kind decision|commitment "…"          │──┐
                       └───────────────────────────────────────────────────┘  │
                                                                              ├──► inbox ──► memories
                       ┌─ Chemin 2 : consolidation 17h30 (probabiliste) ───┐  │   (pending)     (active)
  worklog non          │ session Claude Code planifiée : propose           │──┘                    │
  encore consolidé ───►│ fact|preference, rattrape les décisions non taguées│                      │
                       └───────────────────────────────────────────────────┘                      │
                                                                                                   ▼
   hook SessionStart ◄── aplan brief (index ≤ 40 lignes, avec IDs) ◄── MemoryRetriever ◄────────────┘
                                                                       (FTS5 + entités + récence)
   aplan recall <id> / --q "…"  ◄── récupération profonde à la demande
```

| Couche | Contenu | Mécanisme |
|---|---|---|
| Écriture chaude | `aplan log --kind decision\|commitment\|finding\|blocker` | typage du worklog existant |
| Consolidation | job 17 h 30 : relit les entrées non consolidées, propose des souvenirs durables | session Claude Code planifiée (`CronCreate` / skill `schedule`) |
| File de validation | `aplan inbox` : accepter / fusionner / rejeter | CLI (écran React ultérieur) |
| Store canonique | entité `memory` bi-temporelle, liée à `project` / `task` | migration `012_create_memories.sql` |
| Récupération | `trait MemoryRetriever` → FTS5/BM25 + jointure d'entités + récence | application + `domain/src/rules/recall.rs` |
| Injection auto | `aplan brief` → sortie du hook SessionStart | hook existant, remanié |
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
  user_id        TEXT NOT NULL,
  kind           TEXT NOT NULL,     -- decision | commitment | fact | preference
  title          TEXT NOT NULL,     -- une phrase : ce qu'on retient
  body           TEXT,              -- le contexte : pourquoi, alternatives écartées

  -- bi-temporel (apport de Zep, sans le graphe)
  occurred_at    TEXT NOT NULL,     -- quand ça a été décidé / promis
  recorded_at    TEXT NOT NULL,     -- quand aplan l'a su
  invalidated_at TEXT,              -- NULL = encore vrai
  superseded_by  TEXT REFERENCES memories(id),

  -- provenance
  source         TEXT NOT NULL,     -- claude_session | manual | dreaming
  source_ref     TEXT,              -- id d'entrée worklog, id de session
  status         TEXT NOT NULL,     -- pending | active | rejected

  -- rattachement (= entity linking, gratuit par jointure)
  project_id     TEXT REFERENCES projects(id),
  task_id        TEXT REFERENCES tasks(id)
);

CREATE TABLE memory_stakeholders (       -- « envers qui », « avec qui »
  memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
  person    TEXT NOT NULL,
  PRIMARY KEY (memory_id, person)
);

CREATE VIRTUAL TABLE memories_fts USING fts5(
  title, body, content='memories', content_rowid='rowid'
);

ALTER TABLE worklog_entries ADD COLUMN kind TEXT NOT NULL DEFAULT 'note';
```

### 5.3 Choix défendus

- **`invalidated_at` + `superseded_by` plutôt qu'un `DELETE`** — permet de répondre à la fois à
  « qu'avait-on décidé » et « pourquoi a-t-on changé ». Une décision annulée est une décision avec
  une fin de validité et un successeur, pas une ligne effacée.
- **`status = 'rejected'` conservé comme pierre tombale** — sinon la consolidation re-propose chaque
  soir un candidat déjà rejeté. Le tombstone fait converger la boucle.
- **Pas de colonne `confidence`** — remplacée par la porte de validation humaine. Un score que
  personne ne lit ne sert à rien.
- **Pas de `kind = 'procedure'`** — le procédural (conventions, workflows) est déjà couvert par
  `CLAUDE.md` et les skills ; l'ajouter créerait une troisième source de vérité sur les conventions.

---

## 6. Chemins d'écriture

### 6.1 Deux chemins, précisions différentes

**Chemin 1 — typage à la volée (déterministe, gratuit).** Une entrée explicitement typée devient
un candidat quasi 1:1, **sans extraction LLM**.

| worklog `kind` | candidat ? | devient |
|---|---|---|
| `decision` | oui, direct | `memory.kind = decision` |
| `commitment` | oui, direct | `memory.kind = commitment` + `stakeholders` |
| `finding` | non par défaut | seulement si la consolidation le juge durable → `fact` |
| `blocker` | non | c'est de l'**état** : vit dans la task / les alertes |
| `note` (défaut) | non | — |

**Chemin 2 — consolidation (probabiliste).** Rattrape ce que le chemin 1 rate : faits et
préférences que personne ne pense à typer, décisions prises sans tag.

**Propriété recherchée : dégradation gracieuse.** Si le prompt d'extraction est médiocre — et il le
sera au début — l'intégralité des décisions typées est conservée. Le composant incertain n'est
jamais sur le chemin critique.

### 6.2 Consolidation — horaire et idempotence

- **Horaire : 17 h 30** (le poste est éteint à 22 h).
- **Filigrane obligatoire, pas de fenêtre journalière.** Le job traite « toutes les entrées non
  encore consolidées », curseur porté par une ligne de la table `sync_status` existante. Sans cela,
  toute journée où le poste est éteint à 17 h 30 (client, congé) perdrait définitivement ses
  candidats.
- Corollaire : les entrées loggées après 17 h 30 sont reprises au run suivant, sans perte. L'horaire
  n'est donc pas critique.

### 6.3 Inbox

```bash
aplan inbox --json                      # les candidats pending
aplan inbox accept <id> [--kind …]      # valide (et corrige le kind si besoin)
aplan inbox reject <id>                 # tombstone, ne sera plus re-proposé
aplan inbox merge <id> --into <id>      # fusionne un doublon
aplan remember --json "…" --kind fact   # écriture directe, sans passer par la file
```

Deux garde-fous contre la boucle infinie de propositions :

1. **à la proposition** — la consolidation reçoit les mémoires actives du projet *et* les tombstones
   rejetés, avec instruction de ne proposer que du nouveau ;
2. **à l'acceptation** — contrôle de quasi-doublon via FTS5, qui propose une fusion plutôt qu'un ajout.

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

- **filtre dur** : `invalidated_at IS NULL`, sauf `--history` explicite — garde-fou contre le rappel
  périmé, non négociable ;
- **BM25** issu de FTS5, normalisé ;
- **bonus d'entité** : correspondance `project_id` / `task_id` / `stakeholders` avec le contexte
  courant ;
- **décroissance de récence** sur `occurred_at` ;
- **poids par `kind`** : `decision` et `commitment` devant `fact` sur une question de type
  « qu'avait-on décidé ».

**Pas de Reciprocal Rank Fusion en v1.** Le RRF fusionne *plusieurs listes classées* ; en v1 il n'y
en a qu'une (BM25). Une somme pondérée de signaux normalisés suffit et se débogue. Le RRF est
réservé à l'arrivée du vecteur comme seconde liste.

### 7.2 Cible d'injection : le hook SessionStart, pas `memory/`

Projeter vers `~/.claude/projects/<slug>/memory/MEMORY.md` serait une erreur : ce fichier a déjà un
écrivain — le mécanisme d'auto-mémoire du harness. Deux écrivains sur un fichier généré = divergence
garantie, exactement le défaut reproché à l'approche C.

**Le hook SessionStart est la bonne cible** : injection directe en contexte, aucun fichier partagé,
aucune dépendance aux heuristiques du harness, contrôle total par aplan.

Bénéfice secondaire : le hook actuel déverse 20 tâches brutes dont plusieurs doublons de fixtures
(`Test uppercase kind` ×3, `Test recurring enum` ×3). `aplan brief` le remplace par un brief
dédoublonné et priorisé, dans le même budget de tokens.

```
## Brief — lundi 3 août
Échéances (3) : Cartier certificat J-42 · Pernod assessment J-5 · …
Engagements ouverts (2) : Pierre — archi AI Microsoft périmètre wave 0 [m:a3f]
Décisions actives (projet courant) : [m:7c1] Wave 0 limitée à … (12/06)
À trier : 4 candidats mémoire → `aplan inbox`
Détail : `aplan recall m:7c1` · Recherche : `aplan recall --q "…"`
```

Les IDs permettent le forage à la demande (*just-in-time retrieval*). **Plafond : 40 lignes**, budget
vérifié en test.

### 7.3 Proactivité

| Quand | Quoi |
|---|---|
| **08 h 30** | `aplan brief --morning` → notification bureau (échéances du jour, engagements ouverts, candidats à trier) |
| **17 h 30** | session Claude Code planifiée : consolidation (filigrane) → notification avec le nombre de candidats |
| **à chaque session** | hook SessionStart → brief injecté en contexte |

---

## 8. Lots de livraison

| Lot | Contenu | Valeur livrée |
|---|---|---|
| **0** | **Spike FTS5** : une requête `CREATE VIRTUAL TABLE … USING fts5` avec `sqlx 0.8`. Repli si absent : `LIKE` + index, ou activation de la feature | lève le seul risque bloquant |
| **1** | Migration `012` + domaine + règles de scoring (TDD) + repo + GraphQL + CLI `remember` / `recall` | le store existe |
| **2** | Import des 4 souvenirs actuels | corpus réel pour tester la récupération |
| **3** | `aplan brief` + remaniement du hook SessionStart | **point de rentabilité : valeur quotidienne** |
| **4** | `--kind` sur worklog + mapping des candidats + `aplan inbox` | chemin d'écriture déterministe |
| **5** | Consolidation planifiée 17 h 30 (filigrane `sync_status`) + notification 08 h 30 | capture passive |
| **6** *(plus tard)* | écran inbox React · vecteurs + RRF **si** échecs de rappel constatés | — |

À partir du lot 3, le gain est quotidien même si les lots 4-5 attendent.

---

## 9. Risques

| Risque | Gravité | Traitement |
|---|---|---|
| FTS5 absent du SQLite embarqué par `sqlx 0.8` | bloquant | lot 0, spike d'une requête. Repli : `LIKE` + index, ou activation de la feature |
| Qualité du prompt d'extraction de la consolidation | moyen | composant le plus incertain, mais hors chemin critique grâce au chemin 1 déterministe |
| Boucle de re-proposition des candidats rejetés | moyen | tombstones `status = 'rejected'` + contexte fourni au job |
| Rappel d'une décision périmée | **élevé** (pire mode d'échec) | filtre dur `invalidated_at IS NULL` + modèle bi-temporel |
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
