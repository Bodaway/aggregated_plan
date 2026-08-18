# Lecture de la mémoire — plan 1 : la poussée

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Une session Claude Code voit enfin la mémoire au démarrage : le brief lui parvient, et il porte désormais les règles de méthode (`preference`) en plus des engagements et des décisions.

**Architecture:** Le brief existe déjà, il est déjà budgété (R55 : 40 lignes × 140 caractères) et sa variante `Session` est déjà rendue par `aplan brief`. Ce plan fait deux choses : ajouter une section `preferences` dans `domain/rules/brief.rs` — donc dans le domaine, seul endroit où le plafond est appliqué — puis brancher la sortie de `aplan brief` sur le contexte que le hook `SessionStart` émet déjà. Il ajuste enfin le prompt de consolidation pour que les `fact` du jour soient lisibles le lendemain.

**Tech Stack:** Rust (workspace Cargo, crates `domain` / `application` / `infrastructure` / `api` / `cli`), async-graphql 7, SQLite via sqlx 0.8, hook shell + `jq`.

**Spec:** `docs/superpowers/specs/2026-08-18-lecture-memoire-design.md`

## Global Constraints

- **Plafonds du brief (R55)** : 40 lignes (`BRIEF_MAX_LINES`), 140 caractères par ligne (`BRIEF_MAX_LINE_CHARS`). Appliqués **dans le domaine** et vérifiés par un test sur une entrée pathologique. Aucun rendu concurrent hors du domaine.
- **Ordre de sacrifice de la troncature (R55, amendé)** : les décisions cèdent avant les engagements, qui cèdent avant les échéances, qui cèdent avant les **préférences**. Les préférences sont les dernières coupées.
- **Toute troncature est annoncée** — jamais silencieuse (`(8, 6 affichés)`).
- **Séparation DDD stricte** : `domain` sans I/O ni dépendance hors chrono/serde/uuid/thiserror ; `application` ne dépend que de `domain` ; `infrastructure` implémente les traits ; `api` dépend de tout.
- **TDD** : test d'abord, exécution pour le voir échouer, implémentation minimale, exécution pour le voir passer, commit.
- **Tests backend inline** dans `#[cfg(test)] mod tests`.
- **La crate `mcp` ne compile pas** et est exclue du workspace. Ne jamais lancer de commande cargo qui l'inclut.
- **Spécifications en français**, code et commentaires en anglais. Toute modification du comportement documenté met à jour `SPEC_FONCTIONNELLE.md` et/ou `SPEC_TECHNIQUE.md` **dans le même commit**.
- **Message de commit** : `<sujet impératif>` sans préfixe Jira (aucun ticket ici), sans `Co-Authored-By`, sans `Signed-off-by`.
- **Ne jamais toucher au pointeur `aplan.active_task_id`.** Ne jamais lancer `aplan consolidate mark` ni `record-run`.

---

### Task 1 : la sélection des préférences dans le domaine

**Files:**
- Modify: `backend/crates/domain/src/rules/brief.rs` (constantes vers la ligne 32-34, section « Selection rules » vers la ligne 285)
- Test: même fichier, `#[cfg(test)] mod tests` (à partir de la ligne 685)

**Interfaces:**
- Consumes: `memories_of_kind(&[Memory], MemoryKind) -> Vec<&Memory>` et `section_from(Vec<&Memory>, usize) -> BriefSection<MemoryEntry>`, tous deux privés et déjà présents dans le fichier ; le helper de test `memory(kind, title, days_ago) -> Memory`.
- Produces: `pub const MAX_PREFERENCE_ENTRIES: usize` et `pub fn select_preferences(memories: &[Memory], cap: usize) -> BriefSection<MemoryEntry>`, consommés par la Task 2.

- [ ] **Step 1 : écrire le test qui échoue**

À ajouter dans `mod tests` de `backend/crates/domain/src/rules/brief.rs` :

