# Récurrences : balayage des occurrences périmées, affichage de la dernière, purge du passé

Date : 2026-09-01
État : design validé, en attente de relecture avant plan d'implémentation

## Problème

Le moteur de récurrence sait créer des occurrences et sait annuler celles du futur.
Il ne sait pas balayer le passé. Conséquence observée le 2026-09-01 : sur 97 tâches
`FOLLOWED` ouvertes, 48 étaient des instances de récurrence périmées — 20 « Test
recurring enum », 20 « Test uppercase kind » (deux templates créés par des appels
GraphQL ad hoc contre le serveur vivant, ~123 jours plus tôt) et 8 occurrences en
attente d'une récurrence réelle (« SAFT: rouler le script des heures JIRA. »).

Aucun chemin ne permet de les retirer :

| Verbe | Comportement | Effet sur une instance passée |
|---|---|---|
| `deleteTask` | refuse toute instance récurrente (`application/src/use_cases/task_management.rs:251`) | aucun |
| `cancelRecurrence` | supprime seulement `status == Todo` **et** `occurrence_date >= today` (`application/src/use_cases/recurrence.rs:214-224`) | aucun |
| `skipOccurrence` | passe le statut à `Cancelled`, ne supprime pas | statut seulement |

Deux constats de contexte, tous deux vérifiés dans le code :

1. **Le moteur est dormant.** `materialize_due_occurrences` n'a aucun appelant hors
   de son propre fichier ; `api/src/main.rs` ne lance que trois schedulers (EOD,
   pauses, session reaper). Le seul chemin qui rematérialise est
   `update_recurring_task` (`recurrence.rs:178`). Le junk observé est inerte : il ne
   grossit pas tout seul.
2. **Aucun test du repo n'écrit dans la vraie base.** Les chaînes « Test recurring
   enum » et « Test uppercase kind » sont introuvables dans le dépôt. La pollution
   vient d'appels manuels sur `http://127.0.0.1:3001/graphql`, pas de la suite de
   tests.

## Décisions prises

| Question | Décision |
|---|---|
| Statut terminal des occurrences périmées | **Réutiliser `TaskStatus::Cancelled`** — pas de nouvelle variante |
| Périmètre du balayage | **tout sauf `Done` et `Cancelled`** (donc `Todo`, `InProgress`, `Blocked`) |
| Affichage | **la dernière occurrence due (`occurrence_date <= today`)**, filtre par défaut côté backend |
| Occurrences SAFT en attente | **laissées en inbox** — récurrence réelle, on ne l'annule pas |
| Scheduler | **oui, job quotidien** qui matérialise puis balaie |
| Garde-fou contre la pollution | **base de dév séparée + consigne dans CLAUDE.md**, pas de code |

Réutiliser `Cancelled` est le choix qui porte le plus de valeur cachée : il évite une
migration de rebuild de table pour élargir le `CHECK (status IN …)` de `tasks`, et
donc évite de rejouer le défaut qui a fait échouer le job EOD pendant des semaines
(un `CHECK` à 3 valeurs face à un enum à 4 variantes, corrigé par la migration 013).
Aucune migration, aucun enum GraphQL, aucun changement de front.

## Architecture

Quatre unités, chacune testable isolément.

### §1 — `sweep_stale_occurrences` (application, use case)

```
sweep_stale_occurrences(rec_repo, task_repo, user_id, today) -> Result<usize, AppError>
```

Pour chaque template de l'utilisateur (actifs **et** désactivés — un template
désactivé laisse derrière lui des instances à balayer), toute instance vérifiant
`occurrence_date < today` et `status ∉ {Done, Cancelled}` passe à `Cancelled`.
Retourne le nombre d'instances balayées.

`RecurrenceRepository` n'expose aujourd'hui que `find_active_by_user`
(`application/src/repositories/recurrence_repository.rs:17`). Le balayage a donc
besoin d'une méthode supplémentaire — `find_by_user`, actifs et désactivés — sans
quoi les instances des templates désactivés resteraient éternellement non balayées,
ce qui est précisément le cas des deux templates de test après le ménage.

Ne touche jamais : une instance sans `occurrence_date`, une instance du jour ou du
futur, une tâche non récurrente.

### §2 — Effondrement des récurrences à l'affichage (application + infrastructure)

`TaskFilter` gagne un champ :

```rust
/// Quand true (défaut), une récurrence n'expose que son occurrence due la plus
/// récente : `MAX(occurrence_date)` parmi celles `<= today`. Les occurrences
/// futures de l'horizon sont masquées.
pub collapse_recurrences: bool,
```

`TaskFilter::empty()` l'initialise à `true`. Les tâches à `recurrence_id IS NULL`
sont hors du champ de la règle et remontent toutes.

**Ordre de l'effondrement et du filtre de statut.** L'effondrement choisit
`MAX(occurrence_date)` parmi les occurrences `<= today` **sans regarder le statut** ;
le filtre de statut du appelant s'applique ensuite, sur la ligne retenue. Conséquence
assumée : si la dernière occurrence due est `Done` et qu'une plus ancienne est encore
`Todo`, un `--status todo` ne remonte rien pour cette récurrence. C'est le
comportement voulu — l'occurrence courante est faite, l'ancienne est un résidu que §1
passera à `Cancelled` — et l'ordre inverse (filtrer puis effondrer) ferait
réapparaître exactement les vieilles occurrences que ce design supprime de la vue.

Cas limite à traiter explicitement : une récurrence dont **toutes** les occurrences
sont dans le futur n'a pas de « dernière due ». Elle ne remonte alors pas du tout —
cohérent avec l'intention (« ce qui est à faire maintenant »), et documenté comme
tel.

