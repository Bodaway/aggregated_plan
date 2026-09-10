# Récurrences : balayage, effondrement et purge — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Donner au moteur de récurrence la capacité de balayer le passé — passer les occurrences périmées à `Cancelled`, n'afficher que la dernière occurrence due, et supprimer les instances passées sans temps loggé — puis brancher un job quotidien qui entretient tout ça sans jamais recréer le passé.

**Architecture:** Cinq unités indépendantes dans les couches existantes. Un use case de balayage (`sweep_stale_occurrences`) et un use case de passe (`run_recurrence_pass`) dans `application/use_cases/recurrence.rs` ; un champ d'effondrement sur `TaskFilter` appliqué dans le SQL de `infrastructure/database/task_repo.rs` ; un clamp d'une ligne dans `materialize_due_occurrences` qui empêche toute matérialisation rétroactive ; un quatrième scheduler dans `api/src/jobs.rs` calqué sur `run_eod_scheduler` ; des verbes CLI `aplan recurrence`. Aucune migration : le design réutilise `TaskStatus::Cancelled`.

**Tech Stack:** Rust stable, async-graphql 7, sqlx 0.8 (requêtes runtime), tokio 1, clap (CLI), tests inline `#[cfg(test)] mod tests` + SQLite en mémoire.

**Spec:** `docs/plans/2026-09-01-recurrences-balayage-design.md`

## Global Constraints

- **Aucune migration SQL.** Le statut terminal des occurrences périmées est `TaskStatus::Cancelled`, qui existe depuis la migration `007_add_recurrence.sql`. Ne pas ajouter de variante à `TaskStatus`, ne pas toucher au `CHECK (status IN …)` de `tasks`.
- **Séparation DDD stricte.** `domain` sans I/O ; `application` définit traits et use cases et ne dépend que de `domain` ; `infrastructure` implémente ; `api` dépend de tout.
- **TDD.** Test qui échoue d'abord, puis l'implémentation minimale. Tests inline `#[cfg(test)] mod tests`.
- **Pas de `.unwrap()` en production.** `Result<T, E>` partout ; `sqlx::Error` → `RepositoryError::Database(e.to_string())`.
- **Specs en français, code et commentaires en anglais.**
- **Ne jamais supprimer une instance portant du temps loggé.** Le temps remonte jusqu'à la facture client.
- **Build scopé :** `cargo test -p domain -p application -p infrastructure -p api`. La crate `mcp` ne compile pas au HEAD ; ne jamais lancer `cargo test` sur tout le workspace.
- **Commits :** sujet impératif, sans préfixe de ticket (aucun ticket ouvert), **sans** footer `Co-Authored-By` ni `Signed-off-by`. Stager uniquement les fichiers de la tâche.

---

## Ordre d'exécution et porte manuelle

Les tâches 1 à 4 livrent de quoi nettoyer. **Après la tâche 4, une porte manuelle** : l'utilisateur lance le ménage sur les deux templates de test. Les tâches 5 à 9 ne doivent pas être exécutées avant. Brancher le scheduler (tâche 8) avant le ménage réveillerait la génération sur les templates junk.

---

### Task 1: Lister les templates d'un utilisateur, désactivés compris

Le balayage doit atteindre les instances des templates **désactivés** — c'est exactement l'état des deux templates de test après le ménage. `RecurrenceRepository` n'expose aujourd'hui que `find_active_by_user`.

**Files:**
- Modify: `backend/crates/application/src/repositories/recurrence_repository.rs`
- Modify: `backend/crates/infrastructure/src/database/recurrence_repo.rs`
- Test: inline dans `backend/crates/infrastructure/src/database/recurrence_repo.rs`

**Interfaces:**
- Consumes: rien.
- Produces: `RecurrenceRepository::find_by_user(&self, user_id: UserId) -> Result<Vec<RecurrenceTemplate>, RepositoryError>` — tous les templates de l'utilisateur, actifs et désactivés.

- [ ] **Step 1: Écrire le test qui échoue**

Dans le `mod tests` de `recurrence_repo.rs` (ligne 268). Les helpers existent déjà dans ce module — `setup()` monte le pool `sqlite::memory:` et insère l'utilisateur de test, `user_id()` rend son `UserId`, `make_template(rule)` construit un template :

```rust
    // Test: find_by_user returns deactivated templates, find_active_by_user does not
    #[tokio::test]
    async fn find_by_user_includes_inactive_templates() {
        let pool = setup().await;
        let repo = SqliteRecurrenceRepository::new(pool);

        let active = make_template(RecurrenceRule::Daily { interval: 1 });
        let mut inactive = make_template(RecurrenceRule::Daily { interval: 2 });
        inactive.title = "Inactive".to_string();

        repo.save(&active).await.unwrap();
        repo.save(&inactive).await.unwrap();
        repo.deactivate(inactive.id).await.unwrap();

        let only_active = repo.find_active_by_user(user_id()).await.unwrap();
        assert_eq!(only_active.len(), 1, "find_active_by_user must still hide the deactivated one");

        let all = repo.find_by_user(user_id()).await.unwrap();
        assert_eq!(all.len(), 2, "find_by_user must return the deactivated template as well");
        assert!(all.iter().any(|t| t.id == inactive.id && !t.active));
    }
```

- [ ] **Step 2: Lancer le test, vérifier qu'il échoue**

Run: `cd backend && cargo test -p infrastructure find_by_user_includes_inactive_templates`
Expected: FAIL — `no method named find_by_user found`.

- [ ] **Step 3: Ajouter la méthode au trait**

Dans `recurrence_repository.rs`, après `find_active_by_user` :

```rust
    /// Find every template for a user, **active and deactivated alike**.
    ///
    /// Distinct from [`find_active_by_user`] on purpose: a deactivated template
    /// still owns the instances it generated, and those instances still need
    /// sweeping. Filtering them out here is what would leave a cancelled series'
    /// stale occurrences visible forever.
    async fn find_by_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<RecurrenceTemplate>, RepositoryError>;
```

- [ ] **Step 4: Implémenter côté SQLite**

Dans `recurrence_repo.rs`, calquer sur `find_active_by_user` (ligne 168) en retirant `AND active = 1`. **Ne pas oublier la boucle de chargement des tags** : sans elle les templates reviennent avec `tags` vide, silencieusement.

```rust
    async fn find_by_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<RecurrenceTemplate>, RepositoryError> {
        let rows = sqlx::query("SELECT * FROM task_recurrences WHERE user_id = ?")
            .bind(user_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        let mut templates: Vec<RecurrenceTemplate> =
            rows.iter().map(map_template_row).collect::<Result<_, _>>()?;

        for t in templates.iter_mut() {
            t.tags = load_tags_for_template(&self.pool, &t.id).await?;
        }

        Ok(templates)
    }
```

`map_template_row` et `load_tags_for_template` sont les fonctions libres déjà utilisées par `find_active_by_user` : les réutiliser, ne rien dupliquer.

- [ ] **Step 5: Ajouter la méthode aux doubles de test existants**

`InMemoryRecurrenceRepository` dans `application/src/use_cases/recurrence.rs` (~ligne 418) doit implémenter la nouvelle méthode, sinon la crate `application` ne compile plus :

```rust
        async fn find_by_user(
            &self,
            user_id: UserId,
        ) -> Result<Vec<RecurrenceTemplate>, RepositoryError> {
            let store = self.templates.lock().unwrap();
            Ok(store
                .values()
                .filter(|t| t.user_id == user_id)
                .cloned()
                .collect())
        }
```

