# Apply Spec Review Changes — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Apply the 12 changes from the spec review to SPEC_FONCTIONNELLE.md and SPEC_TECHNIQUE.md, making both specs consistent and implementation-ready.

**Architecture:** Pure documentation changes — editing two markdown specification files. No code changes (the existing codebase is being replaced). CLAUDE.md update is deferred to implementation start.

**Tech Stack:** Markdown editing only.

---

### Task 1: Add Task Scheduling Fields (SPEC_FONCTIONNELLE.md)

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md:593-600` (R01 — granularity rule)
- Modify: `SPEC_FONCTIONNELLE.md:660-681` (Task entity table)
- Modify: `SPEC_FONCTIONNELLE.md:487-504` (US-051, US-052 — alerts)
- Modify: `SPEC_FONCTIONNELLE.md:632-634` (R16, R18 — alert rules)
- Modify: `SPEC_FONCTIONNELLE.md:876-891` (Glossary)

**Step 1: Update R01 — split granularity between tasks and project assignment**

At line 597, replace:

```markdown
| **R01** | Toute planification utilise la granularité **demi-journée** : matin (8h-12h) et après-midi (13h-17h) |
```

With:

```markdown
| **R01a** | L'affectation des développeurs aux projets utilise la granularité **demi-journée** : matin (8h-12h) et après-midi (13h-17h) |
| **R01b** | La planification des **tâches** et des **réunions** utilise des **créneaux horaires** (heures de début et de fin). Les tâches sont représentées visuellement à une taille proportionnelle à leur estimation. |
```

**Step 2: Add scheduling fields to Task entity**

At line 674 (after `échéance`), insert three new rows:

```markdown
| planificationDébut | Date/heure | Non | Date et heure de début planifiée |
| planificationFin | Date/heure | Non | Date et heure de fin planifiée |
| estimationHeures | Décimal | Non | Estimation de la durée en heures. Détermine la taille visuelle de la tâche. |
```

**Step 3: Update US-051 criteria**

At line 487, replace:

```markdown
- L'alerte se déclenche quand le total des tâches planifiées + réunions dépasse la capacité en demi-journées de la semaine
```

With:

```markdown
- L'alerte se déclenche quand le total des heures planifiées (tâches + réunions) dépasse la capacité hebdomadaire en heures
```

**Step 4: Update US-052 criteria**

At lines 501-502, replace:

```markdown
- Un conflit est détecté quand une tâche est planifiée sur un créneau déjà occupé par une réunion Outlook
- Un conflit est détecté quand deux tâches sont planifiées sur le même créneau (demi-journée)
```

With:

```markdown
- Un conflit est détecté quand le créneau horaire d'une tâche chevauche celui d'une réunion Outlook
- Un conflit est détecté quand les créneaux horaires de deux tâches se chevauchent
```

**Step 5: Update R16 and R18**

At line 632, replace:

```markdown
| **R16** | Une alerte de surcharge est émise lorsque la charge totale (tâches planifiées + réunions) dépasse la capacité hebdomadaire (R02). |
```

With:

```markdown
| **R16** | Une alerte de surcharge est émise lorsque la charge totale en heures (tâches planifiées + réunions) dépasse la capacité hebdomadaire. |
```

At line 634, replace:

```markdown
| **R18** | Une alerte de conflit est émise lorsque deux éléments (tâche/réunion) sont planifiés sur le même créneau (même demi-journée). |
```

With:

```markdown
| **R18** | Une alerte de conflit est émise lorsque les créneaux horaires de deux éléments (tâche/réunion) se chevauchent. |
```

**Step 6: Update Glossary**

At line 880, replace:

```markdown
| **Charge** | Nombre de demi-journées consommées par les tâches planifiées et les réunions |
```

With:

```markdown
| **Charge** | Nombre d'heures consommées par les tâches planifiées et les réunions |
```

At line 882, replace:

```markdown
| **Conflit** | Deux éléments (tâche/réunion) planifiés sur le même créneau |
```

With:

```markdown
| **Conflit** | Deux éléments (tâche/réunion) dont les créneaux horaires se chevauchent |
```

Add new glossary entry:

```markdown
| **Créneau horaire** | Plage horaire définie par une heure de début et une heure de fin, utilisée pour planifier tâches et réunions |
| **Estimation** | Durée estimée d'une tâche en heures, déterminant sa taille visuelle dans les vues planning |
```

**Step 7: Commit**

```bash
git add SPEC_FONCTIONNELLE.md
git commit -m "spec(fonctionnelle): add hour-based task scheduling and estimated_hours"
```

---

### Task 2: Add Task Scheduling Fields (SPEC_TECHNIQUE.md)

**Files:**
- Modify: `SPEC_TECHNIQUE.md:651-674` (Task struct)
- Modify: `SPEC_TECHNIQUE.md:838-845` (sort_tasks_by_priority)
- Modify: `SPEC_TECHNIQUE.md:847-875` (workload rules)
- Modify: `SPEC_TECHNIQUE.md:920-950` (conflict/overload alerts)
- Modify: `SPEC_TECHNIQUE.md:1246-1290` (CreateTaskInput + create_personal_task)
- Modify: `SPEC_TECHNIQUE.md:1937-1955` (tasks table)
- Modify: `SPEC_TECHNIQUE.md:2128-2146` (Task GraphQL type)
- Modify: `SPEC_TECHNIQUE.md:2225-2232` (HalfDaySlot type)
- Modify: `SPEC_TECHNIQUE.md:2305-2324` (CreateTaskInput/UpdateTaskInput GraphQL)
- Modify: `SPEC_TECHNIQUE.md:2640-2651` (alert engine process)

**Step 1: Add fields to Task struct**

At `SPEC_TECHNIQUE.md:665` (after `deadline`), insert:

```rust
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub estimated_hours: Option<f32>,
```

**Step 2: Add columns to tasks table**

At `SPEC_TECHNIQUE.md:1949` (after `deadline TEXT,`), insert:

```sql
    planned_start TEXT,
    planned_end TEXT,
    estimated_hours REAL,
