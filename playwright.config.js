import { defineConfig, devices } from '@playwright/test';

const PORT = 8090;

export default defineConfig({
  testDir: 'test/e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['github'], ['list']] : 'list',
  use: {
    baseURL: `http://localhost:${PORT}/Routines/`,
    viewport: { width: 390, height: 844 },
    trace: 'off',
  },
  // Every test gets a fresh browser context, so localStorage starts empty (seed routines only).
  projects: [{ name: 'phone', use: { ...devices['Desktop Chrome'], viewport: { width: 390, height: 844 } } }],
  webServer: {
    command: `node tools/serve.mjs ${PORT} ${process.env.APP_DIR || '.'}`,
    url: `http://localhost:${PORT}/Routines/`,
    reuseExistingServer: !process.env.CI,
  },
});
