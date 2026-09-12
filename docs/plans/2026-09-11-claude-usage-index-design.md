# Index de consommation Claude — document de design

Suite du §9 de `2026-08-27-hud-overlay-tauri-design.md`, qui posait le besoin sans
le mesurer. Ce document le mesure, et **deux hypothèses du design d'origine ne
survivent pas**.

Objet : alimenter le bloc **Neural budget** du HUD — le dernier des sept panneaux
encore sur données factices (`frontend/src/pages/hud/blocks/stub-data.ts`) — avec
la consommation réelle de tokens Claude, lue dans les transcripts locaux.

---

## 1. Ce que la mesure a appris

Corpus au 11/09/2026 : **661 fichiers, 629 Mo**, `~/.claude/projects/**/*.jsonl`,
du 16/07 au 11/09. Le design d'origine comptait 462 fichiers et 434 Mo il y a
deux semaines : **le corpus a grossi de 45 % en quinze jours**, ce qui fixe l'ordre
de grandeur à tenir.

### 1.1 Un `requestId` porte plusieurs lignes, toutes avec le MÊME `usage`

C'est le fait central, et rien ne l'annonçait.

```
lignes assistant avec usage : 56 151
requestId distincts         : 28 675
lignes en doublon           : 27 452   (49 %)
```

Un appel API émet une ligne `assistant` **par bloc de contenu** — un bloc
`thinking`, un bloc `text`, un bloc `tool_use` — et **chacune répète l'objet
`usage` complet de la requête**, à l'identique. Vérifié bloc à bloc :

```
req_011CewptmnnW6Qn354R43vba : 2 lignes, usage identique
  (output 228, input 2, cache_read 32883, cache_creation 25177) × 2
```

Sommer les lignes gonfle la consommation d'un facteur **1,89** sur un transcript
réel. Une jauge « 68 % du plafond » afficherait 36 %. C'est la règle de
correction n°1 : **un `usage` compté une fois par `requestId`**.

### 1.2 Un scan complet coûte quelques secondes

Le design d'origine écrivait « index incrémental **obligatoire**, jamais de
re-scan complet ». Mesuré : **2,1 s en Python** pour parser les 629 Mo et en
extraire tout l'usage.

*Mesuré ensuite sur l'implémentation réelle* : **5,5 s** pour la passe complète —
663 fichiers lus, 28 772 requêtes écrites en base. Le parsing n'est pas le coût
dominant, l'écriture SQLite l'est. La passe suivante, avec le curseur en place,
prend **42 ms** et n'ouvre aucun fichier.

L'incrémental reste souhaitable — relire 629 Mo toutes les cinq minutes pour
quelques lignes neuves est du gaspillage pur — mais il devient une **optimisation
de débit, pas un mécanisme de correction**. La distinction compte : elle autorise
un design où un offset perdu, un fichier réécrit ou une base repartie de zéro se
soldent par un rescan, et non par des chiffres faux.

### 1.3 Le reste du terrain

| Fait mesuré | Conséquence |
|---|---|
| `cwd` présent sur 100 % des lignes, 177 valeurs distinctes | Attribution projet par `cwd`, **pas** par décodage du nom de dossier — `-home-mbt-appfactory-aggregated-plan` ne permet pas de distinguer `aggregated_plan` de `aggregated-plan`. |
| **L'arborescence n'est pas plate** : 166 fichiers à la profondeur 2, 266 à 4, 229 à 6 — les transcripts de sous-agents vivent dans `<session>/subagents/`, et s'imbriquent | Parcours **récursif** obligatoire. Un `read_dir` sur un seul niveau raterait les trois quarts du corpus, soit presque toute la consommation des sous-agents. |
| Ces 177 `cwd` incluent des worktrees (`…/.claude/worktrees/SCB-364-…`) | À normaliser vers le dépôt parent, sinon un même projet se scinde en plusieurs entrées du « top projet ». |
| `timestamp` présent sur 100 % des lignes, ISO-8601 UTC | Pas de cas dégradé à traiter. |
| `requestId` absent sur 18 lignes, toutes de modèle `<synthetic>` | `<synthetic>` n'est pas un modèle mais un marqueur de message fabriqué localement : exclu, ce qui supprime du même coup le seul cas de `requestId` manquant. |
| **26 645 lignes sur 56 151 sont des sidechains** (`isSidechain: true`) | Les sous-agents pèsent près de la moitié de la consommation. Ils comptent — ce sont de vrais tokens — et c'est précisément le chiffre qui rend la jauge intéressante. |
| `usage` porte aussi `cache_creation.ephemeral_{5m,1h}_input_tokens`, `server_tool_use`, `service_tier`, et un tableau `iterations` | `iterations` répète les totaux de la ligne : à ignorer, sous peine d'un second double comptage. |
| Modèles observés | `opus-5` 32 661, `sonnet-5` 15 294, `fable-5-1` 4 426, `fable-5` 3 230, `opus-4-8` 391, `haiku-4-5` 72. |