Chercher tout autre implémenteur du trait (`grep -rn "impl RecurrenceRepository" backend/crates/`) et l'étendre de la même façon.

- [ ] **Step 6: Lancer les tests, vérifier qu'ils passent**

Run: `cd backend && cargo test -p application -p infrastructure`
Expected: PASS, y compris `find_by_user_includes_inactive_templates`.

- [ ] **Step 7: Commit**

```bash
git add backend/crates/application/src/repositories/recurrence_repository.rs \
        backend/crates/application/src/use_cases/recurrence.rs \
        backend/crates/infrastructure/src/database/recurrence_repo.rs
git commit -m "Lister les templates de récurrence désactivés"
```

---

### Task 2: Clamper la fenêtre de matérialisation sur aujourd'hui

Le piège identifié au cadrage : `materialize_due_occurrences` borne `from` sur `starts_on` mais **jamais sur `today`**. Un template dont le watermark a 123 jours génèrerait 123 jours d'occurrences passées au premier tick du scheduler.

**Files:**
- Modify: `backend/crates/application/src/use_cases/recurrence.rs:258-266`
- Test: inline dans le même fichier

**Interfaces:**
- Consumes: rien.
- Produces: aucun changement de signature. `materialize_due_occurrences` ne crée plus jamais d'occurrence antérieure à `today`.

- [ ] **Step 1: Écrire le test de régression qui échoue**

Dans le `mod tests` de `recurrence.rs` :

```rust
// ── Le clamp : un watermark périmé ne rejoue jamais le passé ──────────────
#[tokio::test]
async fn materialize_never_creates_occurrences_before_today() {
    let rec_repo = InMemoryRecurrenceRepository::new();
    let task_repo = InMemoryTaskRepository::new();
    let today = today();

    // A daily template whose watermark stopped 123 days ago — the exact shape of
    // the two test templates found polluting the database on 2026-09-01.
    let mut template = daily_template(test_user_id(), today - Duration::days(200));
    template.last_generated_through = Some(today - Duration::days(123));
    rec_repo.save(&template).await.unwrap();

    materialize_due_occurrences(&rec_repo, &task_repo, test_user_id(), today, 14)
        .await
        .unwrap();

    let instances = task_repo.find_by_recurrence(template.id).await.unwrap();
    assert!(
        !instances.is_empty(),
        "the horizon ahead of today must still be materialized"
    );
    for task in &instances {
        let occ = task.occurrence_date.expect("instance carries an occurrence date");
        assert!(
            occ >= today,
            "materialization must never backfill the past, got {occ} < {today}"
        );
    }
}
```

`daily_template(user, starts_on)` : helper local créant un `RecurrenceTemplate` à règle quotidienne. S'il n'existe pas, l'écrire en s'alignant sur les templates construits par les tests voisins du fichier.

- [ ] **Step 2: Lancer le test, vérifier qu'il échoue**

Run: `cd backend && cargo test -p application materialize_never_creates_occurrences_before_today`
Expected: FAIL — l'assertion `occ >= today` casse sur une date passée.

- [ ] **Step 3: Appliquer le clamp**

Remplacer, dans `materialize_due_occurrences` :

```rust
        // Clamp: never go before starts_on.
        let from = from.max(template.starts_on);
```

par :

```rust
        // Clamp on both ends: never before starts_on, and never before today.
        //
        // The `today` bound is not cosmetic. `last_generated_through` can be months
        // stale on a template nobody materialized (the engine had no scheduler until
        // this change), and without this clamp the first tick would backfill every
        // occurrence since that watermark — recreating in one night exactly the pile
        // of dead instances this work exists to remove. A missed occurrence is a
        // historical fact, not something to regenerate.
        let from = from.max(template.starts_on).max(today);
```

- [ ] **Step 4: Lancer les tests, vérifier qu'ils passent**

Run: `cd backend && cargo test -p application recurrence`
Expected: PASS. Si un test existant échoue parce qu'il matérialisait volontairement dans le passé, lire son intention : s'il vérifiait la génération rétroactive, c'est le comportement qu'on supprime — mettre à jour le test et documenter pourquoi dans son commentaire.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/application/src/use_cases/recurrence.rs
git commit -m "Interdire la matérialisation rétroactive des occurrences

Sans borne sur today, un template au watermark périmé régénère toutes les
occurrences manquantes au premier passage. Le scheduler ajouté plus loin
en aurait créé des centaines d'un coup."
```

---

### Task 3: Purger le passé dans `cancel_recurrence`

Aujourd'hui `cancel_recurrence` ne supprime que `status == Todo` **et** `occurrence_date >= today`. Les instances passées sont indestructibles par tous les chemins.

**Files:**
- Modify: `backend/crates/application/src/use_cases/recurrence.rs:193-226`
- Modify: `backend/crates/api/src/graphql/mutation.rs:1229-1255`
- Modify: `backend/crates/api/src/graphql/types/mod.rs` (nouveau type de sortie)
- Modify: `backend/crates/cli/graphql/schema.graphql`
- Test: inline dans `recurrence.rs`

**Interfaces:**
- Consumes: rien.
- Produces:
  - `pub struct CancelRecurrenceOutcome { pub deleted: usize, pub cancelled: usize }`
  - `cancel_recurrence(rec_repo, task_repo, worklog_repo, id, caller_user_id, today) -> Result<CancelRecurrenceOutcome, AppError>` — **la signature gagne `worklog_repo: &dyn WorklogRepository` en troisième position**.
  - GraphQL `cancelRecurrence(id: ID!): CancelRecurrenceResultGql!` avec `deleted: Int!` et `cancelled: Int!`.

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
// ── Purge du passé : sans temps loggé, on supprime ────────────────────────
#[tokio::test]
async fn cancel_recurrence_deletes_past_instances_without_worklog() {
    let rec_repo = InMemoryRecurrenceRepository::new();
    let task_repo = InMemoryTaskRepository::new();
    let worklog_repo = InMemoryWorklogRepository::new();
    let today = today();

    let template = daily_template(test_user_id(), today - Duration::days(30));
    rec_repo.save(&template).await.unwrap();
    let stale = save_instance(&task_repo, &template, today - Duration::days(10), TaskStatus::Todo).await;

    let outcome = cancel_recurrence(
        &rec_repo, &task_repo, &worklog_repo, template.id, test_user_id(), today,
    )
    .await
    .unwrap();

    assert_eq!(outcome.deleted, 1);
    assert_eq!(outcome.cancelled, 0);
    assert!(task_repo.find_by_id(stale).await.unwrap().is_none());
}

// ── Purge du passé : avec temps loggé, on annule mais on garde ────────────
#[tokio::test]
async fn cancel_recurrence_preserves_past_instances_carrying_worklog() {
    let rec_repo = InMemoryRecurrenceRepository::new();
    let task_repo = InMemoryTaskRepository::new();
    let worklog_repo = InMemoryWorklogRepository::new();
    let today = today();

    let template = daily_template(test_user_id(), today - Duration::days(30));
    rec_repo.save(&template).await.unwrap();
    let worked = save_instance(&task_repo, &template, today - Duration::days(10), TaskStatus::Todo).await;
    worklog_repo.push_entry_for_task(test_user_id(), worked).await;

    let outcome = cancel_recurrence(
        &rec_repo, &task_repo, &worklog_repo, template.id, test_user_id(), today,
    )
    .await
    .unwrap();

    assert_eq!(outcome.deleted, 0, "logged time is billing evidence, never deleted");
    assert_eq!(outcome.cancelled, 1);

    let kept = task_repo.find_by_id(worked).await.unwrap().expect("still there");
    assert_eq!(kept.status, TaskStatus::Cancelled);
}

// ── Le comportement sur le futur ne change pas ────────────────────────────
#[tokio::test]
async fn cancel_recurrence_still_deletes_future_todo_instances() {
    let rec_repo = InMemoryRecurrenceRepository::new();
    let task_repo = InMemoryTaskRepository::new();
    let worklog_repo = InMemoryWorklogRepository::new();
    let today = today();

    let template = daily_template(test_user_id(), today);
    rec_repo.save(&template).await.unwrap();
    let future = save_instance(&task_repo, &template, today + Duration::days(3), TaskStatus::Todo).await;
    let done = save_instance(&task_repo, &template, today + Duration::days(4), TaskStatus::Done).await;

    let outcome = cancel_recurrence(
        &rec_repo, &task_repo, &worklog_repo, template.id, test_user_id(), today,
    )
    .await
    .unwrap();

    assert_eq!(outcome.deleted, 1);
    assert!(task_repo.find_by_id(future).await.unwrap().is_none());
    assert!(task_repo.find_by_id(done).await.unwrap().is_some(), "a Done instance is history");
}
```