```rust
#[test]
fn preferences_are_selected_newest_first() {
    let memories = vec![
        memory(MemoryKind::Preference, "ancienne règle", 90),
        memory(MemoryKind::Preference, "règle du jour", 1),
        memory(MemoryKind::Fact, "un fait, pas une règle", 2),
    ];

    let section = select_preferences(&memories, 10);

    assert_eq!(section.total, 2, "seules les préférences comptent");
    assert_eq!(section.entries[0].title, "règle du jour");
    assert_eq!(section.entries[1].title, "ancienne règle");
}

#[test]
fn preferences_report_what_the_cap_hid() {
    let memories = vec![
        memory(MemoryKind::Preference, "une", 1),
        memory(MemoryKind::Preference, "deux", 2),
        memory(MemoryKind::Preference, "trois", 3),
    ];

    let section = select_preferences(&memories, 2);

    assert_eq!(section.entries.len(), 2);
    assert_eq!(section.total, 3);
    assert_eq!(section.hidden(), 1, "la troncature n'est jamais silencieuse");
}
```

- [ ] **Step 2 : lancer le test pour le voir échouer**

```bash
cd backend && cargo test -p domain preferences_are_selected -- --nocapture
```

Attendu : ÉCHEC de compilation, `cannot find function 'select_preferences' in this scope`.

- [ ] **Step 3 : écrire l'implémentation minimale**

Ajouter la constante à côté de `MAX_DECISION_ENTRIES` (vers la ligne 34) :

```rust
/// Les règles de méthode sont peu nombreuses et très stables : un plafond bas
/// suffit, et il garde la section sous les ~50 tokens qui justifient qu'elle
/// soit la dernière coupée.
pub const MAX_PREFERENCE_ENTRIES: usize = 4;
```

Ajouter la fonction dans la section « Selection rules », juste avant `select_commitments` :

```rust
/// Working rules, **newest first**: a preference restated recently is the one
/// that currently holds. Rendered before everything else and cut last — the
/// section is both the most useful and the cheapest (three short lines).
pub fn select_preferences(memories: &[Memory], cap: usize) -> BriefSection<MemoryEntry> {
    let mut rows = memories_of_kind(memories, MemoryKind::Preference);
    rows.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at).then_with(|| a.id.cmp(&b.id)));
    section_from(rows, cap)
}
```

- [ ] **Step 4 : lancer les tests pour les voir passer**

```bash
cd backend && cargo test -p domain preferences
```

Attendu : les deux tests PASSENT.

- [ ] **Step 5 : commit**

```bash
git add backend/crates/domain/src/rules/brief.rs
git commit -m "Select the preference memories the brief will carry"
```

---

### Task 2 : la section `preferences` dans la composition du brief

**Files:**
- Modify: `backend/crates/domain/src/rules/brief.rs` (`struct Brief` ligne ~152, `Brief::is_silent` ligne ~164, `compose_brief` ligne ~417)
- Test: même fichier, `mod tests`

**Interfaces:**
- Consumes: `select_preferences` et `MAX_PREFERENCE_ENTRIES` (Task 1).
- Produces: le champ `Brief::preferences: BriefSection<MemoryEntry>`, consommé par les Tasks 3 et 4.

- [ ] **Step 1 : écrire le test qui échoue**

```rust
#[test]
fn composed_brief_carries_preferences_with_references() {
    let memories = vec![memory(MemoryKind::Preference, "une idée par slide", 3)];
    let input = BriefInput {
        variant: BriefVariant::Session,
        today: today(),
        now: now(),
        tasks: &[],
        memories: &memories,
        current_project: None,
        pending_count: 0,
        last_consolidation: Some(now()),
    };

    let brief = compose_brief(&input);

    assert_eq!(brief.preferences.entries.len(), 1);
    assert!(
        brief.preferences.entries[0].reference.starts_with("m:"),
        "sans référence courte la ligne est un cul-de-sac (R56)"
    );
}

#[test]
fn a_brief_holding_only_a_preference_is_not_silent() {
    let memories = vec![memory(MemoryKind::Preference, "une idée par slide", 3)];
    let input = BriefInput {
        variant: BriefVariant::Session,
        today: today(),
        now: now(),
        tasks: &[],
        memories: &memories,
        current_project: None,
        pending_count: 0,
        last_consolidation: Some(now()),
    };

    assert!(!compose_brief(&input).is_silent());
}
```

