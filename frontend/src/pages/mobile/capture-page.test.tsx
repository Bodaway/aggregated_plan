import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';

import { createQueue, type PendingCapture, type QueueStore, type CaptureQueue } from '@/lib/capture-queue';

// ── Contrat d'injection de la file (fixé ici, faute d'être écrit ailleurs) ──
//
// `CapturePage` doit accepter une prop optionnelle `queue?: CaptureQueue`.
// En production, en l'absence de la prop, elle vaut par défaut
// `createQueue(openIndexedDbStore())` (Task 4) ; en test on injecte toujours
// `createQueue(memoryStore())` pour ne jamais dépendre d'IndexedDB. On choisit
// une prop plutôt qu'un contexte React ou un paramètre de factory : App.tsx
// monte la page nue sur la route `/m/new` (`<Route path="/m/new"
// element={<CapturePage />} />`, voir le plan), donc la substitution la plus
// directe -- sans ajouter de Provider à la route -- est un prop qui retombe
// sur la valeur par défaut quand elle est absente.
//
// ── urql mock ────────────────────────────────────────────────────────────
//
// Comme pour today-page.test.tsx : aucun hook dédié n'est prévu par la Task 5
// pour cette page, donc 'urql' est moqué directement (dispositif de
// DashboardPage.overdue.test.tsx / DeduplicationPage.test.tsx), pas le
// dispositif "mock du hook" de PriorityMatrixPage/TimesheetPage qui ne
// s'applique pas ici en l'absence d'un tel hook.
const harness = vi.hoisted(() => ({
  mutate: vi.fn(),
}));

vi.mock('urql', () => ({
  // Alimente le <select> projet ; le champ exact importe peu ici, seul
  // compte qu'un résultat non vide ne fasse pas planter le rendu.
  useQuery: () => [
    { data: { projects: [{ id: 'p1', name: 'Projet Un' }] }, fetching: false, error: undefined },
    vi.fn(),
  ],
  useMutation: () => [{ fetching: false, data: null, error: null }, harness.mutate],
}));

import { CapturePage } from './capture-page';

// ── Store en mémoire, jamais IndexedDB ───────────────────────────────────
// Même fabrique que capture-queue.test.ts, pour rester cohérent avec le seul
// autre test qui connaît `QueueStore`.
function memoryStore(): QueueStore {
  let rows: PendingCapture[] = [];
  return {
    read: async () => rows,
    write: async (v: PendingCapture[]) => {
      rows = v;
    },
  };
}

function renderCapture(queue: CaptureQueue) {
  return render(<CapturePage queue={queue} />, { wrapper: MemoryRouter });
}

// UUID v4 strict : la version (4) et le nibble de variant ([89ab]) sont
// vérifiés, pas seulement "36 caractères hexa et des tirets" -- sinon un
// timestamp mal formaté ou un UUID v1 passerait le test sans que la clé
// d'idempotence serveur (migration 023, Task 2) ne s'en aperçoive avant la
// production.
const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

beforeEach(() => {
  harness.mutate.mockReset();
});

describe('CapturePage — file hors ligne à l’échec de la mutation', () => {
  it('met la capture en file quand la mutation échoue', async () => {
    harness.mutate.mockRejectedValue(new Error('offline'));
    const queue = createQueue(memoryStore());
    const user = userEvent.setup();
    renderCapture(queue);

    await user.type(screen.getByLabelText(/titre/i), 'Rappeler Jihane');
    await user.click(screen.getByRole('button', { name: /capturer/i }));

    expect(await screen.findByText(/1 en attente/i)).toBeInTheDocument();
    expect(await queue.listPending()).toHaveLength(1);
  });

  it('envoie une clé d’idempotence au format UUID v4 avec la mutation', async () => {
    harness.mutate.mockResolvedValue({ data: { createTask: { id: 't1' } }, error: undefined });
    const queue = createQueue(memoryStore());
    const user = userEvent.setup();
    renderCapture(queue);

    await user.type(screen.getByLabelText(/titre/i), 'Rappeler Jihane');
    await user.click(screen.getByRole('button', { name: /capturer/i }));

    // Sans clé, un rejeu après une réponse perdue crée un doublon : c'est le
    // contrat de la Task 2 (clientRequestId sur CreateTaskInput).
    await waitFor(() => expect(harness.mutate).toHaveBeenCalled());
    const variables = harness.mutate.mock.calls[0][0] as { input?: { clientRequestId?: string } };
    expect(variables.input?.clientRequestId).toMatch(UUID_V4);
  });

  // Cas défensif ajouté au-delà du plan : un flush réussi doit ramener le
  // badge à zéro. Sans cette garantie, une capture déjà partie continuerait
  // d'afficher "1 en attente" et ferait douter l'utilisateur de ce qui a
  // réellement été envoyé.
  it('ramène le badge à zéro après un flush réussi', async () => {
    const store = memoryStore();
    const queue = createQueue(store);
    // Pré-remplit la file comme le ferait un échec de mutation antérieur,
    // pour isoler ce test du parcours de saisie : seul le flush au montage
    // (Step 3 du plan) est sous test ici.
    await queue.enqueue({
      clientRequestId: '11111111-1111-4111-8111-111111111111',
      title: 'Ancienne capture',
      queuedAt: '2026-09-12T08:00:00Z',
    });
    harness.mutate.mockResolvedValue({ data: { createTask: { id: 't-old' } }, error: undefined });

    renderCapture(queue);

    expect(await screen.findByText(/0 en attente/i)).toBeInTheDocument();
    expect(await queue.listPending()).toHaveLength(0);
  });
});
