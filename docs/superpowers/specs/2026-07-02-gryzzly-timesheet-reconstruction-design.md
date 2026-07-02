# Design — Reconstruction de timesheet Gryzzly

- **Date** : 2026-07-02
- **Statut** : approuvé (design), en attente de relecture avant plan d'implémentation
- **Auteur** : session brainstorming aplan
- **Portée** : backend (domain / application / infrastructure / api), CLI, frontend

---

## 1. Contexte et problème

aplan possède déjà un tracker d'activité (chrono start/stop temps réel + éditeur manuel de
créneaux demi-journée). L'utilisateur — un Tech Lead — **n'arrive pas à se discipliner** pour
l'utiliser : le chrono temps réel et la saisie manuelle demandent une action au moment où l'on
travaille, moment où la discipline lâche.

Le but réel du suivi du temps est de **remplir Gryzzly** (feuille de temps externe) :

- **Granularité** : heures par projet, dont le total boucle la journée (~7,5 h).
- **Cadence** : quotidienne, idéalement en fin de journée.
- **Saisie dans Gryzzly** : manuelle pour l'instant (API peut-être plus tard). aplan doit produire
  des chiffres prêts à recopier, et pouvoir automatiser la saisie ultérieurement.
- **Friction acceptée** : une revue de ~30 s en fin de journée (corriger + valider).

On abandonne le chrono temps réel. On **reconstruit la journée a posteriori** à partir des signaux
que l'utilisateur génère déjà : notes `aplan log` (worklog horodaté), réunions Outlook, commits git.

### Tension fondamentale (à garder à l'esprit)

L'objectif « quasi-zéro discipline » est en tension avec l'exigence de Gryzzly (« heures par projet
qui bouclent la journée »). Moins on log, plus le moteur doit deviner. Un moteur qui **fabrique**
silencieusement une journée plausible-mais-fausse est pire qu'inutile : on le valide sans regarder,
on perd confiance, on arrête de l'utiliser — le problème de départ revient.

**Décision produit** : la reconstruction reste **complète et automatique** (elle remplit jusqu'à la
cible), mais elle est **transparente** — niveaux de confiance, quarantaine du temps inventé dans un
bucket « non attribué », détection des jours off. L'auto-remplissage a lieu, mais il est honnête sur
ce qu'il **sait** vs. ce qu'il **suppose**.

---

## 2. Décisions verrouillées

| # | Décision | Choix |
|---|----------|-------|
| D1 | Philosophie du moteur | **Reconstruction complète auto**, avec garde-fous de transparence (confiance, non-attribué first-class, jours off) |
| D2 | Livraison | **Les 3 surfaces** (CLI + écran React + job fin de journée). Ordre interne : socle d'abord, surfaces ensuite |
| D3 | Temps en réunion | **Facturable** → mappé vers un projet Gryzzly |
| D4 | Git | **Dans la v1** (ingestion via `git log`) |
| D5 | Source de vérité worklog | `worklog_entries` **bruts** ; les `ActivitySlot` sont **ignorés** par le moteur (anti-double-comptage) |
| D6 | Persistance du brouillon | Une seule paire de tables `timesheet_drafts` (jour) + `timesheet_draft_lines` (par projet). `ActivitySlot` reste intact |
| D7 | Contrat GraphQL | Défini **une seule fois** par le moteur, consommé verbatim par les 3 surfaces |
| D8 | Fuseau horaire | `Europe/Paris` par défaut ; **un seul** helper UTC→local, réutilisé par moteur et scheduler |
| D9 | Ingestion git | Lecture **à la volée** via `git log` (connecteur infra), **pas** de table `git_commits` |
| D10 | Rounding + édition | Concept de **valeur épinglée** : une ligne fixée à la main est gelée ; l'arrondi ne redistribue que sur le non-épinglé ; le non-attribué absorbe le résidu |

### Non-objectifs (YAGNI)

- Pas de soumission automatique vers Gryzzly (l'API n'existe pas ; saisie manuelle). Le job et la
  validation ne poussent **jamais** vers Gryzzly.