- [ ] **Step 2 : lancer le test pour le voir échouer**

```bash
cd backend && cargo test -p domain composed_brief_carries_preferences
```

Attendu : ÉCHEC de compilation, `struct 'Brief' has no field named 'preferences'`.

- [ ] **Step 3 : écrire l'implémentation minimale**

Dans `struct Brief`, ajouter le champ **en premier parmi les sections** :

```rust
pub struct Brief {
    pub variant: BriefVariant,
    pub date: NaiveDate,
    /// Working rules. First rendered, last cut.
    pub preferences: BriefSection<MemoryEntry>,
    pub deadlines: BriefSection<DeadlineEntry>,
    pub commitments: BriefSection<MemoryEntry>,
    pub decisions: BriefSection<MemoryEntry>,
    pub decisions_scoped_to_project: bool,
    pub pending_count: usize,
    pub consolidation: ConsolidationAge,
}
```

Dans `Brief::is_silent`, ajouter la section :

```rust
    pub fn is_silent(&self) -> bool {
        self.preferences.is_empty()
            && self.deadlines.is_empty()
            && self.commitments.is_empty()
            && self.decisions.is_empty()
            && self.pending_count == 0
            && !self.consolidation.is_stale()
    }
```

Dans `compose_brief`, réserver le budget des préférences **avant celui des échéances** — c'est ce qui les rend les dernières coupées. Insérer juste après le calcul de `let morning = ...` et avant `let deadlines = ...` :

```rust
    // Reserved before the deadlines, so a pathological deadline list can never
    // squeeze the working rules out: R55's sacrifice order ends with them.
    let preferences = select_preferences(input.memories, MAX_PREFERENCE_ENTRIES);
```

Puis, dans le calcul du budget, retrancher les préférences **avant** les échéances :

```rust
    let mut budget = BRIEF_MAX_LINES.saturating_sub(1);
    if !preferences.is_empty() {
        budget = budget.saturating_sub(1 + preferences.entries.len());
    }
    if !deadlines.is_empty() {
        budget = budget.saturating_sub(1 + deadlines.entries.len());
    }
```

Ajouter le champ à la construction de `Brief` :

```rust
    let mut brief = Brief {
        variant: input.variant,
        date: input.today,
        preferences,
        deadlines,
        commitments,
        decisions,
        decisions_scoped_to_project: !morning && input.current_project.is_some(),
        pending_count: input.pending_count,
        consolidation,
    };
```

Enfin, faire entrer les préférences dans le calcul de la largeur des références. Remplacer les deux chaînages existants :

```rust
    let ids: Vec<MemoryId> = brief
        .preferences
        .entries
        .iter()
        .chain(brief.commitments.entries.iter())
        .chain(brief.decisions.entries.iter())
        .map(|e| e.id)
        .collect();
    let width = memory_reference_width(&ids);
    for entry in brief
        .preferences
        .entries
        .iter_mut()
        .chain(brief.commitments.entries.iter_mut())
        .chain(brief.decisions.entries.iter_mut())
    {
        entry.reference = memory_reference(entry.id, width);
    }
```

- [ ] **Step 4 : lancer toute la suite du domaine pour les voir passer**

```bash
cd backend && cargo test -p domain
```

Attendu : PASSENT, y compris les tests de brief préexistants — leurs littéraux `Brief { .. }` éventuels doivent être complétés du nouveau champ si le compilateur le réclame.

- [ ] **Step 5 : commit**

```bash
git add backend/crates/domain/src/rules/brief.rs
git commit -m "Give the composed brief a preferences section"
```

---

### Task 3 : le rendu de la section, et le plafond de R55

**Files:**
- Modify: `backend/crates/domain/src/rules/brief.rs` (`render_brief` ligne ~589)
- Test: même fichier, `mod tests`

**Interfaces:**
- Consumes: `Brief::preferences` (Task 2), `render_memory_section(&mut Vec<String>, &str, &BriefSection<MemoryEntry>)`, privée et déjà présente.
- Produces: rien de nouveau ; `render_brief(&Brief) -> Vec<String>` garde sa signature.

