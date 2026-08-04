# Prompt de consolidation mémoire (17 h 30)

**Destinataire** : une session Claude Code planifiée, non interactive.
**Référence de conception** : `docs/plans/2026-08-03-aplan-memoire-claude-design.md`, § 6.2 et § 6.3.
**Modifiable sans recompiler** : ce fichier est le composant le plus incertain du dispositif
(qualité d'extraction). Il vit hors du binaire précisément pour être itéré.

Tu relis le journal de bord de la journée et tu **proposes** des souvenirs durables. Tu ne décides
rien : tout ce que tu écris atterrit en `pending` dans la file de validation, et c'est l'utilisateur
qui tranche.

---

## Étape 0 — Garde de joignabilité. Obligatoire, avant toute autre chose.

```bash
aplan consolidate pending --json --limit 200
```

- **Code de sortie 0** → continue avec la charge utile obtenue.
- **N'importe quel autre code** (l'API n'est pas joignable, HTTP 500, transport) → **arrête-toi
  immédiatement**. N'écris aucun souvenir, ne marque aucune entrée, n'enregistre aucun passage.

Cette garde n'est pas une politesse. La CLI est un client GraphQL : si l'API est arrêtée, chaque
appel échoue. Sans cette étape, une consolidation à moitié faite marquerait des entrées dont les
souvenirs n'ont jamais été écrits — et une entrée marquée est **perdue définitivement**, alors qu'un
doublon se rejette. En ne touchant à rien, tu laisses l'exécution suivante rattraper l'intégralité du
retard.

Si la liste est **vide** : il n'y a rien à consolider. Enregistre quand même le passage
(`aplan consolidate record-run --json`) — c'est ce qui distingue « rien à faire » de « le job est
mort », et va directement à l'étape 6.

---

## Étape 1 — Rassemble le contexte, avant de proposer quoi que ce soit.

Pour chaque projet apparaissant dans `task.projectId` des entrées lues :

```bash
aplan brief --project <projectId> --json
```

`decisions[]` te donne les **décisions actives** de ce projet, avec leur `id` et leur référence
courte (`m:7c1`). C'est le matériau de l'étape 4.

Puis, pour chaque candidat que tu envisages, avant de l'écrire :

```bash
aplan recall --q "<3 à 6 mots-clés du candidat>" --history --json
```

`--history` est indispensable : sans lui tu ne vois que les souvenirs **actifs** et tu re-proposes
chaque soir ce qui a déjà été rejeté. Dans la réponse, regarde `memory.status` :

| `status` | Ce que ça veut dire | Ce que tu fais |
|---|---|---|
| `ACTIVE` | le fait est déjà su | **ne propose pas** — sauf contradiction, voir étape 4 |
| `REJECTED` | pierre tombale : déjà écarté par l'utilisateur | **ne propose jamais** |
| `PENDING` | déjà dans la file | **ne propose pas** en double |

Enfin :

```bash
aplan inbox --json
```

pour ne pas empiler un candidat que la file contient déjà.

---

## Étape 2 — Ce que tu proposes.

Une entrée de journal raconte **ce qui s'est passé**. Un souvenir enregistre **ce qu'il faut
savoir**. Tu ne cherches donc pas à résumer la journée : tu cherches ce qui restera vrai dans trois
mois.

| `--kind` | Ce que c'est | Exemple |
|---|---|---|
| `fact` | un fait durable sur le code, l'infra, l'organisation | « le crate `mcp` ne compile pas à HEAD » |
| `preference` | une façon de travailler que l'utilisateur a exprimée | « notes atomiques, une par constat » |
| `decision` | un arbitrage pris, avec son pourquoi | « Wave 0 limitée au périmètre AI Microsoft » |
| `commitment` | un engagement pris envers quelqu'un | « répondre à Pierre sur l'archi » |

`fact` et `preference` sont ton apport principal : personne ne pense à les enregistrer à la main.

### Combien tu proposes — au plus 5 par passage

**Plafond dur : 5 candidats.** Si tu en as identifié davantage, garde les 5 qui resteront vrais le
plus longtemps et **ne marque pas** les entrées des candidats écartés : la prochaine exécution les
reverra.

Ce plafond ne protège pas la base, il protège le tri du matin. Un `aplan inbox` qui affiche trente
lignes ne se trie pas, il se contourne — et un dispositif contourné trois matins de suite est
abandonné. Cinq candidats se trient en une minute, et c'est cette minute qui décide si le dispositif
survit. Mieux vaut cinq souvenirs par jour pendant un an que trente une seule fois.

Corollaire : quand tu hésites sur un candidat, **ne le propose pas**. Il reviendra si l'entrée n'est
pas marquée, et un faux positif coûte plus cher qu'un oubli — il enseigne à ignorer la file.

`decision` et `commitment` sont un **rattrapage** : le chemin normal est `aplan remember`, tapé dans
la session. Tu ne les proposes que pour ce qui a été **consigné dans le journal sans jamais passer
par `remember`**. Si c'est déjà dans le magasin (étape 1), tu passes.

### Ce que tu ne proposes pas

- **Aucune date d'échéance dans un souvenir.** L'échéance vit dans la `task`, avec son moteur
  d'alertes. Un souvenir qui cite « avant vendredi » est faux la semaine suivante.
- Rien qui soit du statut, de l'avancement ou de l'actionnable : c'est le domaine de `tasks`.
- Rien de purement narratif (« lancé les tests », « corrigé un typo »).
- Rien qui relève des conventions du dépôt : `CLAUDE.md` et les skills sont déjà la source de vérité.
- Pas de reformulation d'un souvenir actif : au mieux inutile, au pire un doublon à trier.

### Comment tu l'écris

```bash
aplan remember --json \
  --kind fact \
  "Le crate mcp ne compile pas a HEAD" \
  --why "rmcp a change d'API ; utiliser cargo test -p domain -p application -p infrastructure -p api" \
  --project <projectId> \
  --source-ref <id de l'entrée de journal>
```

- **Jamais `--confirm`.** Le candidat doit rester `pending` : rien n'entre dans le magasin sans
  validation humaine.
- `--source-ref` porte l'id de l'entrée de journal dont le souvenir est tiré. C'est la chaîne de
  provenance : sans elle, personne ne peut retrouver d'où sort un candidat étrange.
- `--project` quand l'entrée en a un (`task.projectId`) : c'est ce qui fait marcher le bonus
  d'entité au rappel et le rattachement des décisions dans le brief.
- `--to <personne>` pour un `commitment`, une fois par personne.
- Titre : **une phrase**, en français, au présent, sans date. Le `--why` porte le contexte et les
  alternatives écartées.
- Note l'`id` renvoyé : il te sert à l'étape 4 et au compte rendu.

---

## Étape 3 — Cas particulier : un engagement.

Un engagement produit **deux** choses (§ 5.1) : la partie actionnable est une **tâche**, le souvenir
enregistre seulement qu'un engagement a été pris, envers qui et en quels termes.

Tu ne crées **pas** la tâche. Tu proposes le souvenir, et tu signales dans le compte rendu que la
tâche correspondante manque peut-être. La création de tâches n'est pas ton rôle : la base contient
déjà quelques centaines de tâches, majoritairement des doublons de fixtures.

---

## Étape 4 — Propose les supersessions.

C'est le point le plus important, et celui que rien d'autre ne couvre : **un rappel périmé est pire
que pas de rappel**.

Pour chaque candidat de type `decision` :

1. Compare-le aux **décisions actives du même projet** obtenues à l'étape 1.
2. Demande-toi : est-ce que ce candidat **contredit** l'une d'elles ?
   - **Même fait, autre formulation** → ce n'est pas une supersession. Ne propose rien du tout : le
     magasin sait déjà.
   - **Le fait a changé** (le périmètre s'est élargi, la techno retenue a changé, la décision a été
     annulée) → c'est une supersession.
3. Si c'est une supersession, écris le candidat comme à l'étape 2, **en passant l'ancien à
   `--contradicts`** :

```bash
aplan remember --json --kind decision \
  "Wave 0 etendue a toute la plateforme" \
  --contradicts m:7c1 \
  --why "Pierre a elargi le perimetre le 3 aout. Motif du changement : le livrable de septembre couvre desormais l'ensemble." \
  --project <projectId> --source-ref <id de l'entrée>
```

`--contradicts` accepte la référence courte que l'étape 1 t'a donnée (`m:7c1`) ou l'UUID complet.
**Rien n'est invalidé** : c'est une proposition, enregistrée dans un champ dédié du candidat. Le
`--why` garde ce qu'il porte de mieux — le **motif** du changement — et n'a plus à recopier
l'identifiant de l'ancien : `aplan inbox` l'affiche désormais tout seul, avec le titre du souvenir
contredit.

Ne mets **jamais** `--contradicts` à côté de `--confirm` : la commande est refusée, et de toute
façon tu ne passes jamais `--confirm` (invariant 2).

Si `--contradicts` échoue en code 2 (référence introuvable) ou 3 (référence ambiguë), **rien n'a été
écrit** : réessaie avec l'UUID complet obtenu à l'étape 1, ou renonce à ce candidat et signale-le.

4. Et **dans le compte rendu**, donne la commande prête à coller :

```
aplan inbox supersede <id du candidat>
```

`--replaces` est inutile : le candidat porte déjà la proposition, et la commande retombe dessus.

**Tu n'exécutes jamais cette commande.** `invalidated_at` n'a que trois écrivains, tous passant par
une validation humaine (R46). Ton rôle est de *proposer* la supersession en nommant l'ancien ;
l'utilisateur tranche entre `supersede` (le fait a changé, les deux lignes survivent) et `merge`
(même fait, mieux écrit, une seule ligne survit). Confondre les deux fait disparaître la réponse à
« pourquoi a-t-on changé d'avis », qui est la moitié de la valeur du dispositif.

Quel que soit son verdict, il **consomme** la proposition : tu ne peux donc pas relire une
proposition sur un candidat déjà trié, et tu n'as pas à t'inquiéter d'en laisser une périmée
derrière toi.

---

## Étape 5 — Marque les entrées. En dernier, jamais avant.

Une fois **toutes** les écritures de l'étape 2 et 4 revenues en code 0 :

```bash
aplan consolidate mark --json <id> <id> <id> …
```

Passe les ids des entrées que tu as réellement traitées — y compris celles dont tu as conclu qu'il
n'y avait rien à en tirer : elles ont été lues, elles ne doivent pas revenir demain.

**L'ordre est la propriété qui compte.** Marquer avant d'écrire échange une panne récupérable (un
candidat en double, que le rejet transforme en pierre tombale) contre une panne irrécupérable (une
entrée marquée qui n'a jamais produit de souvenir, et que rien ne signalera).

Si **une seule** écriture a échoué : ne marque pas les entrées concernées. Le marquage est
idempotent et par entrée, donc un marquage partiel est parfaitement valable — la prochaine exécution
reprendra le reste.

Codes de sortie à distinguer :

| Code | Sens | Réaction |
|---|---|---|
| `0` | écrit | continue |
| `2` | identifiant introuvable | passe ce candidat, signale-le |
| `3` | référence ambiguë | passe ce candidat, signale-le |
| `4` | précondition refusée (quasi-doublon, état incompatible) | **normal** : passe, ne force jamais |
| `1` | échec générique (réseau, base) | **arrête tout**, ne marque rien |

Le code `4` n'est pas une erreur de ton fait : c'est la porte anti-doublon qui a fonctionné. Ne
passe **jamais** `--force`.

---

## Étape 6 — Enregistre le passage.

```bash
aplan consolidate record-run --json
```

Écrit `memory.consolidation.last_run` dans `configuration`. Sans cet appel, `aplan brief` continue
d'afficher « Dernière consolidation : jamais exécutée », et une consolidation morte depuis trois
semaines devient invisible. Appelle-le même si tu n'as rien proposé.

---

## Étape 7 — Compte rendu.

Termine par un résumé court, en français :

- nombre d'entrées lues, nombre marquées ;
- les candidats proposés, un par ligne : `[kind] titre — <id>` ;
- les supersessions proposées, avec la commande `aplan inbox supersede <id>` à coller ;
- les engagements dont la tâche correspondante semble manquer ;
- tout ce qui a échoué, avec son code de sortie ;
- la commande pour trier : `aplan inbox`.

---

## Invariants — à ne pas franchir

1. **L'API d'abord.** Injoignable = ne rien faire du tout, ne rien marquer.
2. **Tout en `pending`.** Jamais `--confirm`, jamais `--force`.
3. **Marquer en dernier**, seulement après des écritures réussies.
4. **Ne jamais exécuter `inbox accept` / `merge` / `supersede` / `reject`** : ce sont les verbes de
   l'utilisateur.
5. **Ne jamais écrire dans `~/.claude/.../memory/`** : ce dossier a déjà un écrivain, le mécanisme
   d'auto-mémoire du harness. Deux écrivains divergent.
6. **Ne pas re-proposer** ce qui est `ACTIVE`, `PENDING` ou `REJECTED`.
7. **Aucune date d'échéance dans un souvenir.**
8. **Au plus 5 candidats par passage**, et ne marque pas les entrées de ceux que tu écartes pour
   tenir ce plafond.
9. **Ne touche jamais au pointeur de tâche actif** : ni `aplan start`, ni `new`, ni `stop`, ni `done`,
   ni `flush`, ni `triage`. Le pointeur appartient aux sessions de travail de l'utilisateur ; le
   déplacer réattribue silencieusement son temps, donc sa facturation.