---

## 2. Décisions

**D1 — Une ligne par requête, pas des seaux horaires.** 28 675 lignes
aujourd'hui, quelques centaines de milliers au pire à l'horizon d'un an : SQLite
n'en a cure. Le gain est ailleurs — avec la requête comme grain, les quatre
affichages du bloc (fenêtre glissante 5 h, sparkline par jour, répartition par
modèle, top projet) sont quatre `SELECT`, et une question qu'on ne s'est pas
encore posée ne demandera pas de re-parser 629 Mo.

**D2 — `request_id` est la clé primaire.** L'idempotence vient de là et non de la
discipline du lecteur : réécrire la même ligne deux fois est un `INSERT OR
REPLACE` sans effet. C'est ce qui rend §1.2 exploitable — un rescan complet est
toujours sûr.

**D3 — L'incrémental est un cache, jamais une source de vérité.** Une table
`claude_usage_files` retient `(path, size, mtime, offset)`. On repart de
`offset` quand `size >= offset` et que le `mtime` a bougé ; **sinon on relit le
fichier entier**. Un fichier rétréci a été compacté ou réécrit : ses offsets ne
veulent plus rien dire. Grâce à D2 ce rescan ne peut pas fausser un total.

**D4 — Un job dans l'API, pas un daemon.** `api/src/jobs` héberge déjà quatre
schedulers (EOD, pauses, moissonneur de sessions, récurrences). Un cinquième ne
coûte ni processus à superviser, ni unité systemd, ni crate. Le design d'origine
parlait d'un `hud-daemon` séparé pour isoler le coût de parsing ; §1.2 dit que ce
coût n'existe pas.

**D5 — Le plafond reste déclaré à la main**, dans la table `configuration`
existante (même famille que `aplan.breaks.*`), sous
`aplan.claude.declared_ceiling_tokens`, lisible et modifiable par
`aplan config`. Inchangé par rapport au design
d'origine, et pour la même raison : le quota d'abonnement n'est exposé par aucune
API publique. La limite reste **visible à l'écran** — le bloc porte déjà la
mention « Set by hand, calibrated against /usage — not measured ».

**D6 — « Consommé » = `input + output + cache_creation`.** La question paraît
secondaire ; les chiffres disent le contraire. Sur les 28 637 requêtes du corpus :

```
input              158 476
output          15 830 335
cache_creation 127 456 085
cache_read   4 989 802 061      <-- 35,8x la somme des trois autres
thinking         6 016 038      <-- sous-ensemble de output
```

Inclure `cache_read` ne ferait pas varier la jauge de quelques pour-cent : il
**est** le total, à 97 %. La jauge cesserait de mesurer une consommation pour
mesurer un taux de cache — deux choses sans rapport, dont une seule intéresse
quelqu'un qui regarde s'il va taper son plafond. `cache_read` est stocké et
affichable à part, jamais additionné.

`thinking_tokens` vit dans `output_tokens_details` et n'a **jamais** dépassé
`output_tokens` sur une seule des 28 637 requêtes : c'est une part de `output`,
pas un poste de plus. Stocké pour information, jamais additionné — l'ajouter
gonflerait le total de 38 % de la production.

Corollaire pour D5 : le plafond saisi à la main doit être calibré contre
**cette** définition. Un plafond relevé dans `/usage` et une consommation
comptée autrement donneraient un ratio qui ne veut rien dire.

**D7 — Aucune purge pour l'instant.** Deux mois d'historique tiennent en 28 675
lignes. Une rétention est un mécanisme à écrire, à tester et à se rappeler ;
elle attendra que le volume la réclame.

---

## 3. Modèle de données — migration `024`

