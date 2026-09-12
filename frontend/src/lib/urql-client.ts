import { Client, cacheExchange, fetchExchange, subscriptionExchange } from 'urql';
import { createClient as createSSEClient } from 'graphql-sse';
import { resolveApiUrl } from './api-origin';

// Origine relative sur http(s) (voir api-origin.ts) : la page et l'API
// partagent la même origine, que ce soit derrière le tunnel Tailscale ou
// derrière le proxy de dev Vite.
const API_URL = resolveApiUrl(window.location.protocol, import.meta.env.VITE_API_URL);

// `graphql-sse` n'utilise que `fetch` en interne (jamais `new URL()` ni
// `EventSource`), donc une URL relative comme `/graphql/sse` se résout
// correctement contre l'origine du document -- pas besoin de la rattacher
// explicitement à `window.location.origin`.
const sseClient = createSSEClient({
  url: `${API_URL}/graphql/sse`,
});

export const urqlClient = new Client({
  url: `${API_URL}/graphql`,
  // `x-aplan-client` is not a secret and carries no identity -- its only job is
  // to force the browser to run a CORS preflight before this request can be
  // sent cross-origin, so the backend's origin allow-list gets a chance to
  // block requests from pages other than this app. See
  // backend/crates/api/src/security.rs for the full rationale. Do not remove
  // it or turn it into an auth token.
  fetchOptions: {
    headers: { 'x-aplan-client': '1' },
  },
  exchanges: [
    cacheExchange,
    fetchExchange,
    subscriptionExchange({
      forwardSubscription: (operation) => ({
        subscribe: (sink) => ({
          unsubscribe: sseClient.subscribe(
            { ...operation, query: operation.query || '' },
            sink as never
          ),
        }),
      }),
    }),
  ],
});