Point de sortie : `aplan ls --all-occurrences` désactive l'effondrement.

### §3 — Purge du passé dans `cancel_recurrence` (application, use case existant)

`cancel_recurrence` conserve son comportement actuel sur le futur et gagne un
traitement du passé :

- instance passée **sans aucune entrée de worklog** → supprimée ;
- instance passée **portant du temps loggé** → passée à `Cancelled`, jamais supprimée.

Le temps loggé remonte jusqu'à la facture client : on ne détruit pas une preuve de
travail pour faire du ménage. La signature de retour devient un couple
`{ deleted, cancelled }` au lieu d'un `usize`, ce qui change `cancelRecurrence(id): Int!`
dans le schéma GraphQL — c'est le seul changement d'interface **GraphQL** du design
(`TaskFilter` est interne, et §5 n'ajoute que de nouveaux verbes CLI).

`worklog_repository.rs:63` expose déjà un `find_by_recurrence` ; la présence de temps
par tâche se lit via le chemin par tâche du même repository.

### §4 — Scheduler quotidien (api/src/jobs.rs)

Quatrième job à côté d'EOD, pauses et session reaper : matérialise l'horizon, puis
balaie les périmées — dans cet ordre, pour qu'une occurrence créée aujourd'hui ne
soit pas balayée par la passe du même tick.

**Clamp obligatoire de la fenêtre de matérialisation.** Aujourd'hui
(`recurrence.rs:258-266`) :

```rust
let from = match template.last_generated_through {
    Some(last) => last + Duration::days(1),
    None => template.starts_on,
};
let from = from.max(template.starts_on);   // clampé sur starts_on, pas sur today
```

Rien ne borne `from` à `today`. Un template dont le watermark a 123 jours
génèrerait, au premier tick, 123 jours d'occurrences passées. Le balayage les
passerait aussitôt en `Cancelled`, mais les lignes seraient créées : brancher le
scheduler sans ce clamp reproduirait en une nuit le problème qu'on nettoie. La
fenêtre devient :

```rust
let from = from.max(template.starts_on).max(today);
```

Une occurrence ratée est un fait historique, pas quelque chose à recréer.

### §5 — Verbes CLI (crates/cli)

```
aplan recurrence list                  # les templates, actifs et désactivés
aplan recurrence cancel <template>     # cancelRecurrence, rapporte deleted + cancelled
aplan recurrence skip <task>           # skipOccurrence
aplan recurrence sweep                 # §1 à la demande
```

Sans ces verbes, rien de ce qui précède n'est actionnable depuis le CLI, et le
ménage du 2026-09-01 reste impossible.

## Ordre des opérations

L'ordre n'est pas indifférent : brancher le scheduler avant le ménage réveillerait
la génération sur les templates junk.

1. §3 (purge) + le clamp de §4 + les verbes `list` / `cancel` / `skip` de §5 —
   livrés ensemble, sans le job. `aplan recurrence sweep` ne peut pas être livré ici :
   il appelle §1, qui n'existe qu'à l'étape 3.
2. Ménage : `aplan recurrence cancel` sur les deux templates de test → 40 instances
   supprimées, templates désactivés. Les 40 ont été vérifiées le 2026-09-01 comme
   ne portant aucune entrée de worklog, elles tombent donc bien dans la branche
   « supprimée » de §3 et non dans la branche « passée à `Cancelled` ». Les 8
   occurrences SAFT ne sont pas touchées.
3. §1 (balayage) + `aplan recurrence sweep` + §2 (effondrement) — les 7 occurrences
   SAFT les plus anciennes passent à `Cancelled`, une seule carte reste affichée.
4. §4 (scheduler) branché en dernier, une fois la base propre et le clamp en place.

## Tests

TDD, tests inline `#[cfg(test)] mod tests`, SQLite en mémoire pour l'intégration.

- **§1** : balaie une instance `Todo` passée ; balaie `InProgress` et `Blocked`
  passées ; ne touche pas `Done` ni `Cancelled` ; ne touche pas une occurrence du
  jour ni du futur ; ne touche pas une tâche sans `recurrence_id` ; balaie les
  instances d'un template désactivé.
- **§2** : deux occurrences dues → seule la plus récente remonte ; occurrences
  futures masquées ; récurrence entièrement future → rien ne remonte ; tâche non
  récurrente jamais filtrée ; `collapse_recurrences: false` remonte tout.
- **§3** : instance passée sans worklog supprimée ; instance passée avec worklog
  passée à `Cancelled` et **présente en base après l'appel** ; comportement sur le
  futur inchangé ; les deux compteurs sont exacts.
- **§4** : un template au watermark vieux de 123 jours ne crée **aucune** occurrence
  antérieure à `today` (le test de régression du clamp) ; matérialisation puis
  balayage dans le bon ordre sur un même tick ; idempotence sur deux ticks.

## Hors périmètre

- Aucune nouvelle variante de `TaskStatus`, aucune migration de schéma.
- Le garde-fou contre la pollution est documentaire (base de dév séparée + consigne
  CLAUDE.md), pas du code : aucun test du repo n'écrit dans la vraie base, il n'y a
  pas de cause à corriger côté code.
- Les 8 occurrences SAFT ne sont ni supprimées ni skippées ; leur récurrence reste
  active.

## Maintenance des specs

`SPEC_FONCTIONNELLE.md` : la règle d'affichage (une récurrence n'expose que son
occurrence due la plus récente) et le statut `cancelled` comme issue d'une
occurrence périmée. `SPEC_TECHNIQUE.md` : le clamp de la fenêtre de matérialisation,
le job quotidien, le champ `collapse_recurrences`, le changement de type de retour
de `cancelRecurrence`.
