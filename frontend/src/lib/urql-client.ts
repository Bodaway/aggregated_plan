import { Client, cacheExchange, fetchExchange, subscriptionExchange } from 'urql';
import { createClient as createSSEClient } from 'graphql-sse';

const API_URL = import.meta.env.VITE_API_URL || 'http://127.0.0.1:3001';

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