- Pas de vrai push/notification OS ni SSE (le `subscription.rs` du backend est un `EmptySubscription`).
  La « notification » de fin de journée est un **badge d'alerte passif** honnête.
- Pas d'expansion des réunions récurrentes (limite connue de la sync Outlook actuelle).
- Pas de stockage historique des commits (lecture fraîche à chaque reconstruction).

---

## 3. Ce qui existe déjà (vérifié dans le code)

| Domaine | État | Références |
|---------|------|-----------|
| Worklog horodaté | ✅ table `worklog_entries` (`logged_at` UTC, `task_id`, `user_id`), requêtable par plage via `WorklogFilter{from,to}` | `migrations/sqlite/006_create_worklog_entries.sql`; `infrastructure/src/database/worklog_repo.rs:121-163` |
| Matérialisation worklog→slots | ✅ `materialize_worklog_time()` + `derive_time_blocks()` (regroupe par demi-journée) | `application/src/use_cases/worklog.rs:90-146`; `domain/src/rules/worklog_time.rs:15-48` |
| Réunions Outlook | ✅ sync Graph **réelle** ; table `meetings` (start/end/title/participants/show_as/`project_id`), requête par date | `infrastructure/src/connectors/outlook/client.rs:30-102`; `application/src/repositories/meeting_repository.rs:17-29` |
| Lien réunion→Gryzzly | ❌ réunion → `project_id` **interne** seulement ; pas de `gryzzly_project_id` sur `projects` | `migrations/sqlite/001_initial.sql:59-71` |
| Lien tâche→Gryzzly | ✅ `task.gryzzly_project_id` snapshoté à l'assignation | `domain/src/types/task.rs:43-47`; `use_cases/gryzzly_assignment.rs:19-31` |
| Git | ❌ **inexistant** (aucune dépendance, table, ni code) | `crates/cli/Cargo.toml` |
| Config journée | ⚠️ heures partielles en dur (8/12/13/17), capacité hebdo 40 h, `working_hours` (fallback 8) lu depuis config ; TZ lue mais peu utilisée ; `DEFAULT_TZ = Europe/Paris` | `domain/src/rules/workload.rs:26-32`; `use_cases/dashboard.rs:79-96` |
| ActivitySlot | ✅ `task_id`, `start/end`, `half_day`, `date` — **pas** de `project_id` | `domain/src/types/activity.rs:6-16` |

> ⚠️ La doc du skill CLI affirme que le worklog « drive automatic time tracking » via un hook
> SessionEnd — **ce hook n'existe pas** dans le backend. La matérialisation est déclenchée
> explicitement (`aplan done`/`flush`). À clarifier dans la doc.

---

## 4. Architecture

```
                 ┌─────────────────────────────────────────────┐
                 │  domain (pur, zéro I/O)                       │
   signaux ─────▶│  rules::reconstruction::reconstruct_day()     │
                 │  rules::project_mapping::resolve_signal()     │
                 └───────────────▲─────────────────────────────┘
                                 │
                 ┌───────────────┴─────────────────────────────┐
                 │  application                                  │
                 │  use_cases::timesheet::reconstruct_timesheet  │  ← collecte + résout + normalise
                 │  use_cases::timesheet::save/validate_draft    │
                 │  use_cases::mapping::learn_mapping            │
                 │  repositories: TimesheetDraftRepository,       │
                 │                SignalMappingRepository         │
                 │  services: GitConnector, GryzzlyCatalog        │
                 └───────────────▲─────────────────────────────┘
                                 │
   ┌─────────────┬───────────────┼───────────────┬──────────────┐
   │ infrastructure (sqlx, git log shell)          │              │
   └─────────────┴───────────────┼───────────────┴──────────────┘
                                 │  UN contrat GraphQL (ReconstructedDay)
        ┌────────────────────────┼────────────────────────┐
        ▼                        ▼                        ▼
   Surface A                Surface B                Surface C
   CLI `aplan timesheet`    écran /timesheet         job fin de journée
```

**Ordre de construction** (même si D2 = les 3) :
1. **Socle** : règle `reconstruction` + règle `project_mapping` + connecteur git + use cases +
   repos + migrations + contrat GraphQL.
2. **Surfaces** : A, B, C en parallèle une fois le socle compilable.

