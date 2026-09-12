# Exposition réseau et PWA mobile — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rendre le cockpit joignable depuis un iPhone 12 via un tunnel Tailscale, et livrer une PWA à deux écrans (plan du jour, capture) dont la capture survit à l'absence de réseau.

**Architecture:** Le bind `127.0.0.1:3001` ne change pas ; `tailscale serve` termine le TLS sur le tailnet et proxie vers le loopback. L'API se met à servir `frontend/dist`, ce qui met la page et `/graphql` sur la même origine et laisse le garde CSRF `x-aplan-client` intact. La file de capture hors-ligne s'adosse à une clé d'idempotence `client_request_id` pour qu'un rejeu ne crée jamais de doublon.

**Tech Stack:** Rust / Axum 0.7 / tower-http `fs` / sqlx 0.8 / async-graphql 7 ; React 18 / Vite 5 / urql 4 / vite-plugin-pwa (Workbox) / IndexedDB ; Vitest, Playwright, `#[tokio::test]`.

**Spec:** `docs/plans/2026-09-12-pwa-mobile-lan-design.md`

## Global Constraints

- Séparation DDD stricte : `domain` sans I/O, les traits dans `application`, les implémentations dans `infrastructure`, Axum et async-graphql dans `api`.
- TDD : le test échoue d'abord, toujours (Red → Green → Refactor).
- Aucun `.unwrap()` en production ; `sqlx::Error` → `RepositoryError::Database(e.to_string())`.
- Les repos infrastructure utilisent des requêtes runtime (`sqlx::query`), jamais `sqlx::query!`.
- **Jamais exercer l'API contre `127.0.0.1:3001`** : c'est la vraie base. Base jetable uniquement (`DATABASE_URL=sqlite:///tmp/aplan-dev.db`), et arrêter `aplan-api.service` avant, le port 3001 étant codé en dur.
- Playwright ne tourne que si `APLAN_E2E_GRAPHQL_URL` est défini et pointe une instance jetable.
- Les specs `SPEC_FONCTIONNELLE.md` / `SPEC_TECHNIQUE.md` sont en français et se mettent à jour dans le même commit que le comportement documenté.
- Fichiers : `snake_case.rs` côté Rust, `kebab-case.tsx` côté frontend.
- Cibles tactiles ≥ 44 px ; `viewport-fit=cover` + `env(safe-area-inset-*)` (iPhone 12 : 390 × 844).
- `tailscale serve`, **jamais `tailscale funnel`**.
- Le mot « tailnet » et le nom d'hôte `.ts.net` ne sont jamais codés en dur dans le frontend : tout est relatif à l'origine.

---

### Task 1 : l'API sert le frontend (origine unique)

Sans ça, aucune page n'est atteignable depuis le tunnel — c'est le prérequis de tout le reste.

**Files:**
- Modify: `backend/crates/api/Cargo.toml` (feature `fs` de `tower-http`)
- Modify: `backend/crates/api/src/main.rs` (`build_router`, lecture de `APLAN_STATIC_DIR`)
- Test: `backend/crates/api/src/main.rs` (module `#[cfg(test)]` à ajouter en fin de fichier)

**Interfaces:**
- Consomme : `build_router(state: state::AppState) -> Router` tel qu'il existe.
- Produit : `build_router(state: AppState, static_dir: Option<PathBuf>) -> Router`. Les appels existants dans `security.rs` (4 tests) passent `None` et doivent continuer à passer sans autre changement.

- [ ] **Step 1 : écrire les tests qui échouent**