```sql
CREATE TABLE claude_usage_requests (
    request_id                  TEXT PRIMARY KEY,
    occurred_at                 TEXT NOT NULL,   -- ISO-8601 UTC, le `timestamp` de la ligne
    model                       TEXT NOT NULL,
    session_id                  TEXT NOT NULL,
    project_path                TEXT NOT NULL,   -- `cwd` normalisé (worktree -> dépôt parent)
    is_sidechain                INTEGER NOT NULL DEFAULT 0,
    input_tokens                INTEGER NOT NULL DEFAULT 0,
    output_tokens               INTEGER NOT NULL DEFAULT 0,
    cache_creation_input_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_input_tokens     INTEGER NOT NULL DEFAULT 0,
    thinking_tokens             INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_cur_occurred_at ON claude_usage_requests (occurred_at);
CREATE INDEX idx_cur_project     ON claude_usage_requests (project_path, occurred_at);

CREATE TABLE claude_usage_files (
    path            TEXT PRIMARY KEY,
    size_bytes      INTEGER NOT NULL,
    mtime           TEXT NOT NULL,
    offset_bytes    INTEGER NOT NULL,
    last_indexed_at TEXT NOT NULL
);
```

**Pourquoi 024 et pas 022.** Le numéro a été pris pendant l'écriture : une autre
session, dans le worktree `feat/pwa-lan`, avait déjà posé et **appliqué** un
`023_add_task_client_request_id.sql` sur la base réelle. sqlx n'applique que les
migrations postérieures à la plus haute déjà passée, si bien qu'un 022 arrivé après
un 023 est ignoré **en silence** — service démarré, aucune erreur, tables absentes.
Constaté exactement comme ça au premier redémarrage. Le 023 vit encore sur sa
branche ; le numéro lui est laissé.

Pas de `user_id` : ces lignes décrivent la machine, pas un utilisateur du
cockpit. C'est le premier écart à la règle « toutes les tables portent
`user_id` » (CLAUDE.md), et il est délibéré — un `user_id` ici serait une colonne
constante que personne ne filtrerait jamais.

`occurred_at` en TEXT ISO-8601 : la convention de toute la base, et l'ordre
lexicographique y est l'ordre chronologique, ce dont les fenêtres glissantes se
contentent.

---

## 4. Découpage en couches

Conforme aux règles DDD du projet.

**`domain/src/rules/claude_usage.rs`** — pur, sans I/O :
- `parse_usage_line(&str) -> Option<UsageRecord>` : une ligne JSONL en un
  enregistrement, `None` pour tout ce qui n'est pas un `assistant` porteur
  d'`usage`, et pour `<synthetic>`.
- `normalize_project_path(cwd) -> String` : replie `…/.claude/worktrees/<x>` sur
  son dépôt parent.
- `UsageWindow::summarize(&[UsageRecord], now, window) -> NeuralBudget` : la
  fenêtre glissante, la sparkline, la répartition par modèle, le top projet.
  Testable sans base ni fichier.

**`application/`** — trait `ClaudeUsageRepository` (`upsert_requests`,
`file_state`, `set_file_state`, `query_window`), et le cas d'usage
`index_claude_usage(repo, root, now)` qui orchestre sans savoir lire un disque.

**`infrastructure/`** — l'implémentation SQLite du trait, et le parcours
récursif de `~/.claude/projects/**/*.jsonl`, lecture depuis l'offset,
`INSERT OR REPLACE` par lots. Récursion écrite à la main sur `std::fs::read_dir`
plutôt qu'une dépendance `walkdir` : une vingtaine de lignes contre une caisse de
plus dans un backend qui n'en a pas besoin ailleurs.

**`api/`** — `jobs::run_claude_usage_scheduler`, et le resolver.

---

## 5. Le job

Toutes les **5 minutes**, alignées sur la fenêtre glissante de 5 h que le bloc
affiche : une jauge fausse de cinq minutes sur trois cents ne se voit pas, et
c'est deux ordres de grandeur moins de réveils qu'un suivi à la seconde.

Une passe complète au démarrage de l'API — c'est la seule qui coûte quelque
chose, et elle coûte moins d'une seconde.

Le job **ne bloque jamais le démarrage** et **n'échoue jamais bruyamment** : un
`~/.claude/projects` absent (une autre machine, un autre utilisateur) est un cas
normal, journalisé une fois, pas une erreur. Même principe que le
`SurfaceController` pour le HUD : un budget sans chiffres est un budget sans
chiffres, pas une panne du cockpit.

## 6. GraphQL

```graphql
type NeuralBudgetGql {
  windowHours: Int!
  consumedTokens: Int!                 # input + output + cache_creation (D6)
  cacheReadTokens: Int!                # à part, jamais dans consumedTokens
  declaredCeiling: Int!
  consumedRatio: Float!
  perDay: [Int!]!                      # sparkline, le plus récent en dernier
  perModel: [ModelUsageGql!]!
  topProject: ProjectUsageGql
}
neuralBudget(windowHours: Int! = 5, sparklineDays: Int! = 10): NeuralBudgetGql!
```

