# Reprise — brancher la lecture de la mémoire par Claude Code

Colle ce prompt tel quel dans une nouvelle session, depuis `/home/mbt/appfactory/aggregated_plan`.

---

Dans le dépôt `aggregated_plan`, la couche mémoire d'aplan écrit tous les jours mais n'est jamais
lue. Je veux qu'on branche la **lecture**. Voici ce qui est déjà établi (état au 18/08/2026) — ne le
re-démontre pas, vérifie seulement ce qui a pu bouger depuis.

## Ce qui marche déjà : l'écriture

- Le timer systemd utilisateur `aplan-consolidate.timer` lance chaque jour à 17h30 une **session
  Claude Code non interactive** : `claude -p "$(cat docs/prompts/consolidation-memoire.md)"`.
  Dernier passage constaté : 17/08 17:31:30, `Result=success`.
- 15 sessions planifiées repérées dans les transcripts depuis le 04/08, et **44 mémoires
  `source=claude_session`** réparties sur 8 jours (~5 par passage, le plafond du prompt).
- La boucle complète tourne : `brief --json` → `consolidate pending` → dédoublonnage
  `recall --q "<vraie question métier>" --history` → `remember --kind fact|decision` →
  `consolidate mark` → `record-run`. **502 des 529** entrées de worklog portent un `consolidated_at`.
- 6 mémoires de plus viennent de l'import one-shot des fichiers du harness
  (`source=manual`, `source_ref=memory-file:*`).

## Ce qui ne marche pas : la lecture

Sur **148 invocations réelles** des verbes mémoire (extraites des `tool_use` Bash de 353
transcripts, 23 sessions) : **138 se produisent dans le dépôt `aggregated_plan` lui-même**, celui où
la fonctionnalité a été développée. Runs planifiés retirés, il ne reste que du développement et du
test (grep dans `SKILL.md`, UUID sentinelle `7f3a1c22-0000-…`, appels `--help`). Les 2 seuls appels
interactifs hors dépôt (13/08, session `2a96f455`) vérifiaient le pipeline de consolidation
lui-même. **Aucune session n'a consulté la mémoire pour éclairer un travail sans rapport.**

Deux causes structurelles, pas un bug :

1. **Rien ne déclenche une lecture.** `~/.claude/hooks/aplan-session-start.sh` (220 lignes) n'injecte
   que les consignes de rattachement de tâche — pas le brief, pas la mémoire. Ni le `CLAUDE.md`
   global ni le skill `aplan` ne demandent un `recall` avant de travailler. Le timer
   `aplan-brief.timer` (~08h30) envoie le brief à l'humain par `notify-send`, pas dans une session.
2. **La file de validation affame le recall.** 40 des 44 mémoires écrites par Claude sont `pending`,
   or le filtre dur du recall est `invalidated_at IS NULL AND status = 'active'`. Une session qui
   lirait n'atteindrait que **9 mémoires actives sur 50** — et les 4 actives d'origine Claude portent
   sur le projet aplan lui-même (migration 012, horaire de consolidation, bug EOD), pas sur de la
   connaissance métier.

## Ce qui a changé depuis

L'onglet **Memory** existe désormais dans l'app web (`/memory`) : bandeau de brief, recherche
`recall` avec bascule d'historique, file de validation avec les 4 verdicts et l'arbitrage des
quasi-doublons, panneau d'import, et capture d'une sélection de texte en mémoire depuis le dashboard.
Vider la file ne passe donc plus obligatoirement par le CLI.
Fichiers : `frontend/src/pages/MemoryPage.tsx`, `frontend/src/components/memory/*`,
`frontend/src/hooks/use-memory.ts`, `frontend/src/lib/memory/*`. Spec : US-097 de
`SPEC_FONCTIONNELLE.md`, plus les sections `MemoryPage` / `SelectionToMemory` /
`Memory stacking layers` de `SPEC_TECHNIQUE.md`.

## Ce que j'attends de cette session

