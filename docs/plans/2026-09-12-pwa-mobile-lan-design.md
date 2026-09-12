# Exposition réseau et PWA mobile — design

*2026-09-12 — branche `feat/pwa-lan`*

## Problème

Le cockpit n'est joignable que depuis la machine. L'API bind `127.0.0.1:3001` en dur
(`api/src/main.rs`), et en production **personne ne sert le frontend** : seul le dev-server
Vite tourne, sur `[::1]:3000`. Une tâche qui naît en réunion ou dans le métro n'a donc aucun
chemin vers le cockpit avant le retour au poste, et elle se perd.

L'objectif est double : joindre aplan depuis un iPhone 12, et pouvoir capter une tâche même
sans réseau.

## Ce qui interdit le changement de bind naïf

`graphql_handler` **n'authentifie rien par requête** : tout résout contre un
`default_user_id` codé en dur (`api/src/state.rs`). `security.rs` le formule déjà sans
détour — *reachability equals authority*. Passer le bind à `0.0.0.0` publierait donc un
cockpit sans mot de passe sur le LAN, avec `updateConfiguration` (sans allow-list de clés) et
`triggerSync` à portée de n'importe quel appareil du réseau.

La seule défense navigateur actuelle est l'en-tête non-safelisté `x-aplan-client`, qui force
un préflight CORS, plus l'allow-list d'origines. Il n'y a ni validation du `Host`, ni TLS.

## Topologie retenue

```
iPhone 12 ──WireGuard──▶ tailnet ──▶ tailscaled (poste)
                                        │  tailscale serve
                                        │  TLS Let's Encrypt pour <machine>.<tailnet>.ts.net
                                        ▼
                                  127.0.0.1:3001   ← bind inchangé
                                  ├─ POST /graphql (API existante)
                                  └─ GET  /*       (ServeDir → frontend/dist)
```

Le bind **ne change pas**. Aucun port n'est ouvert sur `192.168.1.12`, aucune règle nft,
aucun certificat à installer sur l'iPhone. `tailscale serve` écoute sur l'interface tailnet
et proxie vers le loopback.

`serve`, **jamais `funnel`** : `funnel` publierait le cockpit sur l'Internet public. La
distinction est la frontière de sécurité de tout ce document.

### Conséquence : l'API sert le frontend

Mettre la page et `/graphql` derrière la **même origine** (scheme + host + port identiques)
évite d'ajouter l'origine `.ts.net` à l'allow-list CORS, et laisse `x-aplan-client`
fonctionner sans modification — un script same-origin pose l'en-tête librement, une page
tierce ne le peut toujours pas sans passer le préflight.

Coût : la feature `fs` de `tower-http` (absente aujourd'hui), un `ServeDir` avec fallback SPA
sur `index.html`, et une variable `APLAN_STATIC_DIR`. Le répertoire n'est **pas** obligatoire :
s'il est absent ou non configuré, le routeur se monte sans service statique et l'API se
comporte exactement comme aujourd'hui. Le HUD Tauri, qui charge son frontend par le
protocole d'asset `tauri://localhost`, n'est pas concerné.

## Périmètre mobile

Route `/m`, arbre séparé. Les 12 pages desktop ne sont pas touchées : pas de responsive
rétroactif sur `DashboardPage` (29 Ko) ni `SettingsPage` (33 Ko), dont la densité ne survit
pas à 390 px et dont la reprise coûterait plus que les deux écrans ci-dessous réunis.

- **`/m` — Plan du jour.** En retard / aujourd'hui / demain, plus la tâche active. Lecture
  seule. C'est le brief matinal au format pouce.
- **`/m/new` — Capture.** Titre, échéance optionnelle, projet optionnel. Le champ titre prend
  le focus à l'ouverture. C'est la cible de `start_url` du manifeste : l'icône de l'écran
  d'accueil tombe directement sur la capture, qui est l'usage où le téléphone bat le poste.

Contraintes d'écran : `viewport-fit=cover` et `env(safe-area-inset-*)` pour l'encoche et la
barre home (390 × 844), cibles tactiles ≥ 44 px, jamais le survol comme seule affordance.

Pas de redirection automatique depuis `/` selon la largeur : on choisit son URL, le
comportement reste prévisible.

## File de capture hors-ligne

C'est la seule raison pour laquelle une PWA bat un marque-page, et la seule partie qui touche
le schéma.

### Pourquoi une clé d'idempotence

`CreateTaskInput` ne porte **aucun identifiant fourni par le client**. Si le téléphone envoie
une capture, que le serveur la commit, et que la réponse se perd, le rejeu crée un doublon.
Une file de rejeu sans clé d'idempotence est une machine à doublons — dans une base qui
compte déjà ~550 tâches largement polluées de doublons de test.

Migration `023` : `tasks.client_request_id TEXT UNIQUE` (nullable). Champ optionnel
`clientRequestId: ID` sur `CreateTaskInput`. L'insert, **sur conflit de clé, renvoie la tâche
existante** au lieu d'échouer : le rejeu devient un no-op observable, pas une erreur que le
client devrait interpréter. Le desktop n'envoie pas le champ et garde le chemin actuel — la
colonne reste `NULL` pour tout ce qui n'est pas une capture mobile.

### Mécanique client

La file vit dans IndexedDB ; chaque entrée porte son UUID client, généré au moment de la
saisie et non au moment de l'envoi (sinon deux envois de la même saisie porteraient deux
clés). Le precache de la coquille passe par `vite-plugin-pwa` / Workbox.

