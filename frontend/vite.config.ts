import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    port: 3000,
    proxy: {
      // Le client est devenu relatif (voir api-origin.ts). En dev le front est
      // sur 3000 et l'API sur 3001 : ce proxy recrée l'origine unique que
      // l'API fournit elle-même en production. `/graphql/sse` (client
      // graphql-sse) est un préfixe de `/graphql`, donc cette même entrée le
      // couvre aussi -- pas besoin de `ws: true`, l'SSE reste du HTTP simple.
      '/graphql': { target: 'http://127.0.0.1:3001', changeOrigin: false },
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./src/test-setup.ts'],
    exclude: ['e2e/**', 'node_modules/**'],
  },
});
