import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: '127.0.0.1',
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
