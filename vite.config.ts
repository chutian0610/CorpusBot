import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
declare const process: { env: Record<string, string | undefined> };

const apiProxyTarget = process.env.CORPUSBOT_API_PROXY_TARGET ?? 'http://127.0.0.1:1421';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: '127.0.0.1',
    proxy: {
      '/api': {
        target: apiProxyTarget,
        changeOrigin: true,
        ws: false,
      },
    },
    watch: {
      ignored: ['**/src-tauri/**', '**/.wiki-db/**'],
    },
  },
  build: {
    target: 'es2022',
    sourcemap: true,
  },
  test: {
    exclude: ['**/node_modules/**', 'e2e/**'],
  },
});