Helpers à écrire dans le `mod tests` s'ils manquent :

```rust
    async fn save_instance(
        repo: &InMemoryTaskRepository,
        template: &RecurrenceTemplate,
        occurrence_date: NaiveDate,
        status: TaskStatus,
    ) -> TaskId {
        let mut task = instance_from_template(template, occurrence_date);
        task.status = status;
        repo.save(&task).await.unwrap();
        task.id
    }
```

`instance_from_template` construit une `Task` avec `recurrence_id: Some(template.id)` et `occurrence_date: Some(occurrence_date)` ; réutiliser la construction déjà présente dans `materialize_due_occurrences` plutôt que de la réinventer.

`InMemoryWorklogRepository` : double minimal implémentant `WorklogRepository`. Seule `list` est réellement exercée ; les autres méthodes renvoient `Ok(Default::default())` ou `unimplemented!()`. Ajouter `push_entry_for_task(user_id, task_id)` comme helper hors trait.

- [ ] **Step 2: Lancer les tests, vérifier qu'ils échouent**

Run: `cd backend && cargo test -p application cancel_recurrence`
Expected: FAIL — arité de `cancel_recurrence` incorrecte, et `CancelRecurrenceOutcome` inconnu.

- [ ] **Step 3: Écrire le type de sortie et réécrire le use case**

Dans `recurrence.rs`, au-dessus de `cancel_recurrence` :

```rust
/// What a cancellation actually did to the series' instances.
///
/// Two counters rather than one, because the two outcomes are not
/// interchangeable: `deleted` rows are gone, `cancelled` rows are still there and
/// still carry their worklog entries. A caller that reports only a total cannot
/// tell the user which of their history survived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CancelRecurrenceOutcome {
    /// Instances removed outright: they carried no logged time.
    pub deleted: usize,
    /// Instances kept but marked `Cancelled`: they carried logged time, which
    /// reaches the client invoice and must never be destroyed by a cleanup.
    pub cancelled: usize,
}
```

