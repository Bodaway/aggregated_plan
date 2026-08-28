# HUD aplan — Plan 3 : le système visuel et les six blocs

> **Pour les agents exécutants :** SOUS-SKILL REQUISE — utiliser
> `superpowers:subagent-driven-development` (recommandé) ou
> `superpowers:executing-plans`. Les étapes utilisent la syntaxe case à cocher.

**But :** remplacer la coquille vide du plan 1 par le HUD validé en maquette —
six blocs, barre de destinations, ticker — branchés sur les données réelles.

**Architecture :** un socle CSS portant le système visuel (échelle, panneau,
budget de lueur, reflow), puis un composant par bloc consommant les hooks urql
existants. Aucune nouvelle requête GraphQL : tout ce dont les quatre premiers
blocs ont besoin existe déjà.

**Pile technique :** React 18, Vite 5, Tailwind (tokens `cn-*` du plan 1), urql,
Vitest + React Testing Library, Tauri v2.

**Spec :** `docs/plans/2026-08-27-hud-overlay-tauri-design.md`
**Plan 1 (prérequis, terminé) :** `docs/plans/2026-08-27-hud-overlay-plan-1-coque-tauri.md`
**Maquette validée :** `https://claude.ai/code/artifact/e87945be-cdd9-4b7f-a1f1-c1c930a75baf`

---

## Contraintes globales

Le système visuel, arrêté avec l'utilisateur sur maquette. Chaque tâche y est soumise.

- **La lueur est un budget, pas un effet.** Un seul bloc la porte à la fois. Les
  autres sont en filets d'un pixel, fond translucide flouté, zéro ombre.
- **Le bloc dominant est dynamique** : il bascule selon l'urgence, il n'est pas
  fixé à Focus. C'est la tâche 8 qui l'implémente ; jusque-là Focus domine.
- **Deux emprunts au néon, pas un de plus** : le halo du chiffre héros et la barre
  dégradée cyan→rose. Le rose n'apparaît ailleurs que porteur de sens (échéance du
  jour, jauge au-dessus du seuil, alerte).
- **Échelle proportionnelle au conteneur.** `container-type: size` sur la racine,
  `font-size: clamp(10px, 0.74cqw, 30px)`, **tout le reste en `em`**. Aucune taille
  en pixels sauf filets, bordures et rayons de flou.
- **Reflow sous 1500 px de conteneur** (écran du portable) : Focus pleine largeur,
  les cinq autres par paires.
- **`backdrop-filter` est autorisé** — mesuré à ×1,09 sur cette machine (spec §10.2)
  — et il est **nécessaire** : sans lui, un bureau chargé traverse les panneaux et
  noie le texte.
- **Effets par bloc, jamais en passe plein écran** (14,2 Mpx sur deux dalles).
- **Toute animation passe par `useSurfaceVisibility`** (plan 1). Sans exception.
- Palette **CyberNord** via les utilitaires Tailwind `cn-*` du plan 1. Aucune
  couleur en dur dans un composant.
- `font-variant-numeric: tabular-nums` partout où des chiffres s'alignent.
- Spécifications en français ; code, commentaires **et libellés de tests** en anglais.
- Commits : sujet impératif en français, corps expliquant le *pourquoi*. **Pas de
  `Co-Authored-By` ni `Signed-off-by`.** Ne stager que les fichiers de la tâche ;
  le dépôt contient des `*.db-shm.bak-*` / `*.db-wal.bak-*` non suivis à ne jamais
  stager.

## Une discipline née du plan 1

Onze défauts du plan 1 venaient de code que j'avais écrit sans le vérifier :
syntaxe Hyprland dépréciée, fichier obligatoire omis, appel qui ne typecheck pas.

Donc, dans ce plan : **là où le code touche une API existante, le brief donne le
contrat attendu et l'ordre d'aller lire le fichier réel — pas du code inventé.**
Là où le code est nouveau et autonome (CSS, structure de composant, tests), il est
donné en entier. Un implémenteur qui trouve un écart entre le contrat et la réalité
doit le signaler, pas le contourner.