Dans `main.rs`, un module de test qui réutilise le `test_app_state()` de `security.rs` (le rendre `pub(crate)` et l'exposer depuis `security::tests`, ou le déplacer dans un module `test_support`) :

```rust
#[tokio::test]
async fn serves_index_for_unknown_route_when_static_dir_set() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "<!doctype html>APP").unwrap();
    let req = HttpRequest::get("/m/new").body(Body::empty()).unwrap();
    let res = build_router(test_app_state().await, Some(dir.path().to_path_buf()))
        .oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    // Le fallback SPA doit rendre index.html, pas un 404 : sans ça un
    // rechargement sur /m/new casse la PWA.
    assert_eq!(body, b"<!doctype html>APP".as_ref());
}

#[tokio::test]
async fn graphql_still_requires_csrf_header_with_static_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "APP").unwrap();
    let req = HttpRequest::post("/graphql")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"query":"{ __typename }"}"#)).unwrap();
    let res = build_router(test_app_state().await, Some(dir.path().to_path_buf()))
        .oneshot(req).await.unwrap();
    // Le service statique ne doit jamais avaler /graphql ni court-circuiter
    // le garde CSRF.
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn unknown_route_is_404_without_static_dir() {
    let req = HttpRequest::get("/m").body(Body::empty()).unwrap();
    let res = build_router(test_app_state().await, None).oneshot(req).await.unwrap();
    // Sans APLAN_STATIC_DIR le routeur reste rigoureusement l'actuel.
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
```

Ajouter `tempfile = "3"` aux `[dev-dependencies]` de `backend/crates/api/Cargo.toml`.

- [ ] **Step 2 : lancer les tests et vérifier l'échec**

Run: `cd backend && cargo test -p api 2>&1 | tail -20`
Expected: échec de compilation — `build_router` prend un seul argument.

- [ ] **Step 3 : implémenter**

`Cargo.toml` : `tower-http = { version = "0.6", features = ["cors", "trace", "fs"] }`.

Dans `build_router`, après le `.layer(...)` CORS et avant `.with_state(state)` — le `fallback_service` ne s'applique qu'aux routes non résolues, donc `/graphql` et les routes OAuth gardent la priorité :

```rust
pub(crate) fn build_router(state: state::AppState, static_dir: Option<PathBuf>) -> Router {
    // ... routes existantes inchangées ...
    let app = app.layer(/* CorsLayer existant, inchangé */).layer(TraceLayer::new_for_http());
    // Le service statique est monté en `fallback_service` : il ne voit que ce
    // qu'aucune route n'a résolu, donc il ne peut ni masquer `/graphql` ni
    // court-circuiter `require_csrf_header`. Absent, le routeur est
    // rigoureusement celui d'avant -- c'est ce que garantit
    // `unknown_route_is_404_without_static_dir`.
    let app = match static_dir {
        Some(dir) => {
            // `not_found_service` sert index.html pour toute route inconnue :
            // sans ce fallback, un rechargement sur /m/new renvoie 404 et la
            // PWA est cassée dès qu'on la relance depuis l'écran d'accueil.
            let index = ServeFile::new(dir.join("index.html"));
            app.fallback_service(ServeDir::new(dir).not_found_service(index))
        }
        None => app,
    };
    app.with_state(state)
}
```

Imports : `use std::path::PathBuf;` et `use tower_http::services::{ServeDir, ServeFile};`.

Dans `main()`, à l'appel :

```rust
// Répertoire du frontend compilé. Non configuré => l'API se comporte
// exactement comme avant : le service statique n'existe que pour le tunnel.
let static_dir = std::env::var("APLAN_STATIC_DIR").ok().map(PathBuf::from);
let app = build_router(state, static_dir);
```

Mettre à jour les 4 appels de `security.rs` en `crate::build_router(test_app_state().await, None)`.

- [ ] **Step 4 : vérifier que les tests passent**

Run: `cd backend && cargo test -p api 2>&1 | tail -15`
Expected: tous verts, dont les 4 tests CSRF existants.

- [ ] **Step 5 : commit**

```bash
/usr/bin/git add backend/crates/api/Cargo.toml backend/crates/api/src/main.rs backend/crates/api/src/security.rs
/usr/bin/git commit -m "Servir le frontend compilé depuis l'API"
```

---

### Task 2 : clé d'idempotence sur la création de tâche

**Files:**
- Create: `migrations/sqlite/023_add_task_client_request_id.sql`
- Modify: `backend/crates/domain/src/types/task.rs` (champ `client_request_id`)
- Modify: `backend/crates/application/src/repositories/task_repository.rs` (méthode de trait)
- Modify: `backend/crates/application/src/use_cases/task_management.rs` (`CreateTaskInput`, `create_personal_task`)
- Modify: `backend/crates/infrastructure/src/database/task_repo.rs` (INSERT, SELECT, mapping de ligne)
- Modify: `backend/crates/api/src/graphql/types/task.rs` ou le module qui porte `CreateTaskInput` GraphQL, et `backend/crates/api/src/graphql/mutation.rs` (`convert_create_input`)

**Interfaces:**
- Consomme : `create_personal_task(task_repo, user_id, input, today) -> Result<Task, AppError>` ; `task_repo.save(&task)`.
- Produit :
  - `Task.client_request_id: Option<String>`
  - `TaskRepository::find_by_client_request_id(&self, user_id: UserId, client_request_id: &str) -> Result<Option<Task>, RepositoryError>`
  - `application::use_cases::task_management::CreateTaskInput.client_request_id: Option<String>`
  - champ GraphQL `clientRequestId: String` sur `CreateTaskInput` (optionnel)

- [ ] **Step 1 : écrire le test qui échoue (application)**

Dans le module de tests de `task_management.rs`, avec le double de `TaskRepository` déjà utilisé par les tests voisins :

```rust
#[tokio::test]
async fn same_client_request_id_creates_only_one_task() {
    let repo = InMemoryTaskRepo::new();
    let user = Uuid::new_v4();
    let today = NaiveDate::from_ymd_opt(2026, 9, 12).unwrap();
    let input = || CreateTaskInput {
        title: "Capté dans le métro".into(),
        client_request_id: Some("6f1e1d6e-0000-4000-8000-000000000001".into()),
        ..CreateTaskInput::default()
    };

    let first = create_personal_task(&repo, user, input(), today).await.unwrap();
    let second = create_personal_task(&repo, user, input(), today).await.unwrap();

    // Le rejeu doit être un no-op observable : même tâche, pas une erreur que
    // le client aurait à interpréter, et surtout pas un doublon.
    assert_eq!(first.id, second.id);
    assert_eq!(repo.count().await, 1);
}

#[tokio::test]
async fn absent_client_request_id_still_creates_each_time() {
    let repo = InMemoryTaskRepo::new();
    let user = Uuid::new_v4();
    let today = NaiveDate::from_ymd_opt(2026, 9, 12).unwrap();
    let mk = || CreateTaskInput { title: "Sans clé".into(), ..CreateTaskInput::default() };

    let a = create_personal_task(&repo, user, mk(), today).await.unwrap();
    let b = create_personal_task(&repo, user, mk(), today).await.unwrap();

    // Le chemin desktop ne change pas de comportement.
    assert_ne!(a.id, b.id);
    assert_eq!(repo.count().await, 2);
}
```

Si `CreateTaskInput` n'a pas de `Default`, en dériver un ou écrire les champs en toutes lettres dans le test.

- [ ] **Step 2 : lancer et vérifier l'échec**

Run: `cd backend && cargo test -p application same_client_request_id 2>&1 | tail -15`
Expected: échec de compilation — le champ `client_request_id` n'existe pas.

- [ ] **Step 3 : migration**

`migrations/sqlite/023_add_task_client_request_id.sql` :

```sql
-- Clé d'idempotence de la capture mobile hors-ligne.
--
-- La file de capture de la PWA rejoue un envoi dont la réponse s'est perdue.
-- Sans cette clé, un rejeu après un commit réussi côté serveur crée un
-- doublon -- dans une base qui en compte déjà beaucoup. La colonne est
-- NULLable et l'index est partiel : le chemin desktop n'envoie rien et
-- plusieurs tâches peuvent donc coexister avec la valeur NULL, ce qu'un
-- UNIQUE nu autoriserait aussi en SQLite mais que l'index partiel rend
-- explicite et moins cher.
ALTER TABLE tasks ADD COLUMN client_request_id TEXT;

CREATE UNIQUE INDEX idx_tasks_client_request_id
    ON tasks (user_id, client_request_id)
    WHERE client_request_id IS NOT NULL;
```

- [ ] **Step 4 : domaine, application, infrastructure**

`domain/src/types/task.rs` — ajouter avant `created_at` :

```rust
    /// Clé d'idempotence fournie par le client (capture mobile hors-ligne).
    /// `None` pour tout ce qui ne vient pas de la file de la PWA.
    pub client_request_id: Option<String>,
```

Corriger chaque construction littérale de `Task` dans le workspace (`cargo check` les liste).

`application/src/repositories/task_repository.rs` — ajouter au trait :

```rust
    /// Retrouve une tâche par la clé d'idempotence du client, si elle existe.
    async fn find_by_client_request_id(
        &self,
        user_id: UserId,
        client_request_id: &str,
    ) -> Result<Option<Task>, RepositoryError>;
```

`application/src/use_cases/task_management.rs` — champ `pub client_request_id: Option<String>` sur `CreateTaskInput`, puis en tête de `create_personal_task` :

```rust
    // Rejeu de la file hors-ligne : si la clé est déjà connue, on rend la
    // tâche existante. L'index UNIQUE reste le garde-fou d'une vraie course ;
    // ce chemin-ci évite juste d'y arriver dans le cas courant.
    if let Some(key) = input.client_request_id.as_deref() {
        if let Some(existing) = task_repo.find_by_client_request_id(user_id, key).await? {
            return Ok(existing);
        }
    }
```

et `client_request_id: input.client_request_id.clone()` dans le littéral `Task`.

`infrastructure/src/database/task_repo.rs` — ajouter `client_request_id` à l'INSERT/UPSERT de `save`, au SELECT de chaque lecture et au mapping de ligne, puis implémenter :

```rust
    async fn find_by_client_request_id(
        &self,
        user_id: UserId,
        client_request_id: &str,
    ) -> Result<Option<Task>, RepositoryError> {
        let row = sqlx::query("SELECT * FROM tasks WHERE user_id = ? AND client_request_id = ?")
            .bind(user_id.to_string())
            .bind(client_request_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        row.map(|r| self.row_to_task(r)).transpose()
    }
```

(adapter au nom réel du helper de mapping du fichier, et charger les tags comme le font les autres lectures).

- [ ] **Step 5 : vérifier que les tests application passent**

Run: `cd backend && cargo test -p application 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 6 : test d'intégration infrastructure**

Dans le module de tests de `task_repo.rs`, sur SQLite en mémoire :

```rust
#[tokio::test]
async fn find_by_client_request_id_round_trips() {
    let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
    let repo = SqliteTaskRepository::new(pool);
    let user = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let mut task = sample_task(user);
    task.client_request_id = Some("clé-1".into());
    repo.save(&task).await.unwrap();

    let found = repo.find_by_client_request_id(user, "clé-1").await.unwrap();
    assert_eq!(found.map(|t| t.id), Some(task.id));
    // Une clé inconnue ne doit rien renvoyer, pas la première ligne venue.
    assert!(repo.find_by_client_request_id(user, "clé-2").await.unwrap().is_none());
}
```

Run: `cd backend && cargo test -p infrastructure client_request_id 2>&1 | tail -10`

- [ ] **Step 7 : exposer en GraphQL**

Ajouter `pub client_request_id: Option<String>` au `CreateTaskInput` d'async-graphql (doc-comment : « Clé d'idempotence de la capture mobile hors-ligne ; ignorer côté desktop ») et le propager dans `convert_create_input` (`mutation.rs:50`).

Run: `cd backend && cargo test -p api 2>&1 | tail -10`

- [ ] **Step 8 : régénérer le SDL — sur une base jetable**

⚠️ `export-schema` construit le pool **avant** d'imprimer le SDL : lancé sans précaution, il applique la migration 023 à la vraie base.

```bash
cd backend
DATABASE_URL=sqlite:///tmp/aplan-schema.db cargo run -p api -- export-schema > crates/cli/graphql/schema.graphql
```

- [ ] **Step 9 : commit**

```bash
/usr/bin/git add migrations/sqlite/023_add_task_client_request_id.sql backend/crates
/usr/bin/git commit -m "Doter la création de tâche d'une clé d'idempotence"
```

---

### Task 3 : origine relative et proxy de dev

Prérequis du shell mobile : tant que le client pointe `http://127.0.0.1:3001` en dur, la PWA servie depuis `.ts.net` tape une origine qui n'existe pas sur le téléphone.

**Files:**
- Modify: `frontend/src/lib/urql-client.ts`
- Modify: `frontend/vite.config.ts`
- Test: `frontend/src/lib/api-origin.test.ts` (créer), `frontend/src/lib/api-origin.ts` (créer)

**Interfaces:**
- Produit : `resolveApiUrl(protocol: string, override?: string): string` dans `frontend/src/lib/api-origin.ts`.

- [ ] **Step 1 : écrire le test qui échoue**

```ts
import { describe, it, expect } from 'vitest';
import { resolveApiUrl } from './api-origin';

describe('resolveApiUrl', () => {
  it('renvoie une origine vide sur http(s) pour rester same-origin', () => {
    // Servi par l'API elle-même (tunnel ou dev proxifié) : les requêtes
    // doivent rester relatives, sinon la PWA tape 127.0.0.1 depuis l'iPhone.
    expect(resolveApiUrl('https:')).toBe('');
    expect(resolveApiUrl('http:')).toBe('');
  });

  it('garde le loopback absolu hors http, pour le HUD Tauri', () => {
    // La fenêtre de production du HUD charge tauri://localhost : il n'y a
    // aucune origine HTTP à laquelle se rattacher.
    expect(resolveApiUrl('tauri:')).toBe('http://127.0.0.1:3001');
  });

  it('laisse VITE_API_URL primer', () => {
    expect(resolveApiUrl('https:', 'http://ailleurs:9999')).toBe('http://ailleurs:9999');
  });
});
```

- [ ] **Step 2 : lancer et vérifier l'échec**

Run: `cd frontend && pnpm test -- api-origin 2>&1 | tail -15`
Expected: FAIL — le module n'existe pas.

- [ ] **Step 3 : implémenter**

`frontend/src/lib/api-origin.ts` :

```ts
const TAURI_FALLBACK_API_URL = 'http://127.0.0.1:3001';

/**
 * Origine de l'API GraphQL.
 *
 * Sur http(s) on reste relatif : l'API sert le frontend, donc la page et
 * /graphql partagent la même origine — c'est ce qui permet à la PWA servie
 * par le tunnel de fonctionner sans connaître son propre nom d'hôte, et ce
 * qui garde l'en-tête `x-aplan-client` efficace.
 *
 * La fenêtre de production du HUD Tauri charge `tauri://localhost` : aucune
 * origine HTTP à laquelle se rattacher, il lui faut le loopback absolu.
 */
export function resolveApiUrl(protocol: string, override?: string): string {
  if (override) return override;
  return protocol.startsWith('http') ? '' : TAURI_FALLBACK_API_URL;
}
```

`urql-client.ts` : remplacer la constante par

```ts
import { resolveApiUrl } from './api-origin';

const API_URL = resolveApiUrl(window.location.protocol, import.meta.env.VITE_API_URL);
```

`vite.config.ts` — sous `server`, pour que le relatif marche aussi en `pnpm dev` :

```ts
    proxy: {
      // Le client est devenu relatif (voir api-origin.ts). En dev le front est
      // sur 3000 et l'API sur 3001 : ce proxy recrée l'origine unique que
      // l'API fournit elle-même en production.
      '/graphql': { target: 'http://127.0.0.1:3001', changeOrigin: false },
    },
```

- [ ] **Step 4 : vérifier**

Run: `cd frontend && pnpm test 2>&1 | tail -15` puis `pnpm type-check`
Expected: PASS, aucune régression sur les suites existantes.

- [ ] **Step 5 : commit**

```bash
/usr/bin/git add frontend/src/lib/api-origin.ts frontend/src/lib/api-origin.test.ts frontend/src/lib/urql-client.ts frontend/vite.config.ts
/usr/bin/git commit -m "Rendre l'origine de l'API relative et proxifier le dev"
```

---

### Task 4 : la file de capture hors-ligne

Écrite avant les écrans : l'écran de capture s'appuie dessus.

**Files:**
- Create: `frontend/src/lib/capture-queue.ts`
- Test: `frontend/src/lib/capture-queue.test.ts`

**Interfaces:**
- Produit :
  - `type PendingCapture = { clientRequestId: string; title: string; deadline?: string; projectId?: string; queuedAt: string }`
  - `type QueueStore = { read(): Promise<PendingCapture[]>; write(v: PendingCapture[]): Promise<void> }`
  - `createQueue(store: QueueStore): CaptureQueue`, où
    `type CaptureQueue = { enqueue(c: PendingCapture): Promise<void>; listPending(): Promise<PendingCapture[]>; flush(send: (c: PendingCapture) => Promise<void>): Promise<{ sent: number; kept: number }> }`
  - `openIndexedDbStore(): QueueStore`
  - `newClientRequestId(): string`
- Le stockage est abstrait derrière `QueueStore` — implémentation IndexedDB par défaut, implémentation en mémoire dans les tests — pour que la suite ne dépende pas de `fake-indexeddb`.

- [ ] **Step 1 : écrire les tests qui échouent**

```ts
import { describe, it, expect, vi } from 'vitest';
import { createQueue, type PendingCapture } from './capture-queue';

const memoryStore = () => {
  let rows: PendingCapture[] = [];
  return { read: async () => rows, write: async (v: PendingCapture[]) => { rows = v; } };
};

const capture = (id: string): PendingCapture => ({
  clientRequestId: id, title: `t-${id}`, queuedAt: '2026-09-12T08:00:00Z',
});

describe('capture-queue', () => {
  it('garde l’entrée quand l’envoi échoue', async () => {
    const q = createQueue(memoryStore());
    await q.enqueue(capture('a'));
    const res = await q.flush(async () => { throw new Error('offline'); });
    expect(res).toEqual({ sent: 0, kept: 1 });
    expect(await q.listPending()).toHaveLength(1);
  });

  it('vide la file quand l’envoi réussit', async () => {
    const q = createQueue(memoryStore());
    await q.enqueue(capture('a'));
    await q.enqueue(capture('b'));
    const send = vi.fn().mockResolvedValue(undefined);
    expect(await q.flush(send)).toEqual({ sent: 2, kept: 0 });
    expect(await q.listPending()).toHaveLength(0);
  });

  it('n’envoie qu’une fois par clé même si le flush est rejoué', async () => {
    // Deux flushs concurrents (retour réseau + réouverture de l’app) ne
    // doivent pas doubler l’envoi : c’est le pendant client de la clé
    // d’idempotence serveur, qui reste le garde-fou final.
    const q = createQueue(memoryStore());
    await q.enqueue(capture('a'));
    const send = vi.fn().mockResolvedValue(undefined);
    await Promise.all([q.flush(send), q.flush(send)]);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it('conserve la clé fournie à la saisie', async () => {
    // La clé naît au moment de la saisie, pas de l’envoi : sinon deux envois
    // de la même saisie porteraient deux clés et créeraient deux tâches.
    const q = createQueue(memoryStore());
    await q.enqueue(capture('stable'));
    const send = vi.fn().mockResolvedValue(undefined);
    await q.flush(send);
    expect(send.mock.calls[0][0].clientRequestId).toBe('stable');
  });
});
```

- [ ] **Step 2 : lancer et vérifier l'échec**

Run: `cd frontend && pnpm test -- capture-queue 2>&1 | tail -15`
Expected: FAIL — module inexistant.

- [ ] **Step 3 : implémenter**

`createQueue(store: QueueStore)` renvoie `{ enqueue, listPending, flush }`. `flush` prend un verrou par promesse au niveau du module (`let inFlight: Promise<...> | null`) afin que le troisième test passe ; il envoie séquentiellement, retire l'entrée après un envoi réussi, et s'arrête à la première erreur en gardant le reste. Exporter aussi `openIndexedDbStore(): QueueStore` (base `aplan-capture`, magasin `pending`, une seule clé `queue` portant le tableau) et `newClientRequestId(): string` via `crypto.randomUUID()`.

- [ ] **Step 4 : vérifier**

Run: `cd frontend && pnpm test -- capture-queue 2>&1 | tail -10`
Expected: 4 PASS.

- [ ] **Step 5 : commit**

```bash
/usr/bin/git add frontend/src/lib/capture-queue.ts frontend/src/lib/capture-queue.test.ts
/usr/bin/git commit -m "Ajouter la file de capture hors-ligne"
```

---

### Task 5 : les deux écrans mobiles

**Files:**
- Create: `frontend/src/pages/mobile/mobile-shell.tsx` (gabarit : safe-area, barre du bas, badge « n en attente »)
- Create: `frontend/src/pages/mobile/today-page.tsx`, `frontend/src/pages/mobile/today-page.test.tsx`
- Create: `frontend/src/pages/mobile/capture-page.tsx`, `frontend/src/pages/mobile/capture-page.test.tsx`
- Create: `frontend/src/pages/mobile/mobile.css`
- Create: `frontend/src/graphql/queries/mobile.graphql`
- Modify: `frontend/src/App.tsx` (routes `/m` et `/m/new`)

**Interfaces:**
- Consomme : `createQueue`, `openIndexedDbStore`, `newClientRequestId` (Task 4) ; `clientRequestId` sur `CreateTaskInput` (Task 2).
- Produit : les routes `/m` et `/m/new`.

- [ ] **Step 1 : écrire les tests qui échouent**

`today-page.test.tsx` — répartition en trois seaux, avec un `Provider` urql moqué comme le font `PriorityMatrixPage.test.tsx` et `TimesheetPage.test.tsx` :

```tsx
it('range les tâches en retard, aujourd’hui et demain', async () => {
  // 2026-09-12 est la date figée du test.
  renderToday([
    { id: '1', title: 'Vieux', deadline: '2026-09-01' },
    { id: '2', title: 'Jour', deadline: '2026-09-12' },
    { id: '3', title: 'Demain', deadline: '2026-09-13' },
  ]);
  expect(await screen.findByRole('heading', { name: /en retard/i })).toBeInTheDocument();
  expect(within(screen.getByTestId('bucket-overdue')).getByText('Vieux')).toBeInTheDocument();
  expect(within(screen.getByTestId('bucket-today')).getByText('Jour')).toBeInTheDocument();
  expect(within(screen.getByTestId('bucket-tomorrow')).getByText('Demain')).toBeInTheDocument();
});

it('horodate explicitement des données servies hors ligne', async () => {
  renderTodayOffline();
  // Ne jamais faire passer un plan périmé pour le plan du jour.
  expect(await screen.findByText(/hors ligne — vu à/i)).toBeInTheDocument();
});
```

`capture-page.test.tsx` :

```tsx
it('met la capture en file quand la mutation échoue', async () => {
  const { queue } = renderCaptureWithFailingMutation();
  await userEvent.type(screen.getByLabelText(/titre/i), 'Rappeler Jihane');
  await userEvent.click(screen.getByRole('button', { name: /capturer/i }));
  expect(await screen.findByText(/1 en attente/i)).toBeInTheDocument();
  expect(await queue.listPending()).toHaveLength(1);
});

it('envoie une clé d’idempotence avec la mutation', async () => {
  const { mutate } = renderCaptureWithSpy();
  await userEvent.type(screen.getByLabelText(/titre/i), 'Rappeler Jihane');
  await userEvent.click(screen.getByRole('button', { name: /capturer/i }));
  // Sans clé, un rejeu crée un doublon : c'est le contrat de la Task 2.
  expect(mutate.mock.calls[0][1].input.clientRequestId).toMatch(/^[0-9a-f-]{36}$/);
});
```

- [ ] **Step 2 : lancer et vérifier l'échec**

Run: `cd frontend && pnpm test -- mobile 2>&1 | tail -15`
Expected: FAIL — modules inexistants.

- [ ] **Step 3 : implémenter**

`mobile.graphql` :

```graphql
query MobileToday($until: NaiveDate!) {
  tasks(
    filter: {
      deadlineBefore: $until
      status: [TODO, IN_PROGRESS, BLOCKED]
      trackingState: [FOLLOWED]
    }
    first: 200
  ) {
    edges { node { id title deadline urgency status project { name } } }
  }
}
```

`first: 200` explicite : le défaut est `50` en ordre décroissant, ce qui ferait taire silencieusement les plus anciennes échéances — exactement le piège documenté pour les listes non bornées.

`today-page.tsx` : appelle la requête avec `until = demain`, répartit en trois seaux (`deadline < aujourd'hui`, `=== aujourd'hui`, `=== demain`), trie par échéance croissante puis urgence décroissante, et affiche la tâche active lue via `configuration` (`aplan.active_task_id`) suivie de `task(id:)`. Chaque seau porte un `data-testid` (`bucket-overdue`, `bucket-today`, `bucket-tomorrow`) et un titre de section. En erreur réseau, afficher la dernière réponse connue avec « Hors ligne — vu à HH:MM ».

`capture-page.tsx` : champ titre (autofocus), échéance (`<input type="date">`), projet (`<select>` alimenté par `projects`). À la soumission, générer la clé **avant** l'envoi, tenter la mutation, et en cas d'échec `enqueue`. Au montage et sur l'événement `online`, `flush`. Badge « n en attente » permanent.

`mobile.css` : `padding: env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left)`, `min-height: 44px` sur toute cible tactile, `font-size: 16px` minimum sur les champs (en-dessous, iOS zoome au focus).

`App.tsx` : `<Route path="/m" element={<TodayPage />} />` et `<Route path="/m/new" element={<CapturePage />} />`, hors du gabarit desktop, comme `/hud`.

- [ ] **Step 4 : vérifier**

Run: `cd frontend && pnpm test 2>&1 | tail -15` puis `pnpm type-check`
Expected: PASS.

- [ ] **Step 5 : commit**

```bash
/usr/bin/git add frontend/src/pages/mobile frontend/src/graphql/queries/mobile.graphql frontend/src/App.tsx
/usr/bin/git commit -m "Ajouter le shell mobile : plan du jour et capture"
```

---

### Task 6 : PWA — manifeste, icônes, service worker

**Files:**
- Modify: `frontend/package.json` (`vite-plugin-pwa`), `frontend/vite.config.ts`, `frontend/index.html`
- Create: `frontend/public/icons/icon-192.png`, `icon-512.png`, `icon-maskable-512.png`, `apple-touch-icon.png` (180×180)

- [ ] **Step 1 : installer et configurer**

```bash
cd frontend && pnpm add -D vite-plugin-pwa
```

`vite.config.ts` — ajouter le plugin :

```ts
    VitePWA({
      registerType: 'autoUpdate',
      // Seule la coquille est précachée. Les réponses GraphQL ne le sont
      // jamais : un plan du jour périmé qui se présente comme le plan du jour
      // est pire qu'un écran qui dit franchement qu'il est hors ligne.
      workbox: {
        globPatterns: ['**/*.{js,css,html,svg,png,woff2}'],
        navigateFallback: '/index.html',
        navigateFallbackDenylist: [/^\/graphql/, /^\/auth/],
      },
      manifest: {
        name: 'aplan — cockpit',
        short_name: 'aplan',
        description: 'Plan du jour et capture rapide',
        // L'icône de l'écran d'accueil tombe sur la capture : c'est l'usage
        // où le téléphone bat le poste.
        start_url: '/m/new',
        scope: '/m',
        display: 'standalone',
        background_color: '#0b0f14',
        theme_color: '#0b0f14',
        icons: [
          { src: '/icons/icon-192.png', sizes: '192x192', type: 'image/png' },
          { src: '/icons/icon-512.png', sizes: '512x512', type: 'image/png' },
          { src: '/icons/icon-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
        ],
      },
    }),
```

`index.html` — dans le `<head>` :

```html
    <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
    <meta name="apple-mobile-web-app-capable" content="yes" />
    <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent" />
    <link rel="apple-touch-icon" href="/icons/apple-touch-icon.png" />
```

Générer les quatre PNG (aplat `#0b0f14`, glyphe « ap » centré) ; l'icône maskable garde 20 % de marge sur chaque bord pour survivre au masque circulaire d'iOS.

- [ ] **Step 2 : vérifier le build**

Run: `cd frontend && pnpm build 2>&1 | tail -15`
Expected: succès, et `dist/manifest.webmanifest` + `dist/sw.js` présents.

- [ ] **Step 3 : vérifier que le service worker n'avale pas GraphQL**

Run: `grep -c "graphql" frontend/dist/sw.js` puis inspecter la denylist générée.
Expected: `navigateFallbackDenylist` contient bien `/graphql` — sinon une requête API en échec renverrait `index.html`, que urql tenterait de parser en JSON.

- [ ] **Step 4 : commit**

```bash
/usr/bin/git add frontend/package.json frontend/pnpm-lock.yaml frontend/vite.config.ts frontend/index.html frontend/public/icons
/usr/bin/git commit -m "Faire du shell mobile une PWA installable"
```

---

### Task 7 : mise en service et documentation

**Files:**
- Create: `scripts/aplan-serve-tailnet` (script d'installation et de vérification)
- Modify: `~/.config/systemd/user/aplan-api.service` (hors dépôt — documenter la ligne à ajouter)
- Modify: `SPEC_FONCTIONNELLE.md`, `SPEC_TECHNIQUE.md`, `CLAUDE.md`

- [ ] **Step 1 : le script**

`scripts/aplan-serve-tailnet` — vérifie que `tailscale` est installé, que le nœud est connecté, que MagicDNS et les certificats HTTPS sont actifs, **refuse de continuer si `tailscale funnel` est actif**, puis :

```bash
tailscale serve --bg --https=443 http://127.0.0.1:3001
tailscale serve status
```

Le script est idempotent et n'ouvre aucun port sur le LAN.

- [ ] **Step 2 : la ligne systemd**

À ajouter dans `aplan-api.service` (à faire à la main, le fichier est hors dépôt) :

```ini
Environment=APLAN_STATIC_DIR=/home/mbt/appfactory/aggregated_plan/frontend/dist
```

puis `systemctl --user daemon-reload && systemctl --user restart aplan-api.service`. Rappel : l'API est le **binaire installé** `~/.local/bin/aplan-api`, pas un `cargo run` — il faut donc recompiler et remplacer le binaire, sinon le front interroge un back périmé.

- [ ] **Step 3 : documenter**

`SPEC_TECHNIQUE.md` : section « Exposition réseau » (topologie, `serve` vs `funnel`, `APLAN_STATIC_DIR`, origine unique) et « Idempotence de la capture » (migration 023, index partiel).
`SPEC_FONCTIONNELLE.md` : les deux écrans mobiles, le comportement hors ligne, et **le risque accepté** — l'autorité sur le cockpit est l'appartenance au tailnet plus le verrouillage de l'iPhone, sans authentification applicative.
`CLAUDE.md` : la table des migrations passe à 023 et la colonne `client_request_id` est mentionnée.

- [ ] **Step 4 : E2E**

`frontend/e2e/mobile.spec.ts`, viewport 390 × 844, qui ne tourne que si `APLAN_E2E_GRAPHQL_URL` est défini : ouvrir `/m/new`, capturer une tâche, la retrouver dans `/m`.

Run: `APLAN_E2E_GRAPHQL_URL=http://127.0.0.1:3001/graphql pnpm test:e2e mobile` **contre une instance jetable uniquement** (`aplan-api.service` arrêté, `DATABASE_URL=sqlite:///tmp/aplan-dev.db`).

- [ ] **Step 5 : commit**

```bash
/usr/bin/git add scripts/aplan-serve-tailnet SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md CLAUDE.md frontend/e2e/mobile.spec.ts
/usr/bin/git commit -m "Documenter et outiller la mise en service sur le tailnet"
```

---

## Ordre et dépendances

```
Task 1 (statique) ──┐
Task 2 (idempotence)├─▶ Task 5 (écrans) ──▶ Task 6 (PWA) ──▶ Task 7 (mise en service)
Task 3 (origine) ───┤
Task 4 (file) ──────┘
```

Les tâches 1 à 4 sont indépendantes entre elles et peuvent être menées en parallèle. 5 dépend de 2, 3 et 4 ; 6 dépend de 5 ; 7 dépend de tout.
