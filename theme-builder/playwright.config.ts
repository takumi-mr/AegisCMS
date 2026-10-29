import { defineConfig, devices } from '@playwright/test';

const isCI = !!process.env.CI;
const BUILDER_URL = 'http://localhost:15174';

// E2E tests against a real theme dev server (e2e/start-servers.mjs) with Theme Builder in front.
// The tests share the served theme, so they run one at a time.
export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  workers: 1,
  forbidOnly: isCI,
  retries: isCI ? 1 : 0,
  reporter: isCI ? [['github'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: `${BUILDER_URL}/`,
    locale: 'ja-JP',
    trace: 'on-first-retry',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    command: 'node e2e/start-servers.mjs',
    // The page is only useful once the dev server answers through the proxy
    url: `${BUILDER_URL}/__dev/api/files`,
    // The first run compiles the server
    timeout: 15 * 60 * 1000,
    reuseExistingServer: !isCI,
    stdout: 'pipe',
  },
});