- [ ] **Step 1 : écrire le test qui échoue**

```rust
#[test]
fn preferences_are_rendered_before_the_deadlines() {
    let memories = vec![memory(MemoryKind::Preference, "une idée par slide", 3)];
    let tasks = vec![task("rendre le dossier", in_days(1))];
    let input = BriefInput {
        variant: BriefVariant::Session,
        today: today(),
        now: now(),
        tasks: &tasks,
        memories: &memories,
        current_project: None,
        pending_count: 0,
        last_consolidation: Some(now()),
    };

    let lines = render_brief(&compose_brief(&input));
    let preferences = lines.iter().position(|l| l.starts_with("Préférences"));
    let deadlines = lines.iter().position(|l| l.starts_with("Échéances"));

    assert!(preferences.is_some(), "la section doit être rendue");
    assert!(
        preferences < deadlines,
        "une règle de méthode se lit avant une échéance, got {lines:?}"
    );
}

#[test]
fn preferences_survive_a_pathological_brief() {
    // 40 deadlines and 40 decisions: everything else is cut, the working rules
    // are not. This is R55's sacrifice order, end to end.
    let mut memories: Vec<Memory> = (0..40)
        .map(|i| memory(MemoryKind::Decision, &format!("décision {i}"), i))
        .collect();
    memories.push(memory(MemoryKind::Preference, "une idée par slide", 1));
    let tasks: Vec<Task> = (0..40)
        .map(|i| task(&format!("tâche {i}"), in_days(i)))
        .collect();
    let input = BriefInput {
        variant: BriefVariant::Session,
        today: today(),
        now: now(),
        tasks: &tasks,
        memories: &memories,
        current_project: None,
        pending_count: 0,
        last_consolidation: Some(now()),
    };

    let lines = render_brief(&compose_brief(&input));

    assert!(lines.len() <= BRIEF_MAX_LINES, "plafond de R55 : {}", lines.len());
    assert!(lines.iter().all(|l| l.chars().count() <= BRIEF_MAX_LINE_CHARS));
    assert!(
        lines.iter().any(|l| l.contains("une idée par slide")),
        "les préférences sont les dernières coupées, got {lines:?}"
    );
}
```

- [ ] **Step 2 : lancer le test pour le voir échouer**

```bash
cd backend && cargo test -p domain preferences_are_rendered preferences_survive
```

Attendu : ÉCHEC — `preferences.is_some()` est faux, aucune ligne « Préférences » n'est produite.

- [ ] **Step 3 : écrire l'implémentation minimale**

Dans `render_brief`, insérer l'appel **avant** le bloc des échéances, juste après le `if brief.is_silent() { … }` :

```rust
    render_memory_section(&mut lines, "Préférences", &brief.preferences);
```

- [ ] **Step 4 : lancer toute la suite du domaine**

```bash
cd backend && cargo test -p domain
```

Attendu : PASSENT.

- [ ] **Step 5 : commit**

```bash
git add backend/crates/domain/src/rules/brief.rs
git commit -m "Render the preferences at the head of the brief"
```

---

### Task 4 : exposer les préférences en GraphQL

**Files:**
- Modify: `backend/crates/api/src/graphql/types/brief.rs` (`struct BriefGql` ligne ~68, `impl From<Brief> for BriefGql` ligne ~112)
- Test: `backend/crates/api/src/graphql/tests.rs`

**Interfaces:**
- Consumes: `Brief::preferences` (Task 2), le helper privé `memory_entries(&BriefSection<MemoryEntry>) -> Vec<BriefMemoryGql>` déjà présent dans le fichier.
- Produces: les champs GraphQL `preferences: [BriefMemoryGql!]!` et `preferenceTotal: Int!`, consommés par la Task 5.

- [ ] **Step 1 : écrire le test qui échoue**

À ajouter dans `backend/crates/api/src/graphql/tests.rs`, en suivant la forme des tests de brief déjà présents dans ce fichier :

Le helper de montage de ce fichier est `schema_with_one_task() -> (TestSchema, Uuid)` (ligne ~1454) —
il n'existe pas de `test_schema()`.