Remplacer la boucle de `cancel_recurrence` (l'ancien bloc `for task in instances { … }`) par :

```rust
    // Every entry logged anywhere in this series, read once: the per-instance
    // question ("does this one carry time?") is a set membership test, not a
    // query per task.
    let logged_task_ids: std::collections::HashSet<TaskId> = worklog_repo
        .find_by_recurrence(caller_user_id, id, WORKLOG_FILTER_MAX_LIMIT, 0)
        .await?
        .into_iter()
        .map(|entry| entry.task_id)
        .collect();

    let instances = task_repo.find_by_recurrence(id).await?;
    let mut outcome = CancelRecurrenceOutcome::default();

    for mut task in instances {
        let Some(occ) = task.occurrence_date else {
            continue;
        };

        // Already closed by a human decision — leave it exactly as it is.
        if matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled) {
            continue;
        }

        if occ >= today {
            // Unchanged behaviour on the future: a not-yet-due Todo slot is
            // simply removed when the series is cancelled.
            if task.status == TaskStatus::Todo {
                task_repo.delete(task.id).await?;
                outcome.deleted += 1;
            }
            continue;
        }

        if logged_task_ids.contains(&task.id) {
            task.status = TaskStatus::Cancelled;
            task.updated_at = Utc::now();
            task_repo.save(&task).await?;
            outcome.cancelled += 1;
        } else {
            task_repo.delete(task.id).await?;
            outcome.deleted += 1;
        }
    }

    Ok(outcome)
```

Adapter la signature et les `use` (`WorklogRepository`, `WORKLOG_FILTER_MAX_LIMIT`, `HashSet`). Mettre à jour le doc-comment de la fonction pour décrire le traitement du passé.

- [ ] **Step 4: Lancer les tests, vérifier qu'ils passent**

Run: `cd backend && cargo test -p application cancel_recurrence`
Expected: PASS sur les trois tests.

- [ ] **Step 5: Propager au resolver GraphQL**

Dans `api/src/graphql/types/mod.rs`, ajouter :

```rust
/// Result of cancelling a recurrence series.
#[derive(SimpleObject)]
pub struct CancelRecurrenceResultGql {
    /// Instances removed outright (no logged time).
    pub deleted: i32,
    /// Instances kept and marked cancelled because they carry logged time.
    pub cancelled: i32,
}
```

Dans `mutation.rs`, `cancel_recurrence` devient :

```rust
    /// Cancel a recurring task series. Deactivates the template, deletes every
    /// instance that carries no logged time — future *and* past — and marks the
    /// rest cancelled. Logged time reaches the client invoice, so an instance
    /// carrying it is never deleted.
    async fn cancel_recurrence(&self, ctx: &Context<'_>, id: ID) -> Result<CancelRecurrenceResultGql> {
        use domain::types::recurrence::RecurrenceTemplateId;

        let user_id = ctx.data::<UserId>()?;
        let rec_repo = ctx.data::<Arc<dyn RecurrenceRepository>>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let worklog_repo = ctx.data::<Arc<dyn WorklogRepository>>()?;
        let today = chrono::Utc::now().date_naive();

        let template_id = id
            .parse::<RecurrenceTemplateId>()
            .map_err(|e| async_graphql::Error::new(format!("Invalid template ID: {e}")))?;

        let outcome = recurrence_uc::cancel_recurrence(
            rec_repo.as_ref(),
            task_repo.as_ref(),
            worklog_repo.as_ref(),
            template_id,
            *user_id,
            today,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(CancelRecurrenceResultGql {
            deleted: outcome.deleted as i32,
            cancelled: outcome.cancelled as i32,
        })
    }
```

- [ ] **Step 6: Régénérer le schéma CLI**

⚠️ `cargo run -p api -- export-schema` **construit le pool d'abord** et applique donc les migrations en attente à la vraie `aggregated_plan.db`. Il n'y a pas de migration dans ce plan, donc l'effet est nul ici — mais ne pas prendre l'habitude.

Run: `cd backend && cargo run -p api -- export-schema`
Vérifier que `backend/crates/cli/graphql/schema.graphql` contient bien `cancelRecurrence(id: ID!): CancelRecurrenceResultGql!` et le type `CancelRecurrenceResultGql`.

- [ ] **Step 7: Lancer la suite complète**

Run: `cd backend && cargo test -p domain -p application -p infrastructure -p api`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add backend/crates/application/src/use_cases/recurrence.rs \
        backend/crates/api/src/graphql/mutation.rs \
        backend/crates/api/src/graphql/types/mod.rs \
        backend/crates/cli/graphql/schema.graphql
git commit -m "Purger les occurrences passées à l'annulation d'une récurrence

Une instance passée n'était supprimable par aucun chemin : deleteTask les
refuse, cancelRecurrence ne regardait que le futur. Celles qui portent du
temps loggé sont annulées plutôt que supprimées."
```

---

### Task 4: Verbes CLI `aplan recurrence list|cancel|skip`

Sans ces verbes, rien n'est actionnable depuis le CLI et le ménage reste impossible. `sweep` n'est **pas** livré ici : il appelle le use case de la tâche 5.

**Files:**
- Modify: `backend/crates/cli/src/cli.rs`
- Modify: `backend/crates/cli/src/main.rs`
- Create: `backend/crates/cli/src/recurrence_cmd.rs`
- Modify: `backend/crates/cli/src/queries.rs`
- Test: inline dans `cli.rs` (parsing) et `recurrence_cmd.rs`

**Interfaces:**
- Consumes: `cancelRecurrence(id: ID!): CancelRecurrenceResultGql!` (tâche 3), `recurrenceTemplates` et `skipOccurrence` (déjà au schéma).
- Produces: `Commands::Recurrence { cmd: RecurrenceCmd }` avec `RecurrenceCmd::{ List, Cancel { template: String }, Skip { task: String } }` ; fonctions `recurrence_cmd::{list, cancel, skip}(api_url: &str, json: bool, …) -> i32` (le code de sortie, comme les autres commandes).

- [ ] **Step 1: Écrire le test de parsing qui échoue**

Dans le `mod tests` de `cli.rs`, en s'alignant sur le test `Commands::Slots` existant (~ligne 991) :

```rust
    #[test]
    fn parses_recurrence_cancel() {
        let args = Args::parse_from(["aplan", "recurrence", "cancel", "abc123"]);
        match args.command {
            Commands::Recurrence { cmd: RecurrenceCmd::Cancel { template } } => {
                assert_eq!(template, "abc123");
            }
            other => panic!("expected Recurrence/Cancel, got {other:?}"),
        }
    }

    #[test]
    fn parses_recurrence_list() {
        let args = Args::parse_from(["aplan", "recurrence", "list"]);
        assert!(matches!(
            args.command,
            Commands::Recurrence { cmd: RecurrenceCmd::List }
        ));
    }
```

- [ ] **Step 2: Lancer, vérifier l'échec**

Run: `cd backend && cargo test -p cli parses_recurrence`
Expected: FAIL — `Recurrence` n'existe pas dans `Commands`.

- [ ] **Step 3: Déclarer la commande**

Dans `cli.rs`, dans `enum Commands`, à côté de `Slots` :

```rust
    /// Inspect and act on recurring-task templates.
    Recurrence {
        #[command(subcommand)]
        cmd: RecurrenceCmd,
    },
```

Puis, à côté de `SlotsCmd` :

```rust
#[derive(Subcommand, Debug)]
pub enum RecurrenceCmd {
    /// List every recurrence template, active and deactivated alike.
    List,
    /// Cancel a series: deactivate the template, delete the instances that carry
    /// no logged time, and mark the rest cancelled. Instances carrying logged
    /// time are never deleted — that time reaches the client invoice.
    Cancel {
        /// Template id (a prefix is enough when it is unambiguous).
        template: String,
    },
    /// Skip a single occurrence: its status becomes cancelled, the series lives on.
    Skip {
        /// Task reference: UUID, Jira key, or fuzzy title.
        task: String,
    },
}
```

- [ ] **Step 4: Écrire `recurrence_cmd.rs`**

Calquer la structure de `slots_cmd.rs` : requête via `client.rs`, sortie humaine plus `--json` rendant le payload `data.*` brut, codes de sortie `0` succès / `1` erreur réseau ou GraphQL / `2` introuvable / `3` ambigu.

Les trois documents GraphQL à ajouter dans `queries.rs` :

```rust
pub const RECURRENCE_TEMPLATES: &str = r#"
query RecurrenceTemplates {
  recurrenceTemplates {
    id
    title
    active
    startsOn
    endsOn
    lastGeneratedThrough
  }
}
"#;

pub const CANCEL_RECURRENCE: &str = r#"
mutation CancelRecurrence($id: ID!) {
  cancelRecurrence(id: $id) { deleted cancelled }
}
"#;

pub const SKIP_OCCURRENCE: &str = r#"
mutation SkipOccurrence($taskId: ID!) {
  skipOccurrence(taskId: $taskId) { id title status }
}
"#;
```

Vérifier les noms de champs de `RecurrenceTemplateGql` dans `backend/crates/cli/graphql/schema.graphql` (ligne 1690) et retirer du document tout champ absent — un champ inconnu fait échouer la requête entière.

`cancel` doit restituer les **deux** compteurs :

```rust
pub fn cancel(api_url: &str, json: bool, template: &str) -> i32 {
    let vars = serde_json::json!({ "id": template });
    match client::execute(api_url, queries::CANCEL_RECURRENCE, vars) {
        Ok(data) => {
            if json {
                output::print_json(&data);
            } else {
                let r = &data["cancelRecurrence"];
                let deleted = r["deleted"].as_i64().unwrap_or(0);
                let kept = r["cancelled"].as_i64().unwrap_or(0);
                println!(
                    "✓ récurrence annulée — {deleted} instance(s) supprimée(s), \
                     {kept} conservée(s) car elles portent du temps loggé"
                );
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}
```

Aligner `client::execute` et `output::print_json` sur les noms réels de `client.rs` et `output.rs`. Une instance conservée n'est pas un échec : le dire explicitement évite que l'utilisateur croie le ménage incomplet.

- [ ] **Step 5: Brancher le dispatch**

Dans `main.rs`, à côté du bras `Commands::Slots` :

```rust
        cli::Commands::Recurrence { cmd } => match cmd {
            cli::RecurrenceCmd::List => recurrence_cmd::list(&args.api_url, args.json),
            cli::RecurrenceCmd::Cancel { template } => {
                recurrence_cmd::cancel(&args.api_url, args.json, &template)
            }
            cli::RecurrenceCmd::Skip { task } => {
                recurrence_cmd::skip(&args.api_url, args.json, &task)
            }
        },
```

Déclarer `mod recurrence_cmd;` en tête de `main.rs`.

- [ ] **Step 6: Lancer les tests, vérifier qu'ils passent**

Run: `cd backend && cargo test -p cli && cargo build -p cli`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add backend/crates/cli/src/cli.rs backend/crates/cli/src/main.rs \
        backend/crates/cli/src/recurrence_cmd.rs backend/crates/cli/src/queries.rs
git commit -m "Exposer les récurrences au CLI (list, cancel, skip)"
```

---

## ⛔ Porte manuelle — ménage avant la suite

**Ne pas enchaîner sur la tâche 5 sans que l'utilisateur ait fait le ménage.** Installer le binaire CLI reconstruit, puis :

```bash
aplan recurrence list --json          # identifier les deux templates de test
aplan recurrence cancel <template-1>  # « Test recurring enum »
aplan recurrence cancel <template-2>  # « Test uppercase kind »
aplan ls --json --triage inbox --status todo   # vérifier que les 40 ont disparu
```

Attendu : 20 + 20 instances supprimées, 0 conservée (elles ne portent aucun temps loggé — vérifié le 2026-09-01). Les 8 occurrences « SAFT: rouler le script des heures JIRA. » **ne sont pas touchées** : leur récurrence est réelle et reste active.

---

### Task 5: Le balayage des occurrences périmées

**Files:**
- Modify: `backend/crates/application/src/use_cases/recurrence.rs`
- Test: inline dans le même fichier

**Interfaces:**
- Consumes: `RecurrenceRepository::find_by_user` (tâche 1).
- Produces: `sweep_stale_occurrences(rec_repo: &dyn RecurrenceRepository, task_repo: &dyn TaskRepository, user_id: UserId, today: NaiveDate) -> Result<usize, AppError>` — nombre d'instances passées à `Cancelled`.

- [ ] **Step 1: Écrire les tests qui échouent**

```rust
// ── Le balayage ferme tout ce qui est périmé et non clos ──────────────────
#[tokio::test]
async fn sweep_cancels_every_stale_open_status() {
    let rec_repo = InMemoryRecurrenceRepository::new();
    let task_repo = InMemoryTaskRepository::new();
    let today = today();

    let template = daily_template(test_user_id(), today - Duration::days(30));
    rec_repo.save(&template).await.unwrap();

    let stale_todo = save_instance(&task_repo, &template, today - Duration::days(5), TaskStatus::Todo).await;
    let stale_wip = save_instance(&task_repo, &template, today - Duration::days(4), TaskStatus::InProgress).await;
    let stale_blocked = save_instance(&task_repo, &template, today - Duration::days(3), TaskStatus::Blocked).await;

    let swept = sweep_stale_occurrences(&rec_repo, &task_repo, test_user_id(), today)
        .await
        .unwrap();

    assert_eq!(swept, 3);
    for id in [stale_todo, stale_wip, stale_blocked] {
        let t = task_repo.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(t.status, TaskStatus::Cancelled);
    }
}

// ── Ce que le balayage ne touche jamais ───────────────────────────────────
#[tokio::test]
async fn sweep_leaves_closed_current_and_future_alone() {
    let rec_repo = InMemoryRecurrenceRepository::new();
    let task_repo = InMemoryTaskRepository::new();
    let today = today();

    let template = daily_template(test_user_id(), today - Duration::days(30));
    rec_repo.save(&template).await.unwrap();

    let done = save_instance(&task_repo, &template, today - Duration::days(5), TaskStatus::Done).await;
    let already = save_instance(&task_repo, &template, today - Duration::days(4), TaskStatus::Cancelled).await;
    let current = save_instance(&task_repo, &template, today, TaskStatus::Todo).await;
    let future = save_instance(&task_repo, &template, today + Duration::days(2), TaskStatus::Todo).await;

    let swept = sweep_stale_occurrences(&rec_repo, &task_repo, test_user_id(), today)
        .await
        .unwrap();

    assert_eq!(swept, 0);
    assert_eq!(task_repo.find_by_id(done).await.unwrap().unwrap().status, TaskStatus::Done);
    assert_eq!(task_repo.find_by_id(already).await.unwrap().unwrap().status, TaskStatus::Cancelled);
    assert_eq!(task_repo.find_by_id(current).await.unwrap().unwrap().status, TaskStatus::Todo);
    assert_eq!(task_repo.find_by_id(future).await.unwrap().unwrap().status, TaskStatus::Todo);
}

// ── Un template désactivé laisse des instances à balayer ──────────────────
#[tokio::test]
async fn sweep_reaches_instances_of_deactivated_templates() {
    let rec_repo = InMemoryRecurrenceRepository::new();
    let task_repo = InMemoryTaskRepository::new();
    let today = today();

    let template = daily_template(test_user_id(), today - Duration::days(30));
    rec_repo.save(&template).await.unwrap();
    let stale = save_instance(&task_repo, &template, today - Duration::days(5), TaskStatus::Todo).await;
    rec_repo.deactivate(template.id).await.unwrap();

    let swept = sweep_stale_occurrences(&rec_repo, &task_repo, test_user_id(), today)
        .await
        .unwrap();

    assert_eq!(swept, 1, "deactivating a series must not strand its stale instances");
    assert_eq!(
        task_repo.find_by_id(stale).await.unwrap().unwrap().status,
        TaskStatus::Cancelled
    );
}
```

- [ ] **Step 2: Lancer, vérifier l'échec**

Run: `cd backend && cargo test -p application sweep_`
Expected: FAIL — `sweep_stale_occurrences` inconnu.

- [ ] **Step 3: Implémenter**

```rust
/// Close every occurrence the calendar has left behind.
///
/// An instance whose `occurrence_date` is strictly before `today` and whose status
/// is neither `Done` nor `Cancelled` becomes `Cancelled`. Everything else is left
/// exactly as it is: today's and future occurrences are still actionable, and a
/// `Done` or `Cancelled` instance already records a decision this sweep has no
/// business overwriting.
///
/// Walks **every** template of the user, deactivated ones included: cancelling a
/// series does not remove the instances it already generated, and those still need
/// closing.
///
/// Returns the number of instances swept.
pub async fn sweep_stale_occurrences(
    rec_repo: &dyn RecurrenceRepository,
    task_repo: &dyn TaskRepository,
    user_id: UserId,
    today: NaiveDate,
) -> Result<usize, AppError> {
    let templates = rec_repo.find_by_user(user_id).await?;
    let mut swept = 0usize;

    for template in templates {
        for mut task in task_repo.find_by_recurrence(template.id).await? {
            let Some(occ) = task.occurrence_date else {
                continue;
            };
            if occ >= today {
                continue;
            }
            if matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled) {
                continue;
            }

            task.status = TaskStatus::Cancelled;
            task.updated_at = Utc::now();
            task_repo.save(&task).await?;
            swept += 1;
        }
    }

    Ok(swept)
}
```

- [ ] **Step 4: Lancer, vérifier le succès**

Run: `cd backend && cargo test -p application sweep_`
Expected: PASS sur les trois tests.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/application/src/use_cases/recurrence.rs
git commit -m "Balayer les occurrences de récurrence périmées"
```

---

### Task 6: Exposer le balayage (GraphQL + `aplan recurrence sweep`)

**Files:**
- Modify: `backend/crates/api/src/graphql/mutation.rs`
- Modify: `backend/crates/cli/graphql/schema.graphql`
- Modify: `backend/crates/cli/src/cli.rs`, `main.rs`, `recurrence_cmd.rs`, `queries.rs`
- Test: inline (`mutation.rs` via `graphql/tests.rs`, parsing dans `cli.rs`)

**Interfaces:**
- Consumes: `sweep_stale_occurrences` (tâche 5).
- Produces: mutation `sweepStaleOccurrences: Int!` ; `RecurrenceCmd::Sweep` ; `recurrence_cmd::sweep(api_url: &str, json: bool) -> i32`.

- [ ] **Step 1: Écrire le test de resolver qui échoue**

Dans `backend/crates/api/src/graphql/tests.rs`, avec `build_test_schema()` comme les autres tests de mutation :

```rust
#[tokio::test]
async fn sweep_stale_occurrences_closes_a_past_open_instance() {
    let schema = build_test_schema();

    // A daily series that started a month ago, then materialize its horizon from a
    // date well in the past so at least one occurrence is already overdue.
    let created = schema
        .execute(
            r#"mutation {
                 createRecurringTask(input: {
                   title: "Sweep me"
                   rule: { kind: DAILY, interval: 1 }
                   startsOn: "2026-08-01"
                 }) { id }
               }"#,
        )
        .await;
    assert!(created.errors.is_empty(), "{:?}", created.errors);

    let result = schema.execute("mutation { sweepStaleOccurrences }").await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let swept = result.data.into_json().unwrap()["sweepStaleOccurrences"]
        .as_i64()
        .unwrap();
    assert!(swept >= 1, "at least the overdue occurrence must be swept, got {swept}");
}
```

⚠️ `createRecurringTask` **ne matérialise rien** (`recurrence.rs:57` ne fait que `repo.save`), contrairement à ce que prétend le doc-comment du schéma. Le test doit donc créer l'instance périmée explicitement — soit par `createTask` puis une écriture directe de `recurrence_id` / `occurrence_date`, soit en appelant `updateRecurringTask`, qui lui rematérialise. Lire les champs exacts de `CreateRecurringTaskInput` et `RecurrenceRuleInput` dans `schema.graphql` (lignes 1042 et 1681) avant d'écrire la mutation : les noms ci-dessus sont à confirmer, pas à recopier tels quels.

- [ ] **Step 2: Lancer, vérifier l'échec**

Run: `cd backend && cargo test -p api sweep_stale`
Expected: FAIL — `Unknown field "sweepStaleOccurrences"`.

- [ ] **Step 3: Ajouter le resolver**

```rust
    /// Close every recurrence occurrence the calendar has left behind: a past
    /// instance still open becomes cancelled. Returns how many were swept.
    async fn sweep_stale_occurrences(&self, ctx: &Context<'_>) -> Result<i32> {
        let user_id = ctx.data::<UserId>()?;
        let rec_repo = ctx.data::<Arc<dyn RecurrenceRepository>>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let today = chrono::Utc::now().date_naive();

        let swept = recurrence_uc::sweep_stale_occurrences(
            rec_repo.as_ref(),
            task_repo.as_ref(),
            *user_id,
            today,
        )
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(swept as i32)
    }
```

- [ ] **Step 4: Ajouter `Sweep` au CLI**

Dans `RecurrenceCmd` :

```rust
    /// Close every past occurrence still open, across all series.
    Sweep,
```

Bras de dispatch dans `main.rs` :

```rust
            cli::RecurrenceCmd::Sweep => recurrence_cmd::sweep(&args.api_url, args.json),
```

Et `recurrence_cmd::sweep`, calqué sur `cancel`.

- [ ] **Step 5: Régénérer le schéma et lancer les tests**

Run: `cd backend && cargo run -p api -- export-schema && cargo test -p api -p cli`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/crates/api/src/graphql/mutation.rs backend/crates/api/src/graphql/tests.rs \
        backend/crates/cli/graphql/schema.graphql backend/crates/cli/src/
git commit -m "Exposer le balayage des occurrences (GraphQL et CLI)"
```

---

### Task 7: N'afficher que la dernière occurrence due

**Files:**
- Modify: `backend/crates/application/src/repositories/task_repository.rs:9-39`
- Modify: `backend/crates/infrastructure/src/database/task_repo.rs` (construction du SQL de `find_by_user`)
- Modify: `backend/crates/cli/src/cli.rs`, `main.rs`, `commands.rs`, `queries.rs`
- Modify: `backend/crates/api/src/graphql/query.rs` (passage du flag)
- Test: inline dans `task_repo.rs`

**Interfaces:**
- Consumes: rien.
- Produces: `TaskFilter::collapse_recurrences: bool` (défaut `true` dans `TaskFilter::empty()`) ; argument GraphQL `allOccurrences: Boolean` sur la query `tasks` ; flag CLI `aplan ls --all-occurrences`.

- [ ] **Step 1: Écrire les tests qui échouent**

Dans le `mod tests` de `task_repo.rs` (SQLite en mémoire) :

```rust
#[tokio::test]
async fn collapse_keeps_only_the_latest_due_occurrence() {
    let repo = memory_task_repo().await;
    let user = test_user_id();
    let template = RecurrenceTemplateId::new();
    let today = chrono::Utc::now().date_naive();

    let old = insert_occurrence(&repo, user, template, today - Duration::days(7)).await;
    let latest = insert_occurrence(&repo, user, template, today - Duration::days(1)).await;
    let future = insert_occurrence(&repo, user, template, today + Duration::days(3)).await;

    let filter = TaskFilter::empty(); // collapse_recurrences défaut = true
    let found = repo.find_by_user(user, &filter).await.unwrap();
    let ids: Vec<_> = found.iter().map(|t| t.id).collect();

    assert!(ids.contains(&latest), "the latest due occurrence must survive");
    assert!(!ids.contains(&old), "an older due occurrence must be collapsed away");
    assert!(!ids.contains(&future), "a future occurrence is not what is to be done now");
}

#[tokio::test]
async fn collapse_never_touches_non_recurring_tasks() {
    let repo = memory_task_repo().await;
    let user = test_user_id();

    let a = insert_plain_task(&repo, user, "one").await;
    let b = insert_plain_task(&repo, user, "two").await;

    let found = repo.find_by_user(user, &TaskFilter::empty()).await.unwrap();
    let ids: Vec<_> = found.iter().map(|t| t.id).collect();
    assert!(ids.contains(&a) && ids.contains(&b));
}

#[tokio::test]
async fn a_fully_future_series_shows_nothing() {
    let repo = memory_task_repo().await;
    let user = test_user_id();
    let template = RecurrenceTemplateId::new();
    let today = chrono::Utc::now().date_naive();

    insert_occurrence(&repo, user, template, today + Duration::days(1)).await;
    insert_occurrence(&repo, user, template, today + Duration::days(2)).await;

    let found = repo.find_by_user(user, &TaskFilter::empty()).await.unwrap();
    assert!(found.is_empty(), "nothing is due yet, so nothing is shown");
}

#[tokio::test]
async fn opting_out_returns_every_occurrence() {
    let repo = memory_task_repo().await;
    let user = test_user_id();
    let template = RecurrenceTemplateId::new();
    let today = chrono::Utc::now().date_naive();

    insert_occurrence(&repo, user, template, today - Duration::days(7)).await;
    insert_occurrence(&repo, user, template, today - Duration::days(1)).await;
    insert_occurrence(&repo, user, template, today + Duration::days(3)).await;

    let mut filter = TaskFilter::empty();
    filter.collapse_recurrences = false;
    let found = repo.find_by_user(user, &filter).await.unwrap();
    assert_eq!(found.len(), 3);
}
```

- [ ] **Step 2: Lancer, vérifier l'échec**

Run: `cd backend && cargo test -p infrastructure collapse_`
Expected: FAIL — `no field collapse_recurrences on TaskFilter`.

- [ ] **Step 3: Ajouter le champ au filtre**

Dans `task_repository.rs` :

```rust
    /// When true (the default), a recurring series contributes at most one row:
    /// the occurrence with the greatest `occurrence_date` among those at or before
    /// today. Future occurrences of the materialization horizon are hidden, and a
    /// series whose occurrences are all in the future contributes nothing.
    ///
    /// The collapse picks that row **without looking at status**; the caller's
    /// status filter then applies to the row that survived. The reverse order —
    /// filter first, collapse after — would resurrect exactly the old occurrences
    /// this exists to hide.
    ///
    /// Tasks with a NULL `recurrence_id` are outside the rule and always returned.
    pub collapse_recurrences: bool,
```

et dans `TaskFilter::empty()` : `collapse_recurrences: true,`.

- [ ] **Step 4: Appliquer le collapse dans le SQL**

`find_by_user` (`task_repo.rs:211`) assemble son SQL par `sql.push_str(...)` en poussant les valeurs dans `bind_values` **dans le même ordre**. La table y est aliasée `t` — écrire `tasks.` casserait la requête.

Ajouter le bloc à la fin de la chaîne de clauses, après celle de `filter.tracking_state` :

```rust
        if filter.collapse_recurrences {
            sql.push_str(
                " AND ( \
                   t.recurrence_id IS NULL \
                   OR t.occurrence_date = ( \
                        SELECT MAX(t2.occurrence_date) FROM tasks t2 \
                        WHERE t2.recurrence_id = t.recurrence_id \
                          AND t2.user_id = t.user_id \
                          AND t2.occurrence_date <= ? \
                      ) \
                 )",
            );
            bind_values.push(
                chrono::Utc::now()
                    .date_naive()
                    .format("%Y-%m-%d")
                    .to_string(),
            );
        }
```

Le format `%Y-%m-%d` est celui qu'emploient déjà les binds de `deadline_before` / `deadline_after` : `occurrence_date` est stockée en `TEXT` ISO, la comparaison est donc lexicographique et n'est correcte qu'avec ce format exact.

Une série entièrement future rend la sous-requête `NULL`, et `t.occurrence_date = NULL` est faux en SQL : la série disparaît. C'est le comportement attendu, et c'est ce que teste `a_fully_future_series_shows_nothing`.

- [ ] **Step 5: Lancer, vérifier le succès**

Run: `cd backend && cargo test -p infrastructure`
Expected: PASS. Des tests existants peuvent casser s'ils comptaient les occurrences d'une série : les corriger en posant `collapse_recurrences = false` quand ils veulent bien tout voir.

- [ ] **Step 6: Exposer la sortie de secours**

GraphQL : ajouter `all_occurrences: Option<bool>` à la query `tasks` dans `query.rs`, et poser `filter.collapse_recurrences = !all_occurrences.unwrap_or(false)`.

CLI : ajouter à `Commands::Ls`

```rust
        /// Show every occurrence of a recurring task, not just the latest due one.
        #[arg(long)]
        all_occurrences: bool,
```

le propager dans le bras `cli::Commands::Ls { status, triage, all_occurrences }` de `main.rs` et dans `commands::ls`.

- [ ] **Step 7: Lancer la suite complète**

Run: `cd backend && cargo test -p domain -p application -p infrastructure -p api -p cli`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add backend/crates/application/src/repositories/task_repository.rs \
        backend/crates/infrastructure/src/database/task_repo.rs \
        backend/crates/api/src/graphql/query.rs backend/crates/cli/src/
git commit -m "N'exposer qu'une occurrence due par récurrence

Une série ne remonte plus que son occurrence la plus récente à date échue.
--all-occurrences rend la vue complète."
```

---

### Task 8: Le scheduler quotidien

**Files:**
- Modify: `backend/crates/application/src/use_cases/recurrence.rs` (la passe)
- Modify: `backend/crates/application/src/jobs.rs` (`RetryPolicy::recurrence`)
- Modify: `backend/crates/api/src/jobs.rs`
- Modify: `backend/crates/api/src/main.rs`
- Test: inline

**Interfaces:**
- Consumes: `materialize_due_occurrences` clampée (tâche 2), `sweep_stale_occurrences` (tâche 5).
- Produces:
  - `pub struct RecurrencePassOutcome { pub materialized: usize, pub swept: usize }`
  - `run_recurrence_pass(rec_repo, task_repo, user_id, today, horizon_days) -> Result<RecurrencePassOutcome, AppError>`
  - `RetryPolicy::recurrence()`
  - `api::jobs::RecurrenceDeps { rec_repo, task_repo }` et `run_recurrence_scheduler(deps, user_id)`

- [ ] **Step 1: Écrire le test d'ordre qui échoue**

```rust
// ── L'ordre du tick : matérialiser puis balayer, jamais l'inverse ─────────
#[tokio::test]
async fn pass_materializes_then_sweeps_without_eating_todays_slot() {
    let rec_repo = InMemoryRecurrenceRepository::new();
    let task_repo = InMemoryTaskRepository::new();
    let today = today();

    let mut template = daily_template(test_user_id(), today - Duration::days(200));
    template.last_generated_through = Some(today - Duration::days(123));
    rec_repo.save(&template).await.unwrap();
    let stale = save_instance(&task_repo, &template, today - Duration::days(9), TaskStatus::Todo).await;

    let outcome = run_recurrence_pass(&rec_repo, &task_repo, test_user_id(), today, 14)
        .await
        .unwrap();

    assert!(outcome.materialized > 0, "the horizon ahead is materialized");
    assert_eq!(outcome.swept, 1, "only the pre-existing stale instance is swept");
    assert_eq!(
        task_repo.find_by_id(stale).await.unwrap().unwrap().status,
        TaskStatus::Cancelled
    );

    // Nothing the pass just created may have been swept by its own sweep.
    for task in task_repo.find_by_recurrence(template.id).await.unwrap() {
        let occ = task.occurrence_date.unwrap();
        if occ >= today {
            assert_eq!(task.status, TaskStatus::Todo, "a fresh slot must stay open");
        }
    }
}

// ── Idempotence sur deux ticks du même jour ───────────────────────────────
#[tokio::test]
async fn a_second_pass_the_same_day_is_a_no_op() {
    let rec_repo = InMemoryRecurrenceRepository::new();
    let task_repo = InMemoryTaskRepository::new();
    let today = today();

    let template = daily_template(test_user_id(), today);
    rec_repo.save(&template).await.unwrap();

    run_recurrence_pass(&rec_repo, &task_repo, test_user_id(), today, 14).await.unwrap();
    let second = run_recurrence_pass(&rec_repo, &task_repo, test_user_id(), today, 14).await.unwrap();

    assert_eq!(second.materialized, 0);
    assert_eq!(second.swept, 0);
}
```

- [ ] **Step 2: Lancer, vérifier l'échec**

Run: `cd backend && cargo test -p application pass_materializes`
Expected: FAIL — `run_recurrence_pass` inconnu.

- [ ] **Step 3: Écrire la passe**

```rust
/// What one maintenance tick did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecurrencePassOutcome {
    pub materialized: usize,
    pub swept: usize,
}

/// One maintenance tick over the user's recurring series: materialize the horizon
/// ahead, then close what the calendar left behind.
///
/// The order matters and is not interchangeable. Materializing first means the
/// sweep runs against a set that already contains today's fresh slot — and since
/// the sweep only touches `occurrence_date < today`, that slot is out of its
/// reach by construction. Sweeping first would work too, but leaves the invariant
/// resting on timing rather than on the comparison; this way a tick can never
/// close what it has just opened.
pub async fn run_recurrence_pass(
    rec_repo: &dyn RecurrenceRepository,
    task_repo: &dyn TaskRepository,
    user_id: UserId,
    today: NaiveDate,
    horizon_days: i64,
) -> Result<RecurrencePassOutcome, AppError> {
    let materialized =
        materialize_due_occurrences(rec_repo, task_repo, user_id, today, horizon_days).await?;
    let swept = sweep_stale_occurrences(rec_repo, task_repo, user_id, today).await?;
    Ok(RecurrencePassOutcome { materialized, swept })
}
```

- [ ] **Step 4: Ajouter la politique de retry**

Dans `application/src/jobs.rs`, à côté de `session_reaper()` :

```rust
    /// The recurrence maintenance job: a pass every hour while healthy, backing
    /// off to two hours. Slower than every other job on purpose — its unit of work
    /// is the day. An occurrence materializes once per day and a stale one stays
    /// stale; a late tick costs nothing but a slot appearing an hour later, which
    /// is why nothing here justifies the end-of-day job's 5-minute base. Same
    /// escalation shape as the others: third consecutive failure escalates, an
    /// ongoing outage reminds every twelfth attempt.
    pub const fn recurrence() -> Self {
        Self {
            base: Duration::from_secs(60 * 60),
            ceiling: Duration::from_secs(2 * 60 * 60),
            escalate_after: 3,
            reminder_every: 12,
        }
    }
```

- [ ] **Step 5: Écrire le scheduler**

Dans `api/src/jobs.rs`, calqué sur `run_eod_scheduler` :

```rust
/// Dependencies the recurrence maintenance scheduler needs.
pub struct RecurrenceDeps {
    pub rec_repo: Arc<dyn RecurrenceRepository>,
    pub task_repo: Arc<dyn TaskRepository>,
}

/// How far ahead occurrences are materialized. Matches the horizon
/// `update_recurring_task` already uses, so a series looks the same whether it was
/// last touched by an edit or by this job.
const RECURRENCE_HORIZON_DAYS: i64 = 14;

/// Long-lived background task: materialize the horizon, sweep what the calendar
/// left behind, then wait as `RetryPolicy::recurrence()` says. Errors are logged,
/// never fatal — a series that fails to materialize is a slot appearing late, not
/// a reason to take the API down.
pub async fn run_recurrence_scheduler(deps: RecurrenceDeps, user_id: UserId) {
    let policy = RetryPolicy::recurrence();
    let mut health = JobHealth::default();
    loop {
        let attempt = run_recurrence_pass(
            deps.rec_repo.as_ref(),
            deps.task_repo.as_ref(),
            user_id,
            Utc::now().date_naive(),
            RECURRENCE_HORIZON_DAYS,
        )
        .await;

        let failure = match &attempt {
            Ok(_) => None,
            Err(e) => Some(e.to_string()),
        };
        if let Ok(outcome) = &attempt {
            if outcome.materialized > 0 || outcome.swept > 0 {
                tracing::info!(
                    materialized = outcome.materialized,
                    swept = outcome.swept,
                    "recurrence maintenance pass completed"
                );
            }
        }

        let observed = match &failure {
            Some(signature) => AttemptOutcome::Failed { signature },
            None => AttemptOutcome::Succeeded,
        };
        let (next_health, decision) = health.observe(observed, Utc::now(), &policy);
        health = next_health;
        report("recurrence maintenance", decision.log, failure.as_deref(), decision.retry_in);

        tokio::time::sleep(decision.retry_in).await;
    }
}
```

Étendre les `use` en tête du fichier (`RecurrenceRepository`, `run_recurrence_pass`).

- [ ] **Step 6: Brancher dans `main.rs`**

Les deux `Arc` existent déjà : `task_repo` (`main.rs:122`) et `recurrence_repo` (`main.rs:147`).

⚠️ **`recurrence_repo` est *déplacé* dans le builder de schéma à la ligne 207.** Un `recurrence_repo.clone()` écrit après cette ligne ne compile pas. Cloner **avant** la ligne 207 :

```rust
    // Cloned, not moved: the recurrence scheduler spawned below also needs it.
    let recurrence_repo_for_jobs = recurrence_repo.clone();
```

puis, après le spawn du session reaper (~ligne 308) :

```rust
    tokio::spawn(jobs::run_recurrence_scheduler(
        jobs::RecurrenceDeps {
            rec_repo: recurrence_repo_for_jobs,
            task_repo: task_repo.clone(),
        },
        default_user_id,
    ));
```

C'est exactement la manœuvre que le commentaire de la ligne 197 documente déjà pour le scheduler de pauses.

- [ ] **Step 7: Lancer la suite complète**

Run: `cd backend && cargo test -p domain -p application -p infrastructure -p api && cargo clippy -p api`
Expected: PASS, aucun warning clippy neuf.

- [ ] **Step 8: Commit**

```bash
git add backend/crates/application/src/use_cases/recurrence.rs \
        backend/crates/application/src/jobs.rs \
        backend/crates/api/src/jobs.rs backend/crates/api/src/main.rs
git commit -m "Entretenir les récurrences par un job horaire

Matérialise l'horizon puis balaie les occurrences périmées. Le moteur de
récurrence n'avait aucun appelant : rien ne matérialisait ni ne nettoyait."
```

---

### Task 9: Specs et garde-fou documentaire

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md`
- Modify: `SPEC_TECHNIQUE.md`
- Modify: `CLAUDE.md`

**Interfaces:**
- Consumes: tout ce qui précède.
- Produces: rien de code.

- [ ] **Step 1: `SPEC_FONCTIONNELLE.md`**

Ajouter, dans la section des tâches récurrentes : une série n'affiche que son occurrence due la plus récente ; les occurrences passées non traitées passent automatiquement au statut `annulée` ; annuler une série supprime les occurrences sans temps loggé et annule celles qui en portent. Préciser que « annulée » recouvre deux faits — une occurrence sautée volontairement et une occurrence périmée — et que l'UI peut libeller la seconde « abandonnée » sans que ce soit un statut de plus.

- [ ] **Step 2: `SPEC_TECHNIQUE.md`**

Documenter : le clamp `from = max(starts_on, last_generated_through + 1, today)` et pourquoi ; le job horaire (matérialisation puis balayage, `RetryPolicy::recurrence()`) ; le champ `TaskFilter::collapse_recurrences` avec l'ordre collapse-puis-statut ; le passage de `cancelRecurrence` de `Int!` à `CancelRecurrenceResultGql!`.

- [ ] **Step 3: `CLAUDE.md` — le garde-fou**

Ajouter sous « Common Gotchas » :

```markdown
- **Ne jamais exercer l'API GraphQL contre `http://127.0.0.1:3001` pour tester.**
  Ce port sert la vraie base `aggregated_plan.db`. Deux templates de récurrence
  créés par des appels de test en direct y ont laissé 40 tâches mortes, et une
  occurrence de récurrence passée n'était supprimable par aucun chemin. Pour
  exercer l'API à la main, lancer une instance sur une base jetable :
  `DATABASE_URL=sqlite:///tmp/aplan-dev.db cargo run -p api` et pointer le client
  dessus avec `--api-url` / `APLAN_API_URL`.
```

`DATABASE_URL` est bien la variable lue par `main.rs:117` — vérifié, le nom est correct tel quel.

- [ ] **Step 4: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md CLAUDE.md
git commit -m "Documenter le balayage des récurrences et la base de dév jetable"
```