```

**Step 3: Update Task GraphQL type**

At `SPEC_TECHNIQUE.md:2138` (after `deadline: Date`), insert:

```graphql
  plannedStart: DateTime
  plannedEnd: DateTime
  estimatedHours: Float
```

**Step 4: Update CreateTaskInput GraphQL**

At `SPEC_TECHNIQUE.md:2311` (after `deadline: Date`), insert:

```graphql
  plannedStart: DateTime
  plannedEnd: DateTime
  estimatedHours: Float
```

**Step 5: Update UpdateTaskInput GraphQL**

At `SPEC_TECHNIQUE.md:2320` (after `deadline: Date`), insert:

```graphql
  plannedStart: DateTime
  plannedEnd: DateTime
  estimatedHours: Float
```

**Step 6: Update CreateTaskInput struct**

At `SPEC_TECHNIQUE.md:1252` (after `deadline`), insert:

```rust
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub estimated_hours: Option<f32>,
```

**Step 7: Update create_personal_task**

At `SPEC_TECHNIQUE.md:1279` (after `deadline: input.deadline,`), insert:

```rust
        planned_start: input.planned_start,
        planned_end: input.planned_end,
        estimated_hours: input.estimated_hours,
```

**Step 8: Rewrite check_conflict_alerts to use time-range overlap**

Replace `SPEC_TECHNIQUE.md:922-930` with:

```rust
/// R18: Check for scheduling conflicts on a given date.
/// A conflict occurs when two items have overlapping time ranges.
pub fn check_conflict_alerts(
    scheduled_items: &[ScheduledItem],
    date: NaiveDate,
) -> Vec<AlertData> {
    // For each pair of items, check if [start_a, end_a) overlaps [start_b, end_b)
    // Overlap condition: start_a < end_b AND start_b < end_a
}

pub enum ScheduledItem {
    Task { id: TaskId, title: String, start: DateTime<Utc>, end: DateTime<Utc> },
    Meeting { id: MeetingId, title: String, start: DateTime<Utc>, end: DateTime<Utc> },
}
```

**Step 9: Update workload to use hours**

Replace `SPEC_TECHNIQUE.md:852-869` with:

```rust
/// Calculate hours consumed by a meeting.
pub fn meeting_hours(start: DateTime<Utc>, end: DateTime<Utc>) -> f64 {
    (end - start).num_minutes() as f64 / 60.0
}