**Commence par un brainstorming, pas par du code.** Le sujet est un arbitrage de conception, et
plusieurs options s'excluent. Présente-moi les pistes avec leurs coûts avant d'implémenter quoi que
ce soit.

Le cœur de la question : **quel déclencheur de lecture, et qu'est-ce qu'il injecte ?** Pistes à
peser (liste non close) :

- étendre le hook `SessionStart` pour injecter `aplan brief` — simple, mais coûte des tokens à
  *chaque* session, y compris celles qui n'ont rien à voir ;
- injecter un `recall` amorcé par le titre de la tâche à laquelle la session se rattache — plus
  pertinent, mais ne marche que si la session est rattachée ;
- un hook `UserPromptSubmit` qui ne recall que sur certains signaux — plus chirurgical, plus
  fragile ;
- une consigne dans le skill `aplan` ou le `CLAUDE.md` global, en laissant le modèle décider — le
  moins cher, le moins fiable.

Questions à trancher avec moi, pas à supposer : le budget de tokens acceptable par session, ce qui
arrive quand la mémoire injectée est périmée, et si une mémoire `pending` doit être visible d'une
session (aujourd'hui non, par choix : « recaller une décision périmée est le pire échec »).

Sujet lié, ma décision, à me reposer : la consolidation pourrait écrire les `fact` en `--confirm`
(pas de file) et garder la file pour les `decision`. Ça vide le goulot à la source mais retire le
garde-fou humain sur les trois quarts du volume.

## Instruments de mesure à réutiliser

Pour re-mesurer si des lectures se produisent vraiment, après changement — c'est le seul moyen de
savoir si ça a marché :

```bash
# invocations réelles des verbes mémoire, par projet (tool_use Bash, hors session courante)
cd /home/mbt/.claude/projects && python3 - <<'PY'
import json, os, glob, collections, re
pat = re.compile(r'^\s*aplan\s+(recall|brief|remember|inbox|memory|consolidate)\b')
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

```bash
# état du magasin (lecture seule ; SQL autorisé pour l'agrégation, le CLI ne sait pas agréger)
sqlite3 backend/aggregated_plan.db "SELECT source, status, COUNT(*) FROM memories GROUP BY 1,2;"
sqlite3 backend/aggregated_plan.db "SELECT COUNT(*) FROM memories WHERE status='active' AND invalidated_at IS NULL;"
journalctl --user -u aplan-consolidate.service -n 40 --no-pager -o cat   # compte rendu du dernier passage
systemctl --user list-timers --no-pager | rg aplan
```

## Garde-fous

- **Ne déplace jamais le pointeur `aplan.active_task_id`** : c'est l'humain travaillant à la main. Une
  session ne touche que sa propre ligne (`aplan session bind`). Le skill `aplan` documente pourquoi
  (4h35 déjà réattribuées à tort par un agent).
- **Ne lance pas `aplan consolidate mark` / `record-run`** hors d'un passage de consolidation :
  marquer une entrée la rend invisible au passage suivant, sans retour.
- Le magasin contient de **vraies** données. Toute sonde d'écriture doit être nettoyée ; le tri des
  candidats existants est une décision de l'utilisateur, pas la tienne.

## Frictions connues, corrigibles au passage si tu veux

- `recall --project` n'est **pas** un filtre mais un bonus d'entité dans le score (1.309 → 1.609 sur
  la mémoire rattachée) : `RecallQuery` ne porte aucun champ projet, seul `RecallContext`. L'aide CLI
  dit « Restrict the search context to a project », ce qui se lit comme un filtre.
- Un `--project` introuvable renvoie `error: no task matches <token>` — le mot « task » alors que la
  résolution porte sur les projets (`crates/cli/src/memory_cmd.rs:58`).
- La charge JSON de `recall --q` n'expose pas `supersededBy` (9 champs) là où `recall <id>` en expose
  16 : un Claude voit `invalidatedAt` sans savoir par quoi la mémoire a été remplacée.
