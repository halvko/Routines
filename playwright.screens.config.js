// Screenshots for the visual reviewer: `npm run screenshots`. See test/visual/.
import base from './playwright.config.js';
import { defineConfig } from '@playwright/test';

export default defineConfig({
  ...base,
  testDir: 'test/visual',
  retries: 0,
  reporter: 'list',
  use: { ...base.use, deviceScaleFactor: 2 },
});
