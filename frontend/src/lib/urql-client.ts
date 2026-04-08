import { Client, cacheExchange, fetchExchange, subscriptionExchange } from 'urql';
import { createClient as createSSEClient } from 'graphql-sse';

const API_URL = import.meta.env.VITE_API_URL || 'http://127.0.0.1:3001';

const sseClient = createSSEClient({
  url: `${API_URL}/graphql/sse`,
});

export const urqlClient = new Client({
  url: `${API_URL}/graphql`,
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
