# Lecture de la mémoire — plan 2 : la traction

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Donner à une session de quoi interroger tout aplan pendant qu'elle travaille — `aplan search --q` sur les tâches, le worklog, les réunions et les mémoires — et une skill qui sait quand s'en servir.

**Architecture:** Une règle pure `domain/rules/search.rs` porte la normalisation (minuscules + pliage des diacritiques) et la correspondance, pour que la recherche se comporte pareil sur les quatre entités — `memories_fts` plie déjà les accents, un `LIKE` SQLite non. Un cas d'usage compose les dépôts **existants** et réutilise le scoring `recall` tel quel pour les mémoires ; aucun nouveau trait de dépôt n'est nécessaire. La sortie est groupée par entité, jamais fusionnée en un classement unique, et plafonnée à 5 résultats par groupe.

**Tech Stack:** Rust (crates `domain` / `application` / `api` / `cli`), async-graphql 7, sqlx 0.8, clap, graphql_client.

**Spec:** `docs/superpowers/specs/2026-08-18-lecture-memoire-design.md`

**Dépend de :** rien du plan 1 ; les deux plans sont livrables dans l'ordre qu'on veut. Le plan 1 reste prioritaire — il livre de la valeur seul.

## Global Constraints

- **Séparation DDD stricte** : `domain` sans I/O ni dépendance hors chrono/serde/uuid/thiserror ; `application` ne dépend que de `domain` ; `infrastructure` implémente les traits ; `api` dépend de tout.
- **Pas de classement unique.** La sortie est groupée par entité : mémoires par pertinence (scoring `recall` existant), tâches / worklog / réunions par récence. Mélanger un BM25 avec une correspondance de titre produit un ordre qui ne veut rien dire.
- **Plafond de sortie** : `SEARCH_MAX_PER_GROUP = 5`, relevable par `--limit`. Le lecteur est un agent ; une commande qui crache 642 tâches ne sera plus jamais appelée. Toute troncature est annoncée.
- **Aucun plafond caché.** Ne jamais passer par la requête GraphQL `tasks`, qui sort en `first:50` DESC alors que la base compte 642 tâches. Le plafond du worklog côté serveur est `WORKLOG_FILTER_MAX_LIMIT` = 1000 ; il est au-dessus des 572 entrées actuelles, mais la pagination doit être écrite comme si ce n'était pas le cas.
- **Les réunions se cherchent sur une fenêtre, pas sur tout.** `MeetingRepository` n'expose **pas** de `list` : seulement `find_by_id`, `find_by_user_and_date` et `find_by_user_and_range`. La recherche interroge donc une fenêtre glissante de 24 mois en arrière — largement au-dessus des 26 réunions que la base contient — et le dit dans l'aide de la commande. Ajouter une méthode de dépôt serait la seule alternative ; elle n'est pas justifiée à ce volume.
- **Même sémantique que FTS5** : `unicode61 remove_diacritics 2` — « memoire » et « mémoire » doivent ramener les mêmes lignes sur *toutes* les entités.
- **TDD** : test d'abord, exécution pour le voir échouer, implémentation minimale, exécution pour le voir passer, commit.
- **La crate `mcp` ne compile pas** et est exclue du workspace. Ne jamais lancer de commande cargo qui l'inclut.
- **Spécifications en français**, code et commentaires en anglais. Mise à jour dans le même commit que le comportement.
- **Message de commit** : sujet impératif, sans préfixe Jira, sans `Co-Authored-By`, sans `Signed-off-by`.

---

### Task 1 : la correspondance textuelle, règle pure du domaine

**Files:**
- Create: `backend/crates/domain/src/rules/search.rs`
- Modify: `backend/crates/domain/src/rules/mod.rs` (19 lignes, une déclaration par règle)
- Test: dans `search.rs`, `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: rien.
- Produces: `pub fn normalize(text: &str) -> String`, `pub fn parse_terms(query: &str) -> Vec<String>`, `pub fn matches(haystack: &str, terms: &[String]) -> bool`. Consommés par les Tasks 2 et 3.

- [ ] **Step 1 : écrire les tests qui échouent**

Créer `backend/crates/domain/src/rules/search.rs` avec, pour tout contenu :

```rust
//! Cross-entity text matching.
//!
//! `memories_fts` folds diacritics (`unicode61 remove_diacritics 2`), a SQLite
//! `LIKE` does not. Searching memories one way and tasks another would give a
//! query that behaves differently depending on which entity it happens to hit,
//! so the folding lives here — one rule, four entities, no I/O.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_folds_case_and_diacritics() {
        assert_eq!(normalize("Réunion ÉLECTRIQUE"), "reunion electrique");
        assert_eq!(normalize("mémoire"), normalize("MEMOIRE"));
    }

    #[test]
    fn parse_terms_splits_on_whitespace_and_drops_empties() {
        assert_eq!(parse_terms("  WAF   eActions "), vec!["waf", "eactions"]);
        assert!(parse_terms("   ").is_empty());
    }

    #[test]
    fn matches_requires_every_term() {
        let terms = parse_terms("waf eactions");
        assert!(matches("Les 403 sur l'API eActions viennent du WAF Front Door", &terms));
        assert!(!matches("Le WAF de TotalEnergies", &terms));
    }

    #[test]
    fn matches_ignores_accents_on_both_sides() {
        assert!(matches("fenêtre de maintenance", &parse_terms("fenetre")));
        assert!(matches("fenetre de maintenance", &parse_terms("fenêtre")));
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        assert!(!matches("n'importe quoi", &parse_terms("")));
    }
}
```

Puis déclarer le module dans `backend/crates/domain/src/rules/mod.rs`, à la suite des autres :

```rust
pub mod search;
```

- [ ] **Step 2 : lancer les tests pour les voir échouer**

```bash
cd backend && cargo test -p domain --lib rules::search
```

Attendu : ÉCHEC de compilation, `cannot find function 'normalize' in this scope`.

- [ ] **Step 3 : écrire l'implémentation minimale**

Ajouter au-dessus du `mod tests` de `search.rs` :

```rust
/// Lowercase and strip the diacritics FTS5 strips, so the same query reaches
/// memories and tasks alike. Deliberately narrow: the Latin-1 range covers every
/// accent this store actually holds (French, plus the odd German umlaut).
pub fn normalize(text: &str) -> String {
    text.chars()
        .flat_map(|c| c.to_lowercase())
        .map(fold_diacritic)
        .collect()
}