## Structure des fichiers

| Fichier | Responsabilité |
|---|---|
| `frontend/src/pages/hud/hud.css` | le socle : échelle, panneau, budget de lueur, reflow |
| `frontend/src/pages/hud/HudPage.tsx` | grille, séquence de boot, arbitrage du bloc dominant |
| `frontend/src/pages/hud/HudNav.tsx` | barre de destinations |
| `frontend/src/pages/hud/blocks/FocusBlock.tsx` | tâche active, chrono, quarts, charge, pause |
| `frontend/src/pages/hud/blocks/PressureBlock.tsx` | échéances, alertes, capacité |
| `frontend/src/pages/hud/blocks/AgendaBlock.tsx` | prochaine réunion, timeline du jour |
| `frontend/src/pages/hud/blocks/NeuralBudgetBlock.tsx` | conso Claude — **données factices** |
| `frontend/src/pages/hud/blocks/AgentsBlock.tsx` | sessions actives — **données factices** |
| `frontend/src/pages/hud/blocks/StationBlock.tsx` | horloge, CPU, RAM, réseau |
| `frontend/src/pages/hud/blocks/Ticker.tsx` | bandeau d'alertes |
| `frontend/src/pages/hud/useDominantBlock.ts` | qui porte la lueur |

---

## Tâche 1 : Spike — `AuthGate` laisse-t-il monter `/hud` dans la fenêtre Tauri ? *(bloquant)*

Rien ne sert d'écrire six composants s'ils ne s'affichent jamais.

`AuthGate` enveloppe toute l'application depuis `main.tsx`, **au-dessus de `App`**,
donc `/hud` y est soumis. Les observations du plan 1 se contredisent : une fois un
écran de connexion Microsoft dans la fenêtre, une fois la séquence de boot. Un
agent a par ailleurs constaté que `backend/crates/api/src/main.rs:204` fixe en dur
`access-control-allow-origin: http://localhost:3000`, ce qui ferait échouer toute
requête de session depuis l'origine de Tauri.

**Fichiers :** créer `scripts/hud-bench/authgate_probe.py` ; modifier
`docs/plans/2026-08-27-hud-overlay-tauri-design.md` (nouvelle section §10.5).

**Interfaces :** produit une réponse binaire — *`/hud` monte-t-il de façon fiable
dans la fenêtre Tauri ?* Toutes les tâches suivantes en dépendent.

- [ ] **Étape 1 : lire le terrain**

Lire `frontend/src/components/auth/AuthGate.tsx` et `frontend/src/hooks/use-session.ts`
pour établir : sur quoi la porte statue, ce qu'elle rend pendant le chargement, et
ce qu'elle rend en échec. Consigner les trois réponses dans le rapport.

- [ ] **Étape 2 : reproduire dans la vraie fenêtre**

La fenêtre Tauri pointe actuellement sur une maquette statique
(`/hud-mockup.html`, temporaire et non commitée). La faire pointer sur `/hud`,
recompiler, ouvrir, et déterminer **par capture** ce qui s'affiche.

Méthode imposée — **image de différence, pas seuil de couleur.** Deux méthodes par
seuil se sont contredites sur ce projet, l'arbitrage n'a été possible que par
différence :

1. capturer le moniteur sans le HUD, deux fois, pour établir le plancher de bruit ;
2. lancer le HUD, le révéler, capturer toutes les ~300 ms ;
3. comparer les images du HUD **entre elles**, jamais au bureau nu — l'assombrissement
   du special workspace fausse toute comparaison au bureau.

**Ne pas lancer la compilation Tauri soi-même** : les sous-agents ne maintiennent
pas un processus long au-delà de leur tour. Demander au contrôleur et attendre.

- [ ] **Étape 3 : conclure et consigner**

Écrire §10.5 dans la spec avec la date, la méthode, les chiffres bruts, et
laquelle des trois branches s'applique :