---

## 5. Modèle de données

### 5.1 Migration `010_create_timesheet_drafts.sql`

```sql
CREATE TABLE timesheet_drafts (
  id            TEXT PRIMARY KEY,
  user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  date          TEXT NOT NULL,                    -- date LOCALE (ISO), pas UTC
  status        TEXT NOT NULL DEFAULT 'draft',    -- draft | validated | submitted | day_off
  target_hours  REAL NOT NULL,
  total_hours   REAL NOT NULL,
  day_confidence TEXT NOT NULL,                   -- high | medium | low
  blocks_json   TEXT,                             -- timeline sérialisée (recharge sans recalcul)
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  UNIQUE(user_id, date)
);

CREATE TABLE timesheet_draft_lines (
  id                 TEXT PRIMARY KEY,
  draft_id           TEXT NOT NULL REFERENCES timesheet_drafts(id) ON DELETE CASCADE,
  gryzzly_project_id TEXT,                          -- NULL = bucket « non attribué »
  project_name       TEXT,                          -- dénormalisé pour affichage
  hours              REAL NOT NULL,
  is_pinned          INTEGER NOT NULL DEFAULT 0,     -- valeur figée par l'utilisateur (D10)
  confidence         TEXT NOT NULL,                  -- high | medium | low
  source_refs_json   TEXT,                           -- worklogs/réunions/commits contributeurs
  created_at         TEXT NOT NULL
);
CREATE INDEX idx_tsl_draft ON timesheet_draft_lines(draft_id);
```

### 5.2 Migration `011_create_signal_project_mappings.sql`

```sql
CREATE TABLE signal_project_mappings (
  id                   TEXT PRIMARY KEY,
  user_id              TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  kind                 TEXT NOT NULL,   -- repo_path | branch | meeting_subject | meeting_organizer | internal_project
  pattern              TEXT NOT NULL,
  branch_pattern       TEXT,            -- optionnel (kind=repo_path/branch)
  gryzzly_project_id   TEXT NOT NULL,
  gryzzly_project_name TEXT,            -- dénormalisé
  is_enabled           INTEGER NOT NULL DEFAULT 1,
  created_at           TEXT NOT NULL,
  updated_at           TEXT NOT NULL,
  UNIQUE(user_id, kind, pattern)
);
CREATE INDEX idx_spm_user_kind ON signal_project_mappings(user_id, kind, is_enabled);
```

### 5.3 Clés de configuration (table `configuration` existante, aucune migration)

| Clé | Défaut | Rôle |
|-----|--------|------|
| `workday.daily_target_hours` | `7.5` | cible journalière |
| `workday.morning_start_hour` / `_end_hour` | `8` / `12` | fenêtre matin |
| `workday.afternoon_start_hour` / `_end_hour` | `13` / `17` | fenêtre après-midi |
| `gryzzly.rounding_minutes` | `15` | pas d'arrondi (→ 0,25 h) |
| `aplan.timezone` | `Europe/Paris` | conversion UTC→locale (D8) |
| `git.repos` | `` | chemins absolus séparés par virgule (D9) |
| `workday.auto_reconstruct_hour` | `18` | heure de déclenchement du job (Surface C) |
| `aplan.timesheet.last_auto_run` | — | watermark idempotence du job |
| `timesheet.min_signal_hours` | `2.0` | seuil sous lequel un jour est marqué *low-confidence* |

---

## 6. Le moteur de reconstruction

### 6.1 Types (domain)

```
DayInputs      { date, meetings: Vec<MeetingBlock>, signals: Vec<Signal> }   // heures LOCALES
MeetingBlock   { start, end, project_id: Option<String>, kind: Work|OutOfOffice, title, source_ref }
Signal         { at, project_id: Option<String>, kind: Log|Commit, label, source_ref, confidence }
ReconstructionConfig { morning:(u32,u32), afternoon:(u32,u32), daily_target_hours:f64,
                       rounding_hours:f64, min_signal_hours:f64 }

ReconstructedDay { date, allocations: Vec<ProjectAllocation>, unattributed_hours: f64,
                   unresolved: Vec<UnresolvedSignal>, total_hours: f64,
                   day_confidence: Confidence, blocks: Vec<AttributedBlock> }
ProjectAllocation{ gryzzly_project_id, project_name, hours, confidence, source_refs }
```

