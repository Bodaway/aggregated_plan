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

  it('arrête le flush au premier échec et garde la suite en file', async () => {
    // Un flush partiel ne doit rejouer que ce qui n'est pas encore parti : les
    // entrées déjà envoyées ont disparu, celles qui suivent l'échec restent en
    // file pour la prochaine tentative -- sinon le premier envoi réussi est
    // rejoué indéfiniment à chaque retour réseau.
    const q = createQueue(memoryStore());
    await q.enqueue(capture('a'));
    await q.enqueue(capture('b'));
    const send = vi
      .fn()
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(new Error('offline'));

    const res = await q.flush(send);

    expect(res).toEqual({ sent: 1, kept: 1 });
    const pending = await q.listPending();
    expect(pending).toHaveLength(1);
    expect(pending[0].clientRequestId).toBe('b');
  });
});
