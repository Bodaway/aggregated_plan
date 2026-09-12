import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { VitePWA } from 'vite-plugin-pwa';
import path from 'path';

export default defineConfig({
  plugins: [
    react(),
    VitePWA({
      registerType: 'autoUpdate',
      // Seule la coquille est précachée. Les réponses GraphQL ne le sont
      // jamais : un plan du jour périmé qui se présente comme le plan du jour
      // est pire qu'un écran qui dit franchement qu'il est hors ligne.
      workbox: {
        globPatterns: ['**/*.{js,css,html,svg,png,woff2}'],
        navigateFallback: '/index.html',
        navigateFallbackDenylist: [/^\/graphql/, /^\/auth/],
      },
      manifest: {
        name: 'aplan — cockpit',
        short_name: 'aplan',
        description: 'Plan du jour et capture rapide',
        // vite-plugin-pwa met `en` par défaut et ne le déduit pas du
        // `<html lang>` : sans cette ligne, iOS annonce une interface
        // française comme anglaise aux lecteurs d'écran.
        lang: 'fr',
        // L'icône de l'écran d'accueil tombe sur la capture : c'est l'usage
        // où le téléphone bat le poste.
        start_url: '/m/new',
        scope: '/m',
        display: 'standalone',
        background_color: '#0b0f14',
        theme_color: '#0b0f14',
        icons: [
          { src: '/icons/icon-192.png', sizes: '192x192', type: 'image/png' },
          { src: '/icons/icon-512.png', sizes: '512x512', type: 'image/png' },
          {
            src: '/icons/icon-maskable-512.png',
            sizes: '512x512',
            type: 'image/png',
            purpose: 'maskable',
          },
        ],
      },
    }),
  ],
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