Signature pure : `reconstruct_day(inputs: &DayInputs, cfg: &ReconstructionConfig) -> ReconstructedDay`
(fichier `domain/src/rules/reconstruction.rs`, **zéro I/O**). Le use case
`application/src/use_cases/timesheet.rs::reconstruct_timesheet(...)` fait la collecte, la conversion
UTC→locale (helper unique), la résolution de projet, puis appelle la règle pure.

### 6.2 Algorithme

0. **Normalisation d'entrée** (use case) : lire `aplan.timezone`. Convertir chaque instant UTC
   (worklog, réunion, commit) en datetime **local naïf** (`tz.from_utc_datetime(..).naive_local()`,
   comme `materialize_worklog_time`). Résoudre le `gryzzly_project_id` de chaque signal (§7).
   Les bornes du jour sont calculées en **minuit local mappé en UTC**, pas minuit UTC naïf.
1. **Fenêtres** : deux demi-journées depuis la config. Lunch (12:00–13:00) = trou structurel non
   rempli. Chaque demi-journée est traitée indépendamment (aucun bloc ne traverse le lunch).
2. **Ancres réunions** : triées par début, découpées aux fenêtres (une réunion 11:30–14:00 est
   scindée matin/aprem). Réunions **même projet** fusionnées ; réunions **projets différents** qui se
   chevauchent → la plus tôt garde l'intervalle contesté, **les deux restent affichées et l'intervalle
   est signalé** en revue (on ne supprime jamais de temps en silence). Une réunion couvrant
   explicitement 12:00–13:00 = déjeuner de travail → compte (override du trou lunch).
   Réunions **OOO/all-day** → ancre `OutOfOffice` (voir §6.3).
3. **Signaux → blocs (carry-forward)** : dans les intervalles libres (fenêtre − réunions), chaque
   signal ouvre un bloc à son horodatage jusqu'au signal suivant / fin de l'intervalle. Le premier
   signal d'une demi-journée back-fill depuis le début de fenêtre.
4. **Chevauchement** : réunion > signal, toujours. Un commit *pendant* une réunion ne crée pas de
   temps mais reste rattaché comme `source_ref` (même projet) ou remonté en `unresolved`.
5. **Remplissage de trous** : le temps libre résiduel d'une demi-journée travaillée est réparti au
   plus proche voisin. Une demi-journée **vide** reste un vrai trou (surfacé, pas inventé).
6. **Agrégation** : somme des heures par `gryzzly_project_id`. Les blocs à projet `None` →
   `unattributed_hours` ; chaque signal contributeur → `unresolved`.
7. **Normalisation à la cible** : soit `raw` = somme brute.
   - Si `raw >= min_signal_hours` : facteur `daily_target_hours / raw` appliqué à toutes les lignes
     (non épinglées) ; le jour est `day_confidence = high|medium` selon la source.
   - **Garde-fou jour creux** : si `raw < min_signal_hours`, on **n'étale pas** sur les projets
     réels — le déficit `(target − raw)` va dans **`unattributed_hours`**, et `day_confidence = low`.
   - Si `raw == 0` : brouillon vide, notice « aucun signal », total 0 (pas de journée fabriquée).
8. **Arrondi sans dérive (largest-remainder)** : conversion en unités de `rounding_hours`, plancher,
   distribution du reste aux plus grands résidus fractionnaires → chaque ligne est un multiple propre
   **et** la somme = cible exacte. Les lignes **épinglées** (D10) sont gelées ; l'apportionnement ne
   touche que le non-épinglé ; le bucket non-attribué absorbe le résidu final.
9. **Sortie** : `ReconstructedDay` (allocations + non-attribué + unresolved + blocks pour la timeline).
   Persisté dans `timesheet_drafts`/`_lines` (`status='draft'`). Les 3 surfaces rendent cet objet.
   Toute édition passe par `save_timesheet_draft` qui **rejoue l'étape 8** sur les buckets non
   épinglés.

