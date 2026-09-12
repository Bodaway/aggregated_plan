import { test, expect, request as pwRequest } from '@playwright/test';

// `require_csrf_header` (backend/crates/api/src/security.rs) rejette en 403 tout
// `POST /graphql` sans cet en-tête -- sa seule fonction est de forcer un préflight
// CORS, donc sa valeur importe peu, seule sa présence compte. On le pose sur chacune
// de nos requêtes GraphQL brutes ci-dessous (recurring-tasks.spec.ts ne le fait pas
// pour son propre nettoyage -- défaut préexistant signalé, non corrigé ici).
const CSRF_HEADER = { 'x-aplan-client': 'e2e-mobile-spec' };

// Cette suite écrit une vraie tâche en base via l'écran de capture mobile : il n'y a
// aucun bac à sable ici, `GRAPHQL_URL` reçoit de vraies écritures. Il ne doit donc
// jamais viser en dur `http://localhost:3001/graphql` / `127.0.0.1:3001` -- même défaut
// que recurring-tasks.spec.ts, même garde.
const GRAPHQL_URL = process.env.APLAN_E2E_GRAPHQL_URL ?? '';

test.describe('Mobile shell (capture + plan du jour)', () => {
  test.skip(
    !GRAPHQL_URL,
    'Set APLAN_E2E_GRAPHQL_URL to a throwaway GraphQL endpoint (e.g. a disposable ' +
      'backend/DB started just for this run) before running this suite. It creates a ' +
      'real task row via /m/new — never point it at your own aggregated_plan.db / ' +
      'http://localhost:3001/graphql.',
  );

  // Viewport iPhone 12 (390 × 844) : le projet playwright.config.ts ne déclare qu'un
  // profil Desktop Chrome, donc c'est ici, au niveau du fichier, que le parcours
  // mobile impose sa propre taille d'écran -- on ne modifie pas la config partagée.
  test.use({ viewport: { width: 390, height: 844 } });

  // Titre posé au début du test, consommé par afterEach pour le nettoyage.
  let taskTitle = '';

  test.afterEach(async () => {
    if (!taskTitle) return;
    const api = await pwRequest.newContext();
    try {
      // Retrouve la tâche par titre (la capture ne connaît pas son id côté client).
      const res = await api.post(GRAPHQL_URL, {
        headers: CSRF_HEADER,
        data: { query: '{ tasks(first: 500) { edges { node { id title } } } }' },
      });
      const body = await res.json();
      expect(
        res.ok() && !body.errors,
        `cleanup query for "${taskTitle}" failed: ${JSON.stringify(body.errors ?? body)}`,
      ).toBe(true);

      const ids: string[] = (body.data?.tasks?.edges ?? [])
        .map((e: { node: { id: string; title: string } }) => e.node)
        .filter((n: { title: string }) => n.title === taskTitle)
        .map((n: { id: string }) => n.id);

      for (const id of ids) {
        const delRes = await api.post(GRAPHQL_URL, {
          headers: CSRF_HEADER,
          data: { query: `mutation { deleteTask(id: "${id}") }` },
        });
        const delBody = await delRes.json();
        expect(
          delRes.ok() && !delBody.errors && delBody.data?.deleteTask === true,
          `deleteTask cleanup failed for ${id}: ${JSON.stringify(delBody.errors ?? delBody)}`,
        ).toBe(true);
      }
    } finally {
      await api.dispose();
      taskTitle = '';
    }
  });

  test('capture a task on /m/new and find it on /m in the right bucket', async ({ page }) => {
    taskTitle = `Capture mobile E2E ${Date.now()}`;

    // Échéance demain : seau déterministe (`bucket-tomorrow`), sans dépendre de
    // l'heure exacte du run par rapport à minuit comme le ferait "aujourd'hui".
    const tomorrow = new Date();
    tomorrow.setDate(tomorrow.getDate() + 1);
    const deadline = tomorrow.toISOString().slice(0, 10);

    // ── 1. /m/new : le champ titre a le focus, la capture aboutit ──────────────

    await page.goto('/m/new');

    const titleInput = page.getByLabel(/titre/i);
    await expect(titleInput).toBeFocused();

    // Badge au repos avant tout envoi -- point de comparaison pour la suite.
    await expect(page.getByText('0 en attente')).toBeVisible();

    await titleInput.fill(taskTitle);
    await page.getByLabel(/échéance/i).fill(deadline);
    await page.getByRole('button', { name: /capturer/i }).click();

    // Le formulaire se vide dès le clic (avant même la résolution du réseau) --
    // signal que la soumission a démarré, pas encore qu'elle a réussi.
    await expect(titleInput).toHaveValue('');

    // Envoi réussi : le chemin direct de `send()` n'alimente jamais la file
    // d'attente (`queue.enqueue` n'est appelé que dans le `catch` de l'échec
    // réseau) -- c'est cela que "le badge retombe à zéro après un envoi réussi"
    // prouve ici : la capture n'est jamais passée par la file hors-ligne, le
    // badge reste donc au même zéro qu'avant l'envoi plutôt que de grimper puis
    // redescendre.
    await expect(page.getByText('0 en attente')).toBeVisible({ timeout: 5000 });

    // ── 2. /m : la tâche capturée apparaît dans le bon seau ─────────────────────

    await page.goto('/m');

    const tomorrowBucket = page.getByTestId('bucket-tomorrow');
    await expect(tomorrowBucket.getByText(taskTitle)).toBeVisible({ timeout: 8000 });

    // Et nulle part ailleurs -- une régression de tri la ferait apparaître dans
    // un autre seau plutôt que de la faire disparaître.
    await expect(page.getByTestId('bucket-overdue').getByText(taskTitle)).not.toBeVisible();
    await expect(page.getByTestId('bucket-today').getByText(taskTitle)).not.toBeVisible();
  });
});