- *La porte laisse passer de façon fiable* → les tâches suivantes procèdent telles quelles.
- *La porte bloque* → **s'arrêter et remonter au contrôleur.** Deux remèdes
  s'opposent — élargir la liste d'origines du backend, ou sortir `/hud` de la porte —
  et l'un affaiblit une frontière d'authentification. Ce n'est pas à l'implémenteur
  de trancher, et le `CLAUDE.md` du dépôt route tout changement d'auth par une revue
  sécurité.
- *Le comportement est intermittent* → le documenter comme tel, avec le taux observé.

- [ ] **Étape 4 : commit**

```bash
git add scripts/hud-bench/authgate_probe.py docs/plans/2026-08-27-hud-overlay-tauri-design.md
git commit -m "Determiner si AuthGate laisse monter /hud dans la fenetre Tauri

Les observations du plan 1 se contredisaient. Six composants ne valent rien
si la porte d'authentification les empeche de monter."
```

---

## Tâche 2 : Le socle visuel

Tout le système dans une feuille, pour qu'aucun bloc ne le réinvente.

**Fichiers :** créer `frontend/src/pages/hud/hud.css` et
`frontend/src/pages/hud/hud.css.test.ts` ; modifier `HudPage.tsx` (import).

**Interfaces :** produit les classes `.hud`, `.hud-panel`, `.hud-panel--lit`,
`.hud-label`, `.hud-kv`, `.hud-gauge`, `.hud-glowbar`, et les zones de grille
nommées. **Les six blocs les consomment ; aucun ne définit sa propre échelle.**

- [ ] **Étape 1 : écrire le test**

Le socle est du CSS, donc le test porte sur son contrat, pas sur son rendu.

`frontend/src/pages/hud/hud.css.test.ts`

```typescript
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect } from 'vitest';

const CSS = readFileSync(resolve(__dirname, 'hud.css'), 'utf8');

describe('HUD visual foundation', () => {
  it('scales with its container, not the viewport', () => {
    expect(CSS).toMatch(/container-type:\s*size/);
    expect(CSS).toMatch(/font-size:\s*clamp\(10px,\s*0\.74cqw,\s*30px\)/);
    expect(CSS).not.toMatch(/\dvw/);
  });

  it('reflows below the laptop-width breakpoint', () => {
    expect(CSS).toMatch(/@container hud \(max-width:\s*1500px\)/);
  });

  it('blurs panel backdrops so a busy desktop cannot drown the text', () => {
    expect(CSS).toMatch(/backdrop-filter:\s*blur/);
  });

  it('spends its glow on the dominant panel only', () => {
    const lit = (CSS.match(/box-shadow:[^;]*rgba\(8,\s*247,\s*254/g) ?? []).length;
    expect(lit).toBeLessThanOrEqual(2);
  });

  it('takes every colour from the CyberNord tokens', () => {
    const hardcoded = CSS.match(/#[0-9a-fA-F]{6}/g) ?? [];
    expect(hardcoded).toEqual([]);
  });
});
```

- [ ] **Étape 2 : lancer le test pour le voir échouer**

`cd frontend && pnpm vitest run src/pages/hud/hud.css.test.ts`
Attendu : ÉCHEC, `ENOENT ... hud.css`.

- [ ] **Étape 3 : écrire le socle**

Transcrire le CSS de la maquette validée en remplaçant **chaque littéral
hexadécimal par la variable `--cn-*` correspondante** (plan 1, tâche 3). La
maquette utilise des hex parce qu'elle est autonome ; ici les tokens existent.

Source de vérité pour la composition : la maquette temporaire
`frontend/public/hud-mockup.html`, encore présente à ce stade. La lire, ne pas
la deviner. Correspondance des couleurs :

