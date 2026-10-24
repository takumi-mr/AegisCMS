import { defineConfig, devices } from '@playwright/test';

const isCI = !!process.env.CI;

// E2E tests against a real core server (e2e/start-server.mjs) serving the built Admin Studio.
// The tests share its database, so they run one at a time.
export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  workers: 1,
  forbidOnly: isCI,
  retries: isCI ? 1 : 0,
  reporter: isCI ? [['github'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: 'http://127.0.0.1:18080/admin/',
    locale: 'en-US',
    trace: 'on-first-retry',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    command: 'node e2e/start-server.mjs',
    url: 'http://127.0.0.1:18080/api/v1/health',
    // The first run compiles the server
    timeout: 15 * 60 * 1000,
    reuseExistingServer: !isCI,
    stdout: 'pipe',
  },
});
