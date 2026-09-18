import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  globalSetup: './e2e/global-setup.ts',
  timeout: 30_000,
  fullyParallel: false,
  reporter: [['list']],
  use: {
    ...devices['Desktop Chrome'],
    baseURL: 'http://127.0.0.1:1423',
    trace: 'on-first-retry',
  },
  webServer: {
    command: 'pnpm dev:e2e',
    url: 'http://127.0.0.1:1423',
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
    env: {
      E2E_FRONTEND_PORT: '1423',
    },
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