/// R16: Detect overload for a week.
/// Returns Some(excess_hours) if total load exceeds capacity, None otherwise.
pub fn detect_overload(
    planned_task_hours: f64,
    meeting_hours: f64,
    weekly_capacity_hours: f64,
) -> Option<f64> {
    let total = planned_task_hours + meeting_hours;
    if total > weekly_capacity_hours { Some(total - weekly_capacity_hours) } else { None }
}
```

**Step 10: Update alert engine process description**

At `SPEC_TECHNIQUE.md:2647`, replace:

```markdown
   - check_conflict_alerts(tasks_by_half_day, meetings_by_half_day, dates)
```

With:

```markdown
   - check_conflict_alerts(scheduled_items, dates) — using time-range overlap
```

**Step 11: Update HalfDaySlot comment**

At `SPEC_TECHNIQUE.md:2225-2232`, add a comment noting HalfDaySlot is for project assignment view only:

```graphql
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
```

**Step 12: Commit**

```bash
git add SPEC_TECHNIQUE.md
git commit -m "spec(technique): add hour-based task scheduling, estimated_hours, time-range conflicts"
```

---

### Task 3: Remove linked_task_id (Both Specs)

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md:679` (Task entity — liée_à row)
- Modify: `SPEC_TECHNIQUE.md:670` (Task struct — linked_task_id)
- Modify: `SPEC_TECHNIQUE.md:1283` (create_personal_task — linked_task_id)
- Modify: `SPEC_TECHNIQUE.md:1952` (tasks table — linked_task_id column)
- Modify: `SPEC_TECHNIQUE.md:2142` (Task GraphQL — linkedTask)
- Modify: `SPEC_TECHNIQUE.md:2575` (sync preserving local overrides)
- Modify: `SPEC_TECHNIQUE.md:2602` (dedup engine — linked_task_id)

**Step 1: Remove liée_à from functional spec**

Delete line 679:

```markdown
| liée_à | Référence Tâche | Non | Référence vers tâche fusionnée (dédoublonnage) |
```

**Step 2: Remove linked_task_id from Task struct**

Delete line 670:

```rust
    pub linked_task_id: Option<TaskId>,
```

**Step 3: Remove linked_task_id from create_personal_task**

Delete line 1283:

```rust
        linked_task_id: None,
```

**Step 4: Remove linked_task_id column from tasks table**

Delete line 1952:

```sql
    linked_task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
```

**Step 5: Remove linkedTask from GraphQL Task type**

Delete line 2142:

```graphql
  linkedTask: Task
```

**Step 6: Remove linked_task_id from sync override list**

At line 2575, delete:

```markdown
- `linked_task_id`
```

**Step 7: Update dedup engine description**

At line 2602, replace:

```markdown
   - The Excel task's linked_task_id points to the Jira task
```

With:

```markdown
   - A `task_link` record (type: auto_merged) links the Excel task to the Jira task
```

**Step 8: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "spec: remove linked_task_id — use task_links table exclusively for dedup"
```

---

### Task 4: Document Multi-User Decision (SPEC_FONCTIONNELLE.md)

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md:862-870` (section 11.3 Décisions prises)

**Step 1: Add D6 decision**

After line 870 (D5 row), insert:

```markdown
| D6 | Architecture multi-user ready (`user_id` sur toutes les tables, middleware d'authentification) | Prépare le déploiement futur en tant qu'application Microsoft Teams. Utilisateur unique en MVP — utilisateur par défaut créé automatiquement. |
```

**Step 2: Commit**

```bash
git add SPEC_FONCTIONNELLE.md
git commit -m "spec(fonctionnelle): document multi-user readiness as decision D6"
```

---

### Task 5: Move Tags to MVP (SPEC_FONCTIONNELLE.md)

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md:587` (US-064 priority)

**Step 1: Change priority**

At line 587, replace:

```markdown
**Priorité** : Could (v2)
```

With:

```markdown
**Priorité** : Must (MVP v1)
```

**Step 2: Commit**

```bash
git add SPEC_FONCTIONNELLE.md
git commit -m "spec(fonctionnelle): move tags (US-064) from v2 to MVP"
```

---

### Task 6: Project-Meeting Association (Both Specs)

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md:530-538` (US-061 criteria)
- Modify: `SPEC_TECHNIQUE.md:2362-2396` (GraphQL mutations)
- Modify: `SPEC_TECHNIQUE.md:2479-2492` (Outlook sync mapping)

**Step 1: Update US-061 criteria in functional spec**

At line 534-535, replace:

```markdown
  - Les réunions associées au projet (si le projet est dans le titre de la réunion Outlook)
```

With:

```markdown
  - Les réunions associées au projet (détection automatique depuis le titre de la réunion Outlook, modifiable manuellement par l'utilisateur)
```

**Step 2: Add mutation to GraphQL schema**

At `SPEC_TECHNIQUE.md:2393` (after `forceSync` mutation), insert:

```graphql
  # Meeting-Project association
  updateMeetingProject(meetingId: ID!, projectId: ID): Meeting!
```

**Step 3: Add auto-detection note to Outlook mapping**

At `SPEC_TECHNIQUE.md:2489` (after the `isCancelled` mapping row), insert:

```markdown

**Project auto-detection:** After mapping, the sync engine scans the meeting `title` for known project names (case-insensitive substring match). If found, `project_id` is set automatically. The user can override this association via the `updateMeetingProject` mutation.
```

**Step 4: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "spec: project-meeting association with auto-detect and manual override"
```

---

### Task 7: Jira Raw Status Display (SPEC_TECHNIQUE.md)

**Files:**
- Modify: `SPEC_TECHNIQUE.md:651-674` (Task struct)
- Modify: `SPEC_TECHNIQUE.md:1937-1955` (tasks table)
- Modify: `SPEC_TECHNIQUE.md:2128-2146` (Task GraphQL type)
- Modify: `SPEC_TECHNIQUE.md:2448-2454` (Jira mapping table)

**Step 1: Add jira_status to Task struct**

At `SPEC_TECHNIQUE.md:661` (after `source_id`), insert:

```rust
    pub jira_status: Option<String>,
```

**Step 2: Add jira_status column to tasks table**

At `SPEC_TECHNIQUE.md:1944` (after `source_id TEXT,`), insert:

```sql
    jira_status TEXT,
```

**Step 3: Add jiraStatus to GraphQL Task type**

At `SPEC_TECHNIQUE.md:2134` (after `sourceId: String`), insert:

```graphql
  jiraStatus: String
```

**Step 4: Update Jira mapping table**

At `SPEC_TECHNIQUE.md:2450`, replace:

```markdown
| `fields.status.name` | `status` | Map: "To Do"->Todo, "In Progress"->InProgress, "Done"->Done |
```

With:

```markdown
| `fields.status.name` | `jira_status` | Direct — raw Jira status string stored as-is |
| `fields.status.statusCategory.key` | `status` | Map: "new"->Todo, "indeterminate"->InProgress, "done"->Done. Uses Jira status category (3 universal values) rather than custom status names. |
```

**Step 5: Commit**

```bash
git add SPEC_TECHNIQUE.md
git commit -m "spec(technique): display raw Jira status, map via statusCategory for app status"
```

---

### Task 8: Week Starts Monday (Both Specs)

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md:876-891` (Glossary)
- Modify: `SPEC_TECHNIQUE.md:45-53` (Definitions table)

**Step 1: Add week definition to functional spec glossary**

After the existing glossary entries (after line 890), insert:

```markdown
| **Semaine** | Période du lundi au vendredi (5 jours ouvrés). Le lundi est le premier jour de la semaine. |
```

**Step 2: Add week definition to tech spec**

At `SPEC_TECHNIQUE.md:53` (after the `Source` definition row), insert:

```markdown
| **Week** | Monday to Friday (5 business days). Monday is the first day of the week. `week_start_of(date)` returns the Monday of the given date's week. |
```

**Step 3: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "spec: define week as Monday-Friday in both specs"
```

---

### Task 9: Cursor-Based Pagination (SPEC_TECHNIQUE.md)

**Files:**
- Modify: `SPEC_TECHNIQUE.md:2126-2146` (GraphQL types section — add pagination types)
- Modify: `SPEC_TECHNIQUE.md:2340-2358` (Query type)

**Step 1: Add pagination types before Query**

At `SPEC_TECHNIQUE.md:2338` (before `# --- Queries ---`), insert:

```graphql
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
```

**Step 2: Update tasks and alerts queries**

At `SPEC_TECHNIQUE.md:2342`, replace:

```graphql
  tasks(filter: TaskFilter): [Task!]!
```

With:

```graphql
  tasks(filter: TaskFilter, first: Int = 50, after: String): TaskConnection!
```

At `SPEC_TECHNIQUE.md:2348`, replace:

```graphql
  alerts(resolved: Boolean): [Alert!]!
```

With:

```graphql
  alerts(resolved: Boolean, first: Int = 50, after: String): AlertConnection!
```

**Step 3: Commit**

```bash
git add SPEC_TECHNIQUE.md
git commit -m "spec(technique): add cursor-based pagination for tasks and alerts queries"
```

---

### Task 10: Configurable Working Hours (Both Specs)

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md:737-752` (section 8.2 configuration)
- Modify: `SPEC_TECHNIQUE.md:2696` (reminder suppression)
- Modify: `SPEC_TECHNIQUE.md:2730-2757` (configuration parameters)

**Step 1: Add to functional spec configuration**

At `SPEC_FONCTIONNELLE.md:746` (after `déclencheurPériodique`), insert:

```markdown
| heuresDébutTravail | Heure (HH:MM) | 08:00 | Heure de début de la journée de travail |
| heuresFinTravail | Heure (HH:MM) | 17:00 | Heure de fin de la journée de travail |
```

**Step 2: Update tech spec reminder suppression**

At `SPEC_TECHNIQUE.md:2696`, replace:

```markdown
- No reminders on weekends or outside working hours (08:00-17:00)
```

With:

```markdown
- No reminders on weekends or outside configured working hours (default: 08:00-17:00, configurable via `working_hours_start` / `working_hours_end`)
```

**Step 3: Add to tech spec configuration parameters**

At `SPEC_TECHNIQUE.md:2743` (after `periodic_reminder_enabled`), insert:

```markdown
| `working_hours_start` | string | `"08:00"` | Start of working day (HH:MM format) |
| `working_hours_end` | string | `"17:00"` | End of working day (HH:MM format) |
```

**Step 4: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "spec: add configurable working hours (default 08:00-17:00)"
```

---

### Task 11: Add MeetingRepository.find_by_project (SPEC_TECHNIQUE.md)

**Files:**
- Modify: `SPEC_TECHNIQUE.md:1080-1091` (MeetingRepository trait)
- Modify: `SPEC_TECHNIQUE.md:2049-2058` (database indexes)

**Step 1: Add method to MeetingRepository trait**

At `SPEC_TECHNIQUE.md:1091` (before the closing `}`), insert:

```rust
    async fn find_by_project(
        &self, user_id: UserId, project_id: ProjectId,
    ) -> Result<Vec<Meeting>, RepositoryError>;
```

**Step 2: Add database index**

At `SPEC_TECHNIQUE.md:2055` (after `idx_meetings_user_time`), insert:

```sql
CREATE INDEX idx_meetings_project ON meetings(project_id);
```

**Step 3: Commit**

```bash
git add SPEC_TECHNIQUE.md
git commit -m "spec(technique): add MeetingRepository.find_by_project and index"
```

---

### Task 12: Note CLAUDE.md Update Needed (SPEC_TECHNIQUE.md)

**Files:**
- Modify: `SPEC_TECHNIQUE.md:2896-2964` (section 18 MVP scope)

**Step 1: Add note to MVP Phase 1**

At `SPEC_TECHNIQUE.md:2910` (after "Repository integration tests" in Phase 1), insert:

```markdown
- Update `CLAUDE.md` to reflect new tech stack (Rust/Axum backend, GraphQL API, SQLite, urql frontend)
```

**Step 2: Commit**

```bash
git add SPEC_TECHNIQUE.md
git commit -m "spec(technique): note CLAUDE.md update needed in MVP Phase 1"
```

---

### Task 13: Final Review and Verification

**Step 1: Verify all changes are coherent**

Read both files end-to-end and check:
- No broken markdown table formatting
- No dangling references to removed fields (`linked_task_id`, `liée_à`, `linkedTask`)
- Line numbers in tasks above may shift as edits accumulate — apply sequentially

**Step 2: Verify no broken internal references**

Run:
```bash
grep -n "linked_task_id\|liée_à\|linkedTask" SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
```
Expected: no matches (except in dedup engine section which now references `task_link` record instead).

Run:
```bash
grep -n "demi-journée" SPEC_FONCTIONNELLE.md | head -20
```
Verify remaining uses of "demi-journée" are in the context of project assignment, not task scheduling.

**Step 3: Final commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "spec: final review pass — verify internal consistency after all changes"
```

Only commit if there are fixups from the review. If clean, skip this commit.