**Limite à connaître, pas à découvrir :** iOS Safari n'implémente pas la Background Sync API.
Une capture faite sans réseau ne part donc pas d'elle-même en arrière-plan — elle part à la
**prochaine ouverture de l'app** (flush au démarrage, plus l'événement `online`). Un badge
« n en attente » reste visible en permanence pour que ce délai ne soit jamais une surprise.

Le plan du jour n'est **pas** mis en cache. Hors ligne il affiche sa dernière version connue,
horodatée explicitement, plutôt que de faire passer un plan périmé pour le plan du jour.

## Sécurité : ce qui est couvert, ce qui ne l'est pas

Couvert : rien n'est joignable hors du tailnet, le transport est chiffré de bout en bout
(WireGuard puis TLS), et le certificat est un vrai certificat Let's Encrypt — donc contexte
sécurisé natif, sans profil de CA à installer et approuver sur l'iPhone.

**Risque accepté, décidé explicitement :** l'autorité sur le cockpit est l'appartenance au
tailnet plus le verrouillage natif de l'iPhone. Il n'y a **pas** d'authentification
applicative. Trois durcissements ont été proposés et écartés :

- l'allow-list des clés de `updateConfiguration` (qui laisse pointer `gryzzly.base_url` ou
  `jira.base_url` vers un hôte arbitraire, puis exfiltrer un token via `triggerSync`) ;
- un verrou applicatif par passkey ;
- la validation du `Host`, qui fermerait la variante DNS-rebinding que `security.rs`
  documente déjà comme non couverte.

Conséquence concrète : un appareil du tailnet compromis, ou un iPhone perdu déverrouillé, a
autorité pleine sur le cockpit. C'est un arbitrage assumé, pas un oubli — et il change si le
tailnet accueille un jour un appareil qui n'est pas celui de l'utilisateur.

## Prérequis manuels

Interactifs, hors périmètre du code :

1. installer `tailscale` (absent de la machine) et `tailscale up` ;
2. activer **MagicDNS** et **HTTPS Certificates** dans la console d'admin du tailnet — sans
   ce réglage, `tailscale serve` n'obtient pas de certificat, il n'y a pas de contexte
   sécurisé, et donc **ni service worker ni PWA installable** ;
3. installer Tailscale sur l'iPhone et rejoindre le tailnet.

## Tests

TDD, à l'image du reste du dépôt.

- **Domaine / repo** : rejouer deux fois la même `client_request_id` ne crée qu'une tâche et
  renvoie la même ; une insertion sans clé reste possible et laisse la colonne `NULL`.
- **Routeur** : le fallback SPA sert `index.html` sur une route inconnue, `POST /graphql`
  continue d'exiger `x-aplan-client` (non-régression du garde CSRF), et l'absence de
  `APLAN_STATIC_DIR` laisse le routeur identique à l'actuel.
- **Vitest** : un envoi qui échoue met l'entrée en file, un retour de réseau la vide, et un
  double flush ne produit qu'une tâche ; rendu des deux écrans.
- **Playwright** : viewport iPhone 12, contre une base jetable via `APLAN_E2E_GRAPHQL_URL` —
  jamais contre `aggregated_plan.db`.

## Hors périmètre

Le worklog et start/stop sur mobile, les dix autres pages du cockpit, et toute
authentification applicative.