### 6.3 Garde-fous de transparence (résumé)

- **Confiance par ligne** : `high` (worklog/tâche assignée ou réunion mappée), `medium` (mot-clé/repo),
  `low` (remplissage étiré).
- **Jours off / PTO** : événements `show_as='oof'` ou all-day, ou titres matchant un pattern de congé
  → ancre `OutOfOffice` qui **supprime l'étirement** sur les demi-journées couvertes. Action explicite
  `markDayOff(date, scope)` → `status='day_off'`, total 0/partiel, alerte supprimée.
- **Non-attribué first-class** : jamais masqué, toujours en tête des surfaces, destination par défaut
  du surplus étiré des jours creux.
- **Valeurs épinglées** : une correction manuelle gèle la ligne (D10).

---

## 7. Couche de mapping (signal → projet Gryzzly)

Règle pure `domain/src/rules/project_mapping.rs::resolve_signal_project(signal, rules, catalog)
-> ProjectResolution` (réutilise `meeting::is_excluded`).

- **worklog** → `task.gryzzly_project_id` (existe, réutilisé ; **pas** de règle parallèle). Si absent
  → `Unmapped{TaskNotAssigned}`.
- **commit** → règles `branch` (repo+branche, plus spécifique) puis `repo_path` ; ou clé Jira dans la
  branche/message → tâche → projet. Sinon `Unmapped{NoRule}`.
- **réunion** → `is_excluded` d'abord (standup/lunch = bruit) ; puis règle `internal_project`
  (projet interne → Gryzzly, **résout le gap D3**) ; puis `meeting_organizer` (exact) ; puis
  `meeting_subject` (sous-chaîne). Organisateur > mot-clé.
- **projet Gryzzly disparu** du catalogue → dégradé en `Unmapped{StaleMapping, suggested}` (re-confirmer
  en revue) ; revalidé aussi **au rendu** du brouillon, pas seulement à la reconstruction.
- **non mappé** → bucket non-attribué + `unresolved`. La correction en revue appelle
  `learn_mapping(user_id, kind, pattern, project_id)` (**apprentissage une fois**, upsert idempotent).
  Les corrections worklog passent par `assign_gryzzly_task` sur la tâche (pas une règle).

Interfaces : trait `SignalMappingRepository` (application), impl infra ; use cases
`resolve_day_signals`, `learn_mapping`.

---

## 8. Contrat GraphQL (défini une fois — D7)

```graphql
type ReconstructedDay {
  date: NaiveDate!
  status: TimesheetStatus!          # DRAFT | VALIDATED | SUBMITTED | DAY_OFF
  targetHours: Float!
  roundingIncrement: Float!         # heures, ex 0.25
  totalHours: Float!
  dayConfidence: Confidence!        # HIGH | MEDIUM | LOW
  lines: [TimesheetLine!]!
  unattributedHours: Float!
  unresolved: [UnresolvedSignal!]!
  blocks: [AttributedBlock!]!       # timeline
}
type TimesheetLine { gryzzlyProjectId: ID, projectLabel: String!, hours: Float!,
                     confidence: Confidence!, isPinned: Boolean!, sourceRefs: [String!]! }
type AttributedBlock { index: Int!, kind: BlockKind!, label: String!, startTime: DateTime!,
                       endTime: DateTime!, hours: Float!, gryzzlyProjectId: ID, confidence: Confidence! }
type UnresolvedSignal { sourceRef: String!, label: String!, at: DateTime!, reason: UnresolvedReason! }

# Queries
timesheetDraft(date: NaiveDate!): ReconstructedDay
signalMappings: [SignalMapping!]!
gryzzlyProjects(search: String): [GryzzlyProject!]!    # picker (réutilise le catalogue)

# Mutations
runTimesheetReconstruction(date: NaiveDate!): ReconstructedDay
saveTimesheetDraft(date: NaiveDate!, lines: [TimesheetLineInput!]!): ReconstructedDay
validateTimesheet(date: NaiveDate!): ReconstructedDay
reassignBlock(date: NaiveDate!, blockIndex: Int!, gryzzlyProjectId: ID): ReconstructedDay
learnMapping(kind: MappingKind!, pattern: String!, branchPattern: String, gryzzlyProjectId: ID!): SignalMapping
markDayOff(date: NaiveDate!, scope: DayOffScope!): ReconstructedDay   # FULL | MORNING | AFTERNOON
```

