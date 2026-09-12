import { describe, it, expect } from 'vitest';
import { resolveApiUrl } from './api-origin';

describe('resolveApiUrl', () => {
  it('renvoie une origine vide sur http pour rester same-origin', () => {
    // Servi par l'API elle-même (tunnel ou dev proxifié) : les requêtes
    // doivent rester relatives, sinon la PWA tape 127.0.0.1 depuis l'iPhone.
    expect(resolveApiUrl('http:')).toBe('');
  });

  it('renvoie une origine vide sur https, pas seulement sur http', () => {
    // Cas isolé du http: ci-dessus : un `startsWith('http:')` avec les
    // deux-points laisserait passer https: en le manquant, sans faire
    // échouer le test précédent — il faut les deux protocoles couverts
    // séparément pour verrouiller la branche commune.
    expect(resolveApiUrl('https:')).toBe('');
  });

  it('garde le loopback absolu hors http, pour le HUD Tauri', () => {
    // La fenêtre de production du HUD charge tauri://localhost : il n'y a
    // aucune origine HTTP à laquelle se rattacher.
    expect(resolveApiUrl('tauri:')).toBe('http://127.0.0.1:3001');
  });

  it('laisse VITE_API_URL primer', () => {
    expect(resolveApiUrl('https:', 'http://ailleurs:9999')).toBe('http://ailleurs:9999');
  });

  it("ne confond pas une chaîne vide explicite avec l'absence d'override", () => {
    // Si VITE_API_URL est défini mais vide, c'est un choix explicite : la
    // valeur par défaut ne doit pas reprendre la main sans qu'on l'ait
    // demandé. Ça ne se voit pas sur http (le défaut y vaut aussi '') —
    // il faut le protocole Tauri pour distinguer « override vide honoré »
    // de « override ignoré, fallback repris ».
    expect(resolveApiUrl('tauri:', '')).toBe('');
  });
});