fn fold_diacritic(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        other => other,
    }
}

/// The query, cut into normalized terms. Whitespace only — no operators, no
/// quoting: this is not FTS5's query language and must not pretend to be.
pub fn parse_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(normalize)
        .filter(|t| !t.is_empty())
        .collect()
}

/// Every term must appear. An empty term list matches nothing — returning
/// everything would turn a typo into a full table dump.
pub fn matches(haystack: &str, terms: &[String]) -> bool {
    if terms.is_empty() {
        return false;
    }
    let hay = normalize(haystack);
    terms.iter().all(|t| hay.contains(t.as_str()))
}
```

- [ ] **Step 4 : lancer les tests pour les voir passer**

```bash
cd backend && cargo test -p domain --lib rules::search
```

Attendu : les 5 tests PASSENT.

- [ ] **Step 5 : commit**

```bash
git add backend/crates/domain/src/rules/search.rs backend/crates/domain/src/rules/mod.rs
git commit -m "Match query terms the way FTS5 does, across every entity"
```

---

### Task 2 : les types de résultat groupés

**Files:**
- Modify: `backend/crates/domain/src/rules/search.rs`
- Test: même fichier

**Interfaces:**
- Consumes: `matches`, `parse_terms` (Task 1).
- Produces: `pub const SEARCH_MAX_PER_GROUP: usize = 5`, `pub struct SearchHit { pub id: String, pub title: String, pub occurred_on: NaiveDate }`, `pub struct SearchGroup { pub hits: Vec<SearchHit>, pub total: usize }` avec `pub fn hidden(&self) -> usize`, et `pub fn group_from(hits: Vec<SearchHit>, cap: usize) -> SearchGroup`. Consommés par la Task 3.

- [ ] **Step 1 : écrire les tests qui échouent**

À ajouter dans `mod tests` de `search.rs` :

```rust
fn hit(title: &str, day: u32) -> SearchHit {
    SearchHit {
        id: format!("id-{day}"),
        title: title.to_string(),
        occurred_on: NaiveDate::from_ymd_opt(2026, 8, day).expect("valid date"),
    }
}

#[test]
fn a_group_caps_and_says_what_it_hid() {
    let group = group_from(vec![hit("un", 1), hit("deux", 2), hit("trois", 3)], 2);

    assert_eq!(group.hits.len(), 2);
    assert_eq!(group.total, 3);
    assert_eq!(group.hidden(), 1, "la troncature n'est jamais silencieuse");
}

#[test]
fn a_group_under_its_cap_hides_nothing() {
    let group = group_from(vec![hit("un", 1)], SEARCH_MAX_PER_GROUP);

    assert_eq!(group.hidden(), 0);
}
```

- [ ] **Step 2 : lancer les tests pour les voir échouer**

```bash
cd backend && cargo test -p domain --lib rules::search
```

Attendu : ÉCHEC de compilation, `cannot find type 'SearchHit' in this scope`.

- [ ] **Step 3 : écrire l'implémentation minimale**

Ajouter en tête de `search.rs`, sous le `//!` de module :

```rust
use chrono::NaiveDate;

/// How many hits a group shows before it starts hiding them. The caller is an
/// agent: a command that prints 642 tasks is a command nobody calls twice.
pub const SEARCH_MAX_PER_GROUP: usize = 5;

/// One result, reduced to what a caller needs to decide whether to drill in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    pub occurred_on: NaiveDate,
}

/// One entity's results, plus the count they were cut down from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchGroup {
    pub hits: Vec<SearchHit>,
    /// How many matched, before the cap.
    pub total: usize,
}

impl SearchGroup {
    pub fn hidden(&self) -> usize {
        self.total.saturating_sub(self.hits.len())
    }
}

/// Cap a group, remembering what it dropped.
pub fn group_from(hits: Vec<SearchHit>, cap: usize) -> SearchGroup {
    let total = hits.len();
    SearchGroup {
        hits: hits.into_iter().take(cap).collect(),
        total,
    }
}
```