| Maquette | Token |
|---|---|
| `#08f7fe` | `var(--cn-teal)` |
| `#ff2e63` | `var(--cn-red)` |
| `#d08fff` | `var(--cn-purple)` |
| `#00ff9c` | `var(--cn-green)` |
| `#ff6e27` | `var(--cn-orange)` |
| `#ebcb8b` | `var(--cn-yellow)` |
| `#d7e9ee`, `#8ba3ac`, `#5d737d` | dérivées de `--cn-fg` / `--cn-dim`, à déclarer en variables locales `--hud-ink*` |

- [ ] **Étape 4 : lancer le test pour le voir passer**

`cd frontend && pnpm vitest run src/pages/hud/hud.css.test.ts` → SUCCÈS, 5 tests.

- [ ] **Étape 5 : commit**

```bash
git add frontend/src/pages/hud/hud.css frontend/src/pages/hud/hud.css.test.ts frontend/src/pages/hud/HudPage.tsx
git commit -m "Poser le socle visuel du HUD

Une seule feuille porte l'echelle, le panneau, le budget de lueur et le
reflow, pour qu'aucun des six blocs ne reinvente le systeme."
```

---

## Tâche 3 : Barre de destinations

Le HUD est l'accueil de l'overlay ; il faut pouvoir en sortir vers le reste de l'app.

**Fichiers :** créer `HudNav.tsx` et `HudNav.test.tsx` ; modifier `HudPage.tsx`.

**Interfaces :** produit `<HudNav />`. Consomme `useLocation` et `useNavigate` de
`react-router-dom` v6.

Les onze destinations réelles, relevées dans `App.tsx` : `/dashboard`, `/triage`,
`/priority`, `/workload`, `/activity`, `/timesheet`, `/worklog`, `/memory`,
`/dedup`, `/alerts`, `/settings`.

- [ ] **Étape 1 : écrire le test**

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { describe, it, expect } from 'vitest';
import { HudNav } from './HudNav';

const at = (path: string) =>
  render(<MemoryRouter initialEntries={[path]}><HudNav /></MemoryRouter>);

describe('HudNav', () => {
  it('lists every destination of the application', () => {
    at('/hud');
    expect(screen.getAllByRole('link').length).toBe(12); // 11 + le HUD lui-même
  });

  it('marks the current view as the one that is lit', () => {
    at('/hud');
    expect(screen.getByRole('link', { name: /hud/i })).toHaveAttribute('aria-current', 'page');
  });

  it('jumps to a destination when its digit is pressed', async () => {
    at('/hud');
    await userEvent.keyboard('1');
    expect(screen.getByRole('link', { name: /dashboard/i })).toHaveAttribute('aria-current', 'page');
  });

  it('ignores the digit when a text field has focus', async () => {
    at('/hud');
    const input = document.createElement('input');
    document.body.appendChild(input);
    input.focus();
    await userEvent.keyboard('1');
    expect(screen.getByRole('link', { name: /hud/i })).toHaveAttribute('aria-current', 'page');
    input.remove();
  });
});
```

Le quatrième cas n'est pas décoratif : un raccourci à une touche qui vole la frappe
d'un champ de saisie est un défaut classique, et le plan 3 ajoutera une capture
rapide plus tard.

- [ ] **Étape 2 : lancer le test pour le voir échouer** — module introuvable.

- [ ] **Étape 3 : écrire le composant**

Structure imposée par la maquette : `▌APLAN` puis les destinations avec leur
chiffre d'accès, puis `Esc ferme` poussé à droite. La destination courante porte
`aria-current="page"` et la classe allumée. Chaque entrée est un vrai lien
(`<Link>`), pas un `<div>` cliquable — le clavier et les lecteurs d'écran en dépendent.

Vérifier avant d'écrire que `@testing-library/user-event` est bien une dépendance
du projet ; sinon le signaler plutôt que de l'ajouter unilatéralement.

- [ ] **Étape 4 : lancer le test pour le voir passer** — 4 tests verts.

- [ ] **Étape 5 : commit**

```bash
git add frontend/src/pages/hud/HudNav.tsx frontend/src/pages/hud/HudNav.test.tsx frontend/src/pages/hud/HudPage.tsx
git commit -m "Ajouter la barre de destinations du HUD