Les 3 surfaces consomment **ces types exacts** (pas de renommage local).

---

## 9. Surfaces

### 9.1 Surface A — CLI `aplan timesheet` (`crates/cli`)

- `aplan timesheet [--date YYYY-MM-DD]` : reconstruit + revue interactive (défaut : aujourd'hui).
- `aplan timesheet --week [--date D]` : vue semaine lecture-seule + totaux/jour, drill-in par `--date`.
- `aplan timesheet --yes` : non-interactif (valide + persiste) — chemin du job Surface C.
- `aplan timesheet --no-save` / `--json` : dry-run / sortie machine.
- **Deux panneaux** : (1) tableau `# | PROJET (client·projet) | HEURES | source`, ligne TOTAL,
  cible + badge écart (✓/⚠), ligne `?? non attribué Xh` en tête ; (2) timeline ASCII par bande
  horaire (glyphes réunion/log/commit, index `[#]` éditable).
- **REPL** (clavier, sans re-taper) : `m <bloc#> <projet>` mapper, `s <ligne#> <h>` fixer (épingle),
  `a <projet> <h>` ajouter, `mv <h> <de> <vers>`, `p` picker projet cherchable, `r` retirer,
  `off [am|pm]` marquer off, `u` undo, `<enter>` re-render, `y` valider (double `y` si écart≠0), `q`.
- `y` → `saveTimesheetDraft` + `validateTimesheet` ; re-run d'un jour validé **recharge** le brouillon.
- Commandes de mapping : `aplan map add/list/rm`.

### 9.2 Surface B — écran `/timesheet` (frontend React)

- Route `/timesheet` + item de nav après « Activity ».
- `TimesheetReviewPage` : nav jour (réutilise `date-utils`), `TimesheetTimeline` + `ProjectSummarySidebar`
  + `TimesheetBlockSheet`. Hook `useTimesheet(date)` (urql, pattern `useActivity`).
- **Timeline** : réunions verrouillées (hachurées, `show_as`-aware) + blocs de travail colorés par
  projet (map stable `gryzzly_project_id`→couleur depuis `SLOT_COLORS`). Réutilise la math demi-journée
  d'`ActivityTimeline`.
- **Sidebar** : une ligne/projet (swatch, nom, `<input number step=increment>`), total vs cible +
  badge, bouton « Valider & verrouiller ».
- **Édition** : numérique d'abord ; réassignation projet via picker (variante projet de `TaskPicker`) ;
  drag des bords **différé** (post-v1). Chaque édition rejoue la normalisation côté client.
- Consomme le contrat §8 verbatim.

### 9.3 Surface C — job de fin de journée (`crates/api`)

- Tâche tokio `tokio::spawn(run_timesheet_scheduler(...))` dans `main.rs` avant `axum::serve`
  (Arcs partagés des repos, pas de pool dupliqué). **Interval 60 s**, pas de cron (aucun scheduler ;
  les outils MCP Cron sont côté Claude, pas un runtime backend).
- `maybe_run_daily` : lit tz + `auto_reconstruct_hour` + `last_auto_run`. Reconstruit chaque jour
  local non traité `< aujourd'hui`, plus aujourd'hui si `heure >= auto_reconstruct_hour`. Rattrapage
  des jours manqués **plafonné à 7 j**. Watermark = idempotence.
- Persiste `status='draft'` (n'écrase **jamais** `validated`/`submitted`). Écrit une alerte
  `AlertType::TimesheetReady` (Information) dédupliquée `(user_id, type, date)` — réutilise la table
  `alerts` + `AlertRepository.save_batch`. Auto-résolue à la validation.
- **Notification honnête** : badge d'alerte passif (pas de push OS/SSE). Jamais de soumission Gryzzly.

---

## 10. Stratégie de test (TDD — règle pure d'abord)

- **`rules::reconstruction`** (pur) — tests unitaires exhaustifs sur les cas piège :
  jour à 1 signal, jour réunions-seules, PTO/OOO, réunions chevauchantes, déjeuner de travail,
  bascule DST, arrondi (somme == cible), valeurs épinglées gelées, jour vide (total 0).
  **Propriété** : `sum(lines.hours) == total_hours` toujours ; `total_hours == target` sauf jour
  low-signal/off.
- **`rules::project_mapping`** (pur) — précédence (organisateur > mot-clé, branch > repo), staleness,
  exclusion.
- **Intégration** (SQLite in-memory) — `reconstruct_timesheet` : collecte→résolution→persistance ;
  idempotence du job ; re-run recharge un brouillon validé.
- **Frontend** — tests composants `TimesheetTimeline`/`ProjectSummarySidebar` ; E2E du flux
  reconstruire→corriger→valider (Playwright).

---

## 11. Risques et mitigations (issus de la revue adversariale)

| Risque | Sévérité | Mitigation intégrée |
|--------|----------|---------------------|
| Jour à faible signal étiré en journée complète fabriquée | haute | Garde-fou §6.2-7 : surplus → non-attribué, `day_confidence=low`, pas d'étalement sur projets réels |
| PTO / jours off / fériés reconstruits comme jours pleins | haute | Ancres `OutOfOffice` + `markDayOff` + seuil `raw==0`/`< min_signal_hours` |
| Commits = mauvais proxy du temps (0 ou 40) | haute | Les commits **attribuent** un bloc (quel repo), ne **dimensionnent** jamais ; tous les commits d'une demi-journée = un signal de présence |
| Double comptage worklog (bruts vs slots flushés) | haute | D5 : lire `worklog_entries` bruts, ignorer `ActivitySlot` |
| Réunions sans chemin vers un projet Gryzzly | haute | D3 : règle `internal_project` + organisateur/mot-clé |
| Validation à l'aveugle d'un brouillon faux | haute | Confiance par ligne ; acceptation explicite des lignes low/non-attribué avant `y` quand `day_confidence=low` |
| Mappings périmés (projet Gryzzly supprimé) | moyenne | Dégradation `StaleMapping` à la reconstruction **et** au rendu ; validation bloquée si projet absent du catalogue |
| Dérive d'arrondi | moyenne | Largest-remainder + valeurs épinglées |
| TZ/DST au bord de minuit | moyenne | Helper unique UTC→local ; bornes jour = minuit local mappé UTC ; skew DST absorbé par l'arrondi |
| Scheduler : pas de push réel | moyenne | Badge passif assumé ; portée honnête |

---

## 12. Ordre d'implémentation (pour le plan)

1. **Config + migrations** (010, 011) + helper TZ unique + clés config.
2. **Règle `project_mapping`** (pure + tests) + `SignalMappingRepository` + use case `learn_mapping`.
3. **Connecteur git** (infra, `git log`) + résolution commit→projet.
4. **Règle `reconstruction`** (pure + tests, tous les garde-fous) — cœur.
5. **Use cases `timesheet`** (reconstruct/save/validate/markDayOff) + `TimesheetDraftRepository`.
6. **Contrat GraphQL** (types + queries + mutations).
7. **Surface A** (CLI), **Surface B** (React), **Surface C** (job) — en parallèle.
8. **Mise à jour specs** `SPEC_FONCTIONNELLE.md` / `SPEC_TECHNIQUE.md` (règle projet : même commit).

---

## 13. Questions ouvertes résiduelles (à trancher en implémentation, non bloquantes)

- Réunion→Gryzzly : privilégier la règle `internal_project` (mapping projet interne→Gryzzly) comme
  chemin principal, l'organisateur/mot-clé en secours. À confirmer selon la structure réelle des
  projets internes vs Gryzzly.
- Assignations réunion « one-shot » (ce jour seulement, sans règle) : recomputées à chaque revue ou
  persistées ? Proposé : re-mapper est peu coûteux, pas de table d'override pour l'instant.
- Découverte des repos git : liste explicite `git.repos` (retenu) vs scan d'une racine.
- `--week` en écriture (batch-valider) : hors v1, lecture-seule pour préserver la promesse 30 s.
