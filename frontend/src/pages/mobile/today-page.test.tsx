import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';

// ── urql mock ────────────────────────────────────────────────────────────
//
// Task 5 ne prévoit aucun hook dédié pour cette page (seuls
// `today-page.tsx`/`capture-page.tsx`/`mobile-shell.tsx` sont listés comme
// fichiers créés) : `TodayPage` est donc censée appeler `useQuery` de 'urql'
// directement, exactement comme `DashboardPage.tsx` importe `useMutation`
// depuis 'urql' sans hook intermédiaire. C'est ce dispositif — moquer le
// module 'urql' lui-même — que suivent réellement `DashboardPage.overdue
// .test.tsx` et `DeduplicationPage.test.tsx`. Le plan cite
// `PriorityMatrixPage.test.tsx`/`TimesheetPage.test.tsx` comme modèle, mais
// ces deux-là moquent en réalité leur *hook* dédié (`use-priority-matrix`,
// `use-timesheet`), pas 'urql' : il n'y a rien d'équivalent ici puisqu'aucun
// hook n'est prévu, donc c'est le dispositif Dashboard/Dedup qui s'applique.
const harness = vi.hoisted(() => ({
  query: { data: undefined as unknown, fetching: false, error: undefined as unknown },
}));

vi.mock('urql', () => ({
  useQuery: () => [harness.query, vi.fn()],
  useMutation: () => [{ fetching: false, data: null, error: null }, vi.fn()],
}));

import { TodayPage } from './today-page';

// ── Fixtures ─────────────────────────────────────────────────────────────

type TaskFixture = {
  id: string;
  title: string;
  deadline: string | null;
  urgency?: string;
  status?: string;
  projectName?: string | null;
};

function edgeOf(t: TaskFixture) {
  return {
    node: {
      id: t.id,
      title: t.title,
      deadline: t.deadline,
      urgency: t.urgency ?? 'MEDIUM',
      status: t.status ?? 'TODO',
      project: t.projectName ? { name: t.projectName } : null,
    },
  };
}

/** Seed le résultat `useQuery` avec une réponse "tasks" réussie. */
function setTasks(tasks: TaskFixture[]) {
  harness.query = { data: { tasks: { edges: tasks.map(edgeOf) } }, fetching: false, error: undefined };
}

/**
 * Forme d'erreur rendue par urql (voir TaskEditSheet.test.tsx /
 * use-task-edit.test.ts) : pas de constructeur `CombinedError` importable
 * simplement ici, donc on le fabrique à la main.
 */
function combinedError(message: string): Error {
  const err = new Error(`[Network] ${message}`);
  err.name = 'CombinedError';
  Object.assign(err, { graphQLErrors: [], networkError: new Error(message) });
  return err;
}

function renderToday() {
  return render(<TodayPage />, { wrapper: MemoryRouter });
}

// 2026-09-12 est la date figée du test (reprise du plan). On ne fige que
// `Date` (pas les timers), pour que `findBy*`/`waitFor` continuent de
// fonctionner normalement -- même dispositif que
// DashboardPage.overdue.test.tsx.
beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date('2026-09-12T09:00:00Z'));
  harness.query = { data: undefined, fetching: false, error: undefined };
});

afterEach(() => {
  vi.useRealTimers();
});

describe('TodayPage — répartition en trois seaux', () => {
  it('range les tâches en retard, aujourd’hui et demain', async () => {
    setTasks([
      { id: '1', title: 'Vieux', deadline: '2026-09-01' },
      { id: '2', title: 'Jour', deadline: '2026-09-12' },
      { id: '3', title: 'Demain', deadline: '2026-09-13' },
    ]);
    renderToday();

    expect(await screen.findByRole('heading', { name: /en retard/i })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: /aujourd.?hui/i })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: /demain/i })).toBeInTheDocument();

    expect(within(screen.getByTestId('bucket-overdue')).getByText('Vieux')).toBeInTheDocument();
    expect(within(screen.getByTestId('bucket-today')).getByText('Jour')).toBeInTheDocument();
    expect(within(screen.getByTestId('bucket-tomorrow')).getByText('Demain')).toBeInTheDocument();
  });

  // Cas défensif ajouté au-delà du plan : une tâche sans échéance ne doit
  // silencieusement atterrir dans aucun des trois seaux (ni "aujourd'hui"
  // par défaut faute de mieux). Sans cette garantie, un bug de tri masque
  // une tâche de façon indétectable : aucun des trois titres ne le
  // révélerait, et elle ne serait pas non plus signalée comme mal formée.
  it('ne range une tâche sans échéance dans aucun des trois seaux', async () => {
    setTasks([
      { id: '1', title: 'Vieux', deadline: '2026-09-01' },
      { id: '4', title: 'SansEcheance', deadline: null },
    ]);
    renderToday();

    await screen.findByText('Vieux');
    expect(within(screen.getByTestId('bucket-overdue')).queryByText('SansEcheance')).toBeNull();
    expect(within(screen.getByTestId('bucket-today')).queryByText('SansEcheance')).toBeNull();
    expect(within(screen.getByTestId('bucket-tomorrow')).queryByText('SansEcheance')).toBeNull();
  });
});

describe('TodayPage — hors ligne', () => {
  it('horodate explicitement des données servies hors ligne, sans perdre le dernier plan connu', async () => {
    // Un premier rendu réussi doit avoir été vu pour qu'il y ait un "dernier
    // connu" à montrer : sans ce préalable le mode hors-ligne n'aurait rien
    // à afficher.
    setTasks([{ id: '2', title: 'Jour', deadline: '2026-09-12' }]);
    const { rerender } = renderToday();
    await screen.findByText('Jour');

    // Le réseau tombe : `data` redevient undefined et `error` apparaît. Si
    // TodayPage se contentait de refléter l'état urql courant sans garder
    // sa propre copie de la dernière réponse connue, l'écran perdrait le
    // plan du jour au lieu de le montrer périmé -- exactement le piège que
    // le commentaire du plan pointe ("ne jamais faire passer un plan
    // périmé pour le plan du jour" ne veut pas dire "ne rien montrer").
    harness.query = { data: undefined, fetching: false, error: combinedError('offline') };
    rerender(<TodayPage />);

    expect(await screen.findByText(/hors ligne — vu à/i)).toBeInTheDocument();
    expect(screen.getByText('Jour')).toBeInTheDocument();
  });
});