```rust
#[tokio::test]
async fn brief_exposes_preferences() {
    let (schema, _task_id) = schema_with_one_task().await;
    let response = schema
        .execute(
            r#"{ brief(variant: SESSION) { preferences { title reference } preferenceTotal } }"#,
        )
        .await;

    assert!(response.errors.is_empty(), "{:?}", response.errors);
}
```

- [ ] **Step 2 : lancer le test pour le voir échouer**

```bash
cd backend && cargo test -p api brief_exposes_preferences
```

Attendu : ÉCHEC — `Unknown field "preferences" on type "BriefGql"`.

- [ ] **Step 3 : écrire l'implémentation minimale**

Dans `struct BriefGql`, ajouter les deux champs à côté de `commitments` / `commitment_total`, en respectant la documentation par attribut du fichier :

```rust
    /// Working rules. Rendered first, cut last.
    pub preferences: Vec<BriefMemoryGql>,
    /// How many qualified, before the section cap.
    pub preference_total: i32,
```

Dans `impl From<Brief> for BriefGql`, les renseigner :

```rust
            preferences: memory_entries(&brief.preferences),
            preference_total: brief.preferences.total as i32,
```

- [ ] **Step 4 : lancer les tests de l'API**

```bash
cd backend && cargo test -p api
```

Attendu : PASSENT.

- [ ] **Step 5 : régénérer le schéma et commiter**

Attention : `cargo run -p api -- export-schema` **construit le pool d'abord**, donc il applique les migrations en attente à la vraie base `backend/aggregated_plan.db`. C'est attendu ici, mais ne jamais le lancer en croyant que c'est une opération en lecture seule.

`export-schema` **imprime le SDL sur stdout** — il n'écrit aucun fichier. Le schéma que lit la
codegen de la CLI est `backend/crates/cli/graphql/schema.graphql` (`schema_path` dans
`crates/cli/src/queries.rs`), donc la redirection est obligatoire : sans elle, la Task 5 échouera à
la compilation sans que rien n'explique pourquoi.

```bash
cd backend && cargo run -p api -- export-schema > crates/cli/graphql/schema.graphql
cd .. && git add backend/crates/api/src/graphql/types/brief.rs backend/crates/api/src/graphql/tests.rs backend/crates/cli/graphql/schema.graphql
git commit -m "Expose the brief preferences over GraphQL"
```

---

### Task 5 : la CLI transporte les nouveaux champs

**Files:**
- Modify: `backend/crates/cli/graphql/brief.graphql`
- Test: `backend/crates/cli/src/cli.rs` (tests existants de la commande `Brief`, lignes ~736 et ~1029)

**Interfaces:**
- Consumes: les champs GraphQL `preferences` / `preferenceTotal` (Task 4).
- Produces: rien de nouveau côté rendu — `aplan brief` imprime `lines`, rendu par le domaine. Les nouveaux champs ne servent qu'à `--json`.

- [ ] **Step 1 : ajouter les champs au document GraphQL**

Dans `backend/crates/cli/graphql/brief.graphql`, ajouter avant `deadlines` :

```graphql
    preferences {
      id
      reference
      title
      stakeholders
      occurredOn
    }
    preferenceTotal
```

- [ ] **Step 2 : vérifier que la CLI compile contre le nouveau schéma**

```bash
cd backend && cargo check -p cli
```

Attendu : compile. La macro de `graphql_client` valide le document contre le schéma exporté à la Task 4 ; un échec ici signifie que l'export a été oublié.

- [ ] **Step 3 : lancer les tests de la CLI**

```bash
cd backend && cargo test -p cli
```

Attendu : PASSENT.

- [ ] **Step 4 : vérifier à la main que le brief porte bien la section**

Le backend doit tourner (`cargo run -p api` dans un autre terminal).

```bash
aplan brief | head -8
```

Attendu : une section `Préférences` en tête, avant `Échéances`, avec des références `[m:xxx]`.

- [ ] **Step 5 : commit**

```bash
git add backend/crates/cli/graphql/brief.graphql
git commit -m "Carry the brief preferences through the CLI payload"
```

---

