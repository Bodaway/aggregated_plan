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
  // `!== undefined` et non un test de véracité : Vite laisse la variable
  // `undefined` quand elle n'est pas déclarée, mais rend `''` quand elle l'est
  // et qu'elle est vide. Les deux cas sont distincts, et une chaîne vide est
  // une configuration explicite — « pas de préfixe d'origine ». La confondre
  // avec une absence ferait reprendre la main au repli Tauri en silence, sur
  // un réglage que quelqu'un a délibérément posé.
  if (override !== undefined) return override;
  return protocol.startsWith('http') ? '' : TAURI_FALLBACK_API_URL;
}