- [ ] **Step 4 : lancer les tests pour les voir passer**

```bash
cd backend && cargo test -p domain --lib rules::search
```

Attendu : les 7 tests PASSENT.

- [ ] **Step 5 : commit**

```bash
git add backend/crates/domain/src/rules/search.rs
git commit -m "Group search hits per entity, capped and never silently"
```

---

### Task 3 : le cas d'usage qui compose les dépôts existants

**Files:**
- Create: `backend/crates/application/src/use_cases/search.rs`
- Modify: `backend/crates/application/src/use_cases/mod.rs`
- Test: dans `search.rs`, avec les dépôts en mémoire déjà utilisés par `use_cases/brief.rs`

**Note d'écart avec la spec :** la § 5.2 de la spec prévoyait un nouveau trait de dépôt. Ce n'est pas nécessaire : `TaskRepository::list`, `WorklogRepository::list`, `MeetingRepository::list` et le chemin `recall` existant suffisent, et le domaine filtre. Un trait de plus n'achèterait rien qu'une indirection.

**Interfaces:**
- Consumes: `domain::rules::search::{parse_terms, matches, group_from, SearchHit, SearchGroup, SEARCH_MAX_PER_GROUP}` (Tasks 1-2) ; les traits `TaskRepository`, `WorklogRepository`, `MeetingRepository` ; le service de recall déjà utilisé par `memory_cmd`.
- Produces: `pub struct SearchRequest { pub query: String, pub limit: usize }`, `pub struct SearchOutcome { pub tasks: SearchGroup, pub worklog: SearchGroup, pub meetings: SearchGroup, pub memories: SearchGroup }`, et `pub async fn search(...) -> Result<SearchOutcome, RepositoryError>`. Consommés par la Task 4.

- [ ] **Step 1 : écrire le test qui échoue**

Créer `backend/crates/application/src/use_cases/search.rs` et y écrire le test avant tout le reste.

Le montage de référence est celui de `use_cases/brief.rs`, `mod tests`, ligne ~520 : `struct Fixture`
avec `Fixture::new()`, les champs `tasks` / `memories` / `activity` / `config`, les types
`MemTaskRepo` / `MemMemoryRepo`, et le helper `uid()`. **Il n'a ni dépôt de worklog ni dépôt de
réunions** — ce sont les deux fabriques à écrire ici, sur le patron exact de `MemTaskRepo`.

```rust
#[tokio::test]
async fn search_groups_hits_by_entity() {
    let f = Fixture::new();
    f.tasks.create(&task_titled("Réunion WAF eActions")).await.expect("created");
    f.tasks.create(&task_titled("Sans rapport")).await.expect("created");

    let out = search(
        &f.tasks,
        &f.worklog,
        &f.meetings,
        &f.memories,
        uid(),
        SearchRequest { query: "waf".to_string(), limit: SEARCH_MAX_PER_GROUP },
    )
    .await
    .expect("search ran");

    assert_eq!(out.tasks.total, 1, "une seule tâche porte le terme");
    assert_eq!(out.tasks.hits[0].title, "Réunion WAF eActions");
    assert!(out.worklog.hits.is_empty());
}

#[tokio::test]
async fn search_folds_accents_on_tasks_like_it_does_on_memories() {
    let f = Fixture::new();
    f.tasks.create(&task_titled("Fenêtre de maintenance")).await.expect("created");

    let out = search(
        &f.tasks,
        &f.worklog,
        &f.meetings,
        &f.memories,
        uid(),
        SearchRequest { query: "fenetre".to_string(), limit: SEARCH_MAX_PER_GROUP },
    )
    .await
    .expect("search ran");

    assert_eq!(out.tasks.total, 1, "sans pliage, ce terme ne trouverait rien");
}

#[tokio::test]
async fn an_empty_query_returns_nothing_rather_than_everything() {
    let f = Fixture::new();
    f.tasks.create(&task_titled("Réunion WAF eActions")).await.expect("created");

    let out = search(
        &f.tasks,
        &f.worklog,
        &f.meetings,
        &f.memories,
        uid(),
        SearchRequest { query: "   ".to_string(), limit: SEARCH_MAX_PER_GROUP },
    )
    .await
    .expect("search ran");

    assert_eq!(out.tasks.total, 0, "une requête vide ne devient jamais un dump");
}
```

Déclarer le module dans `backend/crates/application/src/use_cases/mod.rs`.

- [ ] **Step 2 : lancer le test pour le voir échouer**

```bash
cd backend && cargo test -p application search_groups_hits
```

Attendu : ÉCHEC de compilation, `cannot find function 'search' in this scope`.

- [ ] **Step 3 : écrire l'implémentation minimale**

Dans `search.rs`, au-dessus du `mod tests`. Points d'attention, tous dictés par les contraintes globales :