Le HUD est l'accueil de l'overlay : sans elle, on ne peut pas rejoindre le
reste de l'application depuis la fenetre."
```

---

## Tâche 4 : Bloc Focus — le dominant

**Fichiers :** créer `blocks/FocusBlock.tsx` et `blocks/FocusBlock.test.tsx` ;
modifier `HudPage.tsx`.

**Interfaces :** produit `<FocusBlock lit={boolean} />`. La prop `lit` décide qui
porte la lueur — la tâche 8 la pilotera ; ici elle vaut toujours `true`.

**Données — contrat, à confronter au réel.** Le bloc affiche : tâche active et son
chrono, les quatre quarts de la journée et leur remplissage, la charge du jour
contre la capacité, le compte à rebours de la prochaine pause.

Ces données existent déjà. **Lire ces fichiers et employer ce qu'ils exposent :**
`frontend/src/hooks/use-activity.ts`, `use-timesheet.ts`, `use-break-rules.ts`,
`use-dashboard.ts`. Ne pas écrire de nouvelle requête GraphQL.

Si un champ nécessaire manque, **le signaler au contrôleur** : cela peut être un
vrai manque d'API, et c'est une décision de périmètre.

- [ ] **Étape 1 : écrire le test**

Tester le comportement du bloc, pas la plomberie : les hooks sont mockés, mais les
assertions portent sur ce que l'utilisateur voit.

```tsx
import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { FocusBlock } from './FocusBlock';

vi.mock('@/hooks/use-activity', () => ({ /* forme relevée à l'étape de lecture */ }));

describe('FocusBlock', () => {
  it('shows the active task and its running chronometer', () => { /* … */ });
  it('marks the current quarter of the day', () => { /* … */ });
  it('warns when the day load exceeds capacity', () => { /* … */ });
  it('falls back to a readable empty state when no task is active', () => { /* … */ });
  it('carries the lit class only when told to', () => { /* … */ });
});
```

**Les corps sont à écrire par l'implémenteur** une fois la forme réelle des hooks
relevée — c'est délibéré : du code de test inventé contre une API non lue est
précisément ce qui a produit onze défauts au plan 1. Les cinq intitulés, eux, sont
le contrat et ne se négocient pas.

- [ ] **Étape 2 : Red → Green** sur les cinq cas.

- [ ] **Étape 3 : écrire le composant**, en reprenant la structure de la maquette :
libellé, tâche, chrono héros, barre dégradée, quarts, pied à deux colonnes.

- [ ] **Étape 4 : vérifier** `pnpm type-check && pnpm test` — aucune régression.

- [ ] **Étape 5 : commit**

---

## Tâche 5 : Blocs Pression et Agenda

Deux blocs, une seule source : ils consomment le même `dailyDashboard`. Les
séparer en deux tâches ferait payer deux fois la lecture de la même API.

**Fichiers :** créer `blocks/PressureBlock.tsx`, `blocks/AgendaBlock.tsx` et leurs
tests ; modifier `HudPage.tsx`.

**Données — vérifié dans `frontend/src/graphql/queries/dashboard.graphql` :**
`dailyDashboard(date)` expose `tasks { title, deadline, urgency, quadrant }`,
`meetings { title, startTime, endTime, durationHours }`, `alerts { alertType,
severity, message, resolved }` et `weeklyWorkload { capacity, totalPlanned,
totalMeetings, overload }`. Passer par `use-dashboard.ts`.

**Pression** affiche les échéances triées par proximité — celles du jour en rose,
c'est un des rares emplois autorisés de cette couleur — plus la capacité en jauge,
en orange au-delà du seuil.

**Agenda** affiche le compte à rebours de la prochaine réunion, son titre, et une
timeline de la journée où les réunions sont des segments violets et l'instant
présent un repère cyan.

- [ ] **Étape 1 : écrire les tests** — pour chaque bloc : rendu nominal, état vide,
et le cas limite qui compte (échéance dépassée pour Pression ; aucune réunion
restante pour Agenda).
- [ ] **Étape 2 : Red → Green.**
- [ ] **Étape 3 : écrire les deux composants.**
- [ ] **Étape 4 : vérifier et commiter.**

---

## Tâche 6 : Blocs Neural budget et Agents — sur données factices

Ces deux blocs dépendent du crate `hud-daemon`, qui est le **plan 2, non écrit**.
Ils sont construits ici pour leur forme, sur des données factices, derrière une
interface typée que le plan 2 viendra satisfaire.

**Fichiers :** créer `blocks/NeuralBudgetBlock.tsx`, `blocks/AgentsBlock.tsx`,
`blocks/stub-data.ts` et les tests.

**Interfaces — le contrat que le plan 2 devra honorer :**

```typescript
export interface NeuralBudget {
  windowHours: number;        // 5
  consumedRatio: number;      // 0..1 contre le plafond déclaré
  declaredCeiling: number;    // tokens, saisi à la main par l'utilisateur
  perDay: number[];           // sparkline, le plus récent en dernier
  perModel: { model: string; tokens: number }[];
  topProject: { name: string; ratio: number } | null;
}