La forme suit le contrat déjà figé côté front (`stub-data.ts`), qui a été écrit
pour ça. Le branchement se solde par la suppression de `stub-data.ts`, du badge
`STUB` et de sa règle CSS.

Côté CLI, `aplan usage` imprime le même résumé — le cockpit se pilote au clavier
autant qu'à l'écran, et c'est aussi le moyen le plus simple de vérifier
l'indexeur sans ouvrir le HUD.

## 7. Ce qui est écarté

- **Un crate `hud-daemon` séparé** — §1.2. Un processus de plus à installer et
  superviser pour économiser une seconde de CPU toutes les cinq minutes.
- **Des seaux horaires pré-agrégés** — D1. Ils économiseraient un facteur dix sur
  une table qui n'est pas grosse, au prix de toute question future.
- **Surveiller les fichiers avec `inotify`** — 661 fichiers, des écritures en
  continu : beaucoup de réveils pour une donnée qu'on affiche par tranches de
  cinq heures.
- **Estimer un coût en euros** — la tarification d'un abonnement n'est pas un
  prix au token. Un chiffre inventé sur un écran finit toujours par être cité.

## 8. Tests

- **Domaine** : le parsing sur des lignes réelles anonymisées, dont le cas
  central — deux lignes d'un même `requestId` portant le même `usage` produisent
  **une** contribution. Le worktree replié sur son parent. `<synthetic>` rejeté.
  `iterations` ignoré. `cache_read` hors du total et `thinking` non additionné à
  `output` (D6), chacun sur un jeu où l'erreur inverse se voit : ×36 pour le
  premier, +38 % de la production pour le second.
- **Infrastructure** : SQLite en mémoire ; réindexer deux fois le même fichier
  laisse les totaux inchangés (D2) ; un fichier rétréci depuis le dernier passage
  est relu du début (D3) ; un fichier grossi n'est relu qu'à partir de l'offset.
- **API** : le resolver sur un jeu figé, fenêtre glissante aux bords inclus.
- **Non-régression de la mesure** : un test tient le facteur 1,89 en évidence —
  un jeu où compter les lignes au lieu des requêtes double le total, et qui
  échoue si quelqu'un « simplifie » la déduplication.

## 8 bis. Ce que la première exécution réelle a confirmé

Passe de l'indexeur sur le corpus de la machine, comparée aux mesures Python du §1
(prises quarante minutes plus tôt, d'où l'écart : le corpus grossissait pendant) :

| | Python (§1) | Indexeur |
|---|---|---|
| requêtes | 28 637 | 28 772 |
| consommé | 143 444 896 | 144 408 899 |
| cache lu | 4 989 802 061 | 5 033 498 320 |
| thinking | 6 016 038 | 6 038 939 |

Trois choses valent d'être notées :

- **28 780 écritures pour 28 772 lignes.** Huit requêtes apparaissent dans deux
  fichiers différents — un transcript de sous-agent qui reprend une ligne de son
  parent. La clé primaire les a absorbées sans que rien n'ait eu à les prévoir.
  C'est D2 au travail, sur un cas que personne n'avait anticipé.
- **Une ligne rejetée sur ~56 000.** Le compteur fonctionne, et le taux de départ
  est connu : c'est la ligne de base contre laquelle un changement de format se
  verra.
- **47 % des requêtes sont des sidechains** (13 518 sur 28 772). Les sous-agents,
  comme annoncé, pèsent près de la moitié.

---

## 9. Risques

| Risque | Traitement |
|---|---|
| Le format du transcript change côté Claude Code | Le parsing renvoie `None` plutôt que d'échouer, et le job journalise un taux de rejet. Un budget qui se fige est visible ; un qui ment ne l'est pas. |
| `requestId` cesse d'être unique par appel | La dédup deviendrait fausse dans l'autre sens (sous-comptage). Le test de non-régression du §8 le rattrape au premier corpus réel. |
| Le corpus grossit plus vite que prévu (+45 % en 15 jours) | À 10× le volume actuel la table reste sous 300 k lignes ; la passe complète, elle, passerait de 5,5 s à environ une minute — supportable au démarrage, et de toute façon payée une seule fois puisque l'incrémental (D3) tient le régime permanent à 42 ms. |
| La jauge ment sur son dénominateur | Assumé et affiché — D5. C'est la seule chose que cet index ne peut pas mesurer. |
