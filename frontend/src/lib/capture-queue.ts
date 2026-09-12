/**
 * File de capture hors-ligne pour l'écran mobile.
 *
 * La capture doit survivre à l'absence de réseau (métro, tunnel Tailscale qui
 * coupe) : une saisie qui échoue reste en file, et un flush ultérieur la
 * rejoue en portant la même `clientRequestId` -- la clé d'idempotence côté
 * serveur (migration 023) absorbe le doublon si l'envoi initial avait en
 * fait réussi. Le stockage est abstrait derrière `QueueStore` pour que
 * `IndexedDB` reste un détail d'implémentation et que les tests utilisent un
 * store en mémoire, sans dépendance à `fake-indexeddb`.
 */

/** Une capture en attente d'envoi, telle que saisie sur l'écran mobile. */
export type PendingCapture = {
  /** Clé d'idempotence générée à la saisie, jamais à l'envoi (voir plus bas). */
  clientRequestId: string;
  title: string;
  deadline?: string;
  projectId?: string;
  /** Horodatage ISO 8601 de la mise en file, pour l'affichage « n en attente ». */
  queuedAt: string;
};

/** Persistance de la file, abstraite pour permettre un double en mémoire dans les tests. */
export type QueueStore = {
  read(): Promise<PendingCapture[]>;
  write(v: PendingCapture[]): Promise<void>;
};

export type CaptureQueue = {
  enqueue(c: PendingCapture): Promise<void>;
  listPending(): Promise<PendingCapture[]>;
  /**
   * Envoie les captures en attente, dans l'ordre, et s'arrête au premier
   * échec : les entrées déjà envoyées sont retirées de la file, celle qui a
   * échoué et toutes celles qui la suivent restent en file pour la prochaine
   * tentative. Sans cet arrêt, une capture déjà passée serait rejouée à
   * chaque flush suivant.
   */
  flush(send: (c: PendingCapture) => Promise<void>): Promise<{ sent: number; kept: number }>;
};

/**
 * Verrou de flush au niveau du module.
 *
 * Le retour réseau (`online`) et la réouverture de l'app peuvent toutes deux
 * déclencher un flush au même instant. Sans ce verrou, les deux tentatives
 * liraient la file avant que la première n'ait eu le temps d'écrire son
 * résultat, et enverraient donc deux fois la même capture -- exactement ce
 * que la clé d'idempotence côté client est censée éviter en amont du
 * serveur. Une simple promesse partagée suffit : le deuxième appel attend le
 * résultat du premier au lieu d'en démarrer un second. Remis à `null` dans
 * un `finally` pour qu'un flush suivant, une fois celui-ci terminé, reparte
 * bien de la file à jour.
 */
let inFlight: Promise<{ sent: number; kept: number }> | null = null;

async function runFlush(
  store: QueueStore,
  send: (c: PendingCapture) => Promise<void>
): Promise<{ sent: number; kept: number }> {
  const pending = await store.read();
  let sent = 0;
  while (sent < pending.length) {
    try {
      await send(pending[sent]);
      sent++;
    } catch {
      break;
    }
  }
  const kept = pending.slice(sent);
  await store.write(kept);
  return { sent, kept: kept.length };
}

export function createQueue(store: QueueStore): CaptureQueue {
  return {
    async enqueue(capture) {
      const pending = await store.read();
      await store.write([...pending, capture]);
    },
    async listPending() {
      return store.read();
    },
    async flush(send) {
      if (inFlight) return inFlight;
      const started = runFlush(store, send).finally(() => {
        inFlight = null;
      });
      inFlight = started;
      return started;
    },
  };
}

const DB_NAME = 'aplan-capture';
const STORE_NAME = 'pending';
// Une seule clé porte tout le tableau : la file est petite (quelques
// captures avant le prochain retour réseau), pas la peine d'une ligne par
// entrée.
const QUEUE_KEY = 'queue';

function openDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, 1);
    request.onupgradeneeded = () => {
      request.result.createObjectStore(STORE_NAME);
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

/** Store `QueueStore` adossé à IndexedDB, pour l'usage réel dans le navigateur. */
export function openIndexedDbStore(): QueueStore {
  return {
    async read() {
      const db = await openDatabase();
      return new Promise<PendingCapture[]>((resolve, reject) => {
        const tx = db.transaction(STORE_NAME, 'readonly');
        const request = tx.objectStore(STORE_NAME).get(QUEUE_KEY);
        request.onsuccess = () => resolve(request.result ?? []);
        request.onerror = () => reject(request.error);
      });
    },
    async write(rows) {
      const db = await openDatabase();
      return new Promise<void>((resolve, reject) => {
        const tx = db.transaction(STORE_NAME, 'readwrite');
        tx.objectStore(STORE_NAME).put(rows, QUEUE_KEY);
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
      });
    },
  };
}

/** Nouvelle clé d'idempotence, à générer à la saisie (voir `PendingCapture`). */
export function newClientRequestId(): string {
  return crypto.randomUUID();
}