export interface ActiveAgent {
  sessionName: string;
  taskTitle: string | null;   // null = session non liée
  lastSeenMinutes: number;    // fraîcheur du transcript
}
```

`stub-data.ts` en exporte une instance plausible et **porte en tête un commentaire
disant qu'il disparaît au plan 2.**

Le bloc Neural budget doit rendre visible que le dénominateur est **déclaré à la
main, pas mesuré** — la spec §9 en fait une limite assumée, et une jauge qui ment
sur son plafond sans le dire est un défaut de conception.

- [ ] **Étape 1 : écrire les tests** — rendu depuis le contrat, état vide (aucune
session active), et l'affichage explicite du plafond déclaré.
- [ ] **Étape 2 : Red → Green.**
- [ ] **Étape 3 : écrire les composants et les données factices.**
- [ ] **Étape 4 : vérifier et commiter.**

---

## Tâche 7 : Bloc Poste et Ticker

**Fichiers :** créer `blocks/StationBlock.tsx`, `blocks/Ticker.tsx` et leurs tests ;
modifier `frontend/src-tauri/src/main.rs` et `Cargo.toml`.

**Poste** affiche l'horloge, la date, puis CPU, RAM et réseau. Ces valeurs sont
locales et éphémères : elles ne passent **pas** par GraphQL mais par une commande
Tauri IPC adossée au crate `sysinfo` (spec §5).

Hors de Tauri — l'application dans le navigateur — la commande n'existe pas. Le
bloc doit alors afficher horloge et date seules, sans case vide ni valeur inventée.

**Ticker** est la bande basse. L'utilisateur l'a jugée utile. Elle affiche les
alertes non résolues de `dailyDashboard`, l'adhérence aux pauses, et l'identité de
version. Une alerte critique est le second emploi autorisé du rose.

- [ ] **Étape 1 : écrire les tests**, dont le cas hors-Tauri pour Poste, et le cas
« aucune alerte » pour Ticker — un ticker vide ne doit pas laisser une bande morte.
- [ ] **Étape 2 : Red → Green.**
- [ ] **Étape 3 : écrire la commande Rust et les deux composants.**
- [ ] **Étape 4 :** demander la compilation au contrôleur, puis vérifier.
- [ ] **Étape 5 : commit.**

---

## Tâche 8 : Le bloc dominant devient dynamique

C'est ce qui transforme la lueur en système. Décision explicite de l'utilisateur :
le dominant **bascule selon l'urgence**, il n'est pas figé sur Focus.

**Fichiers :** créer `useDominantBlock.ts` et son test ; modifier `HudPage.tsx`.

**Interface :** `useDominantBlock(): 'focus' | 'pressure' | 'agenda'`.

Règle d'arbitrage, par priorité décroissante :

1. **`agenda`** si une réunion commence dans moins de 10 minutes — c'est la seule
   chose dont rater le moment a un coût irréversible.
2. **`pressure`** si la capacité dépasse 100 %, ou si une échéance du jour est
   encore ouverte après 15 h.
3. **`focus`** sinon.

Un seul bloc est allumé à la fois : c'est l'invariant du système, et le test doit
le vérifier explicitement plutôt que de vérifier chaque branche isolément.

- [ ] **Étape 1 : écrire le test** — une assertion par branche, **plus** un cas qui
affirme qu'exactement un bloc porte la classe allumée quelles que soient les entrées.
- [ ] **Étape 2 : Red → Green.**
- [ ] **Étape 3 : câbler dans `HudPage`.**
- [ ] **Étape 4 : vérifier et commiter.**

---

## Tâche 9 : Séquence de boot recalée, et retrait de la maquette

Deux dettes du plan 1 se règlent ici.

**La séquence de boot démarre au montage** et dure 1500 ms, alors que la fenêtre
naît sur un special workspace masqué. En usage réel elle se joue en coulisses et
l'utilisateur ne la voit jamais — le défaut a été identifié et reporté ici. Elle
doit démarrer à la **visibilité** de la surface, ce que `useSurfaceVisibility`
fournit déjà.

Question de conception que la revue du plan 1 a laissée ouverte, et qu'il faut
trancher ici : rejouer la séquence à **chaque** ouverture, ou **une seule fois**
par processus ? Rejouer coûte 1,5 s à chaque `SUPER+B`, ce qui séduit trois fois
puis agace. **Décision : une seule fois par processus**, et l'implémenteur doit
signaler s'il pense l'inverse après l'avoir vue à l'usage.

**La maquette temporaire** (`frontend/public/hud-mockup.html` et l'URL détournée
dans `tauri.conf.json`) doit disparaître : elle est remplacée par du vrai code.
Vérifier que `tauri.conf.json` pointe de nouveau sur `/hud`.

**Fichiers :** modifier `HudPage.tsx`, `HudPage.test.tsx`,
`frontend/src-tauri/tauri.conf.json` ; supprimer `frontend/public/hud-mockup.html`.

- [ ] **Étape 1 : écrire les tests** — la séquence ne démarre pas tant que la
surface est masquée ; elle démarre à la première visibilité ; elle ne rejoue pas
à la seconde.
- [ ] **Étape 2 : Red → Green.**
- [ ] **Étape 3 : retirer la maquette et restaurer l'URL.**
- [ ] **Étape 4 :** demander la compilation, puis **vérification finale à l'écran**
par image de différence, sur les deux moniteurs — le 4K en disposition large, le
portable en disposition reflowée.
- [ ] **Étape 5 : commit.**

---

## Hors périmètre

- **Plan 2** — `hud-daemon`, index des transcripts Claude, resolvers GraphQL. Il
  remplacera `stub-data.ts` et rien d'autre.
- **Plan 4** — module waybar pour la veille permanente.
- La vraie icône de fenêtre, encore un placeholder du plan 1.
- Le durcissement CSP, révoqué au plan 1 faute de politique calée sur le protocole
  d'assets de Tauri.
- Toute capture rapide, palette de commandes ou interaction d'écriture depuis le
  HUD : cette version est un afficheur, plus la navigation.

## Définition de terminé

`SUPER+B` ouvre le HUD sur les deux écrans avec sa disposition adaptée à chacun.
Les six blocs affichent des données réelles, sauf Neural budget et Agents qui
affichent des données factices derrière l'interface que le plan 2 honorera. Un
seul bloc porte la lueur, et il change selon l'urgence. La barre de destinations
mène aux onze vues de l'application. La séquence de boot se joue à la première
ouverture visible, une fois. `pnpm type-check` est propre et les 306 tests
existants passent toujours, augmentés de ceux de ce plan.