- les tâches sont lues via `TaskRepository::list` avec un `TaskFilter::empty()` — **jamais** via la requête GraphQL `tasks` et son `first: 50` ;
- le worklog est lu par pages, comme le fait `aplan show --worklog all` : demander `i64::MAX` en une fois rendrait silencieusement les 1000 premières lignes et le résultat *paraîtrait* complet ;
- les mémoires passent par le chemin `recall` existant, dont l'ordre par pertinence est conservé tel quel — ne pas le rejouer ici ;
- tâches, worklog et réunions sont triés par récence décroissante avant plafonnement.

**Quelle date porte chaque résultat** — `SearchHit::occurred_on` n'a pas la même source selon
l'entité, et c'est ce qui donne son sens au tri par récence :

| Entité | Champ | Pourquoi |
|---|---|---|
| tâche | `updated_at.date_naive()` | la dernière fois qu'elle a bougé, pas sa création |
| entrée de worklog | `logged_at.date_naive()` | l'heure murale de l'entrée |
| réunion | date de début | — |
| mémoire | `occurred_at.date_naive()` | même champ que le brief |

```rust
use chrono::{Datelike, Duration, Utc};
use domain::rules::search::{group_from, matches, parse_terms, SearchGroup, SearchHit};

/// How far back meetings are searched. `MeetingRepository` has no unbounded
/// `list`, only a range query — and 24 months covers every meeting the store
/// holds several times over.
const MEETING_SEARCH_MONTHS: i64 = 24;

pub struct SearchRequest {
    pub query: String,
    pub limit: usize,
}

pub struct SearchOutcome {
    pub tasks: SearchGroup,
    pub worklog: SearchGroup,
    pub meetings: SearchGroup,
    pub memories: SearchGroup,
}

fn empty_outcome() -> SearchOutcome {
    SearchOutcome {
        tasks: group_from(Vec::new(), 0),
        worklog: group_from(Vec::new(), 0),
        meetings: group_from(Vec::new(), 0),
        memories: group_from(Vec::new(), 0),
    }
}

pub async fn search(
    task_repo: &dyn TaskRepository,
    worklog_repo: &dyn WorklogRepository,
    meeting_repo: &dyn MeetingRepository,
    memory_repo: &dyn MemoryRepository,
    user_id: UserId,
    request: SearchRequest,
) -> Result<SearchOutcome, RepositoryError> {
    let terms = parse_terms(&request.query);
    if terms.is_empty() {
        // A blank query must never become a full dump: 642 tasks would drown the
        // caller and teach it never to search again.
        return Ok(empty_outcome());
    }

    // Tasks: the repository, never the `tasks` GraphQL query and its first: 50.
    let mut tasks: Vec<SearchHit> = task_repo
        .list(user_id, &TaskFilter::empty())
        .await?
        .into_iter()
        .filter(|t| {
            matches(&t.title, &terms)
                || t.description.as_deref().is_some_and(|d| matches(d, &terms))
        })
        .map(|t| SearchHit {
            id: t.id.to_string(),
            title: t.title,
            occurred_on: t.updated_at.date_naive(),
        })
        .collect();
    tasks.sort_by(|a, b| b.occurred_on.cmp(&a.occurred_on));

    // Worklog: paged, on the precedent of `aplan show --worklog all`. Asking for
    // i64::MAX in one call would silently return the first 1000 rows and the
    // result would *look* complete.
    let mut worklog: Vec<SearchHit> = collect_worklog_pages(worklog_repo, user_id)
        .await?
        .into_iter()
        .filter(|e| matches(&e.body, &terms))
        .map(|e| SearchHit {
            id: e.id.to_string(),
            title: e.body.clone(),
            occurred_on: e.logged_at.date_naive(),
        })
        .collect();
    worklog.sort_by(|a, b| b.occurred_on.cmp(&a.occurred_on));

    let today = Utc::now().date_naive();
    let from = today - Duration::days(MEETING_SEARCH_MONTHS * 30);
    let mut meetings: Vec<SearchHit> = meeting_repo
        .find_by_user_and_range(user_id, from, today)
        .await?
        .into_iter()
        .filter(|m| matches(&m.title, &terms))
        .map(|m| SearchHit {
            id: m.id.to_string(),
            title: m.title.clone(),
            occurred_on: m.start_time.date_naive(),
        })
        .collect();
    meetings.sort_by(|a, b| b.occurred_on.cmp(&a.occurred_on));

    // Memories keep the recall ordering: relevance, not recency. Do not re-sort.
    let memories = recall_hits(memory_repo, user_id, &request.query, request.limit).await?;

    Ok(SearchOutcome {
        tasks: group_from(tasks, request.limit),
        worklog: group_from(worklog, request.limit),
        meetings: group_from(meetings, request.limit),
        memories: group_from(memories, request.limit),
    })
}
```

Deux aides privées restent à écrire dans le même fichier :

- `collect_worklog_pages(worklog_repo, user_id)` — boucle sur `WorklogRepository::list` avec un
  `WorklogFilter` dont l'`offset` avance de 1000 à chaque tour, jusqu'à une page incomplète, avec le
  même garde-fou à 50 pages que `aplan show --worklog all` ;