### Task 6 : amender les spécifications

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md` (R55 ligne 1521, R56 ligne 1522)
- Modify: `SPEC_TECHNIQUE.md` (description de `aplan brief`, ligne ~178-184)

**Interfaces:**
- Consumes: le comportement livré par les Tasks 1 à 5.
- Produces: la référence documentaire dont la Task 7 se réclame.

- [ ] **Step 1 : amender R55**

Dans `SPEC_FONCTIONNELLE.md`, remplacer la fin de R55 — « La troncature est **toujours annoncée** (`(8, 6 affichés)`) et s'applique de la section la moins utile vers la plus utile : les décisions cèdent avant les engagements, qui cèdent avant les échéances. » — par :

```
La troncature est **toujours annoncée** (`(8, 6 affichés)`) et s'applique de la section la moins
utile vers la plus utile : les décisions cèdent avant les engagements, qui cèdent avant les
échéances, qui cèdent avant les **préférences**. Les préférences sont les dernières coupées : elles
sont à la fois les plus utiles — une règle de méthode gouverne toute la session — et les moins
chères, quatre lignes au plus.
```

- [ ] **Step 2 : amender R56**

Dans `SPEC_FONCTIONNELLE.md`, R56, remplacer « sont retenus les souvenirs `commitment` (…) et `decision` (…) » par une formulation qui ajoute les préférences :

```
sont retenus les souvenirs `preference` (les plus récents d'abord — une règle redite récemment est
celle qui vaut ; plafond `MAX_PREFERENCE_ENTRIES` = 4, rendus **en tête** du brief), `commitment`
(les plus anciens d'abord — un engagement pris il y a trois mois est celui qu'on a oublié) et
`decision` (les plus récents d'abord — la question est « où en est le projet »), filtrés par R45.
```

- [ ] **Step 3 : amender la spec technique**

Dans `SPEC_TECHNIQUE.md`, dans la description de `aplan brief`, remplacer « (échéances, engagements ouverts, décisions actives, file de tri, vétusté de la consolidation) » par « (préférences, échéances, engagements ouverts, décisions actives, file de tri, vétusté de la consolidation) ».

- [ ] **Step 4 : relire les deux passages**

```bash
grep -n "préférences" SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md | head
```

Attendu : les trois passages amendés apparaissent.

- [ ] **Step 5 : commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "Document the brief preferences section in R55 and R56"
```

---

### Task 7 : brancher le brief sur le hook `SessionStart`

**Files:**
- Modify: `~/.claude/hooks/aplan-session-start.sh` (la construction de `$context`, et son émission `jq` en toute fin de fichier)

**Interfaces:**
- Consumes: `aplan brief`, qui porte désormais les préférences (Tasks 1 à 5) ; le helper `aplan_bounded()` déjà défini ligne ~35, qui enveloppe chaque appel dans `timeout 3`.
- Produces: le contexte injecté à chaque session — c'est le livrable du plan.

Ce fichier est hors du dépôt (il vit dans `~/.claude/`), donc rien à commiter ici. Sauvegarder avant de modifier.

- [ ] **Step 1 : sauvegarder le hook**

```bash
cp ~/.claude/hooks/aplan-session-start.sh ~/.claude/hooks/aplan-session-start.sh.bak-20260818
```

- [ ] **Step 2 : récupérer le brief, en meilleur effort**

Insérer ce bloc **avant** la construction de `context` (donc avant le `if` qui compose `$context`), après la récupération de `session_json` :

```sh
# Best effort, exactly like the task list above: a backend that is down must
# cost the memory layer, never the task binding. `aplan_bounded` already caps
# the call at 3 s; a non-zero exit leaves brief_block empty and the hook goes on.
brief_block=""
if brief_text=$(aplan_bounded brief 2>/dev/null) && [ -n "$brief_text" ]; then
  brief_block="$brief_text"
fi
```

- [ ] **Step 3 : ajouter le bloc au contexte émis**

Juste avant la ligne finale `jq -nc --arg ctx "$context" …`, ajouter :

```sh
if [ -n "$brief_block" ]; then
  context="${context}

${brief_block}"
fi
```

- [ ] **Step 4 : vérifier la syntaxe et la sortie du hook**

```bash
bash -n ~/.claude/hooks/aplan-session-start.sh && echo "syntaxe ok"
CLAUDE_CODE_SESSION_ID=00000000-0000-0000-0000-000000000000 \
  bash ~/.claude/hooks/aplan-session-start.sh | jq -r '.hookSpecificOutput.additionalContext' | tail -12
```

Attendu : la sortie se termine par le brief, section `Préférences` comprise, et le JSON reste valide.

- [ ] **Step 5 : vérifier la dégradation quand le backend est absent**

Arrêter le backend (ou pointer ailleurs), puis :

```bash
APLAN_API_URL=http://127.0.0.1:9/graphql \
CLAUDE_CODE_SESSION_ID=00000000-0000-0000-0000-000000000000 \
  bash ~/.claude/hooks/aplan-session-start.sh | jq -e '.hookSpecificOutput.additionalContext' >/dev/null && echo "dégradation ok"
```

Attendu : `dégradation ok` — le hook émet toujours un JSON valide, sans la strate mémoire. C'est l'exigence non négociable de la § 4.2 de la spec.

---

### Task 8 : la consolidation écrit les `fact` en `--confirm`

**Files:**
- Modify: `docs/prompts/consolidation-memoire.md`

**Interfaces:**
- Consumes: rien du code — ce fichier vit hors du binaire précisément pour être itérable sans recompiler.
- Produces: des `fact` immédiatement `active`, donc lisibles par le recall dès le lendemain.

- [ ] **Step 1 : lire les consignes d'écriture actuelles**

```bash
grep -n "remember\|--confirm\|--kind" docs/prompts/consolidation-memoire.md
```

- [ ] **Step 2 : passer les `fact` en `--confirm`**

Modifier la consigne d'écriture pour qu'elle distingue les deux régimes. La formulation à obtenir :

```
Écris chaque souvenir avec `aplan remember`.
- `--kind fact` : ajoute **`--confirm`**. Un fait est une observation, pas un engagement ; il entre
  actif et devient lisible par `aplan recall` dès la session suivante. Sans cela, la connaissance la
  plus fraîche serait structurellement la moins lisible.
- `--kind decision` et `--kind commitment` : **pas** de `--confirm`. Le garde-fou humain est conservé
  là où l'enjeu engage — c'est ce que `aplan inbox` sert à trier.
```

- [ ] **Step 3 : vérifier qu'aucune autre consigne ne contredit**

```bash
grep -n "pending\|file de validation\|inbox" docs/prompts/consolidation-memoire.md
```

Attendu : aucune phrase n'affirme plus que *tous* les souvenirs passent par la file. Corriger celles qui le font.

- [ ] **Step 4 : commit**

```bash
git add docs/prompts/consolidation-memoire.md
git commit -m "Write consolidated facts active, keep the queue for decisions"
```

- [ ] **Step 5 : observer le prochain passage**

Le timer tire à 17h30. Le lendemain :

```bash
sqlite3 backend/aggregated_plan.db "SELECT kind, status, COUNT(*) FROM memories WHERE date(recorded_at) = date('now') GROUP BY 1,2;"
journalctl --user -u aplan-consolidate.service -n 20 --no-pager -o cat | tail -20
```

Attendu : les `fact` du jour en `active`, les `decision` éventuelles en `pending`.

---

## Vérification de fin de plan

- [ ] **La suite complète passe**, crate `mcp` exclue :

```bash
cd backend && cargo test -p domain -p application -p infrastructure -p api -p cli
```

- [ ] **Le lint est propre** :

```bash
cd backend && cargo clippy -p domain -p api -p cli
```

- [ ] **Une nouvelle session voit le brief.** Ouvrir une session Claude Code dans un dépôt **autre** que `aggregated_plan` et vérifier que le contexte de démarrage contient la section `Préférences` et le pied de page `Recherche : aplan recall --q "…"`. C'est le seul test qui prouve que le plan a atteint son but : les 138 invocations sur 148 mesurées dans le dépôt aplan disent que tout ce qui ne marche que là ne marche pas.
