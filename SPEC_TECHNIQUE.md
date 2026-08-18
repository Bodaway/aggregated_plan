# Technical Specification — Aggregated Plan

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture Overview](#2-architecture-overview)
3. [Tech Stack](#3-tech-stack)
4. [Project Structure](#4-project-structure)
5. [Backend Architecture](#5-backend-architecture)
6. [Frontend Architecture](#6-frontend-architecture)
7. [Database Schema](#7-database-schema)
8. [GraphQL API](#8-graphql-api)
9. [External Integrations](#9-external-integrations)
10. [Synchronization Engine](#10-synchronization-engine)
11. [Deduplication Engine](#11-deduplication-engine)
12. [Alert Engine](#12-alert-engine)
13. [Activity Tracking](#13-activity-tracking)
14. [Authentication & Security](#14-authentication--security)
15. [Configuration](#15-configuration)
16. [Testing Strategy](#16-testing-strategy)
17. [Deployment](#17-deployment)
18. [MVP Scope](#18-mvp-scope)
19. [Coding Conventions](#19-coding-conventions)

---

## 1. Overview

### 1.1 Purpose

This document is the complete technical specification for **Aggregated Plan**, a personal cockpit for a Tech Lead managing 4-8 software projects with 5-15 people. It aggregates data from Jira, Microsoft Outlook, Excel (SharePoint), and Obsidian into a single dashboard with prioritization, activity tracking, and alerting capabilities.

This specification is self-contained. An implementation agent should be able to build the entire application from this document alone, using the functional specification (`SPEC_FONCTIONNELLE.md`) as the source of business requirements.

### 1.2 Key Constraints

| Constraint | Description |
|-----------|-------------|
| **Functional paradigm** | Pure functions, immutability, algebraic data types, `Result` types — no classes, no inheritance |
| **Multi-user ready** | `user_id` on all tables, auth middleware (no-op locally, Azure AD for Teams) |
| **Teams migration path** | Architecture must support future deployment as a Microsoft Teams Tab application |
| **Read-only integration** | The application never writes back to external sources (Jira, Outlook, Excel) |
| **Offline resilience** | The application remains functional with cached data when sources are unavailable |

### 1.3 Definitions

| Term | Definition |
|------|-----------|
| **Half-day** | Scheduling unit. Morning: 08:00-12:00, Afternoon: 13:00-17:00 |
| **Capacity** | Available half-days per week (default: 10) |
| **Workload** | Half-days consumed by planned tasks + meetings |
| **Source** | External system: Jira, Outlook, Excel, Obsidian |
| **Aggregated task** | A task in the application, possibly merged from multiple sources |
| **Week** | Monday to Friday (5 business days). Monday is the first day of the week. `week_start_of(date)` returns the Monday of the given date's week. |

---

## 2. Architecture Overview

### 2.1 High-Level Architecture

```
+------------------------------------------------------+
|                     Frontend                         |
|          React + TypeScript + urql + Shadcn/ui       |
|                                                      |
|   +----------+ +----------+ +----------+            |
|   |Dashboard | | Priority | | Activity |  ...       |
|   |  Page    | |  Matrix  | | Journal  |            |
|   +----+-----+ +----+-----+ +----+-----+            |
|        |             |            |                  |
|        +-------------+------------+                  |
|                      |                               |
|              urql GraphQL Client                     |
|         (Queries/Mutations + SSE Subscriptions)      |
+----------------------+-------------------------------+
                       | HTTP / SSE
+----------------------+-------------------------------+
|                     Backend                          |
|              Rust + Axum + async-graphql             |
|                                                      |
|  +---------------------------------------------+    |
|  |              API Layer (crate: api)          |    |
|  |     GraphQL Resolvers + Subscriptions        |    |
|  |     Axum HTTP Server + SSE Transport         |    |
|  +----------------------+-----------------------+    |
|  +----------------------+-----------------------+    |
|  |        Application Layer (crate: app)       |    |
|  |     Use Cases + Repository Traits           |    |
|  |     Service Traits (connectors)             |    |
|  +----------------------+-----------------------+    |
|  +----------------------+-----------------------+    |
|  |        Domain Layer (crate: domain)         |    |
|  |     Pure Types + Business Rules             |    |
|  |     Zero external dependencies              |    |
|  +---------------------------------------------+    |
|  +---------------------------------------------+    |
|  |     Infrastructure Layer (crate: infra)     |    |
|  |     SQLite/Postgres Repos + API Clients     |    |
|  |     Sync Engine + Dedup Engine              |    |
|  +----------------------+-----------------------+    |
+----------------------+-------------------------------+
                       |
          +------------+------------+
          |            |            |
     +----+----+ +-----+-----+ +---+------+
     |  Jira   | | Microsoft | |  SQLite  |
     |  REST   | | Graph API | | Database |
     |  API    | | (Outlook  | |          |
     |         | | +SharePt) | |          |
     +---------+ +-----------+ +----------+
```

### 2.2 Communication Patterns

| Pattern | Transport | Direction | Use Case |
|---------|-----------|-----------|----------|
| GraphQL Query | HTTP POST | Client -> Server | Fetch dashboard, tasks, workload |
| GraphQL Mutation | HTTP POST | Client -> Server | Create task, log activity, change priority |
| GraphQL Subscription | SSE | Server -> Client | Sync progress, activity reminders, alert updates |

### 2.3 Data Flow

```
External Sources --sync--> Infrastructure --transform--> Domain Types
                                                              |
                                                        --persist--> SQLite
                                                              |
                                                        --deduplicate--> Merged Tasks
                                                              |
                                                        --alert check--> Alerts
                                                              |
GraphQL Resolvers <--read--  Application Use Cases  <--query--+
       |
       +--> Frontend (urql cache + React state)
```

### 2.4 Layer Dependency Rules

These rules are enforced at compile time via Cargo workspace crate boundaries:

```
domain       ->  (no internal dependencies)
application  ->  domain
infrastructure -> domain, application
api          ->  domain, application, infrastructure
cli          ->  (no internal dependencies — talks to api over HTTP)
```

The **domain** crate has zero dependencies on other internal crates and zero external I/O dependencies. It contains only pure types and pure functions.

The **cli** crate is structurally independent of the domain/application/infrastructure layers — it only depends on `graphql_client` + `reqwest::blocking` and a committed `schema.graphql` exported from the api crate. This keeps it loosely coupled to the backend, lets it be installed standalone via `cargo install --path crates/cli`, and means a backend rename of any field surfaces as a `cargo build` failure on the CLI side rather than a runtime error.

### 2.5 CLI client (`aplan`)

In addition to the React frontend, the system exposes an `aplan` command-line client built as a separate crate. It is keyboard-first, optimized for the tech-lead hot path (start a worklog, change task status, take a fast note), and addresses the same GraphQL API as the frontend on `http://127.0.0.1:3001/graphql`.

- **Topology:** loopback only. The default `--api-url` points to `127.0.0.1:3001/graphql`; an `APLAN_API_URL` env var or `--api-url` flag can override it.
- **Auth:** none. The auth middleware injects the same default user as the frontend.
- **Codegen:** every operation is a `.graphql` file under `crates/cli/graphql/` checked at compile time against a committed `schema.graphql`. Refresh the schema after backend changes via `cargo run -p api -- export-schema > crates/cli/graphql/schema.graphql`.
- **Identifier resolution:** wherever a command takes a TASK argument, the same resolver runs (`@`/`current` → currently-tracked task, UUID → direct, Jira-style key → exact match on `tasks.source_id` via the new `sourceId` filter, anything else → fuzzy match via the new `titleContains` filter).
- **Output:** terse one-line human format by default; `--json` emits the raw GraphQL `data.*` payload for parsing by Claude or shell scripts.
- **Exit codes:** `0` success, `1` generic, `2` not found, `3` ambiguous lookup, `4` precondition failed.
- **Claude integration:** a `.claude/skills/aplan/SKILL.md` ships in-repo so Claude Code uses the CLI instead of crafting GraphQL queries by hand.
- **Worklog CLI verbs:**
  - `aplan log [--task <TASK>] [--at <WHEN>] "<text>"` — appends a timestamped worklog entry (body) to the active task (or `--task` target). This is the primary logging verb for Claude; each call is atomic (one finding/decision/action per call). Calls the `addWorklogEntry` GraphQL mutation internally.
    - `--at <WHEN>` place l'entrée **dans le passé**, en heure locale, pour rédiger après coup une journée déjà écoulée : sans lui, sept entrées écrites lundi à propos de jeudi laissent jeudi à zéro heure et posent un créneau quasi nul sur lundi. Quatre formes acceptées, validées **côté client** (une date mal écrite est un refus, code 4, avant tout aller-retour réseau) : `AAAA-MM-JJTHH:MM[:SS]`, `"AAAA-MM-JJ HH:MM[:SS]"`, `HH:MM[:SS]` (aujourd'hui) et `AAAA-MM-JJ` seul — **qui vaut midi**. Midi n'est pas un milieu arbitraire : une entrée est la preuve des 45 minutes qui la *précèdent* (`presence::build_lanes`), rognées aux fenêtres de travail, donc midi porte 11:15–12:00, entièrement dans la matinée, tandis que minuit ne tombe dans aucune fenêtre et que 08:00 — le début même de la journée — projette son ombre sur 07:15–08:00, avant l'ouverture : les deux factureraient zéro. La valeur part en `loggedAtLocal` **non convertie** (voir cette mutation).
    - `--at` **reconstruit aussi les créneaux de ce jour-là** (`rebuildWorklogProjection`), et c'est nécessaire : `aplan flush` ne regarde que dans sa propre fenêtre, donc une entrée antidatée lui est invisible et son jour continuerait d'afficher zéro heure. Si cette reconstruction échoue, la commande **réussit quand même** — l'entrée est écrite et relancer `log` la dupliquerait — et l'avertissement nomme la réparation idempotente (`aplan slots rebuild`).
    - Ce que `--at` n'invente pas : **les heures viennent toujours de l'étalement des entrées.** Sept entrées antidatées à la même minute valent une minute, exactement comme en direct.
  - `aplan show <TASK> [--worklog N|all] [--json]` — le **côté lecture** de `aplan log` (R-WL-16). Après le détail de la tâche, imprime les entrées **de la plus ancienne à la plus récente**, via la query `worklogEntries(filter: { taskIds: [<T>], limit, offset })`. `TaskGql` ne porte pas de champ worklog : c'est un second aller-retour, émis **seulement** si l'argument n'est pas `0`. L'argument est un `WorklogAmount` (`None` | `Tail(n)` | `All`) analysé par `FromStr` — un seul argument plutôt qu'un compte plus un drapeau `--worklog-all`, les trois réponses étant les valeurs d'une même question ; une valeur invalide est un refus de `clap`, avant tout réseau.
    - **`Tail(n)`** (défaut 10) : une seule requête de `n + 1` lignes, l'entrée surnuméraire étant ce qui permet d'annoncer « … older entries not shown » sans seconde requête de comptage.
    - **`All`** : **pagination** par pages de 1000 (`WORKLOG_FILTER_MAX_LIMIT`, le plafond que le serveur applique à toute requête), jusqu'à une page incomplète. Demander `limit: i64::MAX` en une fois ne marcherait pas : le serveur rendrait silencieusement les 1000 premières lignes et le résultat *paraîtrait* complet — c'est exactement le mode d'échec que cette branche existe pour éviter. `older_exist` y est toujours faux, puisque rien n'a été laissé derrière. Garde-fou à 50 pages avec note sur stderr : au-delà, ce n'est pas une tâche bavarde mais un serveur qui a cessé d'honorer `offset`. L'horodatage est rendu en heure locale — `aplan log --at` lit une horloge murale, réimprimer l'UTC stocké ferait passer le même instant pour deux instants différents selon le verbe qui l'affiche. La colonne d'auteur réutilise l'abréviation à 4 caractères des lignes de recouvrement (`overlap_actor_label`, `manuel` pour l'humain). **Meilleur effort**, sur le précédent de `journal`/`dash` : le détail de la tâche a déjà été imprimé, donc un serveur plus ancien sans l'opération ne doit pas transformer `show` en échec — note sur stderr (stdout reste propre pour la machine) et, en `--json`, un `worklogEntries: null` explicite à côté de `task`, jamais un tableau vide ni une clé absente. En succès, `--json` porte exactement les mêmes entrées, dans le même ordre, que la sortie humaine.
  - `aplan flush [--json] <TASK>` — rebuilds the task's closed activity slots for the local half-days its window touched. The window is only a **selector**: it decides which half-days to rebuild, and every worklog entry of the task in each of those half-days — not only the ones inside the window — then decides what the slots are, via `derive_time_blocks`. Re-running is a no-op; a backdated entry is still picked up. This verb carries no `--session` flag yet, so it resolves against the human's `aplan.active_since`; the `flushWorklogTime` mutation it calls internally also accepts an optional `sessionId` to select a Claude session's own window (`sessions.last_flush_at`) instead — wired up by a later plan's hook rewrite. Does **not** clear the active-task pointer (`aplan.active_task_id`). Used by the `SessionEnd` hook.
  - `aplan reattribute --from <TASK> --to <TASK> {--date AAAA-MM-JJ | --since D [--until D] | --entry <ID>…} [--confirm]` — déplace des entrées de journal **et redérive** les créneaux d'activité qui en découlent (US-RE, R23b). Appelle la mutation `reattributeWorklogEntries`. `--from`/`--to` passent par le résolveur de tâche habituel ; les références d'entrée (`--entry`) sont résolues **côté serveur** par préfixe d'identifiant (`WorklogRepository::find_by_id_prefix`, même contrat que `MemoryRepository::find_by_id_prefix`), une collision étant signalée (code 3) et jamais devinée. `--date` et `--entry` sont mutuellement exclusifs à l'analyse des arguments (clap `conflicts_with`), `--until` exige `--since`. **Aperçu par défaut** : sans `--confirm` la mutation résout tout, calcule le même compte rendu et n'écrit rien — un seul chemin de code, donc l'aperçu ne peut pas dériver de l'écriture. Codes de sortie : 2 tâche/entrée introuvable, 3 référence ambiguë, 4 refus (source = destination, entrée d'une autre tâche, sélection vide, plafond de page atteint), 1 réseau/GraphQL.
- **Verbes de maintenance des créneaux :**
  - `aplan slots rebuild --task <TASK> --date AAAA-MM-JJ [--json]` — reconstruit les créneaux d'une tâche pour un jour local, depuis ses entrées de journal. Appelle `rebuildWorklogProjection`. C'est le chemin de réparation d'une journée dont les entrées sont antidatées : `aplan flush` déduit ses demi-journées de sa propre fenêtre, qui commence au démarrage de la session, donc une entrée horodatée la semaine dernière lui est invisible et ce jour-là continue d'afficher zéro heure. `aplan log --at` l'exécute lui-même ; on l'utilise à la main pour une journée antidatée autrement (l'éditeur d'horodatage de l'UI web, `updateWorklogEntry`) ou quand cette passe automatique a échoué. **Pas de `--confirm`**, contrairement à `repair`, et l'asymétrie est voulue : `repair` peut *perdre* des heures (un orphelin dont les entrées ont disparu est jeté, pas déplacé), tandis que celui-ci ne réécrit jamais que les demi-journées propres à une tâche depuis des entrées toujours présentes — le relancer laisse la même journée. Le format de la date est validé côté client (refus code 4). Une journée sans entrée imprime « logged nothing that day » et sort en 0. Après application, relancer `aplan timesheet --date <jour>` : le brouillon a été reconstruit avant la reconstruction des créneaux.
  - `aplan slots repair --from AAAA-MM-JJ --to AAAA-MM-JJ [--confirm] [--json]` — rend leur tâche aux créneaux qui l'ont perdue (US-SR, R23c). Appelle la mutation `repairOrphanedSlots`. Les deux bornes sont **obligatoires** (`clap`, pas de défaut) et leur **format** est validé côté client — une date mal écrite est un refus (code 4) et non une erreur de coercition de scalaire (code 1) ; l'**ordre** des deux bornes, lui, est validé côté serveur, pour que la règle n'ait qu'un seul propriétaire. **Aperçu par défaut** : sans `--confirm`, la mutation calcule tout et n'écrit rien. Le rendu humain imprime une ligne par date (`N orphelins (Xh) → M créneaux écrits`), le tableau avant/après par tâche — titre inclus, car ces tâches ont été *découvertes* par la réparation et non nommées par l'appelant — et une ligne d'avertissement par date dont les orphelins n'ont plus aucune entrée à réécrire (le seul cas où ce verbe perd des heures). Une plage sans dégât imprime « nothing to repair » et sort en 0. Codes de sortie : 4 refus (plage inversée, date invalide, plafond de page), 1 réseau/GraphQL.
- **Verbe de mémoire du démarrage :**
  - `aplan brief [--morning] [--project <P>] [--date AAAA-MM-JJ]` — imprime le brief de session
    (préférences, échéances, engagements ouverts, décisions actives, file de tri, vétusté de la consolidation),
    **plafonné à 40 lignes** (R55). Destiné au hook `SessionStart`, où il **s'ajoute** à la liste des
    tâches suivies sans la remplacer (R56) : cette liste alimente le sélecteur de tâche du hook.
    La CLI imprime les lignes rendues par `domain::rules::brief` telle quelles — un seul rendu, donc
    le plafond ne peut pas être contourné côté client. `--json` émet `data.brief` brut.
- **Verbe de recherche transverse :**
  - `aplan search --q <TERMES> [--limit N] [--json]` (R64) — cherche dans les tâches (titre et
    description), les entrées de journal et les réunions en lisant chaque dépôt directement
    (`TaskRepository::find_by_user`, pagination complète du journal par pages de
    `WORKLOG_FILTER_MAX_LIMIT`, réunions sur une fenêtre glissante de **24 mois** car
    `MeetingRepository` n'expose aucune liste non bornée, seulement une plage de dates), et dans
    les mémoires via le chemin de rappel existant — **jamais** la query GraphQL `tasks` et son
    `first: 50`. Les résultats sont **groupés par entité**, jamais fusionnés en un classement
    unique : mémoires dans l'ordre du rappel (pertinence), tâches/journal/réunions triés par
    récence. Plafond de **`SEARCH_MAX_PER_GROUP` = 5** résultats par groupe, relevable par
    `--limit` (même valeur par défaut côté serveur et côté CLI) ; un groupe sans résultat est omis,
    toute troncature annoncée (`(12, 5 affichés)`, même formule que `aplan brief`). Les accents
    énumérés dans `fold_diacritic` (aigu/caron/ogonek/ring d'Europe centrale et occidentale, plus
    le letton `ģ ķ ļ ņ`) sont pliés (`domain::rules::search::normalize`) comme le fait
    `memories_fts` (`unicode61 remove_diacritics 2`), pour que la requête se comporte pareil sur
    les quatre entités — sauf, dans les deux moteurs, les lettres à barre ou ligature (`ł`, `ø`,
    `đ`, `æ`, `œ`, `ß`), à saisir telles quelles, et sauf tout diacritique **non énuméré** dans
    `fold_diacritic` : un écart non mesuré, qui plie côté `memories_fts` (FTS5) mais pas côté
    tâches, journal ou réunions (ce module), tant qu'il n'a pas été confirmé via `fts5vocab` et
    ajouté. Une saisie vide ou blanche ne ramène **rien**, jamais les 642 tâches ou les 572
    entrées de journal du magasin. `--json` émet `data.search` brut.
- **Verbes de consolidation (lot 5)** — pilotés par une session Claude Code planifiée, jamais par le
  backend. Les trois acceptent `--json`, ce qui est la condition pour être pilotables :
  - `aplan consolidate pending [--limit N]` (défaut 200) — les entrées de journal dont
    `consolidatedAt` est nul, **de la plus ancienne à la plus récente**. Lecture seule, donc c'est
    aussi la **sonde de joignabilité** que la session exécute en premier (R60) : elle ne marque rien,
    et son échec laisse le filigrane intact.
  - `aplan consolidate mark <id>…` — pose le filigrane. Idempotent et par entrée (R59) ; la sortie
    annonce `marked/requested` pour que l'écart soit visible. Un appel sans identifiant est refusé
    par `clap` (`required = true`).
  - `aplan consolidate record-run` — écrit `memory.consolidation.last_run` dans `configuration`,
    c'est-à-dire la clé que le brief lit (R57).
  - Le jeu d'instructions de la session vit dans `docs/prompts/consolidation-memoire.md`, **hors du
    binaire** : c'est le composant le plus incertain du dispositif, il doit être itérable sans
    recompiler.
- **Codes de sortie des verbes de mémoire** : `2` identifiant introuvable, `3` référence ambiguë,
  `4` précondition refusée (candidat déjà `active`/`rejected`, cible de fusion non active, souvenir
  déjà invalidé, cycle de supersession, saisie sans rien de recherchable), `1` échec générique
  (réseau, base). Le `4` est ce qui permet à un appelant automatisé de sauter un candidat sans
  conclure que le réseau est tombé (R62). `async-graphql` ne transportant pas de code d'erreur, la
  reconnaissance se fait sur le **message rendu** par `AppError` — ces libellés sont donc porteurs de
  contrat et fixés par des tests (`memory_cmd::is_precondition_failure`).

The CLI is a third client alongside the React frontend and the existing `aggregated-plan-mcp` MCP server. The MCP server talks directly to SQLite via the application/infrastructure crates; the CLI deliberately goes over HTTP so it can never race on the database file and shares one source of truth with the frontend.

---

## 3. Tech Stack

### 3.1 Backend

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| Language | Rust | stable (latest) | Type safety, performance, functional paradigm |
| HTTP Framework | Axum | 0.7+ | Async web framework by Tokio team |
| GraphQL | async-graphql | 7+ | GraphQL server with SSE subscription support |
| Database Driver | sqlx | 0.8+ | Compile-time checked SQL, SQLite + Postgres support |
| HTTP Client | reqwest | 0.12+ | Jira and Microsoft Graph API calls |
| Async Runtime | tokio | 1.x | Async runtime for Axum and background tasks |
| Serialization | serde + serde_json | 1.x | JSON serialization/deserialization |
| Date/Time | chrono | 0.4+ | Date and time handling |
| UUID | uuid | 1.x | Unique identifier generation |
| Error Handling | thiserror | 1.x | Derive macro for error types |
| Logging | tracing + tracing-subscriber | 0.1+ | Structured logging |
| Environment | dotenvy | latest | .env file loading |
| CORS | tower-http | 0.5+ | CORS middleware for local dev |
| Task Scheduling | tokio-cron-scheduler | latest | Periodic sync scheduling |

### 3.2 Frontend

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| Language | TypeScript | 5.3+ | Strict mode, all strict flags enabled |
| UI Framework | React | 18+ | Component-based UI |
| Build Tool | Vite | 5+ | Fast dev server and build |
| GraphQL Client | urql | 4+ | Lightweight GraphQL client with SSE subscriptions |
| Subscriptions | graphql-sse | latest | SSE transport for GraphQL subscriptions |
| UI Components | shadcn/ui | latest | Accessible, customizable component library (Radix-based) |
| Styling | Tailwind CSS | 3+ | Utility-first CSS framework |
| Charts | Recharts | 2+ | Workload charts, retrospective visualizations |
| Drag and Drop | @dnd-kit/core + @dnd-kit/sortable | 6+ | Priority matrix drag-and-drop |
| Routing | react-router-dom | 6+ | Client-side routing |
| Date Utilities | date-fns | 3+ | Date formatting and calculations |
| Type Generation | @graphql-codegen/cli | latest | Generate TypeScript types from GraphQL schema |
| Testing | vitest + @testing-library/react | latest | Unit and component tests |
| E2E Testing | Playwright | latest | End-to-end browser tests |

### 3.3 Database

| Phase | Technology | Reason |
|-------|-----------|--------|
| Local (MVP) | SQLite | Zero setup, file-based, perfect for single-user local |
| Teams deployment | PostgreSQL | Multi-user, concurrent access, server deployment |

The transition is handled by **sqlx** which supports both SQLite and PostgreSQL via feature flags. SQL queries use the common subset of both dialects, with migration files per database engine where needed.

---

## 4. Project Structure

```
aggregated-plan/
|
+-- backend/                          # Rust workspace root
|   +-- Cargo.toml                    # Workspace definition
|   +-- .env.example                  # Environment variable template
|   |
|   +-- crates/
|       +-- domain/                   # Pure business logic
|       |   +-- Cargo.toml
|       |   +-- src/
|       |       +-- lib.rs
|       |       +-- types/            # Algebraic data types
|       |       |   +-- mod.rs
|       |       |   +-- task.rs
|       |       |   +-- meeting.rs
|       |       |   +-- project.rs
|       |       |   +-- activity.rs
|       |       |   +-- alert.rs
|       |       |   +-- tag.rs
|       |       |   +-- user.rs
|       |       |   +-- common.rs     # Source, HalfDay, etc.
|       |       |   +-- memory.rs     # R43-R44: semantic memory (migration 012)
|       |       +-- rules/            # Business rules as pure functions
|       |       |   +-- mod.rs
|       |       |   +-- urgency.rs    # R10-R15: urgency calculation
|       |       |   +-- priority.rs   # Quadrant classification, sorting
|       |       |   +-- workload.rs   # R01-R03: capacity, half-day consumption
|       |       |   +-- alerts.rs     # R16-R19: alert detection
|       |       |   +-- dedup.rs      # R08-R09: similarity scoring
|       |       |   +-- recall.rs     # R47-R48: FTS5 query building, recall scoring
|       |       |   +-- memory_import.rs    # R54: frontmatter parsing, type mapping
|       |       |   +-- memory_lifecycle.rs # R50-R53: accept/reject/merge/supersede
|       |       |   +-- brief.rs        # R55-R57: brief composition, 40-line cap, rendering
|       |       +-- errors.rs         # Domain error types
|       |
|       +-- application/              # Use cases and trait definitions
|       |   +-- Cargo.toml
|       |   +-- src/
|       |       +-- lib.rs
|       |       +-- repositories/     # Repository trait definitions
|       |       |   +-- mod.rs
|       |       |   +-- task_repository.rs
|       |       |   +-- meeting_repository.rs
|       |       |   +-- project_repository.rs
|       |       |   +-- activity_repository.rs
|       |       |   +-- alert_repository.rs
|       |       |   +-- tag_repository.rs
|       |       |   +-- sync_status_repository.rs
|       |       |   +-- config_repository.rs
|       |       |   +-- memory_repository.rs
|       |       +-- services/         # External service trait definitions
|       |       |   +-- mod.rs
|       |       |   +-- jira_client.rs
|       |       |   +-- outlook_client.rs
|       |       |   +-- excel_client.rs
|       |       |   +-- memory_retriever.rs  # Recall service (FTS5-backed)
|       |       |   +-- memory_file_source.rs # Harness memory dir reader (READ-ONLY)
|       |       +-- use_cases/        # Application use case functions
|       |       |   +-- mod.rs
|       |       |   +-- dashboard.rs
|       |       |   +-- task_management.rs
|       |       |   +-- priority.rs
|       |       |   +-- activity_tracking.rs
|       |       |   +-- sync.rs
|       |       |   +-- deduplication.rs
|       |       |   +-- alerts.rs
|       |       |   +-- configuration.rs
|       |       |   +-- memory.rs     # remember / get / search / queue / import / supersede
|       |       |   +-- brief.rs      # R55-R57: fetches for `aplan brief` (no rules here)
|       |       +-- dto.rs            # Data transfer objects for use cases
|       |       +-- errors.rs         # Application error types
|       |
|       +-- infrastructure/           # Concrete implementations
|       |   +-- Cargo.toml
|       |   +-- src/
|       |       +-- lib.rs
|       |       +-- database/         # SQLite/Postgres repository implementations
|       |       |   +-- mod.rs
|       |       |   +-- connection.rs # Connection pool setup
|       |       |   +-- task_repo.rs
|       |       |   +-- meeting_repo.rs
|       |       |   +-- project_repo.rs
|       |       |   +-- activity_repo.rs
|       |       |   +-- alert_repo.rs
|       |       |   +-- tag_repo.rs
|       |       |   +-- sync_status_repo.rs
|       |       |   +-- config_repo.rs
|       |       |   +-- memory_repo.rs # SqliteMemoryRepository + SqliteMemoryRetriever (FTS5)
|       |       +-- connectors/       # External API clients
|       |       |   +-- mod.rs
|       |       |   +-- jira/
|       |       |   |   +-- mod.rs
|       |       |   |   +-- client.rs
|       |       |   |   +-- types.rs  # Jira API response types
|       |       |   |   +-- mapper.rs # Jira -> domain type mapping
|       |       |   +-- outlook/
|       |       |   |   +-- mod.rs
|       |       |   |   +-- client.rs
|       |       |   |   +-- types.rs
|       |       |   |   +-- mapper.rs
|       |       |   +-- excel/
|       |       |   |   +-- mod.rs
|       |       |   |   +-- client.rs
|       |       |   |   +-- types.rs
|       |       |   |   +-- mapper.rs
|       |       |   +-- memory_files/  # FsMemoryFileSource (local dir, READ-ONLY)
|       |       |       +-- mod.rs
|       |       +-- sync/             # Synchronization engine
|       |       |   +-- mod.rs
|       |       |   +-- engine.rs
|       |       |   +-- scheduler.rs
|       |       +-- dedup/            # Deduplication engine
|       |           +-- mod.rs
|       |           +-- engine.rs
|       |
|       +-- api/                      # HTTP + GraphQL server
|       |   +-- Cargo.toml
|       |   +-- src/
|       |       +-- main.rs           # Entry point: Axum server + `export-schema` subcommand
|       |       +-- graphql/
|       |       |   +-- mod.rs
|       |       |   +-- schema.rs     # Schema construction
|       |       |   +-- query.rs      # Root query resolvers
|       |       |   +-- mutation.rs   # Root mutation resolvers
|       |       |   +-- subscription.rs # Root subscription resolvers
|       |       |   +-- types/        # GraphQL type definitions
|       |       |       +-- mod.rs
|       |       |       +-- task.rs
|       |       |       +-- meeting.rs
|       |       |       +-- project.rs
|       |       |       +-- dashboard.rs
|       |       |       +-- activity.rs
|       |       |       +-- alert.rs
|       |       |       +-- workload.rs
|       |       |       +-- priority.rs
|       |       |       +-- sync.rs
|       |       |       +-- memory.rs     # MemoryGql, ScoredMemoryGql, inbox results
|       |       |       +-- brief.rs      # BriefGql (`lines` + structured sections)
|       |       +-- middleware/
|       |       |   +-- mod.rs
|       |       |   +-- auth.rs       # Auth middleware (no-op locally)
|       |       +-- context.rs        # Request context (user_id extraction)
|       |       +-- state.rs          # Application state (repos, services)
|       |
|       +-- cli/                      # `aplan` CLI binary (HTTP/GraphQL client)
|           +-- Cargo.toml
|           +-- build.rs              # graphql-client codegen against schema.graphql
|           +-- graphql/
|           |   +-- schema.graphql    # Exported via `cargo run -p api -- export-schema`
|           |   +-- *.graphql         # One operation file per query/mutation
|           +-- src/
|               +-- main.rs           # Entry point: clap dispatch
|               +-- cli.rs            # clap derive: Cli + Commands enum
|               +-- client.rs         # reqwest::blocking + graphql_client wrapper
|               +-- lookup.rs         # Task identifier resolver (UUID/key/fuzzy/current)
|               +-- output.rs         # Exit codes, JSON helper
|               +-- queries.rs        # GraphQLQuery derives, custom scalar mappings
|               +-- commands.rs       # One fn per subcommand
|               +-- timesheet_cmd.rs  # `aplan timesheet` / `aplan map` subcommands
|               +-- memory_cmd.rs     # `aplan remember` / `recall` / `inbox` / `memory` / `brief`
|               +-- consolidate_cmd.rs # `aplan consolidate pending|mark|record-run`
|
+-- frontend/                         # React application
|   +-- package.json
|   +-- tsconfig.json
|   +-- vite.config.ts
|   +-- tailwind.config.ts
|   +-- codegen.ts                    # GraphQL codegen configuration
|   +-- index.html
|   |
|   +-- src/
|       +-- main.tsx                  # Entry point
|       +-- App.tsx                   # Router setup
|       |
|       +-- lib/                      # Utilities and setup
|       |   +-- urql-client.ts        # urql client configuration
|       |   +-- date-utils.ts         # Date formatting helpers
|       |   +-- constants.ts          # Application constants
|       |
|       +-- generated/                # Auto-generated (graphql-codegen)
|       |   +-- graphql.ts            # TypeScript types + operation hooks
|       |
|       +-- graphql/                  # GraphQL operation definitions
|       |   +-- queries/
|       |   |   +-- dashboard.graphql
|       |   |   +-- tasks.graphql
|       |   |   +-- priority-matrix.graphql
|       |   |   +-- workload.graphql
|       |   |   +-- activity.graphql
|       |   |   +-- alerts.graphql
|       |   |   +-- projects.graphql
|       |   +-- mutations/
|       |   |   +-- task.graphql
|       |   |   +-- priority.graphql
|       |   |   +-- activity.graphql
|       |   |   +-- alert.graphql
|       |   |   +-- dedup.graphql
|       |   |   +-- sync.graphql
|       |   |   +-- config.graphql
|       |   +-- subscriptions/
|       |       +-- sync-progress.graphql
|       |       +-- activity-reminder.graphql
|       |       +-- alerts-updated.graphql
|       |
|       +-- hooks/                    # Custom React hooks
|       |   +-- use-dashboard.ts
|       |   +-- use-tasks.ts
|       |   +-- use-priority-matrix.ts
|       |   +-- use-workload.ts
|       |   +-- use-activity.ts
|       |   +-- use-alerts.ts
|       |   +-- use-sync.ts
|       |   +-- use-config.ts
|       |
|       +-- pages/                    # Page-level components
|       |   +-- DashboardPage.tsx
|       |   +-- PriorityMatrixPage.tsx
|       |   +-- WorkloadPage.tsx
|       |   +-- ActivityJournalPage.tsx
|       |   +-- SettingsPage.tsx
|       |   +-- TeamPage.tsx          # v2
|       |   +-- ProjectPage.tsx       # v2
|       |   +-- RetrospectivePage.tsx  # v2
|       |
|       +-- components/               # Reusable UI components
|           +-- layout/
|           |   +-- Sidebar.tsx
|           |   +-- Header.tsx
|           |   +-- PageLayout.tsx
|           +-- task/
|           |   +-- TaskCard.tsx
|           |   +-- TaskList.tsx
|           |   +-- TaskForm.tsx
|           |   +-- TaskQuickAdd.tsx
|           +-- meeting/
|           |   +-- MeetingCard.tsx
|           |   +-- MeetingList.tsx
|           +-- priority/
|           |   +-- PriorityGrid.tsx
|           |   +-- QuadrantColumn.tsx
|           +-- workload/
|           |   +-- WorkloadChart.tsx
|           |   +-- HalfDayGrid.tsx
|           |   +-- WeekNavigator.tsx
|           +-- activity/
|           |   +-- ActivityTimeline.tsx
|           |   +-- ActivitySwitcher.tsx
|           |   +-- SlotEditor.tsx
|           +-- alert/
|           |   +-- AlertBadge.tsx
|           |   +-- AlertPanel.tsx
|           +-- sync/
|           |   +-- SyncStatusBar.tsx
|           +-- dedup/
|               +-- DeduplicationPanel.tsx
|
+-- migrations/                       # Database migrations (sqlx)
|   +-- sqlite/
|   |   +-- 001_initial.sql
|   |   +-- ...                       # 002-011: recurrence, worklog, Gryzzly, timesheet, mappings
|   |   +-- 012_create_memories.sql   # Semantic memory + standalone FTS5 index
|   |   +-- 013_add_proposed_supersedes_and_fix_alert_type_check.sql
|   +-- postgres/
|       +-- 001_initial.sql
|
+-- docs/
|   +-- plans/                        # Design documents
|   +-- prompts/                      # Instruction sets for scheduled Claude sessions
|       +-- consolidation-memoire.md  # 17:30 memory consolidation (§ 10.10)
|
+-- SPEC_FONCTIONNELLE.md             # Functional specification (French)
+-- SPEC_TECHNIQUE.md                 # This file
+-- README.md
```

### 4.1 Cargo Workspace Configuration

```toml
# backend/Cargo.toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.dependencies]
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "1"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
```

```toml
# backend/crates/domain/Cargo.toml
[package]
name = "domain"
version = "0.1.0"
edition = "2021"

[dependencies]
chrono = { workspace = true }
serde = { workspace = true }
uuid = { workspace = true }
# NO other crate dependencies -- this is enforced
```

```toml
# backend/crates/application/Cargo.toml
[package]
name = "application"
version = "0.1.0"
edition = "2021"

[dependencies]
domain = { path = "../domain" }
async-trait = "0.1"
thiserror = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
tokio = { workspace = true }
```

```toml
# backend/crates/infrastructure/Cargo.toml
[package]
name = "infrastructure"
version = "0.1.0"
edition = "2021"

[dependencies]
domain = { path = "../domain" }
application = { path = "../application" }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "chrono", "uuid"] }
reqwest = { version = "0.12", features = ["json"] }
tokio = { workspace = true }
tokio-cron-scheduler = "0.11"
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
```

```toml
# backend/crates/api/Cargo.toml
[package]
name = "api"
version = "0.1.0"
edition = "2021"

[dependencies]
domain = { path = "../domain" }
application = { path = "../application" }
infrastructure = { path = "../infrastructure" }
axum = "0.7"
async-graphql = { version = "7", features = ["chrono", "uuid"] }
async-graphql-axum = "7"
tokio = { workspace = true }
tower = "0.5"
tower-http = { version = "0.5", features = ["cors", "trace"] }
tracing = { workspace = true }
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
serde = { workspace = true }
serde_json = { workspace = true }
uuid = { workspace = true }
dotenvy = "0.15"
```

---

## 5. Backend Architecture

### 5.1 Domain Layer (`crates/domain`)

The domain layer contains **only** pure types and pure functions. It has zero I/O, zero async, and zero dependencies on other internal crates.

#### 5.1.1 Core Types (Algebraic Data Types)

All types are **immutable structs** and **enums**. No methods attached to types -- all logic is in free functions.

```rust
// types/common.rs

use chrono::{NaiveDate, DateTime, Utc};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

pub type UserId = Uuid;
pub type TaskId = Uuid;
pub type MeetingId = Uuid;
pub type ProjectId = Uuid;
pub type ActivitySlotId = Uuid;
pub type AlertId = Uuid;
pub type TagId = Uuid;
pub type TaskLinkId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Source {
    Jira,
    Excel,
    Obsidian,
    Personal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum UrgencyLevel {
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum ImpactLevel {
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HalfDay {
    Morning,
    Afternoon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertType {
    Deadline,
    Overload,
    Conflict,
    TimesheetReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AlertSeverity {
    Information,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectStatus {
    Active,
    Paused,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncSourceStatus {
    Idle,
    Syncing,
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Quadrant {
    UrgentImportant,
    Important,
    Urgent,
    Neither,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackingState {
    Inbox,      // Newly synced, not yet triaged
    Followed,   // User chose to track this task
    Dismissed,  // User chose to ignore this task
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskLinkType {
    AutoMerged,
    ManualMerged,
    Rejected,
}
```

```rust
// types/task.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub user_id: UserId,
    pub title: String,
    pub description: Option<String>,
    /// User-owned markdown notes. Never overwritten by Jira sync —
    /// distinct from `description` which mirrors the Jira ticket body.
    pub notes: Option<String>,
    pub source: Source,
    pub source_id: Option<String>,
    pub jira_status: Option<String>,
    pub status: TaskStatus,
    pub project_id: Option<ProjectId>,
    pub assignee: Option<String>,
    pub deadline: Option<NaiveDate>,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub estimated_hours: Option<f32>,
    pub urgency: UrgencyLevel,
    pub urgency_manual: bool,
    pub impact: ImpactLevel,
    pub tracking_state: TrackingState,
    pub jira_remaining_seconds: Option<i32>,        // From Jira timeestimate
    pub jira_original_estimate_seconds: Option<i32>, // From Jira timeoriginalestimate
    pub jira_time_spent_seconds: Option<i32>,       // From Jira timespent
    pub remaining_hours_override: Option<f32>,       // Local override for remaining time
    pub estimated_hours_override: Option<f32>,       // Local override for estimated time
    /// Delegated-to name. User-owned free text, never overwritten by sync —
    /// same preservation contract as `notes`. Per-occurrence for recurring tasks.
    pub delegated_to: Option<String>,
    pub tags: Vec<TagId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Task {
    /// Effective remaining hours: local override > Jira remaining > None
    pub fn effective_remaining_hours(&self) -> Option<f32> {
        self.remaining_hours_override
            .or(self.jira_remaining_seconds.map(|s| s as f32 / 3600.0))
    }

    /// Effective estimated hours: local override > Jira estimate > estimated_hours
    pub fn effective_estimated_hours(&self) -> Option<f32> {
        self.estimated_hours_override
            .or(self.jira_original_estimate_seconds.map(|s| s as f32 / 3600.0))
            .or(self.estimated_hours)
    }
}
```

```rust
// types/meeting.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: MeetingId,
    pub user_id: UserId,
    pub title: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub location: Option<String>,
    pub participants: Vec<String>,
    pub project_id: Option<ProjectId>,
    pub outlook_id: String,
    pub created_at: DateTime<Utc>,
}
```

```rust
// types/project.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub user_id: UserId,
    pub name: String,
    pub source: Source,
    pub source_id: Option<String>,
    pub status: ProjectStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

```rust
// types/activity.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySlot {
    pub id: ActivitySlotId,
    pub user_id: UserId,
    pub task_id: Option<TaskId>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub half_day: HalfDay,
    pub date: NaiveDate,
    pub created_at: DateTime<Utc>,
}
```

```rust
// types/alert.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: AlertId,
    pub user_id: UserId,
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub related_items: Vec<RelatedItem>,
    pub date: NaiveDate,
    pub resolved: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelatedItem {
    Task(TaskId),
    Meeting(MeetingId),
}
```

```rust
// types/tag.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    pub user_id: UserId,
    pub name: String,
    pub color: Option<String>,
}
```

```rust
// types/user.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
}
```

#### 5.1.2 Business Rules (Pure Functions)

All business rules from the functional spec (R01-R26) are implemented as **pure functions** -- no I/O, no side effects, fully testable with simple input/output assertions.

**Urgency calculation (R10-R15):**

```rust
// rules/urgency.rs

/// R10-R14: Calculate urgency from deadline relative to today.
/// Pure function: deadline x today -> UrgencyLevel
pub fn calculate_urgency(deadline: Option<NaiveDate>, today: NaiveDate) -> UrgencyLevel {
    match deadline {
        None => UrgencyLevel::Low,                                // R10
        Some(d) => {
            let business_days = count_business_days(today, d);
            match business_days {
                n if n < 0 => UrgencyLevel::Critical,             // R14: overdue
                0..=1 => UrgencyLevel::High,                      // R13
                2..=5 => UrgencyLevel::Medium,                    // R12
                _ => UrgencyLevel::Low,                           // R11
            }
        }
    }
}

/// R15: Resolve urgency -- manual override takes precedence.
pub fn resolve_urgency(
    manual_urgency: Option<UrgencyLevel>,
    deadline: Option<NaiveDate>,
    today: NaiveDate,
) -> (UrgencyLevel, bool) {
    match manual_urgency {
        Some(u) => (u, true),                                     // R15: manual prevails
        None => (calculate_urgency(deadline, today), false),
    }
}

/// Count business days between two dates (excluding weekends).
/// Negative if target is in the past.
pub fn count_business_days(from: NaiveDate, to: NaiveDate) -> i64 {
    // Implementation: iterate days, skip Saturday/Sunday
    // Return positive if to > from, negative if to < from
}
```

**Priority and quadrant (sorting rules):**

```rust
// rules/priority.rs

/// Classify a task into a priority quadrant based on urgency and impact.
pub fn determine_quadrant(urgency: UrgencyLevel, impact: ImpactLevel) -> Quadrant {
    let is_urgent = (urgency as u8) >= 3;
    let is_important = (impact as u8) >= 3;
    match (is_urgent, is_important) {
        (true, true) => Quadrant::UrgentImportant,
        (false, true) => Quadrant::Important,
        (true, false) => Quadrant::Urgent,
        (false, false) => Quadrant::Neither,
    }
}

/// Sort tasks by priority: UrgentImportant > Important > Urgent > Neither.
/// Within the same quadrant, sort by closest deadline first.
pub fn sort_tasks_by_priority(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| {
        let qa = determine_quadrant(a.urgency, a.impact);
        let qb = determine_quadrant(b.urgency, b.impact);
        qa.cmp(&qb).then_with(|| a.deadline.cmp(&b.deadline))
    });
}
```

**Workload calculation (R01-R03):**

```rust
// rules/workload.rs

/// Calculate hours consumed by a meeting.
pub fn meeting_hours(start: DateTime<Utc>, end: DateTime<Utc>) -> f64 {
    (end - start).num_minutes() as f64 / 60.0
}

/// R16: Detect overload for a week.
/// Returns Some(excess_hours) if total load exceeds capacity, None otherwise.
///
/// Note: `planned_task_hours` (calculé dans `compute_weekly_workload`) **exclut les tâches
/// Terminées (`Done`) et Annulées (`Cancelled`)** via `Task::counts_toward_workload()`. Ces
/// tâches conservent leur estimation (Jira original estimate / estimation personnelle) mais
/// ne comptent plus dans les totaux d'heures (par jour côté frontend et hebdomadaire côté
/// backend). Les tâches Bloquées (`Blocked`) continuent de compter. Le filtre n'est appliqué
/// qu'au calcul des heures : les tâches terminées restent récupérées et affichées sur le
/// tableau de bord.
pub fn detect_overload(
    planned_task_hours: f64,
    meeting_hours: f64,
    weekly_capacity_hours: f64,
) -> Option<f64> {
    let total = planned_task_hours + meeting_hours;
    if total > weekly_capacity_hours { Some(total - weekly_capacity_hours) } else { None }
}

/// Determine which half-day a datetime falls into.
pub fn half_day_of(hour: u32) -> HalfDay {
    if hour < 13 { HalfDay::Morning } else { HalfDay::Afternoon }
}
```

**Alert detection (R16-R19):**

```rust
// rules/alerts.rs

/// Data needed to generate an alert (not yet persisted).
pub struct AlertData {
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub related_items: Vec<RelatedItem>,
    pub date: NaiveDate,
}

/// R17: Check all tasks for approaching or past deadlines.
pub fn check_deadline_alerts(
    tasks: &[Task],
    today: NaiveDate,
    threshold_days: i64,
) -> Vec<AlertData> {
    tasks.iter().filter_map(|task| {
        let deadline = task.deadline?;
        let days_remaining = count_business_days(today, deadline);
        if days_remaining < 0 {
            Some(AlertData {
                alert_type: AlertType::Deadline,
                severity: AlertSeverity::Critical,
                message: format!("Task '{}' is overdue by {} day(s)", task.title, -days_remaining),
                related_items: vec![RelatedItem::Task(task.id)],
                date: today,
            })
        } else if days_remaining <= threshold_days {
            Some(AlertData {
                alert_type: AlertType::Deadline,
                severity: AlertSeverity::Warning,
                message: format!("Task '{}' is due in {} day(s)", task.title, days_remaining),
                related_items: vec![RelatedItem::Task(task.id)],
                date: today,
            })
        } else {
            None
        }
    }).collect()
}

/// R18: Check for scheduling conflicts on a given date.
/// A conflict occurs when two items have overlapping time ranges.
/// Overlap condition: start_a < end_b AND start_b < end_a
pub fn check_conflict_alerts(
    scheduled_items: &[ScheduledItem],
    date: NaiveDate,
) -> Vec<AlertData> {
    // For each pair of items, check if [start_a, end_a) overlaps [start_b, end_b)
}

pub enum ScheduledItem {
    Task { id: TaskId, title: String, start: DateTime<Utc>, end: DateTime<Utc> },
    Meeting { id: MeetingId, title: String, start: DateTime<Utc>, end: DateTime<Utc> },
}

/// R16: Check overload for the week.
pub fn check_overload_alerts(
    planned_half_days: f64,
    meeting_half_days: f64,
    weekly_capacity: u32,
    week_start: NaiveDate,
) -> Option<AlertData> {
    detect_overload(planned_half_days, meeting_half_days, weekly_capacity).map(|excess| {
        let severity = if excess > 2.0 { AlertSeverity::Critical } else { AlertSeverity::Warning };
        AlertData {
            alert_type: AlertType::Overload,
            severity,
            message: format!("Overloaded by {:.1} half-day(s) this week", excess),
            related_items: vec![],
            date: week_start,
        }
    })
}
```

**Deduplication scoring (R08-R09):**

```rust
// rules/dedup.rs

pub struct SimilarityScore {
    pub title_score: f64,       // 0.0 to 1.0 (weighted token Dice)
    pub assignee_match: bool,
    pub project_match: bool,
    pub overall: f64,           // Title, closed toward 1.0 by matching attributes
}

/// Rareté de chaque mot parmi les titres comparés (R09c). Un préfixe de projet
/// partagé par tout le backlog pèse moins qu'un mot propre à une tâche ; le
/// lissage ramène la pondération à l'uniforme quand les titres sont trop peu
/// nombreux. `TitleCorpus::uniform()` neutralise la pondération.
pub struct TitleCorpus { /* title_count, occurrences */ }

impl TitleCorpus {
    pub fn from_titles<'a, I: IntoIterator<Item = &'a str>>(titles: I) -> Self;
    pub fn uniform() -> Self;
}

/// R08: Check if a Jira ticket key appears in an arbitrary string (Excel row data).
pub fn find_jira_key_in_text(jira_key: &str, text: &str) -> bool {
    text.contains(jira_key)
}

/// R09b: similarité de deux titres — coefficient de Dice sur les mots appariés
/// un à un et pondérés par leur rareté. Insensible à l'ordre des mots, à la
/// ponctuation et aux accents ; une faute de frappe est absorbée par une
/// distance d'édition appliquée mot à mot (seuil `TOKEN_MATCH_THRESHOLD`).
pub fn title_similarity(a: &str, b: &str, corpus: &TitleCorpus) -> f64;

/// R09/R09a: Calculate similarity between two tasks for potential deduplication.
/// Le titre décide seul ; un assigné ou un projet identique comble
/// `ATTRIBUTE_BONUS` (10 %) de l'écart restant jusqu'à 1.0.
pub fn calculate_similarity(
    title_a: &str,
    title_b: &str,
    assignee_a: Option<&str>,
    assignee_b: Option<&str>,
    project_a: Option<&str>,
    project_b: Option<&str>,
    corpus: &TitleCorpus,
) -> SimilarityScore {
    let title_score = title_similarity(title_a, title_b, corpus);
    let assignee_match = match (assignee_a, assignee_b) {
        (Some(a), Some(b)) => a.to_lowercase() == b.to_lowercase(),
        _ => false,
    };
    let project_match = match (project_a, project_b) {
        (Some(a), Some(b)) => a.to_lowercase() == b.to_lowercase(),
        _ => false,
    };
    let bonus = if assignee_match { ATTRIBUTE_BONUS } else { 0.0 }
        + if project_match { ATTRIBUTE_BONUS } else { 0.0 };
    let overall = title_score + (1.0 - title_score) * bonus;

    SimilarityScore { title_score, assignee_match, project_match, overall }
}

/// Normalized Levenshtein distance: 1.0 = identical, 0.0 = completely different.
pub fn normalized_levenshtein(a: &str, b: &str) -> f64 {
    // Standard Levenshtein, normalisée par le nombre de CARACTÈRES du plus long
    // des deux (pas d'octets : un titre accentué aurait un score gonflé).
}

/// Dedup confidence threshold: suggestions above this score are shown to the user.
pub const DEDUP_CONFIDENCE_THRESHOLD: f64 = 0.7;
```

**Worklog time derivation (`derive_time_blocks`):**

```rust
// rules/worklog.rs

use chrono_tz::Tz;

/// Un bloc de travail dérivé, exprimé en heure locale.
pub struct LocalBlock {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub date: NaiveDate,
    pub half_day: HalfDay,
}

/// Durée minimale d'un bloc réduit à un seul horodatage (R-WL-09).
pub const MIN_BLOCK_MINUTES: i64 = 1;

/// Écart maximal entre deux entrées consécutives pour que le travail soit
/// considéré comme continu (R-WL-13). Au-delà, le temps n'a pas été passé sur la
/// tâche : constante du domaine, non configurable. 45 et non 15, car une entrée de
/// journal est un marqueur d'événement, pas un échantillon d'activité.
pub const MAX_CONTINUATION_GAP_MINUTES: i64 = 45;

/// Regroupe des horodatages LOCAUX en blocs de travail.
///
/// Deux coupures : (1) la (date, demi-journée) — matin = heure < 13, après-midi =
/// heure >= 13, via `workload::half_day_of` — qu'un bloc ne franchit jamais, car un
/// créneau persisté ne porte qu'une seule valeur `half_day` et la réattribution se
/// borne par elle ; (2) un écart de plus de `MAX_CONTINUATION_GAP_MINUTES` avec
/// l'horodatage suivant (R-WL-13). Une demi-journée produit donc autant de blocs
/// qu'elle comptait de plages de travail continues.
///
/// Un bloc va du premier au dernier horodatage de sa plage ; pour une plage réduite
/// à un horodatage, `start == end` (l'appelant lui donne `MIN_BLOCK_MINUTES` à la
/// persistance). L'ordre d'entrée est indifférent ; la sortie est triée par (date,
/// demi-journée matin-avant-après-midi, début).
///
/// Fonction pure : aucune I/O, aucun effet de bord.
pub fn derive_time_blocks(local_times: &[NaiveDateTime]) -> Vec<LocalBlock>

/// Durée persistée d'un bloc : son propre écart, plancher à `MIN_BLOCK_MINUTES`.
pub fn block_duration(block: &LocalBlock) -> Duration

/// Heures qu'un ensemble de blocs représente, à la minute — le même calcul que le
/// rapport d'activité applique aux créneaux qui en découlent.
pub fn total_block_hours(blocks: &[LocalBlock]) -> f64
```

La conversion UTC ↔ local (clé `aplan.timezone`) et la résolution de la fenêtre-sélecteur
du flush (`aplan.active_since` pour l'humain, `sessions.last_flush_at` pour une session
Claude — jamais les deux, § 7.3.4) restent dans la couche application : le domaine ne devine
jamais un fuseau.

#### 5.1.3 Domain Errors

```rust
// errors.rs

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("Task not found: {0}")]
    TaskNotFound(TaskId),
    #[error("Project not found: {0}")]
    ProjectNotFound(ProjectId),
    #[error("Invalid urgency value: {0}. Must be 1-4.")]
    InvalidUrgency(u8),
    #[error("Invalid impact value: {0}. Must be 1-4.")]
    InvalidImpact(u8),
    #[error("Activity slot overlap: existing slot covers this time range")]
    ActivitySlotOverlap,
    #[error("Invalid date range: start {start} is after end {end}")]
    InvalidDateRange { start: String, end: String },
}
```

#### 5.1.4 Result Type

```rust
// lib.rs

pub type DomainResult<T> = Result<T, DomainError>;
```

#### 5.1.5 Règles de rappel (`rules/recall.rs`) — mémoire sémantique

Deux responsabilités, toutes deux **pures** (aucune I/O, testables sans base) :

**1. Construction de requête FTS5 — `build_match_query(user_input) -> Result<String, DomainError>`**

La saisie brute ne doit **jamais** atteindre `MATCH` : `-`, `:` et `*` font partie de la syntaxe
de requête FTS5, donc le vocabulaire quotidien fait *échouer* la recherche, pas seulement la
manquer. Mesuré sur le SQLite embarqué :

| Requête brute | Résultat |
|---|---|
| `MATCH 'AP-1234'` | `no such column: 1234` |
| `MATCH 'Cartier: certificat'` | `no such column: Cartier` |
| `MATCH '*'` | `unknown special query` |
| `MATCH 'NOT'` | `fts5: syntax error near "NOT"` |
| `MATCH 'engagements'` / `'engagement*'` | **0** ligne / 1 ligne |

Algorithme :

1. **découpage sur les espaces uniquement** (`split_whitespace`), puis rejet des groupes sans
   aucun caractère alphanumérique ;
2. **chaque groupe devient UNE phrase entre guillemets, ponctuation interne conservée**. Une
   chaîne FTS5 entre guillemets est une phrase littérale : rien à l'intérieur n'est interprété
   comme opérateur, et le tokenizer y découpe quand même des tokens **positionnés**. Un
   guillemet saisi est échappé en le doublant (`a"b` → `"a""b"`), sinon il refermerait la phrase
   et le reste du groupe atteindrait le parseur ;
3. sur les groupes **purement alphabétiques** :
   - suffixe `*` à partir de 4 caractères (`PREFIX_EXPANSION_MIN_LEN`) ;
   - et, à partir de 5 caractères (`DEPLURALIZATION_MIN_LEN`) quand le groupe finit par `s` ou
     `x`, **branche OR dépluralisée** : `("engagements"* OR "engagement"*)` ;
4. **jointure par un `AND` explicite**.

Une saisie sans aucun caractère alphanumérique retourne `DomainError::ValidationError`.

Exemples : `AP-1234` → `"AP-1234"` · `Cartier : certificat` → `"Cartier"* AND "certificat"*` ·
`wave 0` → `"wave"* AND "0"` · `engagements` → `("engagements"* OR "engagement"*)` ·
`NOT` → `"NOT"` (littéral inoffensif).

> **Pourquoi une seule phrase par groupe.** `"AP-1234"` exige que `AP` soit **immédiatement
> suivi** de `1234`. Découper en `"AP" "1234"` produirait un AND **non positionné**, qui
> matcherait un souvenir mentionnant `AP` et `1234` à vingt mots d'intervalle. Test de garde :
> `a_jira_key_query_requires_adjacency`.

> **Pourquoi les deux branches d'expansion.** Le suffixe `*` ne peut que *rallonger* le mot
> saisi : `engagement*` retrouve « engagements », mais `engagements*` ne retrouve **pas**
> « engagement » — or c'est ce second sens qui a été mesuré lors du spike. La branche OR
> dépluralisée couvre donc le sens manquant. Une lemmatisation reste exclue (`porter` est un
> stemmer anglais) et le tokenizer `trigram` n'est pas nécessaire.

> ⚠️ **Le `AND` doit être explicite, et la branche OR parenthésée.** Deux pièges vérifiés contre
> le SQLite embarqué :
> 1. le AND **implicite** de FTS5 (simple espace) n'est défini qu'**entre phrases** ; dès qu'un
>    groupe est un OR parenthésé, `"wave"* ("engagements"* OR …)` lève
>    `fts5: syntax error near "("`. D'où la jointure par ` AND `
>    (test `groups_are_joined_by_an_explicit_and`) ;
> 2. FTS5 lie `AND` plus fort que `OR`, donc sans les parenthèses `a AND b* OR c*` se lirait
>    `(a AND b*) OR c*` — la branche dépluralisée élargirait la requête au lieu de la préciser
>    (test `a_depluralized_branch_still_requires_the_other_groups`).

Une élision reste dans son groupe (`décision d'archi` → `"décision"* AND "d'archi"`) : le
tokenizer y voit les tokens adjacents `d` + `archi`.

**2. Scoring — `score()` / `rank()`**

Somme pondérée de quatre signaux normalisés (pas de RRF : il n'y a qu'une liste classée en v1) :

| Signal | Calcul |
|---|---|
| pertinence | `relevance_from_bm25(bm25)` = `(-bm25).max(0) / (1 + (-bm25).max(0))` |
| bonus d'entité | projet 0,5 + tâche 0,3 + personne 0,2, plafonné à 1,0 |
| récence | `0.5 ^ (âge_jours / 90)` sur `occurred_at`, plafonné à 1,0 dans le futur |
| poids par type | `decision` = `commitment` 1,0 > `fact` 0,6 > `preference` 0,5 |

> ⚠️ **`bm25()` retourne des valeurs NÉGATIVES et plus c'est négatif, meilleure est la
> correspondance** (mesuré : `-0.000001`). La pertinence est donc `-bm25(…)`. Traiter bm25 comme
> un score croissant **inverse le classement** : les meilleurs résultats sortent derniers. Le
> signe est verrouillé par un test d'ordre explicite
> (`better_bm25_match_ranks_first`), et les valeurs non négatives sont ramenées à zéro —
> c'est précisément ce qui rend une erreur de signe détectable par ce test plutôt que
> silencieuse.

`rank()` trie par score décroissant avec un tri **stable** (les ex æquo gardent l'ordre d'entrée).

`build_match_query_any()` est la même construction jointe par `OR` : utilisée par la détection de
quasi-doublons, qui interroge avec un titre entier. Un `AND` y exigerait la présence de *tous* les
mots du titre et manquerait donc exactement les reformulations qu'elle doit attraper ; la précision
revient par le seuil de similarité.

#### 5.1.6 Règles d'import (`rules/memory_import.rs`)

- `parse_memory_file(contents)` — lecteur d'entête **ligne à ligne**, volontairement sans
  dépendance YAML (le crate `domain` n'en prend aucune, et la forme de ces fichiers est fixe :
  scalaires plats plus un bloc `metadata:` indenté). Retourne `name`, `description`,
  `metadata.type`, `metadata.modified` et le corps markdown. Le découpage `clé: valeur` se fait sur
  le **premier** deux-points, et une paire de guillemets est retirée. Absence d'entête →
  `ValidationError`, que l'appelant traduit en « ignoré », jamais en échec global (`MEMORY.md`,
  l'index du harness, est exactement ce cas).
- `kind_for_metadata_type(t)` — `feedback` | `user` → `Preference` ; `project` | `reference` →
  `Fact` ; inconnu ou absent → `Fact`. Jamais `Decision` ni `Commitment` : un import ne se promeut
  pas lui-même.
- `import_source_ref(name, file_name)` — `memory-file:<name>` (repli sur le nom de fichier). C'est
  la **clé d'idempotence** : stable d'un run à l'autre, et insensible au renommage du fichier.

#### 5.1.7 Règles de cycle de vie (`rules/memory_lifecycle.rs`)

Transitions pures : chaque fonction reçoit les lignes concernées et retourne de **nouvelles**
valeurs, si bien que tout le chemin d'écriture est testable sans base.

| Fonction | Effet | Garde-fous |
|---|---|---|
| `accept(candidate, kind_override)` | `pending` → `active`, retypage facultatif | le candidat doit être `pending` ; les dates ne bougent pas |
| `reject(candidate)` | `pending` → `rejected` (pierre tombale) | idem ; n'écrit **pas** `invalidated_at` |
| `merge(candidate, target)` | **une** ligne survit : la cible garde identité et dates, prend la formulation du candidat, union des personnes | candidat `pending`, cible `active` et non invalidée, ids distincts, même utilisateur |
| `supersede(old, successor, chain_from_successor, now)` | **deux** lignes survivent : `old` reçoit `invalidated_at` + `superseded_by` ; `successor` passe `active` | pas d'auto-supersession, pas de cycle, `old` actif et non invalidé, `successor` ni invalidé ni rejeté, même utilisateur |
| `spend_proposal(memory)` (privée) | efface `proposed_supersedes` | appelée par **les quatre** verdicts ci-dessus : une proposition est une question, tout verdict y répond. D'où l'invariant `status <> 'pending'` ⇒ `proposed_supersedes IS NULL` (§ 7.2.1) |
| `title_similarity(a, b)` | max(recouvrement de jetons, Levenshtein normalisé) | le recouvrement attrape la réécriture par réordonnancement, Levenshtein la faute de frappe ; aucun filtre de longueur, sinon `wave 0` et `wave 1` deviendraient identiques |
| `near_duplicates(title, candidates)` | les candidats au-delà de `NEAR_DUPLICATE_THRESHOLD` (0,6), plus similaires d'abord | ne tente **pas** de distinguer reformulation et contradiction : jugement sémantique, laissé à l'humain |

**La détection de cycle prend la chaîne en paramètre.** Parcourir `superseded_by` est une opération
d'I/O ; le domaine ne fait que vérifier que `old` n'y figure pas. C'est le use case qui résout la
chaîne via `MemoryRepository::supersession_chain`.

Deux variantes d'erreur dédiées dans `DomainError` : `MemoryAlreadyInvalidated(id)` et
`MemorySupersessionCycle { old, new }` — plus précises qu'un `ValidationError` et directement
testables.

#### 5.1.8 Règles du brief (`rules/brief.rs`) — R55-R57

Tout est pur : `compose_brief(&BriefInput) -> Brief` choisit et ordonne, `render_brief(&Brief) ->
Vec<String>` produit le texte. L'appelant se contente de lire la base et d'imprimer les lignes. Le
plafond vit **ici** et non dans la CLI, parce que c'est ici qu'il est testable — et parce qu'un
rendu non borné entre dans le contexte du modèle à chaque session.

| Constante | Valeur | Rôle |
|---|---|---|
| `BRIEF_MAX_LINES` | 40 | plafond dur du rendu, vérifié sur une entrée pathologique |
| `BRIEF_MAX_LINE_CHARS` | 140 | plafond par ligne : sans lui, un titre de 500 caractères passerait pour « une ligne » |
| `CONSOLIDATION_STALE_AFTER_DAYS` | 3 | seuil de l'avertissement de vétusté |
| `MAX_DEADLINE_ENTRIES` / `MAX_COMMITMENT_ENTRIES` / `MAX_DECISION_ENTRIES` | 6 / 8 / 6 | plafonds par section |
| `MEMORY_REF_MIN_CHARS` | 3 | longueur de départ de la référence courte (`m:7c1`) |

**Sélection** (`select_deadlines`, `select_commitments`, `select_decisions`) :

- échéances : tâches ouvertes non `dismissed` portant une échéance, hors fixtures de test
  (`is_test_fixture_title` : premier mot `test` ou `fixture`), **dédoublonnées par titre normalisé**
  (une récurrence matérialisée 17 fois n'occupe qu'une ligne, la plus proche), triées par
  **proximité d'aujourd'hui** — clé `(|jours|, jours, titre)`, donc le retard passe devant à
  distance égale. Trier par date pure remplirait la section de tâches en retard de 250 jours
  (le store réel en contient) en chassant l'échéance de la semaine ;
- engagements : `kind = commitment`, `is_recallable()`, **les plus anciens d'abord** ;
- décisions : `kind = decision`, `is_recallable()`, **les plus récentes d'abord**, restreintes au
  projet en focus s'il y en a un ; sinon toutes, une section vide n'apprenant rien. `fact` et
  `preference` n'entrent jamais dans le brief : ils se récupèrent à la demande.

Le filtre dur de R45 est **ré-appliqué ici** sur chaque souvenir, quoi qu'ait fourni l'appelant : un
brief ne doit pas pouvoir porter un fait supersédé, même par accident.

**Budget de lignes.** `compose_brief` réserve d'abord les lignes fixes (en-tête, section échéances,
ligne de file, avertissement, pied de page), puis sert les engagements et enfin les décisions avec ce
qui reste — la troncature part donc de la section la moins utile. `render_brief` applique en dernier
recours `enforce_line_cap`, qui coupe et **annonce** la coupe ; il ne devrait jamais se déclencher,
mais un dépassement silencieux serait une fuite que personne ne remarquerait.

**Références courtes.** `memory_reference(id, n)` coupe l'UUID **hyphéné** (et non la forme
compacte) : un préfixe de la référence reste ainsi un préfixe de la valeur stockée au-delà du
huitième caractère, ce qui rend `find_by_id_prefix` correct pour toute longueur.
`memory_reference_width` élargit la référence jusqu'à ce que toutes celles d'un même brief soient
distinctes. `parse_memory_reference` accepte `[m:7c1]`, `m:7c1`, `7c1` et l'UUID complet, refuse tout
ce qui n'est pas hexadécimal ou tiret — donc rien de ce qui vient de la ligne de commande n'atteint
un motif `LIKE`.

**`use_cases/brief.rs`** ne fait que rassembler : tâches ouvertes, souvenirs `active` et `pending`
(limite `BRIEF_SCAN_LIMIT` = 200 — les compteurs affichés sont donc exacts jusqu'à ce nombre), projet
de la tâche suivie via `ActivitySlotRepository::find_active`, et horodatage
`memory.consolidation.last_run` dans `configuration`. Une clé absente, une valeur invalide **ou un
dépôt de configuration en erreur** se lisent tous comme « jamais exécutée » : le brief rend la panne
visible sans tomber avec elle.

### 5.2 Application Layer (`crates/application`)

The application layer defines **repository traits** (interfaces) and **use case functions**. It depends only on the domain layer.

#### 5.2.1 Repository Traits

Each repository trait is defined as an async trait. Implementations live in the infrastructure layer.

```rust
// repositories/task_repository.rs

use async_trait::async_trait;
use domain::types::*;

pub struct TaskFilter {
    pub status: Option<Vec<TaskStatus>>,
    pub source: Option<Vec<Source>>,
    pub project_id: Option<ProjectId>,
    pub assignee: Option<String>,
    pub deadline_before: Option<NaiveDate>,
    pub deadline_after: Option<NaiveDate>,
    pub tag_ids: Option<Vec<TagId>>,
    pub tracking_state: Option<Vec<TrackingState>>,
    /// Exact match against `tasks.source_id` (e.g. a Jira key like "AP-123").
    /// Used by the CLI to look up a task by Jira key.
    pub source_id: Option<String>,
    /// Case-insensitive substring match against `tasks.title`.
    /// Used by the CLI's fuzzy lookup and the frontend search bar.
    pub title_contains: Option<String>,
}

#[async_trait]
pub trait TaskRepository: Send + Sync {
    async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError>;
    async fn find_by_user(
        &self, user_id: UserId, filter: &TaskFilter,
    ) -> Result<Vec<Task>, RepositoryError>;
    async fn find_by_source(
        &self, user_id: UserId, source: Source, source_id: &str,
    ) -> Result<Option<Task>, RepositoryError>;
    async fn find_by_date_range(
        &self, user_id: UserId, start: NaiveDate, end: NaiveDate,
    ) -> Result<Vec<Task>, RepositoryError>;
    async fn save(&self, task: &Task) -> Result<(), RepositoryError>;
    async fn save_batch(&self, tasks: &[Task]) -> Result<(), RepositoryError>;
    async fn delete(&self, id: TaskId) -> Result<(), RepositoryError>;
    /// Élagage post-synchronisation. DEUX REFUS font partie du contrat :
    ///   1. `keep_ids` VIDE supprime ZÉRO ligne et retourne `Ok(0)`. Un lot vide ne
    ///      porte aucune information d'obsolescence (une requête qui réussit retourne
    ///      zéro ligne pour une clé de projet mal saisie ou un droit retiré aussi bien
    ///      que pour une source réellement vide). L'ancien code supprimait ici
    ///      « toutes les tâches de la source », et `worklog_entries.task_id` est
    ///      `ON DELETE CASCADE`.
    ///   2. une tâche portant du travail consigné n'est JAMAIS supprimée : deux
    ///      `NOT EXISTS` excluent celles qui ont une ligne dans `worklog_entries`
    ///      (cascade) ou dans `activity_slots` (`ON DELETE SET NULL`, donc créneau
    ///      orphelin). Elle cesse d'être rafraîchie et survit localement.
    /// Même contrat que `GryzzlyCatalogRepository::soft_prune_missing`, qui refuse
    /// déjà un `keep_ids` vide. Le refus n'est pas une `RepositoryError` : cette
    /// énumération ne décrit que des échecs techniques (`Database`, `Serialization`).
    async fn delete_stale_by_source(
        &self, user_id: UserId, source: Source, keep_ids: &[String],
    ) -> Result<u64, RepositoryError>;
    /// Returns distinct non-null delegated_to values for a user, sorted alphabetically.
    /// Used to populate the auto-suggestion list in the delegation field.
    /// Default implementation returns an empty vec (no-op for non-SQLite backends).
    async fn list_delegates(&self, user_id: UserId) -> Result<Vec<String>, RepositoryError>;
}
```

```rust
// repositories/meeting_repository.rs

#[async_trait]
pub trait MeetingRepository: Send + Sync {
    async fn find_by_user_and_date(
        &self, user_id: UserId, date: NaiveDate,
    ) -> Result<Vec<Meeting>, RepositoryError>;
    async fn find_by_user_and_range(
        &self, user_id: UserId, start: NaiveDate, end: NaiveDate,
    ) -> Result<Vec<Meeting>, RepositoryError>;
    async fn upsert_batch(&self, meetings: &[Meeting]) -> Result<(), RepositoryError>;
    /// Comme `delete_stale_by_source` : une liste d'identifiants VIDE supprime ZÉRO
    /// ligne et retourne `Ok(0)`. Elle supprimait auparavant toutes les réunions de
    /// l'utilisateur, y compris les rattachements de projet saisis localement.
    async fn delete_stale(
        &self, user_id: UserId, current_outlook_ids: &[String],
    ) -> Result<u64, RepositoryError>;
    async fn find_by_project(
        &self, user_id: UserId, project_id: ProjectId,
    ) -> Result<Vec<Meeting>, RepositoryError>;
}
```

```rust
// repositories/activity_repository.rs

#[async_trait]
pub trait ActivitySlotRepository: Send + Sync {
    async fn find_by_user_and_date(
        &self, user_id: UserId, date: NaiveDate,
    ) -> Result<Vec<ActivitySlot>, RepositoryError>;
    async fn find_active(
        &self, user_id: UserId,
    ) -> Result<Option<ActivitySlot>, RepositoryError>;
    async fn save(&self, slot: &ActivitySlot) -> Result<(), RepositoryError>;
    async fn update(&self, slot: &ActivitySlot) -> Result<(), RepositoryError>;
    async fn delete(&self, id: ActivitySlotId) -> Result<(), RepositoryError>;
}
```

```rust
// repositories/task_link_repository.rs

#[async_trait]
pub trait TaskLinkRepository: Send + Sync {
    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<TaskLink>, RepositoryError>;
    async fn find_rejected_pairs(
        &self, user_id: UserId,
    ) -> Result<Vec<(TaskId, TaskId)>, RepositoryError>;
    async fn save(&self, link: &TaskLink) -> Result<(), RepositoryError>;
    async fn delete(&self, id: TaskLinkId) -> Result<(), RepositoryError>;
}
```

Similar traits for: `ProjectRepository`, `AlertRepository`, `TagRepository`, `SyncStatusRepository`, `ConfigRepository`.

#### 5.2.2 External Service Traits

```rust
// services/jira_client.rs

pub struct JiraTask {
    pub key: String,           // e.g., "PROJ-123"
    pub title: String,
    pub description: Option<String>,
    pub status: String,        // Raw Jira status name
    pub assignee: Option<String>,
    pub deadline: Option<NaiveDate>,
    pub priority: Option<String>,
    pub project_key: String,
    pub project_name: String,
}

#[async_trait]
pub trait JiraClient: Send + Sync {
    async fn fetch_tasks(
        &self, project_keys: &[String], assignees: Option<&[String]>,
    ) -> Result<Vec<JiraTask>, ConnectorError>;
}
```

```rust
// services/graph_token_provider.rs

/// Trait fournissant un access token Microsoft Graph toujours frais.
/// L'implémentation concrète (`RefreshingGraphTokenProvider`) gère le renouvellement
/// silencieux via le refresh token (marge 60 s, rotation incluse).
/// Ce fournisseur est partagé par les connecteurs Outlook et Excel/SharePoint.
#[async_trait]
pub trait GraphTokenProvider: Send + Sync {
    /// Retourne un access token valide, ou une erreur si le renouvellement a échoué.
    async fn valid_access_token(&self, user_id: Uuid) -> Result<String, AppError>;
    /// Indique si un refresh token est stocké (session active).
    async fn is_connected(&self) -> bool;
    /// Retourne l'adresse email du compte connecté, si disponible.
    async fn account(&self) -> Option<String>;
}
```

```rust
// services/outlook_client.rs

pub struct OutlookEvent {
    pub outlook_id: String,
    pub title: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub location: Option<String>,
    pub participants: Vec<String>,
    pub is_cancelled: bool,
}

#[async_trait]
pub trait OutlookClient: Send + Sync {
    async fn fetch_calendar(
        &self, start: NaiveDate, end: NaiveDate,
    ) -> Result<Vec<OutlookEvent>, ConnectorError>;
}
```

```rust
// services/excel_client.rs

pub struct ExcelRow {
    pub row_index: usize,
    pub columns: HashMap<String, String>,   // column_name -> cell_value
}

pub struct ExcelMappingConfig {
    pub sharepoint_path: String,
    pub sheet_name: Option<String>,
    pub title_column: String,
    pub assignee_column: Option<String>,
    pub project_column: Option<String>,
    pub date_column: Option<String>,
    pub jira_key_column: Option<String>,
    pub status_column: Option<String>,
}

#[async_trait]
pub trait ExcelClient: Send + Sync {
    async fn fetch_rows(
        &self, config: &ExcelMappingConfig,
    ) -> Result<Vec<ExcelRow>, ConnectorError>;
}
```

#### 5.2.3 Use Cases

Use cases are **async functions** that compose repository calls with domain logic. They receive trait references (dependency injection via function arguments).

```rust
// use_cases/dashboard.rs

pub struct DailyDashboard {
    pub date: NaiveDate,
    pub tasks: Vec<Task>,
    pub meetings: Vec<Meeting>,
    pub alerts: Vec<Alert>,
    pub weekly_workload: WeeklyWorkload,
    pub sync_statuses: Vec<SyncStatus>,
}

pub async fn get_daily_dashboard(
    task_repo: &dyn TaskRepository,
    meeting_repo: &dyn MeetingRepository,
    alert_repo: &dyn AlertRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    date: NaiveDate,
) -> Result<DailyDashboard, AppError> {
    let tasks = task_repo.find_by_user(user_id, &TaskFilter::for_date(date)).await?;
    let mut sorted_tasks = tasks;
    sort_tasks_by_priority(&mut sorted_tasks);

    let meetings = meeting_repo.find_by_user_and_date(user_id, date).await?;
    let alerts = alert_repo.find_unresolved(user_id).await?;
    let sync_statuses = sync_repo.find_by_user(user_id).await?;

    let week_start = week_start_of(date);
    let weekly_workload = compute_weekly_workload(
        task_repo, meeting_repo, user_id, week_start,
    ).await?;

    Ok(DailyDashboard {
        date, tasks: sorted_tasks, meetings, alerts, weekly_workload, sync_statuses,
    })
}
```

```rust
// use_cases/task_management.rs

pub struct CreateTaskInput {
    pub title: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub project_id: Option<ProjectId>,
    pub deadline: Option<NaiveDate>,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub estimated_hours: Option<f32>,
    pub impact: Option<ImpactLevel>,
    pub urgency: Option<UrgencyLevel>,
    pub tags: Vec<TagId>,
}

pub async fn create_personal_task(
    task_repo: &dyn TaskRepository,
    user_id: UserId,
    input: CreateTaskInput,
    today: NaiveDate,
) -> Result<Task, AppError> {
    let (urgency, urgency_manual) = resolve_urgency(
        input.urgency,
        input.deadline,
        today,
    );

    let task = Task {
        id: Uuid::new_v4(),
        user_id,
        title: input.title,
        description: input.description,
        source: Source::Personal,
        source_id: None,
        status: TaskStatus::Todo,
        project_id: input.project_id,
        assignee: None,
        deadline: input.deadline,
        planned_start: input.planned_start,
        planned_end: input.planned_end,
        estimated_hours: input.estimated_hours,
        urgency,
        urgency_manual,
        impact: input.impact.unwrap_or(ImpactLevel::Medium),
        tags: input.tags,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    task_repo.save(&task).await?;
    Ok(task)
}

pub async fn update_task(/* ... */) -> Result<Task, AppError> { /* ... */ }
pub async fn delete_task(/* ... */) -> Result<(), AppError> { /* ... */ }
pub async fn complete_task(/* ... */) -> Result<Task, AppError> { /* ... */ }
```

```rust
// use_cases/activity_tracking.rs

/// Start tracking a new activity. Closes the currently active slot (if any).
pub async fn start_activity(
    activity_repo: &dyn ActivitySlotRepository,
    user_id: UserId,
    task_id: Option<TaskId>,
    now: DateTime<Utc>,
) -> Result<ActivitySlot, AppError> {
    // Close active slot (R21)
    if let Some(mut active) = activity_repo.find_active(user_id).await? {
        active.end_time = Some(now);
        activity_repo.update(&active).await?;
    }

    let date = now.date_naive();
    let half_day = half_day_of(now.hour());

    let slot = ActivitySlot {
        id: Uuid::new_v4(),
        user_id,
        task_id,
        start_time: now,
        end_time: None,
        half_day,
        date,
        created_at: now,
    };

    activity_repo.save(&slot).await?;
    Ok(slot)
}

pub async fn stop_activity(/* ... */) -> Result<Option<ActivitySlot>, AppError> { /* ... */ }
pub async fn update_activity_slot(/* ... */) -> Result<ActivitySlot, AppError> { /* ... */ }
pub async fn get_activity_journal(/* ... */) -> Result<Vec<ActivitySlot>, AppError> { /* ... */ }

/// Rebuild `task_id`'s closed activity slots from its worklog entries. Superseded
/// (plan 2, § 7.3.4): this once loaded entries with `logged_at > since` and appended
/// a slot per derived block — a single global watermark that lost another task's
/// entries whenever two tasks interleaved, and duplicated slots on any re-run.
///
/// `from` is now a **selector, not a watermark**: it only picks which local
/// half-days to rebuild (paged via `WorklogFilter`, `[from, now)`), and every entry
/// of this task in those half-days — not only the ones after `from` — then decides
/// what the slots are, via `derive_time_blocks`. For each named half-day:
/// `plan_task_projection` reads the task's `source == Worklog` slots already there
/// (`is_rebuildable`) as the deletion candidates, and computes the fresh set from
/// every entry of the task in that half-day; `apply_task_projection` deletes the
/// stale slots and then writes the fresh ones, delete before write so no reader
/// sees the half-day doubled. Does **not** modify `aplan.active_task_id` — the
/// session link is preserved. Re-running is a no-op, and a backdated entry is
/// picked up as soon as its half-day is named, whatever `from` was.
///
/// Called by the `flushWorklogTime` mutation, which resolves `from` to either a
/// Claude session's own `sessions.last_flush_at` or the human's `aplan.active_since`
/// — never both.
pub async fn materialize_worklog_time(
    worklog_repo: &dyn WorklogRepository,
    activity_repo: &dyn ActivitySlotRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    task_id: TaskId,
    from: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<FlushOutcome, AppError> { /* ... */ }

/// Rebuild `task_id`'s projection for one **named local day**, advancing no watermark.
///
/// Exists beside `materialize_worklog_time` because that one *discovers* its
/// half-days from the entries in `[from, now)`, and `from` comes from a flush window
/// that starts when the session did. An entry written with a backdated `logged_at` —
/// what `aplan log --at` produces — sits before that window, so the flush never
/// learns its half-day exists and its hours never reach a slot: the entry is in the
/// journal and the day still bills zero. Here the day is passed in outright.
///
/// The half-days rebuilt are the ones this task **has entries in** on that day, never
/// simply both: a half-day named with no entry behind it puts this task's slots on the
/// delete list with nothing to rewrite them from, which is how a rebuild deletes hours
/// (the same hazard `repair_orphaned_slots` intersects against). An empty result is a
/// success — the task logged nothing that day — not a refusal, because the caller has
/// just written the operator's entry and must not report a failure.
///
/// Called by the `rebuildWorklogProjection` mutation (`aplan log --at`,
/// `aplan slots rebuild`). Idempotent, like every path built on
/// `plan_task_projection`.
pub async fn rebuild_task_local_date(
    worklog_repo: &dyn WorklogRepository,
    activity_repo: &dyn ActivitySlotRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    task_id: TaskId,
    date: NaiveDate,
    now: DateTime<Utc>,
) -> Result<DayRebuildOutcome, AppError> { /* ... */ }
```

```rust
// time.rs

/// The UTC instant a local wall-clock reading names — the inverse of `to_local`, and
/// the only one. A caller that names a past moment in local terms
/// (`aplan log --at 2026-08-06T14:30`, arriving as `addWorklogEntry`'s
/// `loggedAtLocal`) must land on the instant `to_local` maps back to that reading, or
/// the entry documents a different half-day than the operator typed.
///
/// The two readings with no single answer are resolved as `local_day_start` resolves
/// them: an **ambiguous** hour (DST fall-back) takes the earliest of its two instants,
/// so the conversion stays a function; a **nonexistent** one (spring-forward gap) is
/// walked forward an hour rather than reinterpreted as UTC, which would be off by the
/// zone's whole offset and could move the entry to another day.
pub fn local_to_utc(tz: Tz, local: NaiveDateTime) -> DateTime<Utc> { /* ... */ }
```

```rust
// use_cases/sync.rs

pub struct SyncResult {
    pub source: Source,
    pub tasks_created: usize,
    pub tasks_updated: usize,
    pub tasks_removed: usize,
    pub meetings_synced: usize,
    pub errors: Vec<String>,
}

pub async fn sync_jira(
    jira_client: &dyn JiraClient,
    task_repo: &dyn TaskRepository,
    project_repo: &dyn ProjectRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    config: &JiraConfig,
) -> Result<SyncResult, AppError> { /* ... */ }
// NOTE: champs jamais touchés par le sync Jira (préservés à chaque synchro) :
//   - notes (markdown utilisateur)
//   - delegated_to (délégation utilisateur)
//   - urgency_manual (override de priorité)
//   - remaining_hours_override / estimated_hours_override (overrides de temps)
//
// ÉLAGAGE (R07b/R07c/R07d) — trois garanties, chacune ayant son test :
//   1. GARDE « FETCH VIDE » : si `fetch_tasks` réussit en retournant zéro tâche,
//      `delete_stale_by_source` n'est PAS appelé et le motif est poussé dans
//      `errors` (donc dans `sync_status.error_message`). Un fetch vide ne dit rien
//      de l'obsolescence ; un échec dur du connecteur, lui, a déjà interrompu la
//      fonction plus haut. Même forme que la garde de `sync_gryzzly`.
//   2. L'échec de l'élagage est TOLÉRÉ mais JAMAIS avalé : il est poussé dans
//      `errors` comme les autres échecs tolérés de la fonction, et n'interrompt pas
//      la synchronisation (les tâches déjà écrites restent). L'ancien
//      `.unwrap_or(0)` rapportait « 0 tâche supprimée » sur un élagage en échec.
//   3. Le dépôt refuse en plus de supprimer une tâche portant du travail consigné.
// `sync_outlook` porte la même garde « fetch vide » avant `delete_stale`.
// `sync_excel` n'élague pas du tout : il n'appelle aucun `delete_stale_*` et
// retourne toujours `tasks_removed = 0`.

pub async fn sync_outlook(
    outlook_client: &dyn OutlookClient,
    meeting_repo: &dyn MeetingRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    date_range: (NaiveDate, NaiveDate),
) -> Result<SyncResult, AppError> { /* ... */ }

pub async fn sync_excel(
    excel_client: &dyn ExcelClient,
    task_repo: &dyn TaskRepository,
    project_repo: &dyn ProjectRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    config: &ExcelMappingConfig,
) -> Result<SyncResult, AppError> { /* ... */ }

pub async fn sync_all(/* ... */) -> Result<Vec<SyncResult>, AppError> { /* ... */ }
```

**`use_cases/memory.rs` — mémoire sémantique**

```rust
pub struct RememberInput { /* kind, title, body, occurred_at, source, source_ref,
                             confirmed, proposed_supersedes, project_id, task_id,
                             stakeholders */ }
// `proposed_supersedes: Option<MemoryId>` — DÉJÀ résolu par l'appelant : la
// référence écrite par la consolidation est un préfixe court, et
// `resolve_memory_id` est le seul endroit qui les transforme en identifiants.

pub async fn remember(
    memory_repo: &dyn MemoryRepository,
    user_id: UserId,
    input: RememberInput,
    now: DateTime<Utc>,
) -> Result<Memory, AppError>;

pub async fn get_memory(
    memory_repo: &dyn MemoryRepository, user_id: UserId, id: MemoryId,
) -> Result<Option<Memory>, AppError>;

/// Reçoit la saisie BRUTE et la convertit via `domain::rules::recall::build_match_query`.
/// C'est le seul point de conversion : une erreur ici signifie « rien de recherchable »,
/// jamais « FTS5 a planté ».
pub async fn search_memories(
    retriever: &dyn MemoryRetriever,
    user_id: UserId,
    raw_query: &str,
    context: RecallContext,
    include_history: bool,
    limit: u32,
    now: DateTime<Utc>,
) -> Result<Vec<ScoredMemory>, AppError>;

pub async fn list_pending_memories(
    memory_repo: &dyn MemoryRepository, user_id: UserId, limit: u32, offset: u32,
) -> Result<Vec<Memory>, AppError>;
```

Import et cycle de vie (lots 2 et 3) :

```rust
/// Idempotent : ignore tout fichier dont la référence de provenance existe déjà.
pub async fn import_memories(
    memory_repo: &dyn MemoryRepository,
    file_source: &dyn MemoryFileSource,
    user_id: UserId,
    directory: &str,
    now: DateTime<Utc>,
) -> Result<MemoryImportOutcome, AppError>;

pub enum AcceptOutcome { Accepted(Memory), NearDuplicates { candidate: Memory, duplicates: Vec<Memory> } }

/// Refuse l'ajout muet : sans `force`, un quasi-doublon actif renvoie
/// `NearDuplicates` et **rien n'est écrit**.
pub async fn accept_candidate(/* repo, retriever, user_id, id, kind_override, force, now */)
    -> Result<AcceptOutcome, AppError>;
pub async fn reject_candidate(/* repo, user_id, id */) -> Result<Memory, AppError>;
pub async fn merge_candidate(/* repo, user_id, candidate_id, into_id */)
    -> Result<MergeOutcome, AppError>;
/// Sert `aplan inbox supersede` ET `aplan memory supersede`. Seul chemin qui
/// écrit `invalidated_at`.
pub async fn supersede_memory(/* repo, user_id, old_id, successor_id, now */)
    -> Result<SupersedeOutcome, AppError>;

/// Le souvenir qu'une supersession de file doit invalider quand l'appelant n'en a
/// nommé aucun : la proposition portée par le candidat lui-même (§ 7.2.1). C'est
/// ce qui réduit `aplan inbox supersede <id> --replaces <old>` à
/// `aplan inbox supersede <id>`. Un candidat sans proposition est un refus de
/// précondition (`AppError::Validation`, code 4), jamais un no-op silencieux :
/// lire « superseded » sans que rien ne soit invalidé serait le pire résultat.
pub async fn proposed_supersession_target(/* repo, user_id, candidate_id */)
    -> Result<MemoryId, AppError>;
```

Brief et références courtes (lot 4) :

```rust
pub enum MemoryLookup { Found(Memory), NotFound, Ambiguous(Vec<Memory>) }

/// Résout un UUID complet OU la référence courte du brief (`m:7c1`, `[m:7c1]`, `7c1`).
/// Un préfixe qui correspond à plusieurs souvenirs retourne `Ambiguous` : deviner
/// reviendrait à déplier un souvenir que le lecteur ne visait pas.
pub async fn resolve_memory(
    memory_repo: &dyn MemoryRepository, user_id: UserId, token: &str,
) -> Result<MemoryLookup, AppError>;

/// Même résolution, pour les verbes qui ÉCRIVENT : les deux issues sur lesquelles
/// une mutation ne peut pas agir deviennent des erreurs, avant toute écriture.
/// Inconnu -> `AppError::NotFound` (code 2) ; ambigu -> `AppError::Ambiguous`
/// (code 3) portant la liste des candidats. Résolveur UNIQUE, partagé avec le
/// chemin de lecture : la référence courte est le seul identifiant affiché, donc
/// tout verbe qui prend un identifiant doit l'accepter.
pub async fn resolve_memory_id(
    memory_repo: &dyn MemoryRepository, user_id: UserId, token: &str,
) -> Result<MemoryId, AppError>;

/// Les DEUX références d'un verbe qui touche deux souvenirs, résolues avant que
/// l'une d'elles ne serve : une supersession à moitié appliquée masquerait un fait
/// sans successeur, et une fusion effacerait un candidat dans le vide.
pub async fn resolve_memory_id_pair(
    memory_repo: &dyn MemoryRepository, user_id: UserId, first: &str, second: &str,
) -> Result<(MemoryId, MemoryId), AppError>;

/// Formulation UNIQUE de l'ambiguïté (un candidat par ligne, identifiants
/// complets, plafonnée à 5), partagée par la lecture et l'écriture.
pub fn describe_ambiguous_memory(token: &str, candidates: &[Memory]) -> String;

// use_cases/brief.rs
pub const CONSOLIDATION_LAST_RUN_KEY: &str = "memory.consolidation.last_run";
pub const BRIEF_SCAN_LIMIT: u32 = 200;

pub struct BriefRequest { /* variant, project_id, today, now */ }

pub async fn build_brief(
    task_repo: &dyn TaskRepository,
    memory_repo: &dyn MemoryRepository,
    activity_repo: &dyn ActivitySlotRepository,
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    request: BriefRequest,
) -> Result<Brief, AppError>;
```

Consolidation (lot 5) — la machinerie déterministe que pilote la session planifiée. **Aucun client
LLM, aucune clé d'API, aucun *prompt* dans le backend** : c'est la frontière que ce lot préserve.

```rust
// use_cases/consolidation.rs

/// Réexportée, pas redéclarée : `brief` LIT cette clé, ce module l'ÉCRIT. Deux
/// constantes de noms voisins feraient afficher « jamais exécutée » indéfiniment
/// pendant que le job enregistrerait consciencieusement chaque passage.
pub use crate::use_cases::brief::CONSOLIDATION_LAST_RUN_KEY;

pub const CONSOLIDATION_BATCH_LIMIT: u32 = 200;

pub struct MarkConsolidatedOutcome { /* requested, marked, consolidated_at */ }

/// Entrées jamais consolidées, **de la plus ancienne à la plus récente**.
/// `limit = 0` → `CONSOLIDATION_BATCH_LIMIT` ; la borne dure est résolue par
/// `WorklogFilter::effective_limit()`.
pub async fn list_unconsolidated_entries(
    worklog_repo: &dyn WorklogRepository, user_id: UserId, limit: u32,
) -> Result<Vec<WorklogEntry>, AppError>;

/// Pose le filigrane. Une liste vide est un no-op **sans erreur** : un passage qui
/// n'a rien trouvé doit finir proprement, sinon on apprend au job à ignorer ses
/// propres échecs.
pub async fn mark_entries_consolidated(
    worklog_repo: &dyn WorklogRepository, user_id: UserId,
    ids: &[WorklogEntryId], now: DateTime<Utc>,
) -> Result<MarkConsolidatedOutcome, AppError>;

/// Écrit la date du passage dans `configuration`, au format RFC 3339 — celui que
/// le brief reparse.
pub async fn record_consolidation_run(
    config_repo: &dyn ConfigRepository, user_id: UserId, at: DateTime<Utc>,
) -> Result<DateTime<Utc>, AppError>;
```

Le trait `WorklogRepository` reçoit les deux méthodes correspondantes,
`list_unconsolidated(user_id, &WorklogFilter)` et
`mark_consolidated(user_id, &[WorklogEntryId], at) -> u64`, toutes deux avec une **implémentation
par défaut qui échoue explicitement** : un double de test qui ne les implémente pas ne doit pas
pouvoir faire croire au job qu'il n'y a rien à consolider. `WorklogFilter` gagne
`effective_limit()` (`0` → `WORKLOG_FILTER_DEFAULT_LIMIT` = 200, plafond
`WORKLOG_FILTER_MAX_LIMIT` = 1 000), et **les deux** méthodes de listage lient cette valeur : un
`LIMIT 0` renverrait une page vide sans erreur, indiscernable de « plus rien à consolider ».

Traits associés : `repositories::MemoryRepository` (`create` / `find_by_id` /
`find_by_id_prefix` / `list` / `update` / `apply_merge` / `apply_supersession` /
`existing_source_refs` / `supersession_chain`), `services::MemoryRetriever`
(`search(user_id, RecallQuery, now)`) et
`services::MemoryFileSource` (`list(directory)`, **lecture seule** par contrat).
`now` est injecté pour que la décroissance de récence reste déterministe en test.
Bornes : `MEMORY_LIST_DEFAULT_LIMIT` 50 / `MEMORY_LIST_MAX_LIMIT` 500,
`RECALL_DEFAULT_LIMIT` 10 / `RECALL_MAX_LIMIT` 100, `DUPLICATE_SCAN_LIMIT` 25.
La résolution de ces bornes est portée par `MemoryListFilter::effective_limit()` et
`RecallQuery::effective_limit()` (`0` → défaut, au-delà du plafond → plafond) : les
implémentations lient **cette** valeur, jamais le champ `limit` brut. Un filtre construit
par `Default` porte `limit: 0`, et un `LIMIT 0` renvoie une liste vide **sans erreur**.

`find_by_id_prefix` porte une **implémentation par défaut qui échoue explicitement**, afin que les
doubles de test qui ne résolvent jamais de référence continuent de compiler sans qu'un préfixe non
implémenté puisse retourner « rien trouvé » en silence.

#### 5.2.4 Application Errors

```rust
// errors.rs

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Domain error: {0}")]
    Domain(#[from] DomainError),

    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("Connector error: {source} -- {message}")]
    Connector { source: Source, message: String },

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),

    /// Une référence courte correspond à plusieurs lignes : agir dessus serait
    /// deviner. Porte le message complet, candidats compris — l'appelant l'affiche
    /// tel quel (le CLI l'affiche sur stderr et sort en code 3). Pas de préfixe
    /// dans le rendu : le message EST le contrat inter-couches.
    #[error("{0}")]
    Ambiguous(String),
}

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("HTTP error: {status} -- {message}")]
    Http { status: u16, message: String },

    #[error("Authentication failed for {source}")]
    AuthFailed { source: String },

    #[error("Network unreachable: {0}")]
    NetworkError(String),

    #[error("Parsing error: {0}")]
    ParseError(String),
}
```

### 5.3 Infrastructure Layer (`crates/infrastructure`)

#### 5.3.1 Database Connection

```rust
// database/connection.rs

use sqlx::sqlite::SqlitePool;

pub async fn create_sqlite_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePool::connect(database_url).await?;
    sqlx::migrate!("../../migrations/sqlite").run(&pool).await?;
    Ok(pool)
}
```

#### 5.3.2 Repository Implementations

Each repository implementation wraps a `SqlitePool` (or `PgPool`) and implements the corresponding trait from the application layer. Queries use `sqlx::query!` or `sqlx::query_as!` macros for compile-time verification.

Example pattern for all repositories:

```rust
// database/task_repo.rs

pub struct SqliteTaskRepository {
    pool: SqlitePool,
}

pub fn new_sqlite_task_repository(pool: SqlitePool) -> SqliteTaskRepository {
    SqliteTaskRepository { pool }
}

#[async_trait]
impl TaskRepository for SqliteTaskRepository {
    async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError> {
        let id_str = id.to_string();
        let row = sqlx::query_as!(
            TaskRow,
            "SELECT * FROM tasks WHERE id = ?",
            id_str
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(row.map(task_row_to_domain))
    }

    // ... other methods follow the same pattern
}

/// Map a database row to a domain Task. Pure function.
fn task_row_to_domain(row: TaskRow) -> Task { /* ... */ }

/// Map a domain Task to database values. Pure function.
fn task_to_row(task: &Task) -> TaskRow { /* ... */ }
```

**`database/memory_repo.rs` — `SqliteMemoryRepository` + `SqliteMemoryRetriever`**

- `create` ouvre **une seule transaction** pour les trois écritures : `memories`,
  `memory_stakeholders`, puis `memories_fts`. La table FTS est autonome et sans triggers : sans
  cette atomicité, un souvenir pourrait exister sans jamais être retrouvable (§ 7.2).
- La recherche joint l'index et la table :
  `SELECT m.*, bm25(memories_fts) AS bm25_score FROM memories_fts JOIN memories m ON m.id = memories_fts.memory_id WHERE memories_fts MATCH ? AND m.user_id = ?`,
  plus `AND m.invalidated_at IS NULL AND m.status = 'active'` sauf `include_history`,
  puis `ORDER BY bm25_score ASC` (le plus négatif d'abord) `LIMIT ?`.
- **Le SQL ne produit qu'une fenêtre de candidats**, pas le classement final : BM25 ignore les
  bonus d'entité et de récence, donc le dépôt sur-échantillonne (`CANDIDATE_OVERFETCH = 5`,
  plafond `CANDIDATE_MAX = 500`), puis délègue le tri à `domain::rules::recall::rank` et tronque
  à la limite demandée.
- Les personnes concernées sont chargées en **une** requête (`WHERE memory_id IN (…)`) **avant**
  le scoring, faute de quoi le bonus d'entité lirait une liste vide.
- Les pools de test activent `foreign_keys(true)` avec `max_connections(1)` (§ 7.2).
- **Chaque chemin d'écriture réécrit la ligne FTS dans sa propre transaction.** Aucun trigger
  n'existe : `update` fait `DELETE` + `INSERT` sur `memories_fts` (sinon un souvenir retitré
  resterait trouvable sous son ancienne formulation, et introuvable sous la nouvelle), et tout
  chemin de suppression doit effacer la ligne d'index à la main — **aucune clé étrangère ne la
  relie** à `memories`.
- `apply_merge` et `apply_supersession` sont chacun **une seule transaction** (R50).
  `apply_supersession` écrit le **successeur d'abord** : `memories.superseded_by` est une vraie
  clé étrangère, la ligne pointée doit donc satisfaire la contrainte dans la même transaction.
- `supersession_chain` parcourt `superseded_by` avec un ensemble de visités et un plafond
  (`SUPERSESSION_CHAIN_MAX` = 100) : une boucle déjà présente en base doit produire une chaîne
  finie, pas une requête qui ne rend jamais la main.
- `existing_source_refs` échappe les métacaractères `LIKE` (`%`, `_`, `\`) avec
  `ESCAPE '\'` — un préfixe contenant `_` matcherait sinon n'importe quel caractère.
- `find_by_id_prefix` (référence courte du brief) fait `WHERE id LIKE ? ESCAPE '\'` sur le préfixe
  **échappé et minuscule**, borné par `limit.max(1)` — un `LIMIT 0` renverrait « non trouvé » au
  lieu du souvenir. Les tests couvrent `%`, `_` et `7%` : un métacaractère saisi ne doit jamais
  remonter toute la table.
- `connectors/memory_files/` — `FsMemoryFileSource` lit le dossier de mémoire du harness via
  `tokio::fs` : uniquement les `*.md`, sous-dossiers exclus, triés par nom (l'ordre de `read_dir`
  est indéfini), et un fichier illisible est ignoré avec un avertissement plutôt que de faire
  échouer l'import entier. **Aucun appel d'écriture** : le dossier a déjà un écrivain (§ 7.2).

#### 5.3.3 External API Clients

**Jira Client:**

```rust
// connectors/jira/client.rs

pub struct JiraHttpClient {
    http: reqwest::Client,
    base_url: String,
    auth_token: String,
}

pub fn new_jira_client(base_url: String, auth_token: String) -> JiraHttpClient {
    JiraHttpClient {
        http: reqwest::Client::new(),
        base_url,
        auth_token,
    }
}

#[async_trait]
impl JiraClient for JiraHttpClient {
    async fn fetch_tasks(
        &self, project_keys: &[String], assignees: Option<&[String]>,
    ) -> Result<Vec<JiraTask>, ConnectorError> {
        // Build JQL query
        // GET /rest/api/3/search?jql=...
        // Map response to Vec<JiraTask>
    }
}
```

**Microsoft Graph Client (Outlook + Excel):**

```rust
// connectors/outlook/client.rs

pub struct GraphOutlookClient {
    http: reqwest::Client,
    access_token: String,
}

#[async_trait]
impl OutlookClient for GraphOutlookClient {
    async fn fetch_calendar(
        &self, start: NaiveDate, end: NaiveDate,
    ) -> Result<Vec<OutlookEvent>, ConnectorError> {
        // GET /me/calendarView?startDateTime=...&endDateTime=...
        // Map response to Vec<OutlookEvent>
    }
}
```

```rust
// connectors/excel/client.rs

pub struct GraphExcelClient {
    http: reqwest::Client,
    access_token: String,
}

#[async_trait]
impl ExcelClient for GraphExcelClient {
    async fn fetch_rows(
        &self, config: &ExcelMappingConfig,
    ) -> Result<Vec<ExcelRow>, ConnectorError> {
        // GET /sites/{site-id}/drive/items/{item-id}/workbook/worksheets/{sheet}/usedRange
        // Parse table structure using config mapping
        // Map rows to Vec<ExcelRow>
    }
}
```

### 5.4 API Layer (`crates/api`)

#### 5.4.1 Axum Server Setup

```rust
// main.rs

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::init();

    let db_pool = create_sqlite_pool(&env("DATABASE_URL")).await.unwrap();

    // Build repository instances
    let task_repo = Arc::new(new_sqlite_task_repository(db_pool.clone()));
    let meeting_repo = Arc::new(new_sqlite_meeting_repository(db_pool.clone()));
    // ... other repos

    // Build external clients
    let jira_client = Arc::new(new_jira_client(/* ... */));
    let outlook_client = Arc::new(new_graph_outlook_client(/* ... */));
    let excel_client = Arc::new(new_graph_excel_client(/* ... */));

    // Build broadcast channels for subscriptions
    let (sync_tx, _) = broadcast::channel::<SyncEvent>(100);
    let (reminder_tx, _) = broadcast::channel::<ActivityReminder>(100);
    let (alerts_tx, _) = broadcast::channel::<Vec<Alert>>(100);

    // Build GraphQL schema
    let schema = build_schema(task_repo, meeting_repo, /* ... */);

    // Build Axum router
    let app = Router::new()
        .route("/graphql", post(graphql_handler).get(graphql_playground))
        .route("/graphql/sse", get(graphql_sse_handler))
        .layer(CorsLayer::permissive())
        .layer(auth_middleware())
        .with_state(AppState { schema });

    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    tracing::info!("Server running on {}", addr);
    axum::serve(TcpListener::bind(addr).await.unwrap(), app).await.unwrap();
}
```

#### 5.4.2 GraphQL Schema Construction

```rust
// graphql/schema.rs

use async_graphql::Schema;

pub type AppSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

pub fn build_schema(
    task_repo: Arc<dyn TaskRepository>,
    meeting_repo: Arc<dyn MeetingRepository>,
    // ... all repositories and services
) -> AppSchema {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .data(task_repo)
        .data(meeting_repo)
        // ... register all dependencies
        .finish()
}
```

#### 5.4.3 Resolver Pattern

All resolvers follow the same pattern: extract `user_id` from context, call the use case function, return the result.

```rust
// graphql/query.rs

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn daily_dashboard(
        &self,
        ctx: &Context<'_>,
        date: NaiveDate,
    ) -> Result<DailyDashboardGql> {
        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let meeting_repo = ctx.data::<Arc<dyn MeetingRepository>>()?;
        let alert_repo = ctx.data::<Arc<dyn AlertRepository>>()?;
        let sync_repo = ctx.data::<Arc<dyn SyncStatusRepository>>()?;

        use_cases::get_daily_dashboard(
            task_repo.as_ref(),
            meeting_repo.as_ref(),
            alert_repo.as_ref(),
            sync_repo.as_ref(),
            *user_id,
            date,
        ).await.map(Into::into).map_err(Into::into)
    }

    // ... other queries follow the same pattern
}
```

#### 5.4.4 Subscription Implementation

Subscriptions use `async-graphql`'s `Stream` type with `tokio::sync::broadcast` channels.

```rust
// graphql/subscription.rs

pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    async fn sync_progress(
        &self,
        ctx: &Context<'_>,
    ) -> impl Stream<Item = SyncEventGql> {
        let rx = ctx.data::<broadcast::Sender<SyncEvent>>()
            .unwrap()
            .subscribe();
        BroadcastStream::new(rx).filter_map(|r| r.ok().map(Into::into))
    }

    async fn activity_reminder(
        &self,
        ctx: &Context<'_>,
    ) -> impl Stream<Item = ActivityReminderGql> {
        let rx = ctx.data::<broadcast::Sender<ActivityReminder>>()
            .unwrap()
            .subscribe();
        BroadcastStream::new(rx).filter_map(|r| r.ok().map(Into::into))
    }

    async fn alerts_updated(
        &self,
        ctx: &Context<'_>,
    ) -> impl Stream<Item = Vec<AlertGql>> {
        let rx = ctx.data::<broadcast::Sender<Vec<Alert>>>()
            .unwrap()
            .subscribe();
        BroadcastStream::new(rx)
            .filter_map(|r| r.ok().map(|alerts| alerts.into_iter().map(Into::into).collect()))
    }
}
```

The SSE transport is handled by `async-graphql-axum`:

```rust
// In main.rs route setup
.route("/graphql/sse", get(async_graphql_axum::GraphQLSubscription::new(schema)))
```

#### 5.4.5 Auth Middleware

```rust
// middleware/auth.rs

/// In local mode: always injects a default user_id.
/// In Teams mode: validates Azure AD JWT and extracts user_id from claims.
pub fn auth_middleware() -> impl Layer {
    // Local mode: inject default UserId from env or create one
    // Teams mode: validate Bearer token, extract oid claim as UserId
}
```

---

## 6. Frontend Architecture

### 6.1 urql Client Setup

```typescript
// lib/urql-client.ts

import { Client, cacheExchange, fetchExchange, subscriptionExchange } from 'urql'
import { createClient as createSSEClient } from 'graphql-sse'

const sseClient = createSSEClient({
  url: 'http://localhost:3001/graphql/sse',
})

export const urqlClient = new Client({
  url: 'http://localhost:3001/graphql',
  exchanges: [
    cacheExchange,
    fetchExchange,
    subscriptionExchange({
      forwardSubscription: (operation) => ({
        subscribe: (sink) => ({
          unsubscribe: sseClient.subscribe(operation, sink),
        }),
      }),
    }),
  ],
})
```

### 6.2 GraphQL Codegen

```typescript
// codegen.ts

import type { CodegenConfig } from '@graphql-codegen/cli'

const config: CodegenConfig = {
  schema: 'http://localhost:3001/graphql',
  documents: 'src/graphql/**/*.graphql',
  generates: {
    'src/generated/graphql.ts': {
      plugins: [
        'typescript',
        'typescript-operations',
        'typescript-urql',
      ],
    },
  },
}

export default config
```

This generates:
- TypeScript types for all GraphQL types (Task, Meeting, Alert, etc.)
- Typed hooks for all queries, mutations, and subscriptions (`useDailyDashboardQuery`, `useCreateTaskMutation`, etc.)

### 6.3 Pages and Routing

```typescript
// App.tsx

const router = createBrowserRouter([
  {
    path: '/',
    element: <PageLayout />,
    children: [
      { index: true, element: <DashboardPage /> },
      { path: 'triage', element: <TriagePage /> },
      { path: 'priority', element: <PriorityMatrixPage /> },
      { path: 'workload', element: <WorkloadPage /> },
      { path: 'activity', element: <ActivityJournalPage /> },
      { path: 'memory', element: <MemoryPage /> },
      { path: 'settings', element: <SettingsPage /> },
      // v2
      { path: 'team', element: <TeamPage /> },
      { path: 'project/:id', element: <ProjectPage /> },
      { path: 'retrospective', element: <RetrospectivePage /> },
    ],
  },
])
```

### 6.4 Page Specifications

#### DashboardPage (`/`)

The default view. Displays 4 zones as described in US-010:

| Zone | Component | Data Source |
|------|-----------|-------------|
| Followed tasks | `<TaskList>` | `dailyDashboard.tasks` (filtered to `trackingState: FOLLOWED`, sorted by priority) |
| Meetings | `<MeetingList>` | `dailyDashboard.meetings` (sorted by time) |
| Weekly workload | `<WorkloadChart>` | `dailyDashboard.weeklyWorkload` (Recharts bar chart) |
| Alerts | `<AlertPanel>` | `dailyDashboard.alerts` (grouped by severity) |

Additional elements:
- `<SyncStatusBar>` -- Last sync time per source, manual sync button
- `<DateNavigator>` -- Navigate between days (US-011)
- `<ActivitySwitcher>` -- Quick task selection for activity tracking (floating or sidebar)
- `<TaskQuickAdd>` -- Inline task creation

#### TriagePage (`/triage`)

Two-column drag-and-drop interface for task triage (US-042):
- **Inbox column** (amber accent): Tasks with `trackingState: INBOX`, sorted by status then date
- **Following column** (green accent): Tasks with `trackingState: FOLLOWED`
- `@dnd-kit/core` for drag-and-drop between columns (`DndContext`, `useDraggable`, `useDroppable`, `DragOverlay`)
- Each task card shows: Jira key (`sourceId`), title, status badge, assignee, deadline
- Dismiss button (×) on each inbox card calls `setTrackingState(taskId, DISMISSED)`
- "Follow All" button calls `setTrackingStateBatch` for all inbox tasks
- Dashboard only shows tasks with `trackingState: FOLLOWED`

#### PriorityMatrixPage (`/priority`)

2x2 grid with drag-and-drop (US-020, US-021):
- Four `<QuadrantColumn>` components arranged in a grid
- Each quadrant contains `<TaskCard>` components
- `@dnd-kit` for drag-and-drop between quadrants
- Dropping a task into a different quadrant updates its urgency/impact via mutation
- Tasks within a quadrant are sorted by deadline

#### WorkloadPage (`/workload`)

Week view of capacity consumption (US-051):
- `<WeekNavigator>` -- Previous/next week, "Today" button
- `<HalfDayGrid>` -- 5 columns (Mon-Fri) x 2 rows (Morning/Afternoon)
- Each cell shows meetings and tasks assigned to that half-day
- Color coding: green (free), yellow (partially used), red (full/overloaded)
- `<WorkloadChart>` -- Recharts stacked bar chart showing capacity vs. load

#### ActivityJournalPage (`/activity`)

Timeline of the day's activity (US-032):
- `<ActivityTimeline>` -- Vertical timeline with colored blocks per task
- `<SlotEditor>` -- Click a slot to edit start/end time or change task
- Add missing slot button
- Day navigation
- Summary: time tracked vs. untracked

#### SettingsPage (`/settings`)

Configuration interface (section 15):
- Jira connection: URL, API token, project keys
- Microsoft Graph : section informative indiquant que l'authentification est gérée par la porte de connexion au démarrage (« Connecté via la porte de connexion Microsoft »). Le bouton « Se déconnecter » est accessible dans l'en-tête de l'application. Le champ de saisie manuelle du token access Graph précédent est supprimé.
- Excel mapping: SharePoint path, column mapping
- Sync frequency
- Weekly capacity
- Activity reminder settings
- Deadline alert threshold

#### MemoryPage (`/memory`)

Semantic-memory cockpit — the web face of section 6.10 of the functional spec (US-097). No backend
change: every operation already exists on the GraphQL schema.

- **Layout**: `MemoryBriefBar` (counters + consolidation health) → `MemorySearch` (`recall`) →
  validation queue of `PendingMemoryCard` → `MemoryImportPanel`. `MemoryPickerDialog` and
  `RememberSheet` are mounted at the page root.
- **Hooks** (`src/hooks/use-memory.ts`):
  - `useMemoryQueue()` — `pendingMemories` + `brief`, and the verdicts `accept` / `forceAccept` /
    `reject` / `mergeInto` / `supersede`, plus `remember` and `importDirectory`. A verdict that
    lands refetches both queries `network-only`; a verdict the backend **refused** writes nothing
    and stores the returned `nearDuplicates` under the candidate id, which is what puts the card in
    arbitration.
  - `useMemoryRecall()` — one search, `pause`d until the caller actually searches (an empty match
    expression is refused by the backend, exit 4 on the CLI). Instantiated twice: page search and
    picker search.
  - `useMemoryCapture()` — `remember` alone, for the dashboard selection chip.
- **GraphQL variables**: `acceptMemory.force` is `Boolean! = false` in the schema, so the variable
  must be declared `$force: Boolean! = false` — a nullable variable cannot feed a non-null argument.
  `supersedeMemory.old` stays nullable on purpose: left null, the backend falls back to the
  candidate's `proposedSupersedes`.
- **Default import directory**: `MEMORY_IMPORT_DEFAULT_DIR` in `src/lib/constants.ts`, overridable
  with `VITE_MEMORY_IMPORT_DIR`. Resolved by the **backend**, so it must be absolute — nothing
  expands a leading `~`.

#### Memory stacking layers (`src/lib/memory/layers.ts`)

`TASK_SHEET_Z` (50) · `CAPTURE_CHIP_Z` (60) · `MEMORY_BACKDROP_Z` (70) · `MEMORY_SHEET_Z` (80),
applied as inline `zIndex` rather than Tailwind classes so the values are readable from a test (a
`z-[${n}]` template would never reach Tailwind's JIT).

The memory sheet must be **strictly** above the task sheets, not equal to them: `SearchProvider`
renders `TaskEditSheet` *after* `{children}`, so at an equal z-index DOM order hands the win to the
task sheet — whose panel (`max-w-2xl`) then covers the memory sheet (`max-w-xl`) completely. The
symptom is a greyed screen with nothing on it: the memory backdrop paints, the memory panel is
hidden behind the task panel.

#### SelectionToMemory (mounted in `DashboardPage`)

Turns any text selection into a memory. Offers a capture on `mouseup` / `keyup` — the events that
fire *after* the browser has settled the selection (`keydown` fires before it and reads stale
values) — and renders a floating chip when the trimmed selection exceeds `MIN_SELECTION_LENGTH` (4).

`chipPosition()` validates the range geometry before deriving coordinates, and returns `null` — no
chip — when there is none to trust: a zero-area rect (a range whose nodes a re-render replaced still
stringifies, but its rect collapses to 0×0 at the origin) or a rect that does not intersect the
viewport (an inner container scrolled after the selection was made). Both used to yield a position:
`rect.bottom + 8` of a zeroed rect is `8px`, which parked the chip in the **top-left corner of the
screen**. The position is then clamped to the viewport, flipping above the selection when the bottom
lacks room. A `selectionchange` listener drops a stale capture — a re-render can destroy the
selection with no click and no keystroke — but never creates one, since it fires throughout a
drag-select and the chip would follow the cursor. The chip sits at `z-[60]` — **above** every sheet backdrop (`z-40`) and
sheet (`z-50`): a selection made inside an open sheet must stay capturable. The task is resolved by
walking up to the closest `[data-task-id]` ancestor, an attribute `TaskCard` now carries on both
variants. `TaskCard`'s root click is guarded by `isCompletingASelection()`, so selecting card text no
longer opens the edit sheet. `splitSelection()` (`src/lib/memory/selection.ts`) cuts the title at the
last sentence end that fits 120 characters and keeps the whole selection in the body when it has to
elide.

### 6.5 Key Components

#### TaskCard

Displays a single task across all views.

Props: `task: Task`

Content:
- Source badge (Jira icon, Excel icon, Personal icon)
- Title
- Priority indicator (colored dot or border based on quadrant)
- Deadline (with color: red if overdue, orange if close)
- Assignee (if present)
- Project name (if present)
- Tags (colored chips)

#### ActivitySwitcher

Lightweight popup for quick activity tracking (US-030).

Triggered by:
- Post-meeting subscription event
- Periodic reminder subscription event
- Manual "Change task" button

Content:
- List of in-progress tasks (filterable)
- "No task / break" option
- One-click selection fires `startActivity` mutation

#### DeduplicationPanel

Shown when deduplication suggestions exist (US-004).

Content:
- List of suggested matches with confidence score
- Side-by-side comparison of the two tasks
- Accept / Reject buttons per suggestion
- "Don't suggest again" option (saves rejection)

### 6.6 Global Search

#### 6.6.1 Backend — `searchableTasks` query

A lean projection type `SearchableTask` is exposed specifically for the search feature. It avoids fetching heavyweight fields (worklog, activity slots, etc.) that are irrelevant during a search interaction.

```graphql
type SearchableTask {
  id: ID!
  title: String!
  sourceId: String
  source: Source!
  assignee: String
  projectName: String          # Resolved from the related Project row
  tags: [String!]!             # Tag names (not IDs) for direct Fuse.js indexing
  description: String
  status: TaskStatus!
}

type Query {
  # ... existing queries ...
  searchableTasks: [SearchableTask!]!
}
```

**Server-side filter:** only tasks where `tracking_state != 'dismissed'` are returned. The resolver calls `find_by_user` three times (once each for tasks, projects, tags) and resolves `projectName` and tag names in memory using a hashmap — it does **not** use SQL joins.

#### 6.6.2 Frontend — `SearchProvider` and Fuse.js

`SearchProvider` is mounted inside `BrowserRouter` (in `App.tsx`) so all child routes can consume `useSearch()`.

```typescript
// Context shape (SearchContextValue)
interface SearchContextValue {
  readonly query: string;
  readonly setQuery: (q: string) => void;
  readonly matches: readonly FuseResult<SearchableTask>[];  // top Fuse hits
  readonly matchedIds: ReadonlySet<string>;                 // set of matched task ids
  readonly highlightActive: boolean;                        // true when query.length >= 2 and not loading/error
  readonly openTaskId: string | null;
  readonly openTaskInSheet: (id: string) => void;
  readonly closeSheet: () => void;
  readonly clearQuery: () => void;
  readonly loading: boolean;
  readonly error: Error | null;
}
```

**Fuse.js configuration:**

```typescript
const FUSE_OPTIONS: IFuseOptions<SearchableTask> = {
  threshold: 0.35,
  minMatchCharLength: 2,
  ignoreLocation: true,
  includeMatches: true,
  keys: [
    { name: 'title',       weight: 0.40 },
    { name: 'sourceId',    weight: 0.25 },
    { name: 'tags',        weight: 0.15 },
    { name: 'projectName', weight: 0.08 },
    { name: 'assignee',    weight: 0.07 },
    { name: 'description', weight: 0.05 },
  ],
}
```

The `SearchProvider` fetches `searchableTasks` once on mount via the `useSearchableTasks` hook (custom hook, not urql's generated hook). The Fuse index is rebuilt whenever the task list changes. Dismissed tasks are never included (filtered server-side).

#### 6.6.3 Frontend — `HeaderSearchBar` component

Renders inside `Header.tsx`. Responsibilities:

- Controlled input (`role="combobox"`) bound to `SearchContext.query`.
- On `query.length >= 2` and input focused, renders `<SuggestionDropdown>` below the input.
- Owns `activeIndex` state (the highlighted suggestion row). Resets to 0 whenever `matches` changes.
- Handles all keyboard navigation in its own `onKeyDown` handler (see §6.6.4).
- `aria-activedescendant` points to the id of the currently highlighted option (`${listboxId}-option-${activeIndex}`) when the dropdown is open and matches exist.
- Opening a suggestion calls `openTaskInSheet(id)` which opens `<TaskEditSheet>` (no route change).
- Clears the query (and closes the dropdown) when a suggestion is selected or when `Esc` is pressed.

#### 6.6.4 Frontend — Keyboard shortcuts

| Shortcut | Condition | Behaviour |
|----------|-----------|-----------|
| `/` | Active element is **not** `INPUT`, `TEXTAREA`, or `contentEditable` | Focus the search input |
| `Cmd/Ctrl+K` | Unconditional | Focus the search input |
| `Esc` | Search input has focus | Clear query and blur |
| `ArrowDown` | Dropdown open with matches | Advance highlight (clamped at last option) |
| `ArrowUp` | Dropdown open with matches | Move highlight back (clamped at first option) |
| `Enter` | Dropdown open with matches | Open highlighted task in sheet; clear query |

Global shortcuts (`/` and `Cmd/Ctrl+K`) are wired via a `useEffect`+`window.addEventListener` in `HeaderSearchBar`. Arrow/Enter navigation is handled entirely inside the input's `onKeyDown` — `SuggestionDropdown` is presentational and has no keyboard handlers.

#### 6.6.5 Frontend — `TaskCard` highlight classes

`TaskCard` reads `useSearch()`. When `highlightActive` is `true`:

- **Matching card** (task id is in `matchedIds`): `ring-2 ring-blue-500 ring-offset-2`
- **Non-matching card**: `opacity-40 grayscale-[30%]`

When `highlightActive` is `false`, no additional classes are applied and the card renders normally.

---

## 7. Database Schema

### 7.1 Migration: `001_initial.sql`

```sql
-- Users
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Projects
CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('jira', 'excel', 'obsidian', 'personal')),
    source_id TEXT,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'paused', 'completed')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Tasks
CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT,
    notes TEXT,                              -- markdown, user-owned, jamais écrasé par la sync Jira
    delegated_to TEXT,                       -- personne délégataire (texte libre, user-owned, jamais écrasé par la sync). Migration 008_add_delegated_to.sql.
    source TEXT NOT NULL CHECK (source IN ('jira', 'excel', 'obsidian', 'personal')),
    source_id TEXT,
    jira_status TEXT,
    status TEXT NOT NULL DEFAULT 'todo'
        CHECK (status IN ('todo', 'in_progress', 'done', 'blocked')),
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    assignee TEXT,
    deadline TEXT,
    planned_start TEXT,
    planned_end TEXT,
    estimated_hours REAL,
    urgency INTEGER NOT NULL DEFAULT 1 CHECK (urgency BETWEEN 1 AND 4),
    urgency_manual INTEGER NOT NULL DEFAULT 0,
    impact INTEGER NOT NULL DEFAULT 2 CHECK (impact BETWEEN 1 AND 4),
    tracking_state TEXT NOT NULL DEFAULT 'inbox'
        CHECK (tracking_state IN ('inbox', 'followed', 'dismissed')),
    jira_remaining_seconds INTEGER,           -- Jira timeestimate (seconds)
    jira_original_estimate_seconds INTEGER,   -- Jira timeoriginalestimate (seconds)
    jira_time_spent_seconds INTEGER,          -- Jira timespent (seconds)
    remaining_hours_override REAL,            -- Local override for remaining time (hours)
    estimated_hours_override REAL,            -- Local override for estimated time (hours)
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Task deduplication links
CREATE TABLE task_links (
    id TEXT PRIMARY KEY,
    task_id_primary TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    task_id_secondary TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    link_type TEXT NOT NULL
        CHECK (link_type IN ('auto_merged', 'manual_merged', 'rejected')),
    confidence_score REAL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(task_id_primary, task_id_secondary)
);

-- Meetings
CREATE TABLE meetings (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    start_time TEXT NOT NULL,
    end_time TEXT NOT NULL,
    location TEXT,
    participants TEXT,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    outlook_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(user_id, outlook_id)
);

-- Activity slots
CREATE TABLE activity_slots (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    start_time TEXT NOT NULL,
    end_time TEXT,
    half_day TEXT NOT NULL CHECK (half_day IN ('morning', 'afternoon')),
    date TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Alerts
CREATE TABLE alerts (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Les quatre variantes de domain::AlertType. La liste initiale de 001 n'en
    -- portait que trois ; la migration 013 a reconstruit la table (§ 7.2.2).
    alert_type TEXT NOT NULL
        CHECK (alert_type IN ('deadline', 'overload', 'conflict', 'timesheet_ready')),
    severity TEXT NOT NULL
        CHECK (severity IN ('critical', 'warning', 'information')),
    message TEXT NOT NULL,
    related_items TEXT NOT NULL DEFAULT '[]',
    date TEXT NOT NULL,
    resolved INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Tags
CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    color TEXT,
    UNIQUE(user_id, name)
);

-- Task-Tag junction
CREATE TABLE task_tags (
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, tag_id)
);

-- Sync status (les deux CHECK sont ceux d'aujourd'hui : 015 a élargi `source`, 016 `status`)
CREATE TABLE sync_status (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source TEXT NOT NULL
        CHECK (source IN ('jira', 'outlook', 'excel', 'obsidian', 'personal', 'gryzzly')),
    last_sync_at TEXT,
    status TEXT NOT NULL DEFAULT 'idle'
        CHECK (status IN ('idle', 'syncing', 'success', 'error', 'not_configured')),
    error_message TEXT,
    UNIQUE(user_id, source)
);
-- `not_configured` (016) : le connecteur n'a aucun identifiant utilisable, donc aucun sync n'a été
-- tenté. Remplace l'ancien encodage `status = error` + `error_message = "Not configured"`, qui
-- faisait afficher une source simplement non configurée comme une panne. `last_sync_at` reste NULL
-- pour cet état : une source non configurée n'a jamais synchronisé.

-- Configuration (key-value per user)
CREATE TABLE configuration (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    UNIQUE(user_id, key)
);

-- Gryzzly catalog cache (read-only: active projects + their tasks).
-- Refreshed by the `gryzzly` sync source. Denormalized: project_name/customer_name
-- are copied onto each task row. Pruning is soft (is_active = 0), never a hard delete.
CREATE TABLE gryzzly_tasks (
    id                 TEXT PRIMARY KEY,
    user_id            TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    gryzzly_task_id    TEXT NOT NULL,
    name               TEXT NOT NULL,
    gryzzly_project_id TEXT NOT NULL,
    project_name       TEXT NOT NULL,
    customer_name      TEXT,
    is_active          INTEGER NOT NULL DEFAULT 1,
    last_synced_at     TEXT NOT NULL,
    UNIQUE(user_id, gryzzly_task_id)
);

-- Worklog entries (timestamped, task-scoped journal)
CREATE TABLE worklog_entries (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    body       TEXT NOT NULL,
    logged_at  TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Indexes
CREATE INDEX idx_tasks_user ON tasks(user_id);
CREATE INDEX idx_tasks_source ON tasks(user_id, source, source_id);
CREATE INDEX idx_tasks_deadline ON tasks(user_id, deadline);
CREATE INDEX idx_tasks_project ON tasks(project_id);
CREATE INDEX idx_tasks_status ON tasks(user_id, status);
CREATE INDEX idx_meetings_user_time ON meetings(user_id, start_time);
CREATE INDEX idx_meetings_project ON meetings(project_id);
CREATE INDEX idx_activity_user_date ON activity_slots(user_id, date);
CREATE INDEX idx_alerts_user_resolved ON alerts(user_id, resolved);
CREATE INDEX idx_projects_user ON projects(user_id);
CREATE INDEX idx_worklog_entries_user_logged_at ON worklog_entries(user_id, logged_at DESC);
CREATE INDEX idx_worklog_entries_task_logged_at ON worklog_entries(task_id, logged_at DESC);
```

#### Worklog entries

- Validation: `body` non-empty after trim, max 10 000 characters (enforced in the domain layer).
- Ordering: `list` returns entries sorted by `logged_at DESC, created_at DESC`.
- Query limits: default 200 rows, absolute cap 1 000 (clamped in the application layer).
- Cascade: deleting a task removes its worklog entries via the FK.
- GraphQL surface:
  - Query `worklogEntries(filter: WorklogEntryFilterInput): [WorklogEntry!]!`
    - `WorklogEntryFilterInput.recurrenceId: ID` — when provided, routes to `WorklogRepository::find_by_recurrence`; wins over `taskIds` if both are present.
    - `WorklogEntryGql.occurrenceDate: Date` — resolved by loading the task and returning its `occurrence_date`; `null` for non-recurring tasks.
  - Mutation `addWorklogEntry(taskId: ID!, body: String!, loggedAt: DateTime, loggedAtLocal: NaiveDateTime, sessionId: String): WorklogEntry!` — `loggedAt` est un instant UTC absolu (ce que construit l'UI web depuis un `Date` navigateur) ; `loggedAtLocal` est une lecture d'horloge murale dans le fuseau de l'utilisateur, convertie **côté serveur** via `worklog::user_timezone` + `time::local_to_utc`. C'est la forme que la CLI envoie pour `aplan log --at`, et la raison est unique : une conversion heure locale → UTC faite dans la CLI serait la seconde implémentation de cette conversion, et un désaccord entre les deux ferait basculer l'entrée sur un autre jour local — donc sur une autre journée facturée. Les deux arguments ensemble sont **refusés** (même décision exprimée deux fois : un appelant qui envoie deux valeurs divergentes a un bug qu'un vainqueur silencieux cacherait dans des heures facturables). Aucun des deux : l'entrée est horodatée `now`.
  - Mutation `rebuildWorklogProjection(taskId: ID!, date: NaiveDate!): DayRebuildResultGql!` — reconstruit les créneaux d'activité d'**une** tâche pour **un** jour local nommé, sans faire avancer aucun filigrane. Appelle `use_cases::worklog::rebuild_task_local_date`. Ce que `flushWorklogTime` ne peut pas faire : il déduit les demi-journées à reconstruire des entrées **présentes dans sa propre fenêtre**, laquelle commence au démarrage de la session ; une entrée antidatée (`loggedAt`/`loggedAtLocal`, donc `aplan log --at`) se situe avant cette fenêtre, le flush n'apprend jamais que cette demi-journée existe et ses heures n'atteignent jamais le timesheet — l'entrée est au journal et la journée facture zéro. Nommer le jour ferme ce trou. Seules les demi-journées où la tâche **a effectivement des entrées** sont reconstruites : nommer une demi-journée sans entrée derrière mettrait les créneaux de cette tâche sur la liste de suppression sans rien pour les réécrire, ce qui est la manière dont une reconstruction perd des heures. Idempotent. Retourne `{ date, halfDays, slotsDiscarded, slotsWritten }` ; `halfDays` vide = la tâche n'a rien journalisé ce jour-là, rien n'a été touché (un succès, pas un échec).
  - Mutation `updateWorklogEntry(id: ID!, body: String, loggedAt: DateTime): WorklogEntry!`
  - Mutation `deleteWorklogEntry(id: ID!): Boolean!`
  - Mutation `flushWorklogTime(taskId: ID!, sessionId: String): FlushResultGql!` — calls `materialize_worklog_time` for the given task. The window (`sessionId`'s own `sessions.last_flush_at` when provided, otherwise `aplan.active_since`) only selects which local half-days to rebuild; every entry of the task in those half-days then decides the slots. Returns `{ activeSince, slotsWritten }`. Does not modify the active-task pointer. Idempotent: re-running produces the same slots, never duplicates, and a backdated entry is still picked up.
  - Mutation `reattributeWorklogEntries(input: ReattributeWorklogInput!): ReattributionResultGql!` — moves entries between tasks and rebuilds the derived slots. Calls `use_cases::reattribution::reattribute_worklog_entries`. See "Réattribution" below.
  - Mutation `repairOrphanedSlots(input: RepairOrphanedSlotsInput!): SlotRepairResultGql!` — drops the activity slots of a local-date range that lost their `task_id` and rewrites their half-days from the worklog. Calls `use_cases::slot_repair::repair_orphaned_slots`. See "Réparation des créneaux orphelins" below.
- `WorklogRepository::find_by_id_prefix(user_id, prefix, limit)` — `id LIKE ? ESCAPE '\'` on the hyphenated lowercase id, ordered `logged_at DESC`, `limit.max(1)` bound (never `LIMIT 0`). The `escape_like` helper (shared with the memory repository via `database::conversions`) keeps `_` and `%` literal: `_` is a LIKE wildcard, and an unescaped token would resolve a mistyped reference to an arbitrary entry.
- `WorklogRepository::reassign_task(user_id, ids, from_task, to_task, now)` — one transaction, chunked at 400 binds like `mark_consolidated`. `task_id = ?` is part of the `WHERE`: an id list assembled from an earlier read must not pull an entry off a task the caller never named, and the returned count says what actually moved. `consolidated_at` is left untouched — attribution and consolidation are different questions.

#### Réattribution (`use_cases::reattribution`)

- **Domain** (`domain::rules::reattribution`): `plan_reattribution(from, to, selected) -> Result<ReattributionPlan, ReattributionRefusal>` validates the selection (refusals in order: `SameTask`, `EmptySelection`, `ForeignEntry`) and derives the **affected half-days** — `AffectedHalfDay { date, half_day }`, computed with the projection's own `half_day_of`. `slot_hours` and `is_rebuildable` define what a slot is worth and whether it may be replaced. `worklog_time::{block_duration, total_block_hours, MIN_BLOCK_MINUTES, MAX_CONTINUATION_GAP_MINUTES}` define the persisted duration of a block and where a half-day is cut, now shared with `materialize_worklog_time` so one projection is defined once.
- **Slot strategy — re-derive, never re-point.** Slots are a projection of worklog timestamps (one per continuous stretch of work, never straddling a local half-day). Rewriting `task_id` cannot express a partial move: a slot carries the span of *several* entries, so the destination would receive time that never moved and the source would keep none of the time that stayed. The use case therefore moves the entries, deletes the projection of the two tasks in the affected half-days, and rebuilds it from the re-read entries.
- **No double counting.** Deletion and rebuild are scoped to the *same* set: (user, task ∈ {source, destination}, half-day ∈ affected). A third task's slot on that half-day is never read or written; a morning is untouched when only the afternoon moved; only **closed** slots are replaced (an open slot is a running timer). The delete drops **every** closed slot of those tasks in those half-days, so the count per half-day is not part of the argument — a half-day legitimately holds several slots since R-WL-13. Without the delete, a destination that already had a slot that morning would keep it *and* gain the rebuilt one.
- **Reported, not hidden.** The pair's total can legitimately change: a partial move re-spans both sides, and a half-day carrying slots the worklog does not account for (several partial flushes, or a flush predating the 45-minute gap rule) is canonicalised to what the entries now project to. `ReattributionOutcome` carries `selected_entries`, `moved_entries`, `affected_dates`, `slots_discarded`, `slots_rebuilt` and per-task `hours_before`/`hours_after`; the CLI prints a warning when the total moves.
- **Preview parity.** `confirm: false` runs the same selection, validation and projection and returns predicted figures without writing; `confirm: true` applies and then re-reads, so its figures are measured. One resolver, one use case: the preview cannot drift from the write.
- **Page cap.** Selection and repair bind `WORKLOG_FILTER_MAX_LIMIT` and **refuse** a page that came back full (`AppError::Validation`, exit 4) instead of silently correcting the first 1 000 entries of a month.
- **Timezone.** `use_cases::worklog::user_timezone` (`aplan.timezone`, default `Europe/Paris`) is shared with the flush. A local date window is converted with `local_day_start`, which walks forward when a local midnight does not exist rather than dropping it.
- **Not transactional across the two stores.** The entry move is one transaction; the slot repair is a sequence of `delete`/`save` calls, as the flush already is. The repair is a pure function of the entries, so a re-run converges — but a failure between the move and the repair leaves the affected half-days' slots stale until it is re-run.
- `WorklogRepository::find_by_recurrence(user_id, template_id, limit, offset)` — SQL join on `tasks.recurrence_id`; returns all entries for any occurrence of the template ordered by `logged_at DESC`.
- `update_task` per-instance allow-list: recurring instances may update `status`, `plannedStart`, `plannedEnd`, `deadline`, `notes`, `trackingState`, `remainingHoursOverride`, `estimatedHoursOverride`. Template-level fields (`title`, `description`, `urgency`, `impact`, `estimatedHours`, `projectId`, `tags`) must go through `updateRecurringTask`.
- Backward compatibility: the `appendTaskNotes` mutation remains registered but is no longer invoked by the frontend (the activity-timer quick note writes a worklog entry instead).

#### Réparation des créneaux orphelins (`use_cases::slot_repair`)

- **Le dégât.** `INSERT OR REPLACE INTO tasks` supprime avant d'insérer, ce qui déclenche le `ON DELETE SET NULL` de `activity_slots.task_id` : la ligne de `tasks` revient identique, les créneaux qui la désignaient perdent leur attribution. Ils s'affichent « (aucune tâche) » dans `aplan journal` (R22) et la reconstruction de feuille de temps ne peut plus les rattacher à un projet.
- **Pourquoi la machinerie existante n'y suffisait pas.** `plan_task_projection` reconstruit **une** tâche et sa liste de suppression teste `slot.task_id == Some(task_id)` : un `task_id` NULL ne correspond à rien, donc un `flush` laisserait l'orphelin en place **et** écrirait un créneau neuf — la demi-journée facturée deux fois. `aplan flush` ne nomme d'ailleurs que les demi-journées de sa fenêtre courante (jamais une date passée), et `reattribute` refuse source = destination.
- **Domain** (`domain::rules::slot_repair`) : `is_repairable_orphan(slot)` = `task_id` absent **et** `reattribution::is_rebuildable(slot)` (fermé + `source = worklog`) — le prédicat existant est **réutilisé**, pas redérivé : une seconde définition de « ce qui peut être remplacé » est la façon dont le côté protégé de la frontière finit par être oublié. `orphaned_half_days(slots)` en déduit les `AffectedHalfDay` concernées, dédupliquées et triées (matin avant après-midi), depuis les orphelins et non depuis la plage demandée.
- **Ce qui n'est jamais touché.** Un créneau `manual` sans tâche (95 lignes réelles, minuteurs lancés à la main avant la migration `014`) : il n'a jamais eu de tâche, aucune entrée ne le reproduit, le supprimer détruirait du temps que rien ne peut reconstruire. Un créneau ouvert (minuteur en cours) non plus. Une demi-journée de la plage sans orphelin non plus : elle n'est ni relue pour suppression ni réécrite.
- **Le déroulé.** (1) `find_by_user_and_date_range` (créneaux fermés de la plage) → filtre `is_repairable_orphan`. (2) Une **seule** lecture du journal sur la fenêtre locale (`task_ids: None` — quelles tâches sont concernées est précisément ce que l'orphelin a perdu), repliée sur les demi-journées concernées, qui donne pour chaque tâche les demi-journées où elle a des entrées. (3) Un `plan_task_projection` par tâche, sur **ses** demi-journées seulement. (4) Si `confirm` : suppression des orphelins **d'abord** — aucun plan ne peut les réclamer — puis `apply_task_projection` par tâche. La suppression précède l'écriture pour la raison qu'`apply_task_projection` documente déjà : l'ordre inverse laisse une fenêtre où la demi-journée porte les deux.
- **Parité aperçu/écriture.** `SlotRepairOutcome` est lu sur les `RebuildPlan` eux-mêmes (`delete` / `write`), c'est-à-dire sur les listes que l'écriture applique : l'aperçu n'est pas un second calcul et ne peut donc pas dériver. Il porte, par date, `orphansDropped` / `orphanHours` / `slotsDiscarded` / `slotsWritten`, et par tâche `hoursBefore` / `hoursAfter` (les orphelins ne comptent dans le `before` d'aucune tâche : ils ne comptaient pour personne).
- **Un orphelin sans entrée survivante est supprimé sans remplacement**, et cette perte est reportée telle quelle (date avec `orphansDropped > 0` et `slotsWritten == 0`, que la CLI signale en clair). Le conserver laisserait une durée inattribuable dans une demi-journée que la réparation vient de déclarer canonique.
- **Plage vide = succès.** Aucun orphelin dans la plage renvoie un compte rendu vide sans erreur : c'est ce qui rend le verbe rejouable pour vérifier son propre travail. Rejouer après application ne réécrit rien (les créneaux reconstruits ont un `task_id`, donc ne sont plus des orphelins). Refus (`AppError::Validation`, code 4) : plage inversée, et plafond de page partagé avec la réattribution (`refuse_a_truncated_page`).
- **Fuseau et transactions.** Même `user_timezone` et même `local_window` que le flush et la réattribution — deux conversions concurrentes mettraient la même entrée sur deux jours locaux différents. Comme la réattribution, la séquence `delete`/`save` n'est pas transactionnelle : la réparation étant une fonction des entrées, une reprise converge.

#### Migrations ultérieures

| Migration | Fichier | Description |
|-----------|---------|-------------|
| 002–007 | (voir `migrations/sqlite/`) | Récurrence, worklog, CLI, recherche, etc. |
| **008** | `008_add_delegated_to.sql` | `ALTER TABLE tasks ADD COLUMN delegated_to TEXT;` — champ délégation (texte libre, user-owned, jamais écrasé par la sync). |
| 009–011 | (voir `migrations/sqlite/`) | Catalogue Gryzzly, brouillons de feuille de temps, règles de mappage de signaux. |
| **012** | `012_create_memories.sql` | Mémoire sémantique : `memories`, `memory_stakeholders`, table FTS5 autonome `memories_fts`, et `ALTER TABLE worklog_entries ADD COLUMN consolidated_at TEXT` (filigrane de consolidation par entrée). Voir § 7.2. |
| **013** | `013_add_proposed_supersedes_and_fix_alert_type_check.sql` | Deux corrections indépendantes : `memories.proposed_supersedes` (supersession *proposée*, forme structurée) et reconstruction de `alerts` pour que le `CHECK` sur `alert_type` admette `timesheet_ready`. Voir § 7.2.1 et § 7.2.2. |
| **014** | `014_create_sessions.sql` | Sessions Claude Code : table `sessions`, plus `worklog_entries.session_id`, `activity_slots.session_id` et `activity_slots.source`. Voir § 7.3. |
| **015** | `015_fix_sync_status_source_check.sql` | Reconstruction de `sync_status` pour que le `CHECK` sur `source` admette les 6 variantes de `domain::Source` — `gryzzly` et `personal` manquaient depuis 001, ce qui rendait la source `gryzzly` totalement inopérante. Voir § 10.6. |
| **016** | `016_add_project_status_and_not_configured.sql` | `gryzzly_tasks.project_status` (statut du projet propriétaire, `active` \| `done`, NULL = inconnu lu comme actif) et reconstruction de `sync_status` pour que le `CHECK` sur `status` admette `not_configured`. Voir § 10.6. |
| **017** | `017_add_timesheet_unresolved_json.sql` | `ALTER TABLE timesheet_drafts ADD COLUMN unresolved_json TEXT;` — persistance de la liste des signaux non résolus, à côté de `blocks_json`. Simple ajout de colonne (aucune reconstruction de table), nullable, sans `CHECK` : c'est du JSON d'affichage opaque. Shape `[{"sourceRef","label","at"}]`, `at` au format `YYYY-MM-DD HH:MM:SS` **en heure locale**, écrit par `to_draft` et relu par `parse_unresolved_json`. NULL (toute ligne antérieure) se lit « aucune explication disponible » jusqu'à la prochaine reconstruction. **Pourquoi** : la liste était calculée par `reconstruct_day`, renvoyée une fois par la mutation `runTimesheetReconstruction`, puis perdue — l'en-tête n'avait pas où la garder, donc la requête `timesheetDraft` (soit *chaque* chargement de page) répondait une liste vide et la timeline n'affichait plus que des barres anonymes sans moyen de savoir **quoi** était non attribué. |

| **018** | `018_create_timesheet_quarter_shares.sql` | Arbitrage par quart de journée : table `timesheet_quarter_shares` (une ligne par `(brouillon, quart, voie)`) et `ALTER TABLE timesheet_drafts ADD COLUMN lanes_json TEXT`. **Pourquoi une table et pas du JSON** : une part est une **décision de facturation**. `blocks_json` / `unresolved_json` sont documentés comme des charges utiles d'affichage opaques que les lecteurs tolèrent absentes — le bon contrat pour une chronologie, le mauvais pour des heures qui atteignent la facture d'un client. `task_id` est en `ON DELETE SET NULL`, **jamais** `CASCADE` : supprimer une tâche ne doit pas effacer des heures déjà déclarées, et `lane_key` + `label` survivent à la suppression pour que la ligne reste lisible. `is_pinned` porte l'arbitrage de l'utilisateur : une reconstruction le **conserve** et rééquilibre le reste de son quart autour. `lanes_json` est la vue des traces concurrentes (affichage seul, lecture tolérante, shape `[{"laneKey","label","gryzzlyProjectId","intervals":[[début,fin]],"outsideMinutes"}]` en minutes locales depuis minuit). Voir § 7.4. |
### 7.3 Migration `014_create_sessions.sql` — sessions Claude Code

#### 7.3.1 Deux natures d'acteur

Le pointeur global reste ce qu'il était : **l'humain, en manuel**. Les deux clés de
configuration `aplan.active_task_id` et `aplan.active_since` gardent leur sens et leur
comportement mono-tâche, inchangés.

Une ligne de `sessions` est **un Claude**. N sessions ouvertes, chacune sa tâche, chacune son
mode. Rien ne fusionne les deux : le pointeur de l'humain ne bouge jamais parce qu'un Claude a
changé de tâche, et l'inverse est vrai aussi.

```sql
CREATE TABLE sessions (
    id            TEXT PRIMARY KEY,                              -- CLAUDE_CODE_SESSION_ID
    user_id       TEXT NOT NULL REFERENCES users(id),
    task_id       TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    mode          TEXT NOT NULL CHECK (mode IN ('tracking','off')),
    label         TEXT,                                          -- le `cwd` du hook, pour l'affichage
    started_at    TEXT NOT NULL,
    last_seen_at  TEXT NOT NULL,
    last_flush_at TEXT,
    ended_at      TEXT
);
CREATE INDEX idx_sessions_user_open ON sessions(user_id, ended_at);

ALTER TABLE worklog_entries ADD COLUMN session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL;
ALTER TABLE activity_slots  ADD COLUMN session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL;
ALTER TABLE activity_slots  ADD COLUMN source     TEXT;   -- 'worklog' | 'manual'
```

`session_id` à NULL signifie l'humain : le pointeur global n'a pas de ligne de session et n'en
aura jamais. L'énumération de `source` est **appliquée en Rust** (`SlotSource`), pas par un
`CHECK` SQL — corriger un `CHECK` sur une table SQLite existante coûte la reconstruction
complète que la migration 013 a dû faire. Un `source` NULL se lit **`manual`**, la valeur que
rien ne reconstruit : l'inconnu est protégé plutôt qu'effacé.

#### 7.3.2 Résolution de la cible d'un verbe implicite

Pour `log`, `note`, `status` et `done` :

1. `--task <cible>` gagne toujours.
2. Sinon `--session <id>` (ou `CLAUDE_CODE_SESSION_ID`, que le harnais exporte dans chaque appel
   Bash) → la tâche de la session. **Si la session est en mode `off`, refus en code 4** — jamais
   de repli silencieux sur le pointeur global. Ce refus *est* la fonctionnalité : le défaut
   corrigé était un Claude rapportant du travail sur une tâche que l'utilisateur avait
   explicitement déclinée.
3. Sinon le pointeur global.

`remember` est l'exception délibérée : il ne refuse jamais. `--task` gagne, sinon une session
qui tracke rattache la mémoire à sa tâche, sinon la mémoire est créée non rattachée et la
commande réussit — y compris pour une session `off`. Les mémoires sont hors des règles du
worklog (le hook SessionStart le dit déjà pour les sessions non surveillées), et une mémoire non
rattachée n'attribue rien à tort, là où une mauvaise attribution de worklog serait du temps
facturable sur la mauvaise tâche.

#### 7.3.3 Passe de classification de la provenance des slots

La migration laisse `source` à NULL sur toutes les lignes existantes. Une passe unique, au
démarrage de l'API et gardée par la clé `aplan.slot_source_classified`, la remplit depuis les
données. Un slot fermé porteur d'une tâche vient d'un flush **si et seulement si** une entrée de
cette tâche a `logged_at == start_time`, **et** qu'une entrée a `logged_at == end_time` **ou**
que `end_time == start_time + MIN_BLOCK_MINUTES`.

Ce n'est pas la règle envisagée d'abord. Comparer l'intervalle du slot aux blocs que
`derive_time_blocks` produit aujourd'hui teste le *regroupement* des entrées, et ce regroupement
a changé : la coupure à 45 minutes est arrivée avec `abda52a`, et avant elle le flush écrivait de
façon incrémentale contre un watermark. Mesurée sur la base réelle, cette règle ne classait que
12 des 52 candidats. L'invariante qui a survécu au changement est que les bornes d'un slot
**sont** des `logged_at` d'entrées, le flush les recopiant verbatim. Sur la base réelle : 52/52
candidats satisfont la première condition, 42 la première branche de la seconde, les 10 autres sa
seconde branche, 0 inexpliqué. La règle compare des instants UTC exacts, donc elle n'a **besoin
d'aucun fuseau horaire**.

La passe s'exécute **après** le retour anticipé d'`export-schema` : cette commande de génération
de code ne doit pas effectuer une écriture irréversible dont la trace serait avalée par la
redirection de sortie.

#### 7.3.4 Le flush devient une reconstruction idempotente

Avant le plan 2, `flushWorklogTime` lisait un filigrane unique et global
(`aplan.active_since`), matérialisait les entrées plus récentes que lui, puis avançait ce
filigrane pour tout le monde. Le défaut n'exigeait aucune concurrence pour se manifester : dès
que deux tâches s'entrelaçaient, flusher la tâche B avançait le filigrane que la tâche A
attendait encore, et les entrées de A journalisées avant ce flush n'étaient jamais matérialisées
— perdues en silence. Une ré-exécution, elle, dupliquait ce qui avait déjà été écrit, puisque le
filigrane ne portait aucune notion d'idempotence.

**La fenêtre devient un sélecteur, jamais une vérité.** Elle ne sert plus qu'à répondre à une
question : quelles demi-journées locales cette tâche a-t-elle touchées ? Une fois ces
demi-journées identifiées, ce sont **toutes** les entrées de la tâche qui s'y trouvent — pas
seulement celles tombées dans la fenêtre — qui décident des créneaux à écrire. Élargir la fenêtre
ne coûte donc rien : au pire elle nomme une demi-journée déjà à jour, jamais elle n'en oublie
une. C'est ce renversement qui rend une entrée antidatée récupérable : son admission dépend de
la demi-journée locale où elle tombe, jamais d'une comparaison d'horodatage contre le filigrane.

**La reconstruction est bornée à (tâche, demi-journée).** `plan_task_projection` (lecture) et
`apply_task_projection` (écriture), `crates/application/src/use_cases/worklog.rs`, sont le
primitif partagé : pour chaque demi-journée nommée, on supprime les créneaux que la tâche
possède dans cette demi-journée, puis on les réécrit depuis les entrées de cette même tâche dans
cette même demi-journée, via `derive_time_blocks`. Rien d'une autre tâche n'est **écrit** —
c'est ce qui garantit l'isolation entre tâches : le flush de B ne touche ni les créneaux ni la
fenêtre de A. La lecture n'est pas bornée de la même façon : `find_by_user_and_date` renvoie tous
les créneaux de l'utilisateur ce jour-là, et le filtrage par tâche a lieu en mémoire.

**`source` est le verrou de suppression.** `is_rebuildable` (`domain::rules::reattribution`)
n'autorise la suppression que d'un créneau fermé dont `source == Worklog` : c'est la trace que le
flush lui-même a écrit ce créneau, donc le réécrire depuis les mêmes entrées est un no-op ou une
correction. Un créneau `Manual` — minuterie live, saisie UI, provenance que la passe de
classification (§7.3.3) n'a pas pu établir — n'est jamais une cible de suppression, et une valeur
NULL ou illisible est lue côté infrastructure comme `Manual` précisément pour tomber du côté
protégé de cette ligne. Le mode de défaillance de l'inconnu est donc un créneau qui survit sans
être reconstruit, jamais un créneau détruit.

**Un seul primitif, deux appelants.** `plan_task_projection` / `apply_task_projection` ne sont pas
propres au flush : `reattribute_worklog_entries` (§ « Réattribution ») effectue exactement la
même opération sur les demi-journées affectées par un déplacement d'entrées. Un même calcul de
reconstruction, appelé par les deux chemins, ferme la possibilité qu'un aperçu de réattribution
et un flush en arrivent à des chiffres différents pour la même demi-journée.

**Deux fenêtres qui ne se croisent jamais.** `flushWorklogTime` en lit et en avance une des deux,
jamais les deux : avec un `sessionId`, c'est la fenêtre propre à cette session Claude
(`sessions.last_flush_at`, lue par `Session::flush_window_start()`, avancée par
`SessionRepository::set_last_flush`) ; sans lui, c'est le pointeur de l'humain,
`aplan.active_since`. Partager une clé entre tâches est exactement le défaut d'origine — le
partitionnement par acteur (session vs humain) est ce qui l'empêche de se reproduire une fois la
reconstruction elle-même rendue sans état partagé.

De ce renversement découlent trois propriétés, toutes vérifiées par les tests d'application :

- **Idempotence.** Deux flushs de la même tâche sur la même demi-journée produisent le même jeu
  de créneaux ; une ré-exécution — ou deux flushs concurrents qui convergent — n'ajoute jamais de
  doublon.
- **Reprise d'une entrée antidatée.** Une entrée journalisée avec un `logged_at` dans le passé est
  matérialisée dès que sa demi-journée locale est reconstruite, qu'elle tombe ou non dans la
  fenêtre qui a déclenché le flush.
- **Isolation entre tâches.** Flusher une tâche n'écrit rien des créneaux ou de la fenêtre d'une
  autre tâche (la lecture, elle, porte sur tous les créneaux du jour avant le filtrage en
  mémoire) ; deux sessions sur deux tâches distinctes s'exécutent sans jamais se gêner.

#### 7.3.5 Cycle de vie : les hooks, `start`/`stop`, le ramasseur de sessions inactives

**Le hook SessionStart** (`~/.claude/hooks/aplan-session-start.sh`) ne **dérive** jamais l'état
d'une session connue du pointeur global et ne le déplace jamais ; il interroge d'abord
`aplan session show --session <id>` et trie la réponse en cinq issues. Une seule exception lit
le pointeur : quand aucune ligne n'existe (`mode` vide), le hook appelle `aplan current --json`
pour proposer la tâche humaine courante en Option 1 — le script consacre neuf lignes de
commentaire à justifier ce cas précis comme « le seul usage légitime » : rien ne bouge, l'offre
est simplement plus utile qu'une liste vide, et le garde-fou (`-z "$mode"`) empêche toute session
déjà connue d'y retomber.

| État lu | Ce qui est injecté |
|---|---|
| Ligne inconnue (`claudeSession` = null) | `AskUserQuestion` obligatoire (Option 1 = tâche humaine courante, lue via `aplan current`, si elle existe) |
| Connue, `mode = OFF` | Une ligne : logging désactivé pour cette session, ne pas redemander |
| Connue, `mode = TRACKING`, tâche résolue, ligne encore ouverte (`endedAt` vide) | Une ligne confirmant la tâche suivie |
| Connue, `mode = TRACKING`, tâche résolue, mais ligne **fermée** par le ramasseur (`endedAt` non vide) | `AskUserQuestion` obligatoire ; l'option Continuer lance `aplan session bind` (jamais une simple confirmation) puisque la ligne, fermée, ne peut pas encore recevoir de worklog |
| `source = clear` (sur n'importe quelle ligne) | `AskUserQuestion` obligatoire, même sur une ligne déjà connue |
| Backend/CLI inaccessible, id de session absent, réponse sans clé `claudeSession`, ou session planifiée (`APLAN_UNATTENDED`) | Rien n'est injecté — no-op silencieux et délibéré : le hook préfère se taire plutôt que mal deviner |

Le déclenchement de la question couvre en réalité **quatre** conditions, pas seulement la
première ligne du tableau : aucun choix enregistré, `source = clear`, une ligne `TRACKING` dont
la tâche ne résout plus (supprimée entre-temps), ou une ligne `TRACKING` que le ramasseur a
fermée entre-temps (`endedAt` non vide, voir plus bas) — ces deux derniers cas se lisent
« connue » dans la table ci-dessus mais retombent sur la question, le premier faute de titre à
confirmer, le second parce que confirmer un suivi sur une ligne fermée affirmerait quelque chose
de faux : `aplan log` y échoue (exit 4) jusqu'à un `session bind`. `resume` et `compact`
ne redemandent **jamais** parce qu'ils ne sont ni l'un ni l'autre : ils suivent la ligne
enregistrée comme tout `source` hors `clear`. C'est exactement le correctif du défaut
d'origine — la ligne injectée à un re-déclenchement vient désormais de l'enregistrement de la
session, jamais du pointeur global qu'elle aurait sinon relu.

**`start` et `stop` agissent sur la session qui appelle, jamais sur le pointeur humain.** Dès
qu'un id de session est présent (`--session`, ou `CLAUDE_CODE_SESSION_ID` que le harnais exporte
dans chaque appel), `aplan start <tâche>` est `aplan session bind` sous un autre nom : il flush
la tâche que la session quitte, puis la relie à la nouvelle, sans jamais appeler
`set_config_key` sur `aplan.active_task_id` — la même frontière que la fenêtre de flush (§ 7.3.4)
ne doit pas croiser. Sans id de session (terminal nu), le comportement d'avant ce plan est
inchangé : c'est le pointeur humain qui bouge. `aplan stop` fait le même choix, en miroir.

**Le hook SessionEnd** (`aplan-session-end.sh`) flush la tâche de **cette** session
(`aplan --session <id> flush <task_id>`) et **ne ferme jamais la ligne**. Ce n'est pas un
oubli : un id de session Claude Code survit à `claude --resume`, donc fermer la ligne ici ferait
démarrer la session reprise contre une ligne `ended` — un état que `Session::target()`
(`domain/src/types/session.rs:144-152`) refuse déjà par construction et que la table du hook
SessionStart ci-dessus ne prévoit pas. Le ramasseur de sessions inactives est donc le seul point
qui ferme une ligne, ce qui n'est sûr que parce que le flush est une reconstruction idempotente
(§ 7.3.4) : un second flush sur la même fenêtre ne duplique rien.

**Le ramasseur** (`reap_idle_sessions`, `crates/application/src/use_cases/session_reaper.rs`)
tourne sur son propre ordonnancement (`run_session_reaper_scheduler`, `crates/api/src/jobs.rs`,
`RetryPolicy::session_reaper()` : base 15 min, plafond 45 min) et ferme toute session dont
`last_seen_at` dépasse le seuil `aplan.session_idle_timeout_hours` (défaut 12, borné à
`1..=8760`, voir § 15.1). **L'ordre est flush puis fermeture, jamais l'inverse** : fermer une ligne
avant de la flusher la priverait pour toujours de toute fenêtre qui la sélectionnerait encore —
le temps serait perdu, pas seulement retardé. Une session sans tâche liée (`mode = off`, ou un
bind qui n'a jamais eu lieu) n'a rien à matérialiser et est fermée directement. L'échec d'une
session — flush, avance du filigrane, ou fermeture — est journalisé et sauté, jamais propagé :
le ramasseur tourne sans surveillance, et une session bloquée ne doit pas empêcher les suivantes
d'être traitées dans le même passage.

**« Inactive » veut dire « aucune écriture aplan », pas « aucune activité ».** `last_seen_at`
n'avance qu'à `resolve_session_target` (`application/…/session_tracking.rs:136`) et à la
résolution de session côté mutation (`api/…/mutation.rs:158`) — c'est-à-dire uniquement à une
écriture `aplan`, jamais à une simple lecture ni à une conversation qui n'en déclenche aucune.
Une session qui lit et discute pendant 12 h sans jamais appeler `aplan log`/`start`/`stop`/`flush`
est donc « inactive » selon cette définition, et peut être fermée par le ramasseur en plein
milieu de son travail, sans qu'aucun hook ne l'en avertisse. Le chemin de reprise est le même que
pour toute ligne fermée : `aplan session bind`, qui rouvre la ligne (voir la table du hook
SessionStart ci-dessus). Le seuil de 12 h lui-même n'est pas remis en cause ici — c'est un
arbitrage produit, pas un défaut de code.

#### 7.3.6 Recouvrement entre tâches : visible, jamais corrigé

Rien n'est stocké ni réparé. `find_overlaps` (`crates/domain/src/rules/overlap.rs`) est un
calcul pur, exécuté à la lecture, sur les créneaux **fermés** d'un utilisateur : deux créneaux
sur des tâches **différentes** dont les intervalles demi-ouverts `[start_time, end_time[` se
croisent comptent pour un recouvrement, mesuré en minutes de l'intersection — une intersection de
moins d'une minute vaut `0` après troncature, et le domaine la rend telle quelle : ce n'est
**pas** lui qui la cache, mais l'affichage. `aplan journal` et `aplan dash` filtrent les paires à
`minutes == 0` avant impression (`commands.rs:684`, `:813`), une décision d'affichage et non une
règle du domaine. Trois exclusions s'appliquent avant
même de considérer une paire : un créneau **ouvert** ne porte encore aucune heure mesurée ; un
créneau **non étiqueté** (`task_id = None`) est un temps attribué à personne, donc ne peut pas
être « deux tâches qui réclament la même heure » ; et une paire sur la **même** tâche n'est
jamais un recouvrement — une tâche a légitimement plusieurs plages dans une même demi-journée.
Se toucher n'est pas se recouvrir : deux créneaux dont l'un se termine exactement où l'autre
commence partagent zéro minute, donc une journée de créneaux bout-à-bout ne signale rien. La
fonction rend des **paires**, jamais une plage fusionnée : trois créneaux qui se recouvrent
mutuellement donnent trois paires, chacune nommant les deux créneaux en cause — une plage
fusionnée dirait « quelque chose s'est recouvert ici » sans dire lesquelles des tâches sont
entrées en collision, or c'est précisément ce dont l'utilisateur a besoin pour arbitrer.

C'est une décision produit assumée, pas un défaut à corriger : chaque tâche garde le temps que
ses propres entrées documentent, le double comptage est **accepté et signalé**, et l'arbitrage
reste entre les mains de l'humain à la revue `aplan timesheet` qui existe déjà. Aucune alerte
n'est levée pour un recouvrement — l'affichage seul suffit.

Exposé de façon additive par `activityOverlaps(date)` en GraphQL — un second aller-retour, pas un
champ ajouté aux requêtes existantes, puisque `journal` et `dash` n'ont sinon aucune raison de
porter un champ qu'elles n'utilisent pas autrement — et affiché dans les trois commandes, mais
**pas par le même chemin pour les trois** : seules `journal` et `dash` interrogent
`activityOverlaps(date)` ; `timesheet` ne l'appelle jamais et recalcule son propre écart en
relisant `activityJournal` (`timesheet_cmd.rs:50-71`), sans jamais passer par `find_overlaps` —
c'est une des raisons pour lesquelles sa mesure diffère de celle de `journal` (voir plus bas).

- **`aplan journal`** — une ligne par paire (via `activityOverlaps`), les deux tâches nommées et
  l'acteur de chaque côté identifié (session ou humain).
- **`aplan dash`** — une ligne de résumé si la journée porte au moins un recouvrement (via
  `activityOverlaps`) : nombre de paires et leur total de minutes **additionné**, pas dédupliqué
  — un créneau présent dans deux paires compte deux fois. C'est volontaire : cette ligne rapporte
  l'ampleur du problème, pas une quantité de temps à réconcilier (c'est le rôle de `timesheet`,
  avec l'union des intervalles) ; jamais le détail par paire, qui reste celui de `journal`.
- **`aplan timesheet`** — l'écart entre le total brut des créneaux étiquetés (relus via
  `activityJournal`, jamais via `activityOverlaps`) et la durée couverte par leur **union**
  d'intervalles, calculée localement dans le CLI (`union_minutes`, `timesheet_cmd.rs`).

**`timesheet` et `journal` répondent à deux questions différentes, et c'est volontaire.** L'écart
de `timesheet` (`brut − couvert`) exclut les créneaux non étiquetés mais compte encore deux
plages qui se recouvrent sur la **même** tâche — ce que `find_overlaps` exclut délibérément,
une tâche ayant légitimement plusieurs plages dans la journée. `timesheet` répond donc à
« combien du temps journalisé aujourd'hui est doublement compté, toutes causes confondues »
(on ne peut pas facturer 8 h 20 dans 7 h 30 d'horloge murale, quelle que soit la tâche en cause),
tandis que `journal` répond à « quelles deux tâches sont entrées en collision ». Les deux mesures
peuvent légitimement diverger : un utilisateur peut voir un écart dans `timesheet` sans aucune
ligne correspondante dans `journal`. Ne pas aligner l'une sur l'autre — les « corriger » pour
qu'elles convergent ferait disparaître l'une des deux questions à laquelle chacune répond
correctement.

**Le recouvrement est absent de `--json`, dans les trois commandes.** `journal`, `dash` et
`timesheet` retournent chacune leur résultat avant le second aller-retour dès que `--json` est
demandé : le paquet JSON est le contrat de l'API, pas le résumé humain, et rien n'y porte de champ
de recouvrement. Une Claude qui lit `aplan journal --json` ne peut donc voir aucune collision
signalée, ni aucun autre consommateur programmatique — l'indicateur est **réservé à l'humain**.
Le skill `aplan` (`.claude/skills/aplan/SKILL.md`) recommande pourtant `--json` par défaut ; il
précise désormais qu'une Claude qui veut voir un recouvrement doit lancer la forme texte, faute
de quoi rien ne le lui montrera.

### 7.2 Migration `012_create_memories.sql` — mémoire sémantique

```sql
CREATE TABLE memories (
  id             TEXT PRIMARY KEY,
  user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  kind           TEXT NOT NULL,     -- decision | commitment | fact | preference
  title          TEXT NOT NULL,
  body           TEXT,

  -- bi-temporel
  occurred_at    TEXT NOT NULL,     -- quand ça a été décidé / promis
  recorded_at    TEXT NOT NULL,     -- quand aplan l'a su
  invalidated_at TEXT,              -- NULL = encore vrai
  superseded_by  TEXT REFERENCES memories(id) ON DELETE SET NULL,

  -- provenance
  source         TEXT NOT NULL,     -- claude_session | manual | dreaming
  source_ref     TEXT,              -- id d'entrée worklog / de session. PAS de FK.
  status         TEXT NOT NULL,     -- pending | active | rejected

  -- entity linking (par jointure)
  project_id     TEXT REFERENCES projects(id) ON DELETE SET NULL,
  task_id        TEXT REFERENCES tasks(id)    ON DELETE SET NULL
);

-- Prédicat de `list` (user_id + status + project_id optionnel) et tri « du plus
-- récent au plus ancien » de la file de validation et de l'historique.
CREATE INDEX idx_memories_user_status ON memories(user_id, status, project_id);
CREATE INDEX idx_memories_occurred_at ON memories(user_id, occurred_at DESC);

CREATE TABLE memory_stakeholders (
  memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
  person    TEXT NOT NULL,
  PRIMARY KEY (memory_id, person)
);
-- « Quels engagements envers Pierre ? » est une question de premier ordre, et la
-- PRIMARY KEY n'indexe que (memory_id, person) : la recherche inverse a son index.
CREATE INDEX idx_memory_stakeholders_person ON memory_stakeholders(person);

CREATE VIRTUAL TABLE memories_fts USING fts5(
  memory_id UNINDEXED,
  title,
  body,
  tokenize = 'unicode61 remove_diacritics 2'
);

ALTER TABLE worklog_entries ADD COLUMN consolidated_at TEXT;
```

**Choix de conception, et pourquoi ils ne doivent pas être « simplifiés » :**

- **Table FTS5 autonome, pas `content='memories'`.** Vérifié sur le SQLite réellement embarqué
  par `sqlx 0.8` : avec une table à contenu externe et sans triggers, après un `INSERT` dans la
  table source, `MATCH` renvoie **0 ligne** tandis que `SELECT count(*) FROM …_fts` renvoie **1**.
  Le contrôle d'intégrité le plus naturel masque donc la panne. La table autonome est écrite par
  le dépôt **dans la même transaction** que la ligne `memories` — d'où l'absence de triggers.
  Corollaire de test : **tout test d'indexation doit interroger par `MATCH`, jamais par `count(*)`.**
- **Risque d'orphelin FTS.** Une table virtuelle n'accepte **aucune clé étrangère** : `memory_id`
  n'est pas contraint et supprimer une ligne `memories` **ne cascade pas** vers `memories_fts`.
  Chaque chemin d'écriture entretient l'index à la main, dans la transaction de l'appelant :
  insertion → `INSERT` (`create`), reformulation → `DELETE` + `INSERT` (`update`, sinon le souvenir
  ne reste trouvable que sous son ANCIEN libellé), suppression → `DELETE` d'abord (`apply_merge`
  sur la ligne écartée). Un `DELETE` oublié laisse un orphelin qui continue de répondre à `MATCH`
  pour un souvenir disparu : la jointure l'écarte et la recherche sous-retourne silencieusement.
- **`tokenize = 'unicode61 remove_diacritics 2'` explicite.** Le tokenizer plie les accents
  (`limitee` retrouve « limitée »), mais ne fait **aucune lemmatisation** : `engagements` ne
  retrouve pas « engagement ». D'où l'extension par préfixe côté domaine (voir § 5, `rules/recall.rs`).
  `porter` est exclu : c'est un stemmer anglais, il dégraderait du texte français.
- **`ON DELETE SET NULL` sur `project_id` / `task_id`** : supprimer une tâche ne doit pas effacer
  le souvenir de la décision qui l'a produite. Corollaire assumé : `source_ref` ne porte **pas**
  de FK (les `worklog_entries` disparaissent en CASCADE avec leur tâche), une chaîne de provenance
  pendante étant préférable à un souvenir supprimé.
- **Pas de colonne `confidence`** : remplacée par la porte de validation humaine (`status`).
- **`consolidated_at` est un marqueur par entrée, pas un curseur global.** Un curseur horodaté
  sauterait définitivement toute entrée insérée tardivement avec un `logged_at` antérieur, et
  `sync_status` ne peut pas porter ce filigrane (`CHECK (source IN ('jira','outlook','excel','obsidian'))`,
  et SQLite ne sait pas `ALTER` une contrainte `CHECK`).
  La colonne est lue et écrite **uniquement** par `SqliteWorklogRepository::list_unconsolidated` /
  `mark_consolidated` ; elle n'est **pas** portée par `domain::WorklogEntry` et n'apparaît pas dans
  `WorklogEntryGql`. C'est un choix : le filigrane appartient au dispositif de mémoire, pas à ce
  qu'une entrée de journal *signifie*, et la valeur elle-même n'est jamais nécessaire — la lecture
  filtre sur `IS NULL`, l'écriture renvoie un nombre de lignes. Le prédicat
  `consolidated_at IS NULL` figure aussi dans le `WHERE` de la mise à jour : le premier marquage
  gagne, donc un passage relancé après un crash ne réécrit pas la date réelle. Le lot est marqué en
  **une transaction**, découpée en tranches de 400 identifiants pour rester sous
  `SQLITE_MAX_VARIABLE_NUMBER`.
- **Les pools de test doivent activer `foreign_keys(true)`.** Le pool de production l'active
  (`infrastructure/src/database/connection.rs`) mais un `SqlitePool::connect("sqlite::memory:")` nu
  ne l'active pas : sans cela une violation de FK reste verte en TDD et n'apparaît qu'en runtime.
  Les tests de `memory_repo.rs` construisent leur pool via `SqliteConnectOptions … .foreign_keys(true)`
  avec `max_connections(1)` (une base en mémoire par connexion, sinon).
- **Garde d'environnement** : `memory_repo.rs` porte un module de tests
  `fts5_environment_guard` qui vérifie que FTS5 est créable avec ce tokenizer, que `bm25()` est
  **négatif** (mesuré : `-0.000001`) et que l'ordre `ASC` place bien la meilleure correspondance
  en tête. Une montée de version de `sqlx` qui perdrait FTS5 échouera ici, bruyamment, au lieu
  de vider silencieusement tous les rappels.

### 7.2.1 Migration `013` — `memories.proposed_supersedes`

```sql
ALTER TABLE memories ADD COLUMN proposed_supersedes TEXT REFERENCES memories(id) ON DELETE SET NULL;

CREATE INDEX idx_memories_proposed_supersedes
    ON memories(proposed_supersedes) WHERE proposed_supersedes IS NOT NULL;
```

**Ce que la colonne porte.** `superseded_by` enregistre une supersession qui a **eu lieu** ;
`proposed_supersedes` enregistre celle que la consolidation **propose**. Les deux doivent rester
distinctes : le filtre dur de rappel lit `invalidated_at`, et une proposition est précisément une
affirmation que personne n'a encore validée — elle ne doit jamais masquer un souvenir encore vrai.
Avant cette colonne, la proposition vivait en prose dans le `body` du candidat : aucune surface ne
pouvait la lire, aucun verbe ne pouvait l'appliquer, et le triage devait recopier un identifiant
lu dans un paragraphe.

**Invariant du cycle de vie : une proposition n'existe que sur une ligne `pending`.**
`Memory::new` **refuse** une proposition portée par une ligne qui saute la file (`--confirm`,
donc `active`, ou `rejected`) — le verbe correct dans ce cas est `aplan memory supersede`. Et
chaque verdict de la file la **consomme** (`domain::rules::memory_lifecycle::spend_proposal`) :

| Verdict | Sort de `proposed_supersedes` | Pourquoi |
|---|---|---|
| `accept` | effacée | Accepter, c'est répondre « non, c'est un fait nouveau, on garde les deux » : ce verbe n'invalide rien. Laisser la proposition ferait annoncer indéfiniment un conflit que l'utilisateur vient d'écarter. |
| `reject` | effacée | La pierre tombale conserve le libellé (c'est ce qui fait converger la boucle de proposition), pas la question : il n'y a plus rien à trancher. |
| `merge` | effacée, et **jamais héritée** par le survivant | Un merge dit « même fait » : il n'y a pas de contradiction. Et comme la porte anti-doublon propose `merge` **et** `supersede` sur la *même* paire, la proposition nomme d'ordinaire la cible du merge — l'hériter ferait proposer au survivant de se superséder lui-même. |
| `supersede` | effacée | La proposition est **honorée** : `invalidated_at` + `superseded_by` portent désormais le même fait sous forme structurée. Le garder stockerait une vérité en deux exemplaires, qui divergeraient. |

Conséquence lisible par n'importe quel lecteur : **`status <> 'pending'` implique
`proposed_supersedes IS NULL`**. Une proposition trouvée dans la base est donc toujours une
question ouverte, jamais un vestige. L'invariant est tenu par le domaine et non par un `CHECK` —
`memories` n'en porte aucun, pas même sur `kind` ou `status` (§ 7.2).

**Cas résiduel assumé** : le souvenir *nommé* par une proposition peut être invalidé par une autre
supersession entre le passage de 17 h 30 qui l'a proposée et le triage du lendemain matin. La
proposition n'est donc pas revalidée à l'écriture (une affirmation molle ne doit pas faire échouer
un `remember`), mais les deux surfaces sont honnêtes : `aplan inbox` marque une proposition dont la
cible n'est **déjà plus vraie**, et `supersede` la refuse (`MemoryAlreadyInvalidated`, code 4).

**`ON DELETE SET NULL`**, même raisonnement que `superseded_by` : `apply_merge` supprime la ligne
écartée, et un identifiant pendant qu'aucun lecteur ne peut résoudre serait pire qu'une absence de
proposition. L'index est **partiel** : seules quelques lignes portent une proposition, et c'est la
colonne que SQLite doit parcourir pour appliquer `SET NULL` à chaque suppression d'un `memories`.

**Surface.** `MemoryGql.proposedSupersedes: ID` (l'identifiant) et `MemoryGql.contradicts: MemoryGql`
(le souvenir nommé, résolu — un identifiant seul ne dit pas *quelle* décision est contredite).
`RememberInputGql.proposedSupersedes: ID` accepte une **référence courte** (`m:7c1`), résolue par le
même résolveur que tous les autres verbes. `supersedeMemory(old: ID, by: ID!)` : `old` est devenu
**nullable** et, omis, retombe sur la proposition portée par `by`
(`use_cases::memory::proposed_supersession_target`). Un candidat qui ne propose rien est un refus de
précondition (code 4), jamais une supersession de rien.

### 7.2.2 Migration `013` — reconstruction de `alerts`

`domain::AlertType` compte **quatre** variantes et `alert_type_to_str` associe `TimesheetReady` à
`timesheet_ready` (`infrastructure/src/database/conversions.rs`), mais le `CHECK` écrit en `001` n'en
listait que trois. La reconstruction de feuille de temps de fin de journée échouait donc à **chaque
passage** depuis la fusion de la fonctionnalité — `(code: 275) CHECK constraint failed` — et c'est
**tout le job** qui s'arrêtait, pas seulement l'alerte. Seul le journal du service le montrait.

SQLite ne sait pas `ALTER` une contrainte `CHECK` : la migration applique la
[reconstruction de table documentée](https://sqlite.org/lang_altertable.html#otherxform)
(`CREATE TABLE new_alerts` avec les quatre valeurs → `INSERT … SELECT` colonne par colonne →
`DROP TABLE alerts` → `RENAME` → recréation de `idx_alerts_user_resolved`), avec quatre écarts
explicites par rapport aux 12 étapes :

- **étapes 2 et 11 (BEGIN / COMMIT)** : ce sont celles de `sqlx`, qui exécute chaque migration dans
  une transaction — un échec en cours de route laisse donc l'ancienne table intacte ;
- **étapes 1 et 12 (`PRAGMA foreign_keys` off/on)** : volontairement absentes. Ce pragma est un
  **no-op documenté à l'intérieur d'une transaction**, et il est inutile ici : `alerts` n'est que
  table *fille*. Rien ne la référence, donc ni le `DROP` ni le `RENAME` ne peut toucher la clé
  étrangère d'une autre table ;
- **étape 3 (inventaire)** : `sqlite_master` ne contient pour `alerts` qu'une table et un index
  explicite — aucun trigger, aucune vue. L'étape 9 et la moitié de l'étape 8 sont donc vides ;
- **étape 10 (`PRAGMA foreign_key_check`)** : ne peut pas faire échouer une migration depuis du SQL
  (elle renvoie des lignes au lieu de lever), donc elle est vérifiée dans la suite de tests
  (`database::connection::migration_tests`).

**Garde-fou de non-régression** : `alert_repo.rs` porte un test qui insère une alerte de **chaque**
variante de `AlertType`, la liste étant écrite via un `match` exhaustif. Ajouter une variante sans
migration devient une erreur de **compilation** au lieu d'un `CHECK constraint failed` levé le soir
au fond d'un job d'arrière-plan.

### 7.3 Notes

- All IDs are UUIDs stored as TEXT (both SQLite and Postgres support this).
- All datetimes are stored as ISO 8601 TEXT in SQLite. For the PostgreSQL migration, these become `TIMESTAMPTZ` columns.
- `participants` in meetings and `related_items` in alerts are stored as JSON TEXT arrays.
- Boolean fields use INTEGER (0/1) in SQLite, becoming `BOOLEAN` in Postgres.
- `sqlx` handles the dialect differences transparently via its `Any` pool or feature-flagged query macros.

---

### 7.4 Migration `018` — voies de présence et arbitrage par quart

#### 7.4.1 Ce qui a été supprimé, et pourquoi

`reconstruct_day` modélisait la journée sur **une seule piste** : chaque intervalle libre
était crédité au signal qui l'ouvrait (le « report », `reconstruction.rs:304-338` avant
suppression). Sur une journée à plusieurs sessions concurrentes, cette règle est
structurellement fausse et pas seulement imprécise : le 2026-08-10, le bloc 13:00–16:02 est
allé en entier à la tâche qui avait journalisé la première après le déjeuner, tandis qu'une
tâche active tout l'après-midi n'a récolté que six éclats de 1 à 6 minutes — **0,29 h pour
une journée de travail**.

Sont supprimés avec leurs tests : `reconstruct_day`, `finalize_day`, `is_low_signal`,
`AttributedBlock`, `BlockKind`, `Signal`, `SignalKind`, `DayInputs`, `MeetingBlock`,
`MeetingKind`, `EditedLine`, `renormalize_lines`, la mutation `saveTimesheetDraft` et le
type `TimesheetLineInput`. Sont conservés et réutilisés : `ReconstructionConfig`,
`apportion_to_target` (répartition au plus fort reste, avec seaux épinglés — exactement un
quart dont certaines parts sont fixées à la main), `Bucket`, `UnresolvedSignal`,
`ProjectAllocation`.

#### 7.4.2 Le pipeline

```
traces  →  voies  →  quarts  →  parts  →  lignes
```

| Étape | Couche | Module |
|---|---|---|
| traces | application | `use_cases/timesheet.rs` — entrées de journal, commits git, réunions, créneaux `manual` |
| voies | domaine (pur) | `rules/presence.rs` — `build_lanes`, `minutes_in`, `covered_minutes` |
| quarts | domaine (pur) | `rules/quarters.rs` — `quarters`, `allocate_quarter`, `allocate_day` |
| parts | domaine (pur) | `rules/quarters.rs` — `Share`, `Pin`, `DayPin` |
| lignes | application | somme des parts par `gryzzly_project_id` |

**`presence.rs` — l'ombre portée.** Un point de trace à `T` couvre
`[max(T − MAX_CONTINUATION_GAP_MINUTES, point précédent de la MÊME voie), T]`. Deux
écrêtages, deux rôles distincts : le point précédent de la voie empêche deux entrées
consécutives de compter deux fois la même minute ; le plafond de 45 minutes empêche une
entrée isolée de réclamer une matinée entière. La constante est **importée** de
`rules/worklog_time.rs` — pas redéclarée, pas configurable : c'est une règle métier qui
porte déjà sa justification mesurée, et un seuil qui différerait entre le journal et la
feuille de temps ferait divergier les deux vues par construction.

Les voies **se chevauchent** et `covered_minutes` est une **union**, jamais une somme : trois
voies concurrentes ne rendent pas un quart trois fois mieux attesté (245 minutes de présence
dans un quart de 120 minutes, sur données réelles du 2026-08-10).

**`quarters.rs` — la répartition.** `allocate_quarter` pondère chaque voie par ses minutes de
présence dans le quart, retire d'abord les minutes d'absence, puis appel
`apportion_to_target(&seaux, heures_déclarables, incrément)`. Invariant : les parts d'un
quart totalisent **exactement** sa durée déclarable, sur l'incrément d'arrondi. Une part
épinglée devient un seau `pinned` et le reste se rééquilibre autour.

#### 7.4.3 Trois changements de comportement délibérés

1. **Le total de la journée est la somme des quarts**, pas `workday.daily_target_hours`. Un
   quart qui totalise 2,00 h par construction ne peut pas simultanément totaliser 1,875 h.
   L'objectif devient une **vérification** signalée à l'écran et en CLI.
2. **L'épinglage au niveau de la ligne disparaît.** Les lignes sont dérivées des parts ; une
   ligne épinglée serait une seconde source de vérité que les quarts ne pourraient pas
   expliquer.
3. **La chronologie mono-piste est remplacée par les voies.** `blocks_json` n'est plus écrit ;
   une journée persistée avant ce changement n'affiche aucune vue des traces jusqu'à sa
   prochaine reconstruction, ce que l'écran énonce explicitement au lieu d'afficher une
   bande vide.

#### 7.4.4 Contrat GraphQL

`ReconstructedDayGql` gagne `lanes`, `quarters`, `outsideWorkday` et perd `blocks`.
Mutations : `setQuarterShare(date, quarterIndex, laneKey, hours)`,
`clearQuarterShare(date, quarterIndex, laneKey)`, `resetQuarter(date, quarterIndex)`.
`saveTimesheetDraft` est supprimée. Les intervalles voyagent en **minutes locales depuis
minuit** (`Int`) et non en horodatages : une voie est dessinée contre la grille du jour, et
un client qui doit analyser des datetimes pour positionner une barre finira par se tromper de
fuseau.

Sur relecture d'un brouillon (`from_draft`), les quarts sont reconstitués depuis les lignes
de parts et les plages configurées. Deux champs ne le sont pas : la confiance propre d'un
quart et ses heures d'absence sont des propriétés des **traces**, pas de la décision — une
journée relue rapporte la confiance du **jour** sur chaque quart et aucune absence.
Reconstruire rafraîchit les deux.

## 8. GraphQL API

### 8.1 Full Schema

```graphql
scalar DateTime
scalar Date
scalar JSON

# --- Enums ---

enum Source {
  JIRA
  EXCEL
  OBSIDIAN
  PERSONAL
}

enum TaskStatus {
  TODO
  IN_PROGRESS
  DONE
  BLOCKED
}

enum HalfDay {
  MORNING
  AFTERNOON
}

enum AlertType {
  DEADLINE
  OVERLOAD
  CONFLICT
  TIMESHEET_READY
}

enum AlertSeverity {
  CRITICAL
  WARNING
  INFORMATION
}

enum ProjectStatus {
  ACTIVE
  PAUSED
  COMPLETED
}

enum SyncSourceStatus {
  IDLE
  SYNCING
  SUCCESS
  ERROR
}

enum TrackingState {
  INBOX
  FOLLOWED
  DISMISSED
}

# --- Core Types ---

type Task {
  id: ID!
  title: String!
  description: String
  notes: String                        # Markdown user-owned, preserved across Jira syncs
  delegatedTo: String                  # Free-text delegate name, user-owned, preserved across syncs
  source: Source!
  sourceId: String
  jiraStatus: String
  status: TaskStatus!
  project: Project
  assignee: String
  deadline: Date
  plannedStart: DateTime
  plannedEnd: DateTime
  estimatedHours: Float
  urgency: Int!
  urgencyManual: Boolean!
  impact: Int!
  trackingState: TrackingState!
  jiraRemainingSeconds: Int
  jiraOriginalEstimateSeconds: Int
  jiraTimeSpentSeconds: Int
  remainingHoursOverride: Float
  estimatedHoursOverride: Float
  effectiveRemainingHours: Float       # Computed: override > Jira remaining > None
  effectiveEstimatedHours: Float       # Computed: override > Jira estimate > estimatedHours
  tags: [Tag!]!
  quadrant: String!
  createdAt: DateTime!
  updatedAt: DateTime!
}

type Meeting {
  id: ID!
  title: String!
  startTime: DateTime!
  endTime: DateTime!
  location: String
  participants: [String!]!
  project: Project
  durationHours: Float!
  halfDayConsumption: Float!
}

type Project {
  id: ID!
  name: String!
  source: Source!
  sourceId: String
  status: ProjectStatus!
  taskCount: Int!
  openTaskCount: Int!
}

type ActivitySlot {
  id: ID!
  task: Task
  startTime: DateTime!
  endTime: DateTime
  halfDay: HalfDay!
  date: Date!
  durationMinutes: Int
}

type Alert {
  id: ID!
  alertType: AlertType!
  severity: AlertSeverity!
  message: String!
  relatedTasks: [Task!]!
  relatedMeetings: [Meeting!]!
  date: Date!
  resolved: Boolean!
  createdAt: DateTime!
}

type Tag {
  id: ID!
  name: String!
  color: String
}

type SyncStatus {
  source: Source!
  lastSyncAt: DateTime
  status: SyncSourceStatus!
  errorMessage: String
}

# --- Timesheet Enums ---

enum TimesheetStatusGql {
  DRAFT
  VALIDATED
  SUBMITTED
}

enum ConfidenceGql {
  HIGH
  MEDIUM
  LOW
}

enum BlockKindGql {
  MEETING
  ACTIVITY
  UNTRACKED
}

enum MappingKindGql {
  REPOSITORY
  SUBJECT
  ORGANIZER
  INTERNAL_PROJECT
}

enum DayOffScopeGql {
  FULL_DAY
  MORNING
  AFTERNOON
}

# --- Composite Types ---

type DailyDashboard {
  date: Date!
  tasks: [Task!]!
  meetings: [Meeting!]!
  alerts: [Alert!]!
  weeklyWorkload: WeeklyWorkload!
  syncStatuses: [SyncStatus!]!
}

type WeeklyWorkload {
  weekStart: Date!
  capacity: Int!
  halfDays: [HalfDaySlot!]!
  totalPlanned: Float!
  totalMeetings: Float!
  overload: Float
}

# HalfDaySlot is used for the project assignment view (developer-to-project allocation).
# Individual tasks and meetings use hour-based time slots (plannedStart/plannedEnd).
type HalfDaySlot {
  date: Date!
  halfDay: HalfDay!
  meetings: [Meeting!]!
  tasks: [Task!]!
  consumption: Float!
  isFree: Boolean!
}

type PriorityMatrix {
  urgentImportant: [Task!]!
  important: [Task!]!
  urgent: [Task!]!
  neither: [Task!]!
}

type DeduplicationSuggestion {
  id: ID!
  taskA: Task!
  taskB: Task!
  confidenceScore: Float!
  titleSimilarity: Float!
  assigneeMatch: Boolean!
  projectMatch: Boolean!
}

# --- Timesheet Types ---

type ReconstructedDayGql {
  date: Date!
  status: TimesheetStatusGql!
  targetHours: Float!
  roundingIncrement: Float!
  totalHours: Float!
  dayConfidence: ConfidenceGql!
  lines: [EditedLineGql!]!
  unattributedHours: Float!
  unresolved: [UnresolvedGql!]!
  blocks: [BlockGql!]!
}

# `unresolved` et `blocks` sont servis AUSSI par la requête `timesheetDraft`, et plus
# seulement par la mutation `runTimesheetReconstruction` : `from_draft` les relit des
# colonnes `unresolved_json` / `blocks_json` du brouillon (migration 017). Les deux
# sont best-effort — JSON absent ou illisible donne une liste vide, jamais une erreur
# de requête, un jour antérieur à la colonne restant simplement sans explication.

# Le type réel des blocs est `AttributedBlockGql`
# (`start_time`, `end_time`, `gryzzlyProjectId`, `kind`, `hours`, `sourceRefs`, `originLabel`).
#
# `originLabel: String` (nullable) est le **libellé secondaire d'affichage** d'un bloc : le nom
# humain de ce dont il provient — titre de la **tâche** propriétaire pour un bloc `WORK`, **sujet
# de la réunion** pour un bloc `MEETING`. Non ambigu par construction : `reconstruct_day` bâtit
# chaque bloc à partir d'UN seul signal ou d'UNE seule réunion (d'où `source_refs` toujours à un
# élément), donc le libellé nomme cette origine unique — aucune jointure, aucune agrégation.
# Le champ naît sur `Signal.origin_label` (distinct de `Signal.label`, qui est le texte du signal
# lui-même : note de journal ou message de commit), est rempli dans `reconstruct_timesheet` avec
# le titre de la tâche DÉJÀ chargée pour résoudre le projet (aucune requête supplémentaire), et
# vaut `null` quand l'origine n'a pas de nom connu (commit sans clé Jira résolue).
#
# Persistance : `to_draft` l'écrit dans `blocks_json` sous la clé `originLabel`. **Aucune
# migration** — `blocks_json` est un blob JSON opaque. `parse_blocks_json` lit la clé de façon
# **optionnelle** : une journée reconstruite avant l'ajout du champ n'a pas la clé et doit rendre
# `null` pour CE bloc, sans jamais réduire la liste entière à vide (c'est exactement la panne
# livrée par `unresolved_json`). Seuls `start`, `end` et `kind` restent obligatoires.

type EditedLineGql {
  gryzzlyProjectId: String!
  projectName: String!
  hours: Float!
  isPinned: Boolean!
}

type UnresolvedGql {
  blockId: String!
  kind: BlockKindGql!
  title: String
  confidence: ConfidenceGql!
  hours: Float!
}

type BlockGql {
  blockId: String!
  kind: BlockKindGql!
  title: String
  startTime: DateTime
  endTime: DateTime
  confidence: ConfidenceGql!
}

type MappingSignalGql {
  id: String!
  kind: MappingKindGql!
  pattern: String!
  branchPattern: String
  gryzzlyProjectId: String!
  projectName: String!
  usageCount: Int!
}

input TimesheetLineInputGql {
  gryzzlyProjectId: String!
  hours: Float!
  isPinned: Boolean
}

# --- Microsoft Sign-In Gate ---

# Statut de session Microsoft. `authenticated` est vrai si un refresh token valide est stocké.
# `account` contient l'adresse email du compte connecté (null si non connecté).
type SessionGql {
  authenticated: Boolean!
  account: String
}

# --- Search ---

# Lean projection for global search. Excludes dismissed tasks (server-side filter).
# projectName and tags (names) are pre-resolved to avoid N+1 on the client.
type SearchableTask {
  id: ID!
  title: String!
  sourceId: String
  source: Source!
  assignee: String
  projectName: String
  tags: [String!]!
  description: String
  status: TaskStatus!
}

# --- v2 Types ---

type TeamMemberView {
  name: String!
  projects: [ProjectAllocation!]!
  totalLoad: Float!
  isOverloaded: Boolean!
}

type ProjectAllocation {
  project: Project!
  taskCount: Int!
  estimatedLoad: Float!
}

type WeeklyRetrospective {
  weekStart: Date!
  timeByProject: [ProjectTime!]!
  timeByTag: [TagTime!]!
  completedTasks: Int!
  remainingTasks: Int!
  dailyBreakdown: [DailyBreakdown!]!
}

type ProjectTime {
  project: Project!
  halfDays: Float!
  percentage: Float!
}

type TagTime {
  tag: Tag!
  halfDays: Float!
  percentage: Float!
}

type DailyBreakdown {
  date: Date!
  slots: [ActivitySlot!]!
  totalTrackedMinutes: Int!
}

# --- Input Types ---

input TaskFilter {
  status: [TaskStatus!]
  source: [Source!]
  trackingState: [TrackingState!]
  projectId: ID
  assignee: String
  deadlineBefore: Date
  deadlineAfter: Date
  tagIds: [ID!]
  sourceId: String           # Exact match on tasks.source_id (e.g. Jira key)
  titleContains: String      # Case-insensitive substring match on title
}

input CreateTaskInput {
  title: String!
  description: String
  notes: String                    # Markdown notes locales (préservées des syncs)
  projectId: ID
  deadline: Date
  plannedStart: DateTime
  plannedEnd: DateTime
  estimatedHours: Float
  impact: Int
  urgency: Int
  tagIds: [ID!]
}

input UpdateTaskInput {
  title: String
  description: String
  notes: String                    # null = clear, absent = don't change
  delegatedTo: String              # valeur = définir, null explicite = effacer, absent = inchangé (MaybeUndefined)
  projectId: ID
  deadline: Date
  plannedStart: DateTime
  plannedEnd: DateTime
  estimatedHours: Float
  status: TaskStatus
  impact: Int
  urgency: Int
  tagIds: [ID!]
  remainingHoursOverride: Float    # null = clear override, absent = don't change
  estimatedHoursOverride: Float    # null = clear override, absent = don't change
}

input CreateActivitySlotInput {
  startTime: DateTime!
  endTime: DateTime!
  taskId: ID           # optional task association
}

input UpdateActivitySlotInput {
  taskId: MaybeUndefined<ID>  # null = clear task association, absent = no change
  startTime: DateTime
  endTime: DateTime
}

input TeamFilter {
  projectId: ID
  assignee: String
  weekStart: Date
}

# --- Pagination ---

type PageInfo {
  hasNextPage: Boolean!
  endCursor: String
}

type TaskEdge {
  node: Task!
  cursor: String!
}

type TaskConnection {
  edges: [TaskEdge!]!
  pageInfo: PageInfo!
  totalCount: Int!
}

type AlertEdge {
  node: Alert!
  cursor: String!
}

type AlertConnection {
  edges: [AlertEdge!]!
  pageInfo: PageInfo!
  totalCount: Int!
}

# --- Queries ---

type Query {
  dailyDashboard(date: Date!): DailyDashboard!
  tasks(filter: TaskFilter, first: Int = 50, after: String): TaskConnection!
  task(id: ID!): Task
  priorityMatrix: PriorityMatrix!
  weeklyWorkload(weekStart: Date!): WeeklyWorkload!
  activityJournal(date: Date!): [ActivitySlot!]!
  currentActivity: ActivitySlot
  alerts(resolved: Boolean, first: Int = 50, after: String): AlertConnection!
  projects: [Project!]!
  project(id: ID!): Project
  tags: [Tag!]!
  syncStatuses: [SyncStatus!]!
  deduplicationSuggestions: [DeduplicationSuggestion!]!
  configuration: JSON!
  # Délégation — noms distincts triés (auto-complétion pour le champ delegatedTo)
  delegates: [String!]!
  # Search — lean projection, excludes dismissed tasks, used by the global search bar
  searchableTasks: [SearchableTask!]!
  # Microsoft Sign-In Gate — statut de session (authentifié / compte)
  session: SessionGql!
  # v2
  teamView(filter: TeamFilter): [TeamMemberView!]!
  weeklyRetrospective(weekStart: Date!): WeeklyRetrospective!
  
  # Timesheet operations
  timesheetDraft(date: Date!): ReconstructedDayGql!
  signalMappings: [MappingSignalGql!]!

  # Semantic memory
  # `id` accepte un UUID complet OU la référence courte du brief (`m:7c1`, `7c1`).
  # Un préfixe ambigu est une ERREUR, jamais un souvenir choisi au hasard.
  memory(id: ID!): MemoryGql
  # `q` est de la saisie BRUTE : `AP-1234` et `Cartier : certificat` sont sûrs ici.
  recall(
    q: String!
    projectId: ID
    taskId: ID
    stakeholders: [String!]
    includeHistory: Boolean! = false
    limit: Int! = 10
  ): [ScoredMemoryGql!]!
  pendingMemories(limit: Int! = 50, offset: Int! = 0): [MemoryGql!]!
  # Filigrane de consolidation, côté lecture : les entrées de journal jamais lues
  # par la consolidation, de la PLUS ANCIENNE à la plus récente. Marqueur par
  # entrée, pas curseur horodaté (R59).
  unconsolidatedWorklogEntries(limit: Int! = 200): [WorklogEntryGql!]!
  # Brief de démarrage de session. `lines` porte le rendu déjà plafonné à 40 lignes ;
  # les champs structurés servent les clients qui veulent leur propre mise en forme.
  # S'AJOUTE à la liste des tâches suivies, ne la remplace pas.
  brief(
    variant: BriefVariantGql! = SESSION
    projectId: ID
    date: NaiveDate
  ): BriefGql!
}

# --- Mutations ---

type Mutation {
  # Task management
  createTask(input: CreateTaskInput!): Task!
  updateTask(id: ID!, input: UpdateTaskInput!): Task!
  deleteTask(id: ID!): Boolean!
  # Append text to a task's user-owned `notes` field (used by the activity quick-note input)
  appendTaskNotes(taskId: ID!, text: String!): Task!

  # Worklog time materialization — rebuilds a task's closed activity slots in the
  # local half-days its window touched. The window (sessionId's own last-flush
  # mark, or aplan.active_since when sessionId is omitted) only selects which
  # half-days to rebuild; it does not clear the active-task pointer.
  # Idempotent: re-running produces the same slots, never duplicates.
  flushWorklogTime(taskId: ID!, sessionId: String): FlushResultGql!

  # Réattribution — moves worklog entries between tasks and REBUILDS the slots they
  # project to, in the affected half-days only. `confirm` defaults to false: the call
  # then reports what it would do and writes nothing.
  reattributeWorklogEntries(input: ReattributeWorklogInput!): ReattributionResultGql!

  # Réparation des créneaux orphelins — supprime les créneaux d'une plage de jours
  # locaux qui ont perdu leur task_id (ON DELETE SET NULL déclenché par un
  # INSERT OR REPLACE INTO tasks) et réécrit leurs demi-journées depuis le journal.
  # Ne touche jamais un créneau `manual` : sans tâche, ce n'est pas un dégât.
  # `confirm` par défaut à false : l'appel décrit alors ce qu'il ferait, sans écrire.
  repairOrphanedSlots(input: RepairOrphanedSlotsInput!): SlotRepairResultGql!

  # Triage / Tracking state
  setTrackingState(taskId: ID!, state: TrackingState!): Task!
  setTrackingStateBatch(taskIds: [ID!]!, state: TrackingState!): [Task!]!

  # Priority
  updatePriority(taskId: ID!, urgency: Int, impact: Int): Task!
  resetUrgency(taskId: ID!): Task!

  # Activity tracking
  startActivity(taskId: ID): ActivitySlot!
  stopActivity: ActivitySlot
  createActivitySlot(input: CreateActivitySlotInput!): ActivitySlot!
  updateActivitySlot(id: ID!, input: UpdateActivitySlotInput!): ActivitySlot!
  deleteActivitySlot(id: ID!): Boolean!

  # Alerts
  resolveAlert(id: ID!): Alert!

  # Deduplication
  linkTasks(taskIdPrimary: ID!, taskIdSecondary: ID!): Boolean!
  unlinkTasks(taskIdPrimary: ID!, taskIdSecondary: ID!): Boolean!
  confirmDeduplication(suggestionId: ID!, accept: Boolean!): Boolean!

  # Tags
  createTag(name: String!, color: String): Tag!
  updateTag(id: ID!, name: String, color: String): Tag!
  deleteTag(id: ID!): Boolean!

  # Sync
  forceSync(source: Source): [SyncStatus!]!

  # Meeting-Project association
  updateMeetingProject(meetingId: ID!, projectId: ID): Meeting!

  # Configuration
  updateConfiguration(key: String!, value: JSON!): Boolean!

  # Microsoft Sign-In Gate — efface les jetons stockés (déconnexion)
  signOut: Boolean!

  # Timesheet operations
  runTimesheetReconstruction(date: Date!): ReconstructedDayGql!
  saveTimesheetDraft(date: Date!, lines: [TimesheetLineInputGql!]!): ReconstructedDayGql!
  validateTimesheet(date: Date!): ReconstructedDayGql!
  markDayOff(date: Date!, scope: DayOffScopeGql!): ReconstructedDayGql!
  learnMapping(kind: MappingKindGql!, pattern: String!, branchPattern: String, gryzzlyProjectId: String!): Boolean!

  # Semantic memory
  remember(input: RememberInputGql!): MemoryGql!

  # Validation queue. `accepted` est null quand `nearDuplicates` est non vide :
  # rien n'a été écrit, l'appelant doit choisir merge/supersede ou forcer.
  #
  # TOUT argument d'identifiant ci-dessous accepte un UUID complet OU la référence
  # courte affichée par le brief et l'inbox (`m:7c1`, `7c1`) : c'est le seul
  # identifiant que l'utilisateur voit passer. Même résolution que `memory(id:)`
  # (application::use_cases::memory::resolve_memory_id). Inconnu -> « Not found »,
  # ambigu -> erreur listant les candidats. Pour les verbes à DEUX identifiants,
  # les deux sont résolus AVANT toute écriture.
  acceptMemory(id: ID!, kind: MemoryKindGql, force: Boolean! = false): AcceptMemoryResultGql!
  rejectMemory(id: ID!): MemoryGql!
  # Une seule ligne survit — efface l'historique.
  mergeMemory(id: ID!, into: ID!): MergeMemoryResultGql!
  # Les deux lignes survivent. SEUL chemin qui écrit invalidatedAt.
  # `old` omis => retombe sur by.proposedSupersedes (la proposition portée par le
  # candidat). Un candidat qui ne propose rien est refusé (code 4), jamais une
  # supersession de rien.
  supersedeMemory(old: ID, by: ID!): SupersedeMemoryResultGql!
  # Import one-shot, idempotent, lecture seule sur le dossier.
  importMemories(directory: String!): MemoryImportResultGql!

  # Consolidation (lot 5). Filigrane côté écriture : à appeler UNIQUEMENT après que
  # les souvenirs produits par ces entrées sont persistés (R59). Idempotent — un id
  # déjà marqué ou appartenant à un autre utilisateur ne déplace aucune ligne et
  # n'est pas une erreur, d'où `marked` <= `requested`.
  markWorklogEntriesConsolidated(ids: [ID!]!): MarkConsolidatedResultGql!
  # Écrit `memory.consolidation.last_run` dans `configuration` — la clé que lit le
  # brief (R57). `at` vaut maintenant par défaut.
  recordConsolidationRun(at: DateTime): ConsolidationRunGql!
}

type FlushResultGql {
  activeSince: DateTime!       # début de la fenêtre-sélecteur du prochain flush (pas un filigrane)
  slotsWritten: Int!           # nombre de créneaux écrits par ce flush
}

type MarkConsolidatedResultGql {
  requested: Int!              # nombre d'ids soumis
  marked: Int!                 # nombre de lignes réellement passées de non marqué à marqué
  consolidatedAt: DateTime!
}

# Réattribution (US-RE). Une seule des deux sélections à la fois : les entrées
# explicites, ou la fenêtre de dates locales de la tâche source.
input ReattributeWorklogInput {
  fromTask: ID!
  toTask: ID!
  entryRefs: [String!]         # UUID complet ou préfixe ; ambiguïté signalée, jamais devinée
  since: NaiveDate             # premier jour local (inclus)
  until: NaiveDate             # dernier jour local (inclus) ; défaut = since
  confirm: Boolean             # absent ou false ⇒ aperçu, rien n'est écrit
}

type ReattributionResultGql {
  applied: Boolean!            # false ⇒ aucune écriture
  selectedEntries: [ID!]!
  movedEntries: Int!           # 0 en aperçu ; < selectedEntries si une ligne a quitté la source entre-temps
  affectedDates: [NaiveDate!]!
  slotsDiscarded: Int!         # créneaux fermés des deux tâches retirés des demi-journées touchées
  slotsRebuilt: Int!           # créneaux réécrits depuis les entrées
  source: TaskTimeChangeGql!
  destination: TaskTimeChangeGql!
}

type TaskTimeChangeGql {
  taskId: ID!
  hoursBefore: Float!
  hoursAfter: Float!
}

# Réparation des créneaux orphelins (US-SR). Les deux bornes sont obligatoires :
# aucune valeur par défaut n'est défendable pour une réécriture d'historique.
input RepairOrphanedSlotsInput {
  from: NaiveDate!             # premier jour local (inclus)
  to: NaiveDate!               # dernier jour local (inclus)
  confirm: Boolean             # absent ou false ⇒ aperçu, rien n'est écrit
}

type SlotRepairResultGql {
  applied: Boolean!            # false ⇒ aucune écriture
  from: NaiveDate!
  to: NaiveDate!
  dates: [DateRepairGql!]!     # une entrée par jour porteur d'un orphelin ; vide ⇒ plage saine
  tasks: [RepairedTaskGql!]!   # les tâches découvertes, pas nommées par l'appelant
  orphansDropped: Int!
  orphanHours: Float!          # ce que les orphelins valaient ; à comparer aux hoursAfter
  slotsDiscarded: Int!         # créneaux propres aux tâches reconstruites, remplacés
  slotsWritten: Int!
}

type DateRepairGql {
  date: NaiveDate!
  orphansDropped: Int!
  orphanHours: Float!
  slotsDiscarded: Int!
  slotsWritten: Int!           # 0 alors que orphansDropped > 0 ⇒ du temps perdu, signalé
}

type RepairedTaskGql {
  taskId: ID!
  task: TaskGql                # hydratée : le rapport doit nommer ce qu'il réécrit
  hoursBefore: Float!          # projection propre à la tâche ; hors orphelins
  hoursAfter: Float!
}

type ConsolidationRunGql {
  key: String!                 # `memory.consolidation.last_run`, pour vérifier la cible
  ranAt: DateTime!
}

type AcceptMemoryResultGql {
  accepted: MemoryGql          # null si bloqué par un quasi-doublon
  nearDuplicates: [MemoryGql!]!
}

type MergeMemoryResultGql {
  survivor: MemoryGql!
  discardedId: ID!
}

type SupersedeMemoryResultGql {
  invalidated: MemoryGql!      # porte invalidatedAt + supersededBy
  successor: MemoryGql!
}

type SkippedMemoryFileGql {
  fileName: String!
  reason: String!              # already_imported | no_frontmatter | no_title
}

type MemoryImportResultGql {
  imported: [MemoryGql!]!
  skipped: [SkippedMemoryFileGql!]!
  importedCount: Int!
  skippedCount: Int!
}

# --- Semantic memory (migration 012) ---

enum MemoryKindGql { DECISION COMMITMENT FACT PREFERENCE }
enum MemorySourceGql { CLAUDE_SESSION MANUAL DREAMING }
enum MemoryStatusGql { PENDING ACTIVE REJECTED }

type MemoryGql {
  id: ID!
  kind: MemoryKindGql!
  title: String!
  body: String
  occurredAt: DateTime!
  recordedAt: DateTime!
  invalidatedAt: DateTime      # null = encore vrai
  supersededBy: ID
  # Supersession PROPOSÉE, pas encore appliquée (migration 013, § 7.2.1). Ne
  # figure que sur un candidat PENDING : tout verdict de la file l'efface.
  proposedSupersedes: ID
  source: MemorySourceGql!
  sourceRef: String
  status: MemoryStatusGql!
  projectId: ID
  taskId: ID
  stakeholders: [String!]!
  # Le souvenir nommé par proposedSupersedes, résolu : un identifiant seul ne dit
  # pas QUELLE décision est contredite. null si rien n'est proposé, ou si le
  # souvenir nommé a été supprimé depuis (la colonne est ON DELETE SET NULL).
  contradicts: MemoryGql
}

type ScoredMemoryGql {
  memory: MemoryGql!
  score: Float!
}

input RememberInputGql {
  kind: MemoryKindGql!
  title: String!
  body: String
  occurredAt: DateTime
  source: MemorySourceGql      # défaut : CLAUDE_SESSION
  sourceRef: String
  confirmed: Boolean           # défaut : false → status = PENDING
  # Le souvenir actif que ce candidat CONTREDIT : UUID complet ou référence
  # courte (m:7c1). Incompatible avec confirmed: true — une proposition est une
  # question posée à la file, et une ligne confirmée n'y entre pas.
  proposedSupersedes: ID
  projectId: ID
  taskId: ID
  stakeholders: [String!]
}

# --- Subscriptions ---

type SyncEvent {
  source: Source!
  status: SyncSourceStatus!
  progress: Float
  message: String
}

type ActivityReminder {
  reminderType: String!
  message: String!
  suggestedTasks: [Task!]!
  endedMeeting: Meeting
}

type Subscription {
  syncProgress: SyncEvent!
  activityReminder: ActivityReminder!
  alertsUpdated: [Alert!]!
}
```

### 8.2 GraphQL Schema Codegen (SDL Regeneration)

After any changes to the GraphQL schema (types, queries, mutations, enums), the exported SDL must be regenerated to keep the CLI's committed `schema.graphql` file in sync with the backend.

**Command:**

```bash
cd backend
cargo run -p api -- export-schema > crates/cli/graphql/schema.graphql
```

This exports the complete GraphQL SDL from the async-graphql schema introspection and writes it to the CLI's schema file. The CLI's `build.rs` uses this committed schema to perform compile-time validation of GraphQL operations (via `graphql_client`'s codegen).

**When to regenerate:**
- After adding, modifying, or removing any GraphQL type, query, mutation, subscription, or enum
- Before committing backend code that changes the API surface
- Before running CLI tests or builds to avoid stale-schema errors

**Important:** The regeneration must happen **before** any CLI code that uses the new schema is committed, to avoid breaking the CLI build.

---

## 9. External Integrations

### 9.1 Jira REST API

**Authentication:** API token (Basic Auth with `email:token` base64-encoded) or OAuth 2.0 (for Jira Cloud).

**Endpoints used:**

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/rest/api/3/search` | GET | Search issues with JQL |
| `/rest/api/3/project` | GET | List all accessible projects |

**JQL Queries:**
```
project IN ({configured_keys})
  AND (assignee = currentUser() OR assignee IN ({team_members}))
  ORDER BY updated DESC
```

**Response mapping (Jira -> Domain):**

| Jira Field | Domain Field | Transformation |
|-----------|-------------|---------------|
| `key` | `source_id` | Direct |
| `fields.summary` | `title` | Direct |
| `fields.description` | `description` | ADF -> plain text |
| `fields.status.name` | `jira_status` | Direct — raw Jira status string stored as-is |
| `fields.status.statusCategory.key` | `status` | Map: "new"->Todo, "indeterminate"->InProgress, "done"->Done. Uses Jira status category (3 universal values) rather than custom status names. |
| `fields.assignee.displayName` | `assignee` | Direct |
| `fields.duedate` | `deadline` | Parse ISO date |
| `fields.project.key` | project `source_id` | Direct |
| `fields.project.name` | project `name` | Direct |

**Rate limiting:** Jira Cloud allows ~100 requests/minute. The sync should paginate with `maxResults=100` and respect rate limit headers.

### 9.2 Microsoft Graph API

**Authentication:** Flux *authorization code* (client confidentiel) contre l'application Entra mono-locataire `12dd5cbd-f897-4184-a473-8effc7a93aba`. Scopes demandés : `Calendars.Read Files.Read.All offline_access openid profile`. Le **consentement administrateur est accordé au niveau du locataire** (`consentType: AllPrincipals`) — la connexion n'affiche aucune invite de consentement.

**Porte d'authentification Microsoft (démarrage de l'application) :**

L'application est bloquée au démarrage tant qu'aucune session Microsoft valide n'est détectée. Un seul jeton Graph couvre à la fois le connecteur Outlook (calendrier) et le connecteur Excel/SharePoint.

1. L'application frontend charge le composant `AuthGate`, qui interroge la query GraphQL `session { authenticated account }`.
2. Si `authenticated` est `false`, l'`AuthGate` affiche l'écran de connexion avec un bouton « Se connecter avec Microsoft » pointant vers `GET /auth/microsoft/login`.
3. Le navigateur est redirigé vers `GET /auth/microsoft/login`, qui génère un état CSRF à usage unique (TTL 10 min, stocké en mémoire), puis redirige vers `https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize`.
4. Microsoft redirige vers `GET /auth/microsoft/callback?code=...&state=...`. Le handler valide l'état CSRF, échange le code contre un `TokenSet` (`access_token` + `refresh_token` + `expires_at`) via l'endpoint token Microsoft, persiste les jetons chiffrés dans la table `configuration` (clés `microsoft.*`), puis redirige le navigateur vers `http://localhost:3000/?auth=connected`.
5. L'`AuthGate` détecte le paramètre `auth=connected`, rafraîchit la query `session` et lève le blocage — l'application s'affiche normalement. L'en-tête affiche l'adresse email du compte connecté et un bouton « Se déconnecter ».

**Routes Axum :**

| Méthode | Chemin | Description |
|---------|--------|-------------|
| `GET` | `/auth/microsoft/login` | Génère l'état CSRF, redirige vers la page d'autorisation Microsoft |
| `GET` | `/auth/microsoft/callback` | Échange le code, persiste les jetons, redirige vers `http://localhost:3000/?auth=connected` (ou `?auth=error&reason=...` en cas d'échec) |

**Renouvellement automatique — `RefreshingGraphTokenProvider` :**

`RefreshingGraphTokenProvider` (infrastructure) implémente le trait `GraphTokenProvider` (application). Ce fournisseur est **partagé** par les connecteurs Outlook et Excel/SharePoint — un seul appel `valid_access_token` alimente les deux lors d'une synchronisation. À chaque demande de jeton, il :

1. Lit `microsoft.access_token` et `microsoft.token_expires_at` depuis la table `configuration`.
2. Si l'expiration est dans moins de 60 secondes, appelle l'endpoint token Microsoft avec le `refresh_token` stocké, met à jour les quatre clés (`microsoft.access_token`, `microsoft.refresh_token`, `microsoft.token_expires_at`, `microsoft.account`) — rotation du refresh token incluse.
3. Retourne le jeton d'accès frais.

En cas d'erreur `invalid_grant` lors du renouvellement, `microsoft.refresh_token` et `microsoft.access_token` sont effacés (mis à `""`), la session est invalidée et `session { authenticated }` retourne `false` — l'`AuthGate` ramène l'utilisateur à l'écran de connexion.

**Correctif horizon de synchronisation :** La synchronisation Outlook lit la clé de configuration `outlook.calendar_days` (défaut : 14) pour calculer la fenêtre temporelle. L'horizon fixe précédent de 30 jours est supprimé.

**Token management:** The backend stores refresh tokens (encrypted) and automatically refreshes access tokens. For local mode, the initial auth flow is handled by the interactive sign-in routes above.

**Endpoints used:**

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/me/calendarView?startDateTime=...&endDateTime=...` | GET | Calendar events in range |
| `/sites/{site-id}/drive/root:/{path}` | GET | Locate Excel file on SharePoint |
| `/sites/{site-id}/drive/items/{item-id}/workbook/worksheets/{sheet}/usedRange` | GET | Read Excel data |

**Calendar event mapping (Graph -> Domain):**

| Graph Field | Domain Field | Transformation |
|------------|-------------|---------------|
| `id` | `outlook_id` | Direct |
| `subject` | `title` | Direct |
| `start.dateTime` | `start_time` | Parse ISO datetime |
| `end.dateTime` | `end_time` | Parse ISO datetime |
| `location.displayName` | `location` | Direct |
| `attendees[].emailAddress.name` | `participants` | Extract names |
| `isCancelled` | -- | If true, skip/delete |

**Project auto-detection:** After mapping, the sync engine scans the meeting `title` for known project names (case-insensitive substring match). If found, `project_id` is set automatically. The user can override this association via the `updateMeetingProject` mutation.

**Excel data mapping:**
The Excel file structure is fully configurable (R24-R26). The `ExcelMappingConfig` defines which columns map to which fields. The raw cell values are read from the `usedRange` response and mapped to `ExcelRow` structs using the configured column names.

### 9.3 Obsidian Integration (v2)

**Method:** Direct filesystem access. The backend reads `.md` files from the configured vault path.

**Parsing rules:**
- Scan files matching configured glob patterns (e.g., `**/*.md`)
- Extract tasks matching Markdown checkbox syntax: `- [ ] Task text` and `- [x] Completed task`
- Identify tasks tagged with configured tags (e.g., `#task`, `#todo`)
- Extract metadata: file path (as source reference), tags, completion status

**Note:** This is local file I/O only, unsuitable for Teams deployment. In Teams mode, Obsidian integration would be disabled or replaced with a file upload mechanism.

---

## 10. Synchronization Engine

### 10.1 Architecture

The sync engine runs as a background task in the Axum server process, using `tokio-cron-scheduler` for periodic execution.

```
+------------------------------------------+
|           Sync Scheduler                 |
|   (tokio-cron-scheduler, configurable)   |
+------------------------------------------+
|                                          |
|  +-----------+ +----------+ +--------+  |
|  | Jira Sync | | Outlook  | | Excel  |  |
|  |  Worker   | |  Sync    | |  Sync  |  |
|  +-----+-----+ +----+-----+ +---+----+  |
|        |             |           |       |
|        +-------------+-----------+       |
|                      |                   |
|              Sync Coordinator            |
|        (sequences sync steps,            |
|         emits SSE events)                |
+------------------------------------------+
```

### 10.2 Sync Flow (per source)

```
1. Update sync_status -> "syncing"
2. Emit SyncEvent subscription (status: SYNCING)
3. Fetch data from external API
   |-- Success: proceed to step 4
   +-- Failure: update sync_status -> "error", emit SyncEvent, stop
4. Transform external data -> domain types (mapper functions)
5. Reconcile with existing data:
   |-- New items -> INSERT
   |-- Changed items -> UPDATE (preserve local overrides: manual urgency, tags)
   +-- Deleted items -> mark for removal (notify user if local data exists)
6. Run deduplication engine (Jira <-> Excel)
7. Run alert engine (deadline, overload, conflict checks)
8. Update sync_status -> "success" + timestamp
9. Emit SyncEvent subscription (status: SUCCESS)
10. Emit alertsUpdated subscription (if alerts changed)
```

### 10.3 Sync Rules

| Rule | Implementation |
|------|---------------|
| **R04** | Sync on app open: triggered when first GraphQL query is received (or explicit `forceSync` mutation) |
| **R05** | Background sync interval: `tokio-cron-scheduler` job, configurable (default: 15 min) |
| **R06** | Cache: all synced data is in SQLite. If API call fails, existing data remains |
| **R07** | Local data (personal tasks, activity journal, priorities) never depends on sync |

### 10.4 Idempotency

Sync operations are idempotent. Running the same sync twice with unchanged external data produces no database changes. This is achieved by:
- Matching on `(user_id, source, source_id)` to detect existing records
- Comparing field values before UPDATE (only write if changed)
- Using `UPSERT` (INSERT ON CONFLICT UPDATE) for meetings (matched on `outlook_id`)

### 10.5 Preserving Local Overrides

When a synced task is updated from the source, the following local fields are **never overwritten**:
- `urgency` + `urgency_manual` (if `urgency_manual = true`)
- `impact`
- `tags`

These fields belong to the user's local enrichment and persist across syncs.

### 10.6 Source de synchronisation `gryzzly`

La source `gryzzly` synchronise en lecture seule le **catalogue Gryzzly** (projets actifs **et terminés**, avec leurs tâches). Contrairement aux autres sources, elle ne crée pas de tâches aplan : elle alimente une table cache (`gryzzly_tasks`) utilisée pour proposer une tâche Gryzzly lors de la déclaration d'activité.

> **Migration 015 — prérequis absolu.** Le CHECK de `sync_status.source` écrit en 001 n'autorisait
> que 4 valeurs (`jira`, `outlook`, `excel`, `obsidian`) alors que `domain::Source` en compte 6 :
> `gryzzly` et `personal` manquaient. Comme l'étape 1 ci-dessous écrit `sync_status(gryzzly)`, **tout**
> `aplan sync --source gryzzly` échouait sur `(code: 275) CHECK constraint failed` avant d'atteindre le
> connecteur — la source n'avait donc jamais tourné une seule fois, indépendamment de l'authentification.
> Même classe de bug que le CHECK `alerts.alert_type` corrigé par 013. La migration 015 élargit la
> contrainte, et `sync_status_accepts_every_source_variant` (dans `database::connection`) fait échouer
> le prochain ajout de variante ici plutôt qu'en production.

Déroulé de `sync_gryzzly` (use case `application::use_cases::sync::sync_gryzzly`) :

1. `sync_status(gryzzly)` -> `syncing`.
2. `GryzzlyClient::fetch_projects(active_only = false)` puis `GryzzlyClient::fetch_tasks(project_ids)`.
   - **`active_only = false`** : les projets **terminés** (`status = "done"`) entrent eux aussi dans le
     catalogue, marqués comme tels, au lieu d'être élagués et donc invisibles. Seuls les projets
     **soft-deleted** (`deleted_at` non nul) sont exclus, dans les deux modes.
   - En cas d'échec d'un appel : `sync_status` -> `error` (message du connecteur) et retour d'une `AppError::Connector`.
3. **Garde « empty-fetch »** : si `fetch_tasks` retourne une liste vide, l'élagage (`soft_prune_missing`) est **ignoré** afin de ne jamais désactiver des lignes du catalogue sur un fetch transitoirement vide (un assignment existant doit toujours pouvoir être résolu). `sync_status` est marqué en `error` avec le message `empty catalog fetch — skipping prune` et le résultat indique `tasks_created = 0`, `tasks_removed = 0`.
4. Sinon, chaque tâche est `upsert`ée dans `gryzzly_tasks` (clé `(user_id, gryzzly_task_id)`), avec dénormalisation du nom de projet et du client (`customer_name`) issus des projets.
5. `soft_prune_missing(user_id, keep_ids)` : désactive (`is_active = 0`) toute ligne de l'utilisateur dont le `gryzzly_task_id` n'est pas dans le lot synchronisé. **Jamais de suppression physique** — une ligne désactivée reste résoluble par `find_by_gryzzly_task_id`.
6. `sync_status(gryzzly)` -> `success`.

Les compteurs `tasks_created` / `tasks_removed` du `SyncResult` comptent des **lignes de catalogue**, pas des tâches aplan.

#### Contrat de l'API interne Gryzzly

Gryzzly ne délivre **aucune clé d'API**. L'API est de style RPC : chaque méthode est un `POST`
sur `https://api.gryzzly.io/<méthode>` (pas de préfixe `/v1`), avec un corps JSON et une
enveloppe `{ok, payload}` — y compris pour les lectures. Le `POST` est donc le transport, pas une
écriture : l'intégration reste **strictement en lecture seule** et n'appelle jamais
`declarations.create` / `.update` / `.delete`.

| Méthode | Corps | Rôle |
|---------|-------|------|
| `view/projects.list` | `{filter:"", range:"", search:"", limit:500}` | liste des projets |
| `expandedProjectMetrics.get` | `{project_id}` | projet complet, dont l'arbre `tasks` |
| `self.getIdentity` | `{}` | sonde de connectivité uniquement |

- **`limit` plafonne à 500.** Envoyer 1000 renvoie
  `{"ok":false,"errors":["decoding: invalid_argument: limit (out of range, max=500)"]}`.
- **`limit` est une taille de lot *avant* filtrage, pas une taille de page.** Les pages arrivent
  donc plus courtes que demandé, et une page courte — voire vide — ne signifie pas la fin des
  données. La pagination se fait par le paramètre `cursor` (on renvoie la valeur reçue) et **ne
  s'arrête que lorsque `cursor` est nul ou vide**. Une garde de 200 pages transforme un curseur qui
  ne se termine jamais en erreur plutôt qu'en boucle infinie.
- **Activité d'un projet** : `status == "active"` et `deleted_at` nul. Valeurs observées de
  `status` : `active`, `done`. Il n'existe aucun champ `archived`.
- **Activité d'une tâche** : `completed_at` et `deleted_at` nuls — **et rien d'autre**. Les tâches
  `is_container` (regroupements, non déclarables) sont **conservées** dans le catalogue.

> **Changement de sémantique de `gryzzly_tasks.is_active` (migration 016).** `map_task` combinait
> auparavant l'activité du projet à celle de la tâche, ce qui rendait une tâche d'un projet **clos**
> indistinguable d'une tâche **supprimée** dans Gryzzly : les deux arrivaient avec `is_active = 0`,
> et l'interface affichait `stale` dans les deux cas. Désormais `is_active` ne décrit que la tâche
> elle-même, et l'état du projet voyage séparément dans `project_status` (`active` | `done`, NULL =
> inconnu, lu comme actif — une ligne écrite par `scripts/gryzzly/import_catalog.py` précède la
> colonne et ne doit pas s'afficher comme terminée). Conséquence attendue au premier sync suivant :
> les lignes désactivées uniquement parce que leur projet était clos redeviennent actives, donc le
> nombre de tâches actives augmente d'un coup. C'est la correction, pas une régression.

#### Authentification

L'unique identifiant est le cookie de session `remember_token` posé sur `.gryzzly.io` par la
connexion SSO Microsoft sur `app.gryzzly.io`, envoyé en en-tête `Authorization: User <token>`. Sa
durée de vie est **fixe : 7 jours, non glissante** — utiliser l'application ne la prolonge pas, il
faut donc se reconnecter une fois par semaine.

Le jeton est fourni par un `GryzzlyTokenSource` (trait applicatif), avec deux implémentations dans
l'infrastructure, dans l'ordre de préférence de `forceSync` :

1. `StaticTokenSource` — la valeur de `gryzzly.token`, collée à la main.
2. `BrowserCookieTokenSource` — lecture du cookie dans un profil navigateur local de la famille
   Chromium (`Cookies` à la racine du profil ou sous `Network/`). Le fichier est ouvert en
   `read_only` + `immutable` (donc lisible même navigateur ouvert), puis la valeur est déchiffrée :
   clé PBKDF2-HMAC-SHA1 (sel `saltysalt`, 1 itération, 16 octets) et AES-128-CBC (IV = 16 espaces).
   Le mot de passe dépend du tag de version — `v10` : littéral `peanuts` ; `v11` : secret
   « Safe Storage » du trousseau, lu via `secret-tool`. Les Chromium récents préfixent le clair
   d'une empreinte SHA-256 de 32 octets, qui est retirée.

Si aucun cookie n'est trouvé et que `gryzzly.token` est vide, le client n'est pas construit et la
source est marquée `Not configured`. Un cookie **expiré**, lui, produit une erreur datée invitant à
se reconnecter (et non un simple `Not configured`).

Clés de configuration :

| Clé | Type | Défaut | Description |
|-----|------|--------|-------------|
| `gryzzly.base_url` | string | `https://api.gryzzly.io` | URL de base de l'API interne Gryzzly (pas de préfixe `/v1`). |
| `gryzzly.token` | string (secret) | `""` | Jeton de session collé à la main (`User <token>`, préfixe optionnel). Prioritaire sur le cookie ; sert d'échappatoire si la lecture du cookie casse. |
| `gryzzly.cookie_profile` | string | `""` | Chemin absolu vers un fichier `Cookies` de profil navigateur. Vide = détection automatique. |

Le client `HttpGryzzlyClient` est construit dynamiquement par la mutation `forceSync` à partir de ces clés (à l'image des connecteurs Jira/Outlook/Excel). Le repository `SqliteGryzzlyCatalogRepository` est injecté dans le contexte GraphQL au démarrage.

### 10.7 Mutation `assignGryzzlyTask`

La mutation GraphQL `assignGryzzlyTask(taskId: ID!, gryzzlyTaskId: ID): Task` permet d'associer (ou de dissocier) une tâche aplan à une tâche du catalogue Gryzzly.

**Use case** : `application::use_cases::gryzzly_assignment::assign_gryzzly_task(task_repo, catalog_repo, task_id, gryzzly_task_id: Option<String>)`.

Comportement :
- Si `gryzzlyTaskId` est fourni : le use case vérifie que la tâche existe dans `gryzzly_tasks` via `find_by_gryzzly_task_id` (renvoie une `AppError::Validation` si absente) puis **snapshote** le `gryzzly_project_id` du catalogue dans la tâche — ainsi une future déclaration d'heures n'a pas besoin d'un catalogue en vie.
- Si `gryzzlyTaskId` est `null` : les deux champs `gryzzly_task_id` et `gryzzly_project_id` de la tâche sont mis à `null` (dissociation).
- La tâche mise à jour est persistée via `task_repo.save` et renvoyée au client.

### 10.8 Requête `gryzzlyTasks` et champ `Task.gryzzlyTask`

#### Requête `gryzzlyTasks`

```graphql
gryzzlyTasks(search: String, projectFilter: String, limit: Int = 100): [GryzzlyTaskGql!]!
```

Retourne les entrées **actives** (`is_active = 1`) du catalogue Gryzzly de l'utilisateur courant, triées par `project_name` puis `name`, plafonnées à `limit`.

- `search` : filtre optionnel sur le nom de la tâche ou du projet (recherche insensible à la casse).
- `projectFilter` : filtre optionnel de correspondance exacte sur `project_name`.
- `limit` : nombre maximum de résultats (défaut 100).

Type de retour `GryzzlyTaskGql` :

| Champ | Type GraphQL | Description |
|-------|-------------|-------------|
| `gryzzlyTaskId` | `String!` | Identifiant Gryzzly de la tâche |
| `name` | `String!` | Libellé de la tâche |
| `gryzzlyProjectId` | `String!` | Identifiant Gryzzly du projet |
| `projectName` | `String!` | Nom du projet (dénormalisé) |
| `customerName` | `String` | Nom du client (optionnel) |

#### Champ `Task.gryzzlyTask`

```graphql
type Task {
  # ...
  gryzzlyTask: AssignedGryzzlyTaskGql
}
```

Résout l'assignment Gryzzly d'une tâche aplan. Retourne `null` si la tâche n'est pas assignée à une tâche Gryzzly. Sinon expose trois états de péremption via le champ `stale` :

| État | `stale` | `name` | Description |
|------|---------|--------|-------------|
| 1 — actif | `false` | `Some` | La ligne de catalogue est active et à jour |
| 2 — désactivé | `true` | `Some` | La ligne de catalogue a été soft-désactivée (tâche archivée côté Gryzzly) |
| 3 — absent | `true` | `None` | La ligne de catalogue est introuvable (assignment orphelin) — **jamais de panique** |

Type `AssignedGryzzlyTaskGql` :

| Champ | Type GraphQL | Description |
|-------|-------------|-------------|
| `gryzzlyTaskId` | `String!` | Identifiant Gryzzly de la tâche |
| `name` | `String` | Libellé (null si état 3) |
| `projectName` | `String` | Nom du projet (null si état 3) |
| `stale` | `Boolean!` | Vrai si la ligne est désactivée ou absente |

La résolution utilise `GryzzlyCatalogRepository::find_by_gryzzly_task_id` qui retourne la ligne **quelle que soit sa valeur de `is_active`**, permettant l'affichage des états 2 et 3 sans jamais déclencher de panique.

#### Surfaces frontend d'assignation

Deux déclencheurs, une seule liste — le corps du menu est partagé pour que les badges `stale` /
`terminé` ne puissent pas diverger entre les deux surfaces.

| Fichier | Rôle |
|---------|------|
| `frontend/src/components/gryzzly/GryzzlyTaskOptionList.tsx` | Corps commun : champ de recherche, regroupement par projet, option « Clear assignment », badges. Monté **uniquement à l'ouverture**, pour que la requête catalogue ne parte pas une fois par puce fermée à l'écran (le dashboard en affiche des dizaines). |
| `frontend/src/components/gryzzly/GryzzlyTaskPicker.tsx` | Déclencheur pleine largeur du volet d'édition ; liste ancrée en `absolute`. |
| `frontend/src/components/gryzzly/GryzzlyTaskMenu.tsx` | Déclencheur en puce des cartes de tâche (dashboard), calibré sur `StatusMenu`. |
| `frontend/src/hooks/use-assign-gryzzly-task.ts` | Mutation `assignGryzzlyTask` partagée (`assign` / `clear`). |

`GryzzlyTaskMenu` **portalise** sa liste dans `document.body` en position `fixed` : les colonnes de
jour du dashboard sont des conteneurs `overflow-hidden` défilants, qui rogneraient un menu positionné
en `absolute`. Trois conséquences assumées, chacune couverte par un test :

1. Un portail continue de propager les événements React jusqu'à la carte, dont le `onClick` ouvre le
   volet d'édition — la puce et le conteneur du menu arrêtent donc `click` et `pointerDown`.
   `pointerDown` sert aussi à empêcher le capteur dnd-kit de la carte de réclamer le geste.
2. La détection du clic extérieur teste l'appartenance aux **deux** nœuds (puce et liste) : une fois
   portalisée, la liste n'est plus un descendant DOM de la puce.
3. Le défilement **réancre** la liste au lieu de la fermer. Fermer sur `scroll` rendait le menu
   inutilisable : la mise au point automatique du champ de recherche fait défiler son propre
   conteneur, donc le menu se refermait aussitôt ouvert. Seule une puce sortie du viewport ferme.

### 10.9 End-of-Day Auto-Reconstruction Scheduler

#### Architecture

The end-of-day job is a plain long-lived tokio task inside the Axum server (**pas**
`tokio-cron-scheduler`) : une boucle qui exécute un passage, puis attend la durée que lui dicte une
politique pure, indéfiniment. Chaque passage reconstruit le brouillon de feuille de temps des jours
dus et lève une alerte passive `TimesheetReady`.

**Implementation location:** `api/src/jobs.rs` (le sommeil et les appels `tracing` — rien d'autre),
`application/src/jobs.rs` (la politique, pure et testable).

- `run_eod_scheduler(deps: EodDeps, user_id: UserId)` — boucle `run_eod_pass` → décision → `sleep`.
  Appelée une fois au démarrage depuis `main.rs`. Le premier passage a lieu **immédiatement**, les
  suivants après le délai décidé par la politique.
- `run_eod_pass(...) -> Result<EodPassOutcome, AppError>` — le cas d'usage
  (`application::use_cases::timesheet`) qui exécute un passage pour un utilisateur.
- `application::jobs::{RetryPolicy, JobHealth, AttemptOutcome, AttemptDecision, LogEntry}` — la
  machine à états : `JobHealth::observe(outcome, now, &policy)` consomme l'état, rend le nouvel état
  plus un délai et, éventuellement, la ligne de journal à imprimer. Aucune I/O, aucune horloge,
  aucun `tracing` : `now` est un paramètre, ce qui rend la courbe de repli testable sans attendre
  l'horloge murale (12 tests unitaires dans le module).

#### Configuration Keys

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `workday.auto_reconstruct_hour` | Integer (0-23) | `18` | Hour of day (in local timezone, `aplan.timezone`) at which the EOD job triggers. Example: 18 = 6 PM. |
| `aplan.timesheet.last_auto_run` | Text (ISO 8601) | `null` | Watermark timestamp of the last successful auto-reconstruction pass. Used to prevent re-runs and detect catch-up scenarios. |
| `aplan.timezone` | Text (IANA zone) | `Europe/Paris` | Timezone used to resolve the local hour for EOD trigger and to interpret worklog entry timestamps. |

#### Scheduler Behavior

1. **Trigger Condition:** le passage tourne toutes les **5 minutes** (base de `RetryPolicy::end_of_day()`)
   et interroge le watermark : ce sont `compute_target_dates` et `aplan.timesheet.last_auto_run` qui
   décident du travail dû, pas le tick. La cadence a été relevée de 60 s à 5 min — pour une tâche de
   fin de journée, la réactivité à la minute n'a aucune valeur et coûtait 1 440 passages par jour.

2. **Watermark Logic (Idempotency):**
   - Load `aplan.timesheet.last_auto_run` from configuration (format `%Y-%m-%d`, date locale — pas
     un horodatage, malgré le nom historique de la clé).
   - If the watermark is on the same day as today (in local timezone), skip the pass (already ran today).
   - If the watermark is on a previous day, proceed.
   - En fin de passage, le watermark n'avance que sur le **préfixe ininterrompu** de jours
     reconstruits. Si le rattrapage traite 06-06, 06-07, 06-08 et que 06-07 échoue, le watermark
     s'arrête à 06-06 : passer par-dessus un jour en échec le perdrait définitivement.

3. **Catch-Up Window:** If the watermark is more than 7 days old (stale), only reconstruct the last 7 days of drafts (configurable, fixed at 7 days for MVP). This prevents re-running reconstruction on very old dates.

#### Reconstruction Semantics (`run_eod_pass`)

The use case performs the following steps for the target date (today in local timezone):

1. **Load Configuration:**
   - Read `aplan.timezone`, `workday.auto_reconstruct_hour` from `config_repo`.
   - Resolve today's date in the configured timezone.

2. **Load Existing Timesheet Draft:**
   - Fetch the draft timesheet for today via `timesheet_repo.find_by_date(user_id, today)`.
   - If a draft already exists with status `Validated` or `Submitted`, **do not overwrite** — return early with no error. (Guard from R-TS-xx: never clobber a finalized draft.)

3. **Reconstruct Draft:**
   - Call `reconstruct_timesheet(...)` from the timesheet module with the exact same logic as the React surface (Plan 3):
     - Read the day's worklog entries and git commits as point-in-time **signals** (the reconstruction has its own carry-forward inside the configured half-day windows; it does **not** go through `derive_time_blocks`, and the 15-minute rule of R-WL-13 therefore does not apply to it).
     - Load all meetings for today.
     - Compute suggested allocations using learned mapping rules (if any) and fallback heuristics.
     - Return a new draft with status `Draft`, lines (project→hours mappings), unattributed hours, and blocks.
   - Persist the reconstructed draft via `timesheet_repo.save(draft)`.

4. **Emit TimesheetReady Alert:**
   - Create a passive `Alert` with:
     - `alert_type: AlertType::TimesheetReady`
     - `severity: AlertSeverity::Information`
     - `message: "Feuille de temps prête pour révision"`
     - `date: today`
     - `related_items: []` (no specific task/meeting links)
   - Persist via `alert_repo.save(alert)`.
   - Emit an `alertsUpdated` subscription to notify any connected frontend clients.
   - **Prérequis schéma** : `alerts.alert_type` doit admettre `timesheet_ready`. La liste `CHECK`
     de `001` n'en portait que trois valeurs, si bien que cette insertion levait
     `(code: 275) CHECK constraint failed` et faisait **avorter tout le job** — à chaque passage,
     depuis la fusion de la fonctionnalité, visible uniquement dans le journal du service. Corrigé
     par la reconstruction de table de la migration `013` (§ 7.2.2). L'étape est en outre devenue
     **accessoire** : son échec ne détruit plus la reconstruction (voir « Résilience » ci-dessous).

5. **Update Watermark:**
   - `aplan.timesheet.last_auto_run` = dernière date du préfixe ininterrompu (`%Y-%m-%d`).

#### Résilience : ce qui est fatal, ce qui est toléré

`run_eod_pass` rend un `EodPassOutcome { processed: Vec<NaiveDate>, degraded: Vec<EodStepFailure> }`
au lieu d'un simple `Vec<NaiveDate>`. Le passage n'est plus tout-ou-rien : il garde ce qui a réussi
et **rapporte** le reste. Rien n'est avalé en silence — l'appelant journalise le détail de `degraded`
(via `EodPassOutcome::degradation_signature()`, soumis à la déduplication décrite plus bas), et un
passage dégradé compte comme un **échec** pour le repli exponentiel : ce qu'il a toléré reste cassé.

| Étape (`EodStep`) | Échec | Conséquence |
|---|---|---|
| Lecture de configuration (`aplan.timezone`, heure de déclenchement, watermark) | **fatal** (`Err`) | Sans elles, impossible de savoir ce qui est dû : aucun travail n'est tenté. |
| `Reconstruction` d'un jour | toléré, jour par jour | Le jour est ignoré, les **autres jours du rattrapage continuent**, et le watermark s'arrête avant ce jour pour qu'il soit réessayé. |
| `ReadyAlert` (alerte passive) | toléré | Le brouillon déjà enregistré est conservé, le jour compte comme traité, le watermark avance. Seule l'alerte manque. |
| `Watermark` (écriture de la clé) | toléré | Le travail persisté est conservé ; le prochain passage refera les mêmes jours (l'opération est idempotente). |

Reste tout-ou-rien **à l'intérieur d'un jour** : `reconstruct_timesheet` lit worklog, commits git,
réunions et catalogue puis écrit un brouillon en un seul enchaînement — une lecture en échec (Graph
indisponible, dépôt git absent) perd la journée entière, pas seulement le signal manquant. C'est
volontaire : un brouillon reconstruit à partir d'une moitié des signaux serait faux sans le dire.

#### Repli exponentiel et journalisation déduplicée

Le vice corrigé : un échec permanent ne faisait pas avancer le watermark, donc le passage était
réessayé toutes les 60 s **indéfiniment**, chaque tentative imprimant un `warn` identique au
caractère près — 61 lignes identiques en un quart d'heure, pendant trois semaines. Le signal se
noyait dans sa propre répétition : rien ne distinguait une panne permanente d'un incident passager.

`RetryPolicy::end_of_day()` : base 5 min, plafond 30 min, escalade au **3ᵉ** échec consécutif,
rappel tous les **12** échecs.

| Échecs consécutifs | Délai avant la tentative suivante | Journal |
|---|---|---|
| 1 | 5 min | `warn` — première occurrence |
| 2 | 10 min | supprimé si l'erreur est identique |
| 3 | 20 min | **`error`** — escalade |
| 4 et au-delà | 30 min (plafond) | supprimé, puis `error` de rappel aux échecs 15, 27, 39… |

- **Une erreur *différente* est journalisée immédiatement**, même en pleine série : une nouvelle
  erreur est une information, pas une répétition.
- La ligne escaladée porte **le compte d'échecs consécutifs, la durée de la panne**
  (`humanize_duration` → `3w 0d`, `2d 4h`, `45m`) et le nombre de répétitions supprimées :
  « failing for 3w 0d (4021 consecutive attempts) — it will not fix itself ».
- Un succès remet l'état à zéro et, s'il met fin à une série, imprime **une** ligne `info`
  « recovered » avec la durée de la panne.
- Coût d'une panne permanente : **≈ 50 tentatives et 5 lignes** de journal par jour (mesuré par le
  test `a_permanent_failure_costs_dozens_of_lines_a_day_not_thousands`), contre 1 440 tentatives et
  1 440 lignes identiques avant.

#### Important Guarantees

- **Never auto-submits to Gryzzly:** The job only creates or updates a local draft. It does not call any Gryzzly API endpoint to submit time.
- **Never clobbers validated/submitted drafts:** If a user has already validated or submitted a draft, the EOD pass skips that date silently.
- **Passive alert only:** The `TimesheetReady` alert is displayed in the alerts zone of the dashboard and in the `/alerts` query. There is no OS-level push notification, no SMS, no email — only in-app visibility.
- **Idempotent watermark:** Re-running the job on the same day (clock drift, scheduler restart) has no effect; the watermark prevents duplicate reconstruction.
- **Jamais fatal pour le serveur :** aucune erreur du job ne remonte à `main` ; la boucle survit à
  tout, y compris à une base en lecture seule.

#### GraphQL Surface

The `TimesheetReady` alert is exposed via the existing `alerts` query (no new query needed):

```graphql
query Alerts {
  alerts {
    id
    alertType       # Can be DEADLINE, OVERLOAD, CONFLICT, TIMESHEET_READY
    severity        # CRITICAL, WARNING, INFORMATION
    message
    date
    resolved
    createdAt
  }
}
```

Clients filter on `alertType == TIMESHEET_READY` to find end-of-day reconstruction alerts. Severity is always `INFORMATION` for this alert type.

### 10.10 Consolidation mémoire de 17 h 30

#### Où le job vit — et pourquoi pas ici

Contrairement au job de fin de journée (§ 10.9), **la consolidation mémoire n'est pas une tâche
tokio du serveur**. C'est une **session Claude Code planifiée** qui pilote la CLI. La raison est une
frontière, pas une commodité : le backend Rust ne contient aujourd'hui aucun code LLM, et y faire
entrer un client de modèle, une clé d'API et du *prompt engineering* serait disproportionné pour une
extraction qui tourne une fois par jour. La session consomme le modèle déjà payé, laisse la
séparation DDD intacte, et rend le *prompt* itérable sans recompiler.

| Élément | Emplacement |
|---|---|
| Jeu d'instructions de la session | `docs/prompts/consolidation-memoire.md` (hors binaire, versionné) |
| Machinerie déterministe | `application::use_cases::consolidation` + `SqliteWorklogRepository` |
| Surface pilotable | `aplan consolidate {pending,mark,record-run} --json` |
| Filigrane par entrée | `worklog_entries.consolidated_at` (migration `012`) |
| Date du dernier passage | `configuration['memory.consolidation.last_run']` |
| Planification | **hors dépôt** — `CronCreate` / skill `schedule`, à installer |

#### Séquence d'un passage

```
0. aplan consolidate pending --json     ← sonde de joignabilité ET lecture du lot
   └─ échec ⇒ ARRÊT TOTAL, aucun marqueur posé (R60)
1. aplan brief --project <p> --json     ← décisions actives du projet (matière des supersessions)
   aplan recall --q "…" --history --json ← actifs + PENDING + pierres tombales (anti-boucle)
   aplan inbox --json                    ← file déjà remplie
2. aplan remember --json … (sans --confirm, avec --source-ref <id d'entrée>)
3. aplan consolidate mark --json <id>…  ← EN DERNIER, après écritures réussies (R59)
4. aplan consolidate record-run --json  ← rend une panne visible dans le brief (R57)
```

`--history` à l'étape 1 n'est pas un détail : sans lui, la session ne voit que les souvenirs
`ACTIVE` et re-propose chaque soir ce que l'utilisateur a déjà rejeté. La réponse porte
`memory.status`, ce qui permet de distinguer `ACTIVE` (déjà su), `PENDING` (déjà en file) et
`REJECTED` (pierre tombale, à ne jamais re-proposer).

#### Garanties

- **Rien n'entre en `ACTIVE`.** La session n'emploie ni `--confirm` ni `--force`, et n'exécute aucun
  verbe de la file (`accept` / `merge` / `supersede` / `reject`) : ce sont les verbes de
  l'utilisateur. Une supersession est *proposée* — le candidat nomme l'ancien identifiant dans son
  `--why`, et le compte rendu donne la commande `aplan inbox supersede … --replaces …` à coller
  (R61). `invalidated_at` conserve donc ses trois écrivains humains (R46).
- **Le marqueur est posé après les écritures.** Un doublon devient une pierre tombale au premier
  rejet ; une entrée marquée sans souvenir est perdue en silence. L'ordre inverse échangerait une
  panne récupérable contre une panne irrécupérable.
- **L'horaire n'est pas critique.** Les entrées consignées après le passage sont reprises au suivant :
  le filigrane par entrée, et non l'heure, porte la correction.
- **Une exécution manquée ne perd rien.** Poste éteint, congé, client : le lot suivant contient tout
  le retard, borné par `CONSOLIDATION_BATCH_LIMIT` (200) et drainé sur plusieurs passages.
- **Un passage à vide s'enregistre quand même** (`record-run`) : c'est ce qui distingue « rien à
  consolider » de « le job est mort », distinction que le brief affiche au-delà de 3 jours (R57).

#### Prérequis hors dépôt, à installer

1. Le hook `~/.claude/hooks/aplan-session-start.sh` impose aujourd'hui `AskUserQuestion` comme
   première action. Une session planifiée non interactive va donc bloquer ou brûler son tour : le
   hook doit détecter le mode non interactif et sauter la question.
2. L'API doit tourner en service (`systemd --user`). Sans cela, la garde de joignabilité se contente
   de rendre la panne visible — elle ne l'empêche pas.

---

## 11. Deduplication Engine

### 11.1 Process

The deduplication engine runs after each sync that involves both Jira and Excel data.

```
1. Fetch all Jira-sourced tasks for the user
2. Fetch all Excel-sourced tasks for the user
3. Fetch existing task_links (both merged and rejected)
4. For each Jira task:
   a. Check R08: search for Jira key in all Excel rows
      |-- Found -> auto-merge (create task_link with type "auto_merged")
      +-- Not found -> proceed to step b
   b. Check R09: calculate similarity with each unlinked Excel task
      |-- Score >= DEDUP_CONFIDENCE_THRESHOLD and pair not rejected
      |   -> create DeduplicationSuggestion for user review
      +-- Score < threshold -> no action
5. For merged tasks (auto or confirmed):
   - Survivor selection (R08b):
       a. Exactly one task is from Jira -> Jira task is the survivor
       b. Both or neither from Jira -> task already Followed in the dashboard is the survivor
       c. Neither Followed -> primary task (task_id_primary in the link) is the survivor (deterministic fallback)
   - The survivor's tracking_state is set to Followed (made visible).
   - If the survivor has no planned_start/deadline, it inherits those fields from the loser.
   - A `task_link` record links survivor (task_id_primary) to loser (task_id_secondary).
   - The loser (task_id_secondary) is hidden from the dashboard and all task lists:
     `find_by_user` and `find_by_date_range` exclude any task that is the
     `task_id_secondary` of an `auto_merged`/`manual_merged` link. The loser's own
     `tracking_state` is left unchanged (non-destructive — reversed simply by unlinking).
   - Surviving Jira task: fields follow the merge table in §11.2; Excel data enriches missing fields.
```

### 11.2 Merge Rules

When two tasks are merged (R08 auto-merge or R09 user-confirmed merge):

| Field | Source of Truth | Fallback |
|-------|---------------|----------|
| `title` | Jira | Excel |
| `status` | Jira | Excel |
| `assignee` | Jira | Excel |
| `deadline` | Jira | Excel |
| `description` | Jira | Excel |
| `notes` | Local (préservé) | Local (préservé) |
| `project_id` | Jira | Excel |
| Planning dates (from Excel) | Excel | -- |
| Tags/categories | Merge both | -- |

### 11.3 User Interactions

- **Accept suggestion**: Applies survivor selection (R08b), creates a `task_link` with type `auto_merged` (via `confirm_suggestion`). The loser (`task_id_secondary`) is hidden from all views through the link-based exclusion above; its `tracking_state` is not modified.
- **Reject suggestion**: Creates a `task_link` with type `rejected`. The pair is never suggested again.
- **Manual link**: User can manually link any two tasks via `linkTasks` mutation. Survivor selection (R08b) applies.
- **Manual unlink**: User can break a link via `unlinkTasks` mutation, which deletes the `task_link`. The loser becomes visible again automatically (it is no longer a merge `task_id_secondary`); no `tracking_state` is restored, since hiding is driven solely by the link and the loser's state was never changed. The survivor keeps its `Followed` state.

---

## 12. Alert Engine

### 12.1 Process

The alert engine runs:
- After each sync completes
- After task priority/deadline changes
- On demand (when dashboard is loaded)

```
1. Collect current state:
   - All active tasks for the user
   - All meetings for the relevant period
   - Current configuration (capacity, threshold)
2. Run pure domain functions:
   - check_deadline_alerts(tasks, today, threshold)
   - check_overload_alerts(tasks, meetings, capacity, week_start)
   - check_conflict_alerts(scheduled_items, dates) — using time-range overlap
3. Diff new alerts against existing alerts:
   - New alerts -> INSERT
   - Existing alerts still valid -> keep
   - Existing alerts no longer valid -> auto-resolve
4. Emit alertsUpdated subscription if changes occurred
```

### 12.2 Alert Severity (R19)

| Severity | Conditions |
|----------|-----------|
| **Critical** | Deadline overdue (R14), capacity exceeded by > 2 half-days |
| **Warning** | Deadline within threshold (R17), capacity exceeded by <= 2 half-days |
| **Information** | Scheduling conflict (R18), minor capacity warning |

### 12.3 Alert Lifecycle

1. **Created**: Alert generated by engine
2. **Active**: Displayed in dashboard alert zone
3. **Resolved**: User marks as resolved (via `resolveAlert` mutation) or condition no longer applies (auto-resolved)

Resolved alerts are kept in history but hidden from the active alert panel.

---

## 13. Activity Tracking

### 13.1 Interaction Model

Activity tracking uses three trigger types (US-031):

| Trigger | Mechanism | Implementation |
|---------|-----------|---------------|
| **Post-meeting** | After a meeting ends | Background task checks meetings against current time. When `end_time` passes, emits `activityReminder` subscription with `reminderType: "post_meeting"` |
| **Periodic** | Configurable interval | `tokio-cron-scheduler` job emits `activityReminder` subscription with `reminderType: "periodic"` at configured interval (default: 2h) |
| **Manual** | User clicks button | Frontend sends `startActivity` mutation directly |

### 13.2 Activity Slot Rules

| Rule | Implementation |
|------|---------------|
| **R20** | `ActivitySlot` struct: task_id, start_time, end_time, half_day, date |
| **R21** | `start_activity` use case: closes active slot (sets `end_time = now`), opens new slot |
| **R22** | Gaps between slots are "untracked". The frontend displays them as gray blocks on the timeline. |
| **R23** | `update_activity_slot` and `delete_activity_slot` mutations allow corrections |
| **R24** | `createActivitySlot` mutation allows manual creation of a slot with explicit `startTime`, `endTime`, and optional `taskId`. The `date` and `half_day` fields are derived from `startTime`. |
| **R25** | Validation: `endTime > startTime` is enforced in the use case on both `createActivitySlot` and `updateActivitySlot`. A `ValidationError` is returned if violated. |
| **R26** | When `updateActivitySlot` changes `startTime`, `half_day` is recomputed from the new `startTime`. The `date` field is not modified (read-only after creation). |
| **R27** | In `UpdateActivitySlotInput`, `taskId` uses a `MaybeUndefined` wrapper: `null` clears the task association, absent field leaves it unchanged. |

### 13.3 Task Selector Ordering (R28)

The task list displayed in the activity timer selector (`ActivitySwitcher`) is ordered as follows:

1. **Tâches du jour** (groupe prioritaire) : toute tâche dont `planned_start` (converti dans le fuseau `aplan.timezone`) tombe sur la date courante locale, **ou** dont `deadline` est égale à la date courante locale.
2. **Autres tâches** : toutes les tâches suivies restantes, dans l'ordre habituel par priorité.

Aucune tâche n'est filtrée ou masquée dans ce sélecteur — seul l'ordre change.

À l'intérieur du groupe « tâches du jour », le tri secondaire est : urgence décroissante (`Critical → Low`), puis impact décroissant (`Critical → Low`).

Côté frontend (`ActivityTimer`), le tri est appliqué côté client avant de rendre la liste déroulante. Côté backend, aucune modification de l'API n'est requise : la logique de tri vit dans le composant React.

### 13.4 Reminder Suppression

- No reminders on weekends or outside configured working hours (default: 08:00-17:00, configurable via `working_hours_start` / `working_hours_end`)
- Post-meeting reminders: only for meetings the user attended (not declined)
- If the user already changed activity within the last 15 minutes, skip the periodic reminder

### 13.5 Worklog-Driven Time Tracking (Claude Code integration)

When Claude Code drives the cockpit via the `aplan` CLI, time is tracked through worklog entries rather than open activity slots:

| Aspect | Detail |
|--------|--------|
| **Session link** | `aplan start <task>` writes `aplan.active_task_id` (config key). No `ActivitySlot` is opened. |
| **Time materialization** | Closed `ActivitySlot` records are derived from worklog-entry `logged_at` timestamps via `derive_time_blocks` (domain rule) and persisted by `materialize_worklog_time` (use case). |
| **Granularity** | One slot per (task, continuous stretch of work). A stretch runs from its first to its last entry and stops wherever two consecutive entries are more than `MAX_CONTINUATION_GAP_MINUTES` apart (R-WL-13), so one half-day may hold several slots; it never straddles the half-day boundary (morning = local hour < 13, afternoon >= 13, per `aplan.timezone`). |
| **Trigger** | Materialization runs on `aplan stop`, `aplan done`, and on every Claude Code `SessionEnd` hook (`~/.claude/hooks/aplan-session-end.sh`). The hook calls `aplan flush <task_id>`, which calls the `flushWorklogTime` mutation — still against the human's window today, since the hook does not yet pass a session id (a later plan rewrites the hooks to flush the ending session's own window instead). |
| **Flush window** | Not a watermark: it only selects which local half-days to rebuild, and every entry of the task in those half-days decides the slots, so re-running never duplicates and a backdated entry is still picked up. `flushWorklogTime` reads and advances one of two windows, never both: a Claude session's own `sessions.last_flush_at` when a `sessionId` is passed, otherwise the human's `aplan.active_since`. |
| **No open slots** | There is never an open (`end_time IS NULL`) slot associated with the active-task pointer. The existing `start_activity` / `stop_activity` use cases are unaffected and continue to work for UI-driven tracking. |
| **Correction** | A wrong attribution is fixed with `aplan reattribute --from <task> --to <task> {--date D \| --since D --until D \| --entry ID…} [--confirm]`: the entries move and the slots of the two tasks are **re-derived in the affected half-days**. Preview by default. Because slots are a projection, correcting the entries is the only way to correct the time — editing a slot would leave it disagreeing with the journal it came from. |
| **Repair** | A slot that lost its `task_id` altogether — `ON DELETE SET NULL` fired by an `INSERT OR REPLACE INTO tasks`, showing as "(no task)" in `aplan journal` — is fixed with `aplan slots repair --from D --to D [--confirm]`: the orphan is dropped and its half-day rewritten from the worklog entries, which still carry the attribution. Preview by default. A `manual` slot with no task is never touched: it is a hand-run timer, not damage. |
| **Agents** | A subagent must never run `aplan start` / `new` / `stop` / `done` / `flush` / `triage`: the active-task pointer belongs to the parent session, and an agent that moves it redirects the parent's time onto its own task. See `.claude/skills/aplan/SKILL.md`. |

---

## 14. Authentication & Security

### 14.1 Local Mode (MVP)

| Aspect | Implementation |
|--------|---------------|
| User auth | None. A default user is created at first startup. `user_id` is injected by middleware automatically. |
| API tokens (Jira) | Stored encrypted in `configuration` table. Encryption key derived from a local secret (generated at first startup, stored in a `.secret` file). |
| Graph tokens | OAuth2 tokens (access + refresh) obtenus via le flux interactif (§9.2) et stockés chiffrés dans la table `configuration` (clés `microsoft.*`). Le backend renouvelle silencieusement l'access token via `RefreshingGraphTokenProvider`. En cas d'`invalid_grant`, les jetons sont effacés et la session est invalidée. |
| CORS | Restreint à `http://localhost:3000` (frontend Vite). Toute autre origine est rejetée. |
| Protection CSRF | L'état `state` du flux OAuth est un token aléatoire à usage unique (TTL 10 min, stocké en mémoire côté serveur). Toute valeur `state` absente, inconnue ou expirée entraîne le rejet de la callback. |
| Masquage des secrets | La query GraphQL `configuration` remplace les valeurs dont la clé correspond aux patterns `*.token`, `*.secret`, `*.client_secret`, `*.password`, `*.api_key`, `*.access_token`, `*.refresh_token` par `"********"`. Les valeurs brutes ne sont jamais exposées via GraphQL. |

### 14.2 Teams Mode (Future)

| Aspect | Implementation |
|--------|---------------|
| User auth | Azure AD / Microsoft Entra ID. JWT validation middleware extracts `oid` claim as `UserId`. |
| API tokens | Per-user, stored encrypted. Graph tokens obtained via Teams SSO (on-behalf-of flow). |
| CORS | Restricted to Teams origin |
| HTTPS | Required (Azure deployment) |

### 14.3 Token Encryption

Sensitive values (API tokens, refresh tokens) are encrypted at rest using AES-256-GCM:
- **Local mode**: Encryption key stored in `backend/.secret` (auto-generated, git-ignored)
- **Teams mode**: Encryption key from Azure Key Vault or environment variable

---

## 15. Configuration

### 15.1 Configuration Parameters

All parameters from the functional spec (section 8.2) are stored in the `configuration` table as key-value pairs with JSON values.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `weekly_capacity` | integer | `10` | Half-days per week |
| `sync_frequency_minutes` | integer | `15` | Background sync interval |
| `activity_reminder_minutes` | integer | `120` | Activity reminder interval |
| `deadline_alert_threshold_days` | integer | `2` | Days before deadline to trigger alert |
| `post_meeting_reminder_enabled` | boolean | `true` | Enable post-meeting activity prompt |
| `periodic_reminder_enabled` | boolean | `true` | Enable periodic activity prompt |
| `working_hours_start` | string | `"08:00"` | Start of working day (HH:MM format) |
| `working_hours_end` | string | `"17:00"` | End of working day (HH:MM format) |
| `jira_base_url` | string | `""` | Jira instance URL |
| `jira_api_token` | string (encrypted) | `""` | Jira API token |
| `jira_email` | string | `""` | Jira user email |
| `jira_project_keys` | string[] | `[]` | Jira project keys to sync |
| `jira_team_members` | string[] | `[]` | Team member usernames for Jira |
| `graph_client_id` | string | `""` | Azure AD app client ID (lecture seule, identique à `MICROSOFT_CLIENT_ID`) |
| `graph_tenant_id` | string | `""` | Azure AD tenant ID (lecture seule, identique à `MICROSOFT_TENANT_ID`) |
| `microsoft.access_token` | string (encrypted) | `""` | Access token Microsoft Graph — géré par la machine (`RefreshingGraphTokenProvider`), ne pas saisir à la main |
| `microsoft.refresh_token` | string (encrypted) | `""` | Refresh token OAuth — persisté après la connexion interactive, renouvelé automatiquement |
| `microsoft.token_expires_at` | string (ISO 8601) | `""` | Horodatage d'expiration de l'access token |
| `microsoft.account` | string | `""` | Adresse email du compte Microsoft connecté |
| `outlook.calendar_days` | integer | `14` | Horizon en jours pour la synchronisation du calendrier Outlook |
| `outlook.exclude_patterns` | string (multiligne) | `""` | Liste de titres de réunions à exclure de la synchronisation
| `aplan.active_task_id` | string (UUID) | `""` | Task UUID the current Claude Code session is linked to. Set by `aplan start`, cleared by `aplan stop`/`aplan done`. No open activity slot is associated — time is derived from worklog timestamps. |
| `aplan.active_since` | string (ISO 8601 UTC) | `""` | Selector for the **human's** flush window, not a watermark: `materialize_worklog_time` uses it only to pick which local half-days a task touched, then rebuilds from every entry of that task in each of them. Updated to `now()` by each `flushWorklogTime` call made without a `sessionId`; a session-scoped call (`sessionId` set) reads and advances `sessions.last_flush_at` instead and never touches this key. |
| `aplan.timezone` | string (IANA tz) | `"Europe/Paris"` | Timezone used by `derive_time_blocks` to convert UTC worklog timestamps into local day/half-day boundaries. | (une entrée par ligne) ; exclusion par sous-chaîne insensible à la casse, appliquée dans `sync_outlook` via `domain::rules::meeting::is_excluded` ; les réunions déjà synchronisées correspondant à un nouveau motif sont purgées par `delete_stale` au prochain sync ; appliqué de façon cohérente par `sync_source` ET `sync_all` |
| `aplan.session_idle_timeout_hours` | integer | `12` | Hours a session may go quiet before `run_session_reaper_scheduler` closes it, flushing its worklog time first. Read fresh every pass. Missing, unparseable, or outside `1..=8760` (one year) all fall back to the default — the last two so a corrupt value cannot reap every open session on the next tick nor overflow the `chrono::Duration::hours` cutoff computation. |
| `excel_sharepoint_path` | string | `""` | SharePoint path to Excel file |
| `excel_sheet_name` | string | `""` | Sheet name in Excel |
| `excel_mapping` | object | `{}` | Column name -> field mapping |
| `gryzzly.base_url` | string | `https://api.gryzzly.io` | URL de base de l'API interne Gryzzly (pas de préfixe `/v1`). |
| `gryzzly.token` | string (secret) | `""` | Jeton de session Gryzzly collé à la main. Prioritaire sur le cookie navigateur ; échappatoire si la lecture du cookie casse. Si les deux manquent, la source `gryzzly` est marquée `Not configured`. |
| `gryzzly.cookie_profile` | string | `""` | Chemin absolu vers un fichier `Cookies` de profil navigateur. Vide = détection automatique. |
| `obsidian_vault_path` | string | `""` | Path to Obsidian vault (v2) |
| `obsidian_task_tags` | string[] | `["#task"]` | Tags identifying tasks in Obsidian (v2) |

### 15.2 Environment Variables

Sensitive bootstrapping values can be set via environment variables (`.env` file):

```bash
DATABASE_URL=sqlite://data/aggregated-plan.db
SERVER_PORT=3001
RUST_LOG=info
# Optional: override DB-stored values
JIRA_BASE_URL=https://mycompany.atlassian.net
JIRA_API_TOKEN=...
GRAPH_CLIENT_ID=...
GRAPH_TENANT_ID=...
# Microsoft OAuth (flux authorization code — porte d'authentification)
MICROSOFT_CLIENT_ID=12dd5cbd-f897-4184-a473-8effc7a93aba
MICROSOFT_TENANT_ID=0ca0e5b0-fbba-4994-839d-8d47b96d86db
MICROSOFT_CLIENT_SECRET=...
MICROSOFT_REDIRECT_URI=http://localhost:3001/auth/microsoft/callback
```

Environment variables take precedence over database-stored configuration for the same key.

---

## 16. Testing Strategy

### 16.1 Backend Testing

| Layer | Test Type | Tool | What to Test |
|-------|----------|------|-------------|
| **Domain** | Unit tests | `cargo test` | All business rules (urgency, priority, workload, alerts, dedup). Pure functions = simple input/output assertions. |
| **Application** | Unit tests with mocks | `cargo test` + `mockall` | Use cases with mock repository implementations. Verify correct orchestration. |
| **Infrastructure** | Integration tests | `cargo test` + SQLite in-memory | Repository implementations against real SQLite. Test CRUD, queries, edge cases. |
| **Infrastructure** | Integration tests | `cargo test` + `wiremock` | External API clients against mocked HTTP responses. Test request building, response parsing, error handling. |
| **API** | Integration tests | `cargo test` + `axum::test` | Full GraphQL queries/mutations against test server with in-memory DB. |

**Coverage target:** 80% lines, branches, functions (measured with `cargo-tarpaulin`).

### 16.2 Frontend Testing

| Scope | Test Type | Tool | What to Test |
|-------|----------|------|-------------|
| **Components** | Unit/render tests | vitest + @testing-library/react | Component rendering, user interactions, props handling |
| **Hooks** | Unit tests | vitest + renderHook | Custom hook behavior, state transitions |
| **GraphQL** | Integration | vitest + MSW | Mock GraphQL responses, test data flow |
| **Pages** | Integration | vitest + @testing-library/react + MSW | Full page rendering with mocked API |

**Coverage target:** 80% lines, branches, functions (measured with `vitest --coverage`).

### 16.3 End-to-End Testing

| Scope | Tool | What to Test |
|-------|------|-------------|
| Critical paths | Playwright | Dashboard load, create task, drag priority, log activity, settings save |

E2E tests run against the full stack (backend + frontend) with a test SQLite database and mocked external APIs (wiremock for Jira/Graph).

### 16.4 Test File Organization

Tests are colocated with source code:
- Rust: `#[cfg(test)] mod tests { ... }` in the same file, or `tests/` directory at crate root for integration tests
- Frontend: `__tests__/` directories next to source, or `.test.tsx` suffix

---

## 17. Deployment

### 17.1 Local Development

```bash
# Prerequisites: Rust toolchain, Node.js >= 18, pnpm

# Backend
cd backend
cp .env.example .env    # Configure API tokens
cargo run               # Starts on port 3001

# Frontend
cd frontend
pnpm install
pnpm dev                # Starts on port 3000 (Vite dev server)
```

The frontend proxies `/graphql` requests to `http://localhost:3001` via Vite's proxy configuration.

### 17.2 Production Build (Local)

```bash
# Backend: compile release binary
cd backend
cargo build --release
# Binary at: target/release/api

# Frontend: build static assets
cd frontend
pnpm build
# Output at: dist/

# The Axum server can serve the frontend dist/ as static files
```

### 17.3 Teams Deployment (Future)

```
+---------------------------------------+
|           Azure Cloud                 |
|                                       |
|  +-------------+  +---------------+  |
|  | Azure App   |  |  PostgreSQL   |  |
|  | Service     |  |  (Azure DB)   |  |
|  | (Rust API)  |--|               |  |
|  +------+------+  +---------------+  |
|         |                             |
|  +------+------+  +---------------+  |
|  | Static Web  |  |  Azure Key    |  |
|  | App (React) |  |  Vault        |  |
|  +-------------+  +---------------+  |
|                                       |
|  +-------------+                     |
|  | Azure AD    |                     |
|  | App Reg     |                     |
|  +-------------+                     |
+---------------------------------------+
         |
         | Teams Tab (iframe)
         v
+-----------------+
| Microsoft Teams  |
| (Tab App)        |
+-----------------+
```

Migration steps:
1. Switch `sqlx` feature from `sqlite` to `postgres`
2. Run migrations against PostgreSQL
3. Enable Azure AD JWT validation in auth middleware
4. Configure Teams Tab manifest to point to the frontend URL
5. Implement Teams SSO for Microsoft Graph token acquisition (on-behalf-of flow)
6. Deploy backend to Azure App Service, frontend to Azure Static Web Apps

---

## 18. MVP Scope

### 18.1 MVP v1 -- Implementation Order

The MVP should be built in this order, with each phase being independently testable:

**Phase 1: Foundation**
- Backend project setup (Cargo workspace, 4 crates)
- Database schema + migrations (SQLite)
- Domain types (all structs and enums)
- Domain business rules (urgency, priority, workload, alerts, dedup)
- Domain unit tests
- Repository traits (application layer)
- SQLite repository implementations (infrastructure layer)
- Repository integration tests
- Update `CLAUDE.md` to reflect new tech stack (Rust/Axum backend, GraphQL API, SQLite, urql frontend)

**Phase 2: Core API**
- GraphQL schema setup (async-graphql + Axum)
- Query resolvers: `tasks`, `task`, `projects`, `tags`
- Mutation resolvers: `createTask`, `updateTask`, `deleteTask`, `updatePriority`
- Personal task management (full CRUD)
- Frontend project setup (Vite + React + urql + Tailwind + shadcn/ui)
- GraphQL codegen pipeline
- Basic TaskCard, TaskList, TaskForm components

**Phase 3: Dashboard**
- `dailyDashboard` query resolver
- `weeklyWorkload` query resolver
- `priorityMatrix` query resolver
- DashboardPage with 4 zones
- PriorityMatrixPage with drag-and-drop
- WorkloadPage with chart and half-day grid
- Date navigation

**Phase 4: External Integrations**
- Jira connector (infrastructure)
- Microsoft Graph connector -- Outlook calendar (infrastructure)
- Microsoft Graph connector -- Excel/SharePoint (infrastructure)
- Sync engine (scheduler + coordinator)
- `forceSync` mutation + `syncProgress` subscription
- SyncStatusBar component
- Settings page (connection configuration)

**Phase 5: Deduplication**
- Deduplication engine
- `deduplicationSuggestions` query
- `confirmDeduplication`, `linkTasks`, `unlinkTasks` mutations
- DeduplicationPanel component

**Phase 6: Alerts**
- Alert engine (runs post-sync and on-demand)
- `alerts` query + `resolveAlert` mutation + `alertsUpdated` subscription
- AlertPanel and AlertBadge components

**Phase 7: Activity Tracking**
- Activity slot CRUD (use cases + resolvers + repos)
- `startActivity` / `stopActivity` mutations
- `activityReminder` subscription (post-meeting + periodic)
- ActivityJournalPage with timeline
- ActivitySwitcher component

### 18.2 v2 Features (Post-MVP)

- Team view (US-060)
- Project consolidated view (US-061)
- Weekly retrospective (US-062)
- Project workload dashboard (US-063)
- Tags and categories (US-064)
- Obsidian integration (US-005)

---

## 19. Coding Conventions

### 19.1 Rust Conventions

| Rule | Description |
|------|-------------|
| **No classes** | Rust has no classes. Use structs + free functions. No `impl` blocks with methods on domain types -- all logic in free functions. |
| **Immutability** | All struct fields are immutable by default. Use owned values, not mutable references, for transformations. Return new values instead of mutating. |
| **Result everywhere** | All fallible functions return `Result<T, E>`. No `.unwrap()` or `.expect()` in production code (only in tests). |
| **No panic** | No `panic!`, `todo!`, or `unimplemented!` in production code. |
| **Pattern matching** | Use `match` exhaustively. No wildcard `_` catch-all unless intentional and commented. |
| **Iterator combinators** | Prefer `.map()`, `.filter()`, `.fold()`, `.flat_map()` over imperative loops. |
| **Type aliases** | Use type aliases for IDs: `type TaskId = Uuid`. |
| **Error types** | Use `thiserror` derive macro for error enums. |
| **Naming** | Types: `PascalCase`. Functions: `snake_case`. Constants: `UPPER_SNAKE_CASE`. Files: `snake_case.rs`. |
| **Module structure** | One module = one responsibility. Use `mod.rs` for module declarations. |

### 19.2 TypeScript/React Conventions

| Rule | Description |
|------|-------------|
| **No classes** | Function components only. No `class` keyword anywhere. |
| **Immutability** | `const` over `let`. Never `var`. Immutable state updates (`...spread`). |
| **No `any`** | Use `unknown` with type guards if type is genuinely unknown. |
| **Functional** | `map`/`filter`/`reduce` over loops. Pure utility functions. Function composition. |
| **Result pattern** | For complex operations, use discriminated unions: `{ ok: true; value: T } | { ok: false; error: E }` |
| **Components** | Arrow function components. Props as destructured typed objects. |
| **Hooks** | Custom hooks for all non-trivial state logic. |
| **Naming** | Components: `PascalCase`. Hooks: `useCamelCase`. Functions: `camelCase`. Files: `kebab-case.tsx`. |
| **Formatting** | Prettier: single quotes, trailing commas, 100 char width, 2-space indent. |

### 19.3 General Principles

- **YAGNI**: Do not build features not specified. Do not add configurability beyond what is listed.
- **Single responsibility**: One file = one main export/concept.
- **Composition over inheritance**: Always. No inheritance anywhere.
- **Explicit over implicit**: Prefer verbose clarity over clever brevity.
- **Tests first**: Write tests before implementation. Red -> Green -> Refactor.
- **Domain purity**: Domain logic must be testable without any I/O, database, or HTTP setup.

---

## 20. Recurring Tasks

### 20.1 Database Schema — Migration `007_add_recurrence.sql`

```sql
CREATE TABLE task_recurrences (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL REFERENCES users(id),
    title           TEXT NOT NULL,
    description     TEXT,
    notes           TEXT,
    project_id      TEXT REFERENCES projects(id) ON DELETE SET NULL,
    urgency         INTEGER NOT NULL CHECK (urgency BETWEEN 1 AND 4),
    urgency_manual  INTEGER NOT NULL DEFAULT 1,  -- always true for recurring templates
    impact          INTEGER NOT NULL CHECK (impact BETWEEN 1 AND 4),
    estimated_hours REAL,
    rule_json       TEXT NOT NULL,               -- JSON-serialized RecurrenceRule (tagged enum)
    starts_on       TEXT NOT NULL,               -- ISO date: first allowed occurrence
    ends_on         TEXT,                        -- ISO date: nil = never (R32b)
    max_occurrences INTEGER,                     -- nil = never (R32c)
    last_generated_through TEXT,                 -- watermark: last date checked for generation
    active          INTEGER NOT NULL DEFAULT 1,  -- 0 = soft-deleted (cancelRecurrence)
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE INDEX idx_recurrences_user_active ON task_recurrences(user_id, active);

CREATE TABLE task_recurrence_tags (
    template_id TEXT NOT NULL REFERENCES task_recurrences(id) ON DELETE CASCADE,
    tag_id      TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (template_id, tag_id)
);

-- Two new columns on the existing tasks table
ALTER TABLE tasks ADD COLUMN recurrence_id   TEXT REFERENCES task_recurrences(id);
ALTER TABLE tasks ADD COLUMN occurrence_date TEXT;   -- ISO date; the slot this instance fills

-- Unique partial index: one instance per (template, date). Enables idempotent INSERT OR IGNORE.
CREATE UNIQUE INDEX idx_tasks_recurrence_slot
    ON tasks(recurrence_id, occurrence_date)
    WHERE recurrence_id IS NOT NULL;

CREATE INDEX idx_tasks_recurrence ON tasks(recurrence_id);
```

The `tasks.status` CHECK constraint must be extended to include `'cancelled'`. Because SQLite does not support `ALTER TABLE … ALTER COLUMN`, migration `007` rebuilds the `tasks` table (rename → recreate with new CHECK → copy → drop old), following the same pattern used in earlier migrations.

### 20.2 Domain Types

#### `RecurrenceRule` enum

```rust
// domain/src/types/recurrence.rs

pub type RecurrenceTemplateId = Uuid;

/// Serialized to/from JSON with serde tag "kind" (snake_case variants).
/// Stored in task_recurrences.rule_json.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecurrenceRule {
    /// Every `interval` days (interval >= 1).
    Daily { interval: u8 },

    /// Every `interval` weeks on the days indicated by `weekdays` bitmask
    /// (Mon = bit 0, Tue = bit 1, …, Sun = bit 6). At least one bit must be set.
    Weekly { interval: u8, weekdays: u8 },

    /// Every `interval` months on day `day` of the month (1–31).
    /// day = 31 means "last day of month" (R35).
    /// For day 1–30, months shorter than `day` are skipped (R35).
    MonthlyByDay { interval: u8, day: u8 },

    /// Every `interval` months on the Nth `weekday`
    /// (e.g. "first Tuesday", "last Friday").
    MonthlyByWeekday { interval: u8, week: WeekOfMonth, weekday: chrono::Weekday },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WeekOfMonth { First, Second, Third, Fourth, Last }

pub struct RecurrenceTemplate {
    pub id: RecurrenceTemplateId,
    pub user_id: UserId,
    pub title: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub project_id: Option<ProjectId>,
    pub urgency: UrgencyLevel,
    pub impact: ImpactLevel,
    pub estimated_hours: Option<f32>,
    pub tags: Vec<TagId>,
    pub rule: RecurrenceRule,
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
    pub max_occurrences: Option<u32>,
    pub last_generated_through: Option<NaiveDate>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

#### `TaskStatus::Cancelled`

A new variant is added to the existing `TaskStatus` enum (R33):

```rust
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
    Blocked,
    Cancelled,   // used exclusively for skipped recurring instances
}
```

The database value is `'cancelled'` (lowercase, consistent with other variants).

#### New fields on `Task`

```rust
pub struct Task {
    // ... existing fields unchanged ...
    pub recurrence_id: Option<RecurrenceTemplateId>,  // None for one-shot tasks
    pub occurrence_date: Option<NaiveDate>,            // None for one-shot tasks
}
```

`task.is_recurring()` is a convenience predicate: `self.recurrence_id.is_some()`.

#### Pure date-generation functions

```rust
// domain/src/rules/recurrence.rs

impl RecurrenceRule {
    /// Returns all occurrence dates in the inclusive window [from, to]
    /// anchored to `starts_on`. Bounded — callers pass narrow windows (≤ 14 days + buffer).
    pub fn occurrences_in(
        &self,
        starts_on: NaiveDate,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Vec<NaiveDate>;

    /// Returns the next occurrence strictly after `previous`, or None if the
    /// rule produces no further dates from that point.
    pub fn next_after(
        &self,
        starts_on: NaiveDate,
        previous: NaiveDate,
    ) -> Option<NaiveDate>;
}
```

Both functions are pure (zero I/O). The month-rollover policy (R35) is enforced inside these functions for `MonthlyByDay`.

### 20.3 Application Layer

#### Repository trait

```rust
// application/src/repositories/recurrence_repository.rs

#[async_trait]
pub trait RecurrenceRepository: Send + Sync {
    async fn find_by_id(&self, id: RecurrenceTemplateId)
        -> RepositoryResult<Option<RecurrenceTemplate>>;
    async fn find_active_by_user(&self, user_id: UserId)
        -> RepositoryResult<Vec<RecurrenceTemplate>>;
    async fn save(&self, template: &RecurrenceTemplate) -> RepositoryResult<()>;
    async fn deactivate(&self, id: RecurrenceTemplateId) -> RepositoryResult<()>;
}
```

#### Use cases

```rust
// application/src/use_cases/recurrence.rs

/// Create a new recurrence template and materialize the first horizon of instances.
pub async fn create_recurring_task(input, recurrence_repo, task_repo, today)
    -> Result<RecurrenceTemplate, AppError>;

/// Update template fields; delete future Todo-with-no-worklog instances; re-materialize.
/// Past instances and instances with worklog entries or status != Todo are preserved (R36).
pub async fn update_recurring_task(id, input, recurrence_repo, task_repo, worklog_repo, today)
    -> Result<RecurrenceTemplate, AppError>;

/// Soft-delete the template (active = false); delete future Todo instances.
pub async fn cancel_recurrence(id, recurrence_repo, task_repo)
    -> Result<usize, AppError>;   // returns count of deleted instances

/// Set task.status = Cancelled. Idempotent. Rejects if task.recurrence_id is None (R33).
pub async fn skip_occurrence(task_id, task_repo)
    -> Result<Task, AppError>;

/// Lazy materialization: called before every tasks/priorityMatrix/dashboard query
/// and after triggerSync. Generates instances for [last_generated_through+1, today+14].
/// INSERT OR IGNORE ensures idempotency (R37). Updates last_generated_through watermark.
pub async fn materialize_due_occurrences(user_id, today, horizon_days, recurrence_repo, task_repo)
    -> Result<usize, AppError>;   // returns count of new instances created
```

`carry_forward_tasks` in `task_management.rs` is modified to filter out tasks where `recurrence_id.is_some()` before applying the Monday-rebase logic (R34). `update_task` and `delete_task` return `AppError::Forbidden` when called on a task with a non-null `recurrence_id`; callers must use `update_recurring_task` / `cancel_recurrence` instead.

### 20.4 GraphQL API

#### New types

```graphql
# Four concrete rule variants (union discriminant via `kind` field)
type DailyRule        { kind: String! interval: Int! }
type WeeklyRule       { kind: String! interval: Int! weekdays: Int! }  # bitmask
type MonthlyByDayRule { kind: String! interval: Int! day: Int! }
type MonthlyByWeekdayRule { kind: String! interval: Int! week: String! weekday: String! }

union RecurrenceRule = DailyRule | WeeklyRule | MonthlyByDayRule | MonthlyByWeekdayRule

type RecurrenceTemplate {
  id: ID!
  title: String!
  description: String
  notes: String
  projectId: ID
  urgency: Int!
  impact: Int!
  estimatedHours: Float
  rule: RecurrenceRule!
  startsOn: Date!
  endsOn: Date
  maxOccurrences: Int
  active: Boolean!
  tags: [Tag!]!
}
```

#### Extensions to `Task`

```graphql
type Task {
  # ... existing fields ...
  recurrenceId: ID          # null for one-shot tasks
  occurrenceDate: Date      # null for one-shot tasks
  isRecurring: Boolean!     # = recurrenceId != null
}
```

`TaskStatus` enum gains the `CANCELLED` variant.

#### New operations

```graphql
extend type Query {
  recurrenceTemplates: [RecurrenceTemplate!]!
}

extend type Mutation {
  createRecurringTask(input: CreateRecurringTaskInput!): RecurrenceTemplate!
  updateRecurringTask(id: ID!, input: UpdateRecurringTaskInput!): RecurrenceTemplate!
  cancelRecurrence(id: ID!): Int!      # count of deleted future instances
  skipOccurrence(taskId: ID!): Task!   # sets status = CANCELLED
}
```

`tasks`, `priorityMatrix`, and `dashboard` resolvers call `materialize_due_occurrences` at the start of each request before loading data (R37). `triggerSync` mutation does the same after sync completes.

#### Frontend additions

| File | Change |
|------|--------|
| `frontend/src/graphql/mutations/recurrence.graphql` | New file — four mutations above |
| `frontend/src/graphql/queries/tasks.graphql` | Add `recurrenceId`, `occurrenceDate`, `isRecurring` to selection set |
| `frontend/src/lib/recurrence.ts` | Encode/decode helpers mirroring `RecurrenceRule` enum shape |
| `frontend/src/components/task/RecurrencePicker.tsx` | Frequency picker + end-policy section (new component) |
| `frontend/src/components/task/TaskCreateSheet.tsx` | Embed `RecurrencePicker` between planned-date and priority |
| `frontend/src/components/task/TaskEditSheet.tsx` | Same; add "Skip this occurrence" button when `task.isRecurring` |
| `frontend/src/components/task/TaskCard.tsx` | Violet repeat icon (12px, `text-violet-600`) when `isRecurring` |
| `frontend/src/pages/PriorityMatrixPage.tsx` | Filter `status === 'done' && isRecurring` from matrix display |
| `frontend/src/hooks/use-task-edit.ts` | Branch on `task.recurrenceId` to call `updateRecurringTask` |

### 20.5 Out of scope (MVP)

The following are documented as future follow-ups:
- Edit-this-occurrence-only (per-instance override without touching the template).
- Pause/resume a series.
- Worklog re-attribution between sibling instances.
- True user-local timezone recurrence (current anchor: 08:00 UTC ≈ 10:00 Paris; ~1h DST drift twice/year is acceptable for a single-user local tool).
- Background cron materialization (currently lazy on read; no instance generated when the app is idle).