- `recall_hits(memory_repo, user_id, query, limit)` — appelle le chemin `recall` déjà en place
  (`services::memory_retriever`) et transforme chaque `ScoredMemory` en `SearchHit` **sans toucher
  à l'ordre**, `occurred_on` valant `occurred_at.date_naive()`.

Noms de champs **vérifiés** contre les types du domaine avant écriture : `WorklogEntry::body` (et
non `content` — le plan avait d'abord deviné faux), `WorklogEntry::logged_at`, `Meeting::title`,
`Meeting::start_time`, `Task::updated_at`. `TaskFilter::empty()` existe. `WorklogFilter` porte
`limit: u32` et `offset: u32` — pas des `i64`.

- [ ] **Step 4 : lancer les tests de l'application**

```bash
cd backend && cargo test -p application
```

Attendu : PASSENT.

- [ ] **Step 5 : commit**

```bash
git add backend/crates/application/src/use_cases/search.rs backend/crates/application/src/use_cases/mod.rs
git commit -m "Compose a cross-entity search over the existing repositories"
```

---

### Task 4 : exposer `search` en GraphQL

**Files:**
- Create: `backend/crates/api/src/graphql/types/search.rs`
- Modify: `backend/crates/api/src/graphql/types/mod.rs`
- Modify: `backend/crates/api/src/graphql/query.rs` (à la suite du résolveur `brief`, ligne ~695)
- Test: `backend/crates/api/src/graphql/tests.rs`

**Interfaces:**
- Consumes: `SearchOutcome`, `SearchRequest`, `search` (Task 3).
- Produces: la requête GraphQL `search(q: String!, limit: Int): SearchGql!`, avec `SearchGql { tasks, taskTotal, worklog, worklogTotal, meetings, meetingTotal, memories, memoryTotal }` et `SearchHitGql { id, title, occurredOn }`. Consommés par la Task 5.

- [ ] **Step 1 : écrire le test qui échoue**

Dans `backend/crates/api/src/graphql/tests.rs`, sur le modèle des tests de `brief` :

Le helper de montage de ce fichier est `schema_with_one_task() -> (TestSchema, Uuid)` (ligne ~1454) —
il n'existe pas de `test_schema()`.

```rust
#[tokio::test]
async fn search_returns_grouped_hits() {
    let (schema, _task_id) = schema_with_one_task().await;
    let response = schema
        .execute(r#"{ search(q: "waf") { tasks { id title occurredOn } taskTotal memoryTotal } }"#)
        .await;

    assert!(response.errors.is_empty(), "{:?}", response.errors);
}

#[tokio::test]
async fn search_with_an_empty_query_returns_nothing_rather_than_everything() {
    let (schema, _task_id) = schema_with_one_task().await;
    let response = schema.execute(r#"{ search(q: "   ") { taskTotal } }"#).await;

    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().expect("json");
    assert_eq!(data["search"]["taskTotal"], 0);
}
```

- [ ] **Step 2 : lancer le test pour le voir échouer**

```bash
cd backend && cargo test -p api search_returns_grouped_hits
```

Attendu : ÉCHEC — `Unknown field "search" on type "Query"`.

- [ ] **Step 3 : écrire l'implémentation minimale**

Créer `types/search.rs` avec `SearchHitGql` et `SearchGql`, plus un `impl From<SearchOutcome> for SearchGql` qui aplatit chaque `SearchGroup` en `(Vec<SearchHitGql>, i32)` — même forme que `memory_entries` dans `types/brief.rs`. Déclarer le module dans `types/mod.rs`. Ajouter le résolveur dans `query.rs`, à la suite de `brief`, en reprenant sa façon de résoudre l'utilisateur courant et les dépôts depuis le contexte.

- [ ] **Step 4 : lancer les tests de l'API**

```bash
cd backend && cargo test -p api
```

Attendu : PASSENT.

- [ ] **Step 5 : régénérer le schéma et commiter**

Rappel : `export-schema` construit le pool d'abord, donc il applique les migrations en attente à la vraie base `backend/aggregated_plan.db`. Ce n'est pas une opération en lecture seule.

`export-schema` **imprime le SDL sur stdout** — il n'écrit aucun fichier, et `backend/schema.graphql`
n'existe pas. Le schéma que lit la codegen de la CLI est `backend/crates/cli/graphql/schema.graphql`
(`schema_path` dans `crates/cli/src/queries.rs`).

```bash
cd backend && cargo run -p api -- export-schema > crates/cli/graphql/schema.graphql
cd .. && git add backend/crates/api/src/graphql/ backend/crates/cli/graphql/schema.graphql
git commit -m "Expose the cross-entity search over GraphQL"
```

---

### Task 5 : la commande `aplan search`

**Files:**
- Create: `backend/crates/cli/graphql/search.graphql`
- Create: `backend/crates/cli/src/search_cmd.rs`
- Modify: `backend/crates/cli/src/cli.rs` (enum `Commands`, à la suite de `Brief` ligne ~346)
- Modify: `backend/crates/cli/src/main.rs` (dispatch, à la suite de `Commands::Brief` ligne ~225)
- Modify: `backend/crates/cli/src/queries.rs` (déclaration du document, à la suite de `brief.graphql` ligne ~397)
- Test: `backend/crates/cli/src/cli.rs`, `mod tests`

**Interfaces:**
- Consumes: la requête GraphQL `search` (Task 4).
- Produces: la commande `aplan search --q <TERMES> [--limit N] [--json]`, consommée par la skill (Task 8).

- [ ] **Step 1 : écrire le test d'analyse d'arguments qui échoue**

Dans `backend/crates/cli/src/cli.rs`, `mod tests`, sur le modèle des tests `Commands::Brief` existants :

```rust
#[test]
fn search_parses_its_query_and_limit() {
    let args = Args::parse_from(["aplan", "search", "--q", "waf eactions", "--limit", "10"]);
    match args.command {
        Commands::Search { q, limit } => {
            assert_eq!(q, "waf eactions");
            assert_eq!(limit, 10);
        }
        other => panic!("expected Search, got {other:?}"),
    }
}

#[test]
fn search_defaults_to_the_group_cap() {
    let args = Args::parse_from(["aplan", "search", "--q", "waf"]);
    match args.command {
        Commands::Search { limit, .. } => assert_eq!(limit, 5),
        other => panic!("expected Search, got {other:?}"),
    }
}
```

- [ ] **Step 2 : lancer le test pour le voir échouer**

```bash
cd backend && cargo test -p cli search_parses_its_query
```

Attendu : ÉCHEC de compilation, `no variant named 'Search' found for enum 'Commands'`.

- [ ] **Step 3 : écrire l'implémentation minimale**

Dans `cli.rs`, ajouter la variante à la suite de `Brief` :

```rust
    /// Search across everything aplan holds: tasks, worklog entries, meetings and
    /// memories. Results are grouped per entity — memories by relevance, the rest
    /// by recency — and capped, because the caller is usually an agent.
    Search {
        /// What to look for. Whitespace-separated terms, all of which must appear.
        /// Accents are folded, so `fenetre` finds `fenêtre`.
        #[arg(long)]
        q: String,
        /// How many results per group.
        #[arg(long, default_value_t = 5)]
        limit: i64,
    },
```

Créer `search.graphql` demandant les huit champs de `SearchGql`, le déclarer dans `queries.rs` sur le modèle de `brief.graphql`, écrire `search_cmd::search(api_url, json, &q, limit)` sur le modèle de `memory_cmd::recall_search`, et brancher le dispatch dans `main.rs`.

Le rendu humain : un en-tête par groupe non vide, annonçant la troncature quand il y en a (`Tâches (12, 5 affichées)`), une ligne par résultat avec sa date, et un groupe vide simplement omis. Aucun résultat du tout imprime `no match for "<termes>"` et sort en 0 — ce n'est pas une erreur.

- [ ] **Step 4 : lancer les tests de la CLI**

```bash
cd backend && cargo test -p cli
```

Attendu : PASSENT.

- [ ] **Step 5 : vérifier à la main sur les vraies données**

Le backend doit tourner.

```bash
aplan search --q "waf eactions"
aplan search --q "fenetre maintenance"
aplan search --q "   "
```

Attendu : le premier ramène la mémoire du WAF `azrpwafeact01` et les tâches eActions ; le deuxième trouve « fenêtre de maintenance » malgré l'accent manquant ; le troisième ne ramène rien et sort en 0.

- [ ] **Step 6 : commit**

```bash
git add backend/crates/cli/
git commit -m "Add aplan search across tasks, worklog, meetings and memories"
```

---

### Task 6 : documenter la commande

**Files:**
- Modify: `SPEC_TECHNIQUE.md` (section des verbes de mémoire, autour de la ligne 178)
- Modify: `SPEC_FONCTIONNELLE.md` (une nouvelle règle à la suite de R57)

**Interfaces:**
- Consumes: le comportement livré par les Tasks 1 à 5.
- Produces: la référence que la skill (Task 8) cite.

- [ ] **Step 1 : ajouter la règle fonctionnelle**

Dans `SPEC_FONCTIONNELLE.md`, à la suite de R57, ajouter une règle sur ce modèle de rédaction :

```
| **R58** | **Recherche transverse** : `aplan search --q` cherche dans les tâches (titre et
description), les entrées de worklog, les réunions et les mémoires. Les résultats sont **groupés par
entité**, jamais fusionnés en un classement unique : mélanger un score BM25 de mémoire à une
correspondance de titre de tâche produit un ordre qui ne veut rien dire. Les mémoires gardent l'ordre
du recall, les autres entités sont triées par récence. Plafond de **5 résultats par groupe**,
relevable par `--limit`, toute troncature annoncée. Les accents sont pliés comme le fait
`memories_fts` (`unicode61 remove_diacritics 2`), pour que la même requête se comporte pareil sur les
quatre entités. Une requête vide ne ramène **rien** — jamais tout.
```

Renuméroter si R58 est déjà pris.

- [ ] **Step 2 : ajouter la commande à la spec technique**

Dans `SPEC_TECHNIQUE.md`, à la suite de la description de `aplan brief`, décrire `aplan search --q <TERMES> [--limit N] [--json]` : les quatre entités couvertes, le groupement, le plafond, le pliage des accents, et le fait qu'elle n'emprunte **pas** la requête `tasks` et son `first: 50`.

- [ ] **Step 3 : relire**

```bash
grep -n "aplan search" SPEC_TECHNIQUE.md SPEC_FONCTIONNELLE.md
```

- [ ] **Step 4 : commit**

```bash
git add SPEC_TECHNIQUE.md SPEC_FONCTIONNELLE.md
git commit -m "Document the cross-entity search verb"
```

---

### Task 7 : les trois frictions du recall

La § 10 de la spec les relève comme « corrigibles au passage ». La troisième compte vraiment pour ce
plan : une session qui lit `recall --q` voit `invalidatedAt` sans savoir par quoi la mémoire a été
remplacée — c'est exactement le cas où elle devrait proposer un remplacement plutôt que d'ignorer.

**Files:**
- Modify: `backend/crates/cli/src/memory_cmd.rs` (ligne ~58 pour le message d'erreur)
- Modify: `backend/crates/cli/graphql/` (le document de `recall --q`)
- Modify: `backend/crates/cli/src/cli.rs` (aide de `--project` sur la commande `Recall`)

**Interfaces:**
- Consumes: la charge GraphQL de `recall <id>`, qui expose déjà `supersededBy` parmi ses 16 champs.
- Produces: rien de nouveau ; trois corrections d'ergonomie.

- [ ] **Step 1 : exposer `supersededBy` dans la charge de `recall --q`**

Ajouter le champ au document GraphQL de la recherche de mémoires, à côté de `invalidatedAt`. La
charge passe de 9 à 10 champs ; le rendu humain n'a pas à changer, c'est `--json` qui en profite.

```bash
grep -rn "invalidatedAt" backend/crates/cli/graphql/
```

- [ ] **Step 2 : corriger le mot « task » dans l'erreur de `--project`**

`crates/cli/src/memory_cmd.rs:58` renvoie `error: no task matches <token>` alors que la résolution
porte sur les **projets**. Remplacer par `error: no project matches <token>`.

- [ ] **Step 3 : dire ce que `--project` fait vraiment**

L'aide dit « Restrict the search context to a project », ce qui se lit comme un filtre. Ce n'en est
pas un : `RecallQuery` ne porte aucun champ projet, seul `RecallContext` — le projet est un **bonus
d'entité dans le score** (1.309 → 1.609 sur la mémoire rattachée). Récrire l'aide :

```rust
        /// Favour memories attached to this project. A bonus in the ranking, not
        /// a filter: memories from other projects still come back, lower down.
```

- [ ] **Step 4 : vérifier**

```bash
cd backend && cargo test -p cli && cargo check -p cli
aplan recall --project inexistant --q "waf" 2>&1 | head -2
```

Attendu : le message parle de `project`, plus de `task`.

- [ ] **Step 5 : commit**

```bash
git add backend/crates/cli/
git commit -m "Say what recall --project does, and name projects as projects"
```

---

### Task 8 : la skill

**Files:**
- Create: `~/.claude/skills/aplan-memoire/SKILL.md`

Le fichier vit hors du dépôt, rien à commiter.

**Interfaces:**
- Consumes: `aplan search --q` (Task 5), `aplan recall --q` / `aplan recall <id>`, `aplan show`.
- Produces: le déclenchement du processus 2 — ce que la Task 9 mesure.

- [ ] **Step 1 : écrire la skill**

Créer `~/.claude/skills/aplan-memoire/SKILL.md`. La description est la pièce décisive : c'est elle, et elle seule, qui décide si la skill se charge.

```markdown
---
name: aplan-memoire
description: Use when the conversation touches a client, system or subject the user has worked on before — Pernod Ricard, TotalEnergies, SAFT, Cartier, eActions, SharePoint, Gryzzly, Snowflake, ADF, WAF/Front Door — or when the user asks what is already known ("est-ce qu'on avait déjà…", "comment on avait fait pour…", "qu'est-ce que je sais de…", "on en était où sur…"). Searches the aplan store — memories, tasks, worklog, meetings — before answering.
---

# Chercher dans aplan avant de répondre

Le magasin aplan contient la connaissance métier accumulée par l'utilisateur : des faits
vérifiés sur des clients et des systèmes, des décisions prises et leur pourquoi, des règles de
méthode, et l'historique de ce qui a été fait. Il est écrit tous les jours et n'est presque jamais
lu — c'est cette skill qui corrige ça.

## Quand chercher

Chercher **avant** de répondre, pas après :

- le sujet nomme un client ou un système déjà rencontré ;
- l'utilisateur demande ce qui est déjà su, ou ce qui avait été décidé ;
- tu t'apprêtes à affirmer quelque chose sur un environnement client — c'est exactement là qu'une
  mémoire peut te contredire.

Ne pas chercher quand le sujet est purement local au dépôt courant et n'a pas d'histoire.

## Comment chercher

```bash
aplan search --q "<termes>"      # tâches, worklog, réunions, mémoires — groupés
aplan recall --q "<termes>"      # les mémoires seules, classées par pertinence
aplan recall <id|m:xxx>          # le détail d'une mémoire : son corps, son pourquoi
aplan show <tâche>               # une tâche et la fin de son worklog
```

Les termes sont libres, les accents sont pliés. Une requête trop précise ne ramène rien : le
moteur exige **tous** les termes. Élargir plutôt que d'abandonner.

## Comment lire un résultat

Chaque mémoire porte une date. C'est un **indice daté**, pas une vérité : un fait sur un
environnement client peut avoir été vrai en mai et faux en août.

## Quand une mémoire contredit ce que tu observes

Ne jamais invalider, superséder ou réécrire de ta propre initiative. Signaler, et proposer :

> La mémoire `m:26a` (06/08) dit que les 403 viennent du WAF `azrpwafeact01`, or je vois ici une
> 403 renvoyée par l'application elle-même. Deux options : on ignore la mémoire pour cette fois, ou
> on la remplace. Tu veux laquelle ?

Attendre la réponse. Si l'utilisateur choisit le remplacement, écrire la nouvelle mémoire avec
`aplan remember` et lui signaler l'ancienne à invalider depuis l'onglet Memory. L'écriture reste un
acte qu'il valide.
```

- [ ] **Step 2 : vérifier que le harnais voit la skill**

Ouvrir une nouvelle session Claude Code et vérifier que `aplan-memoire` apparaît dans la liste des skills disponibles.

- [ ] **Step 3 : vérifier le déclenchement**

Dans une session ouverte sur un dépôt **autre** que `aggregated_plan`, poser une question qui porte un signal (« qu'est-ce qu'on sait du WAF chez eActions ? ») et vérifier que la skill se charge et qu'un `aplan search` ou `aplan recall` part.

Si elle ne se déclenche pas, la description est le seul levier : l'élargir, en gardant les noms propres — ce sont les signaux les plus discriminants.

---

### Task 9 : la mesure, à J+15

C'est le critère de succès du dispositif entier, pas une formalité.

- [ ] **Step 1 : noter la date de mise en service**

```bash
aplan log "Skill aplan-memoire et aplan search mis en service. Mesure de contrôle à J+15."
```

- [ ] **Step 2 : quinze jours plus tard, relancer l'instrument à l'identique**

Le script vit dans `docs/prompts/reprise-lecture-memoire.md`, section « Instruments de mesure ». Le lancer sans le modifier — un instrument modifié ne compare plus rien.

```bash
cd /home/mbt/.claude/projects && python3 - <<'PY'
import json, os, glob, collections, re
pat = re.compile(r'^\s*aplan\s+(recall|brief|remember|inbox|memory|consolidate|search)\b')
byproj = collections.defaultdict(collections.Counter)
sessions = collections.defaultdict(set)
for f in glob.glob("**/*.jsonl", recursive=True):
    proj, sid = f.split(os.sep)[0], os.path.basename(f)[:-6]
    for line in open(f, encoding="utf-8", errors="replace"):
        if "aplan" not in line: continue
        try: rec = json.loads(line)
        except Exception: continue
        c = (rec.get("message") or {}).get("content")
        if not isinstance(c, list): continue
        for b in c:
            if isinstance(b, dict) and b.get("type") == "tool_use":
                cmd = (b.get("input") or {}).get("command")
                if isinstance(cmd, str):
                    for part in re.split(r'[;&|\n]+', cmd):
                        m = pat.match(part)
                        if m: byproj[proj][m.group(1)] += 1; sessions[proj].add(sid)
for p in sorted(byproj, key=lambda k: -sum(byproj[k].values())):
    print(f"{sum(byproj[p].values()):>4}  {len(sessions[p])} sess.  {p[:55]:<55} {dict(byproj[p])}")
PY
```

Le verbe `search` a été ajouté au motif — c'est le seul écart admis, sans quoi la nouvelle commande serait invisible à sa propre mesure.

- [ ] **Step 3 : trancher**

Ligne de base : 148 invocations, 138 dans le dépôt `aggregated_plan`, **zéro** lecture métier hors dépôt.

- Le compteur hors dépôt aplan décolle → le dispositif marche, on s'arrête là.
- Il reste à zéro → la skill ne se déclenche pas. Deux recours dans l'ordre : élargir sa description (gratuit), puis reconsidérer l'outil MCP (§ 5.1 de la spec) — c'est-à-dire réparer une crate qui n'a jamais compilé, ce que la mesure aura alors justifié.

---

## Vérification de fin de plan

- [ ] **La suite complète passe**, crate `mcp` exclue :

```bash
cd backend && cargo test -p domain -p application -p infrastructure -p api -p cli
```

- [ ] **Le lint est propre** :

```bash
cd backend && cargo clippy -p domain -p application -p api -p cli
```

- [ ] **Le pliage des accents est cohérent sur les quatre entités.** La preuve tient en deux commandes : le même terme, avec et sans accent, doit ramener le même nombre de résultats dans chaque groupe.

```bash
aplan search --q "fenetre" --json | jq '{t:.search.taskTotal, w:.search.worklogTotal, m:.search.memoryTotal}'
aplan search --q "fenêtre" --json | jq '{t:.search.taskTotal, w:.search.worklogTotal, m:.search.memoryTotal}'
```
