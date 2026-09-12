import { useCallback, useEffect, useState, type FormEvent } from 'react';
import { useMutation, useQuery } from 'urql';
import {
  createQueue,
  openIndexedDbStore,
  newClientRequestId,
  type CaptureQueue,
  type PendingCapture,
} from '@/lib/capture-queue';
import { MOBILE_CREATE_TASK_MUTATION, MOBILE_PROJECTS_QUERY } from '@/graphql/queries/mobile';
import { MobileShell } from './mobile-shell';

interface MobileProject {
  readonly id: string;
  readonly name: string;
}

interface MobileProjectsData {
  readonly projects: readonly MobileProject[];
}

interface CreateTaskResult {
  readonly createTask: { readonly id: string };
}

/**
 * File par défaut, adossée à IndexedDB. Créée une seule fois au chargement du
 * module : `openIndexedDbStore()` ouvre une connexion à chaque appel, et rien
 * ne justifie d'en ouvrir plusieurs pour un seul écran.
 */
const defaultQueue = createQueue(openIndexedDbStore());

interface CapturePageProps {
  /**
   * Injectée par les tests pour ne jamais dépendre d'IndexedDB (voir
   * capture-page.test.tsx). En production, `App.tsx` monte `<CapturePage />`
   * nue sur `/m/new` : la valeur par défaut couvre ce cas.
   */
  readonly queue?: CaptureQueue;
}

/**
 * Écran de capture rapide : titre, échéance, projet. La clé d'idempotence est
 * générée à la saisie (juste avant l'envoi, jamais à l'intérieur d'un rejeu)
 * et voyage avec la tentative live comme avec l'entrée mise en file, pour
 * qu'un rejeu ne crée jamais de doublon (migration 023).
 */
export function CapturePage({ queue = defaultQueue }: CapturePageProps) {
  const [title, setTitle] = useState('');
  const [deadline, setDeadline] = useState('');
  const [projectId, setProjectId] = useState('');
  const [pendingCount, setPendingCount] = useState(0);

  const [projectsResult] = useQuery<MobileProjectsData>({ query: MOBILE_PROJECTS_QUERY });
  const projects = projectsResult.data?.projects ?? [];

  const [, mutate] = useMutation<CreateTaskResult>(MOBILE_CREATE_TASK_MUTATION);

  const refreshPendingCount = useCallback(async () => {
    const pending = await queue.listPending();
    setPendingCount(pending.length);
  }, [queue]);

  const send = useCallback(
    async (capture: PendingCapture) => {
      const result = await mutate({
        input: {
          title: capture.title,
          deadline: capture.deadline || null,
          projectId: capture.projectId || null,
          clientRequestId: capture.clientRequestId,
        },
      });
      if (result.error) throw result.error;
    },
    [mutate],
  );

  // Flush au montage (une capture peut avoir été mise en file lors d'une
  // session précédente) et sur le retour réseau : rejouer dès que possible
  // plutôt qu'attendre la prochaine saisie.
  useEffect(() => {
    function runFlush() {
      queue.flush(send).finally(refreshPendingCount);
    }
    runFlush();
    window.addEventListener('online', runFlush);
    return () => window.removeEventListener('online', runFlush);
  }, [queue, send, refreshPendingCount]);

  const handleSubmit = useCallback(
    async (e: FormEvent<HTMLFormElement>) => {
      e.preventDefault();
      const trimmedTitle = title.trim();
      if (!trimmedTitle) return;

      // Générée ici, avant l'envoi : un rejeu depuis la file doit porter la
      // même clé que la tentative live, sinon la migration 023 ne voit plus
      // le doublon venir.
      const capture: PendingCapture = {
        clientRequestId: newClientRequestId(),
        title: trimmedTitle,
        deadline: deadline || undefined,
        projectId: projectId || undefined,
        queuedAt: new Date().toISOString(),
      };

      setTitle('');
      setDeadline('');
      setProjectId('');

      try {
        await send(capture);
      } catch {
        await queue.enqueue(capture);
        await refreshPendingCount();
      }
    },
    [title, deadline, projectId, send, queue, refreshPendingCount],
  );

  return (
    <MobileShell pendingCount={pendingCount}>
      <h2>Capture rapide</h2>
      <form className="mobile-form" onSubmit={e => void handleSubmit(e)}>
        <label htmlFor="capture-title">
          Titre
          <input
            id="capture-title"
            type="text"
            autoFocus
            value={title}
            onChange={e => setTitle(e.target.value)}
          />
        </label>

        <label htmlFor="capture-deadline">
          Échéance
          <input
            id="capture-deadline"
            type="date"
            value={deadline}
            onChange={e => setDeadline(e.target.value)}
          />
        </label>

        <label htmlFor="capture-project">
          Projet
          <select
            id="capture-project"
            value={projectId}
            onChange={e => setProjectId(e.target.value)}
          >
            <option value="">Aucun</option>
            {projects.map(p => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>

        <button type="submit">Capturer</button>
      </form>
    </MobileShell>
  );
}
