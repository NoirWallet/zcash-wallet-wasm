import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  testMatch: 'voting_browser.spec.mjs',
  timeout: 60000,
  workers: 1,
  use: {
    baseURL: 'http://127.0.0.1:8000',
    launchOptions: {
      ...(process.env.CHROME ? { executablePath: process.env.CHROME } : {}),
      args: ['--no-sandbox'],
    },
  },
  projects: [
    { name: 'desktop', use: { viewport: { width: 1440, height: 900 } } },
    { name: 'mobile', use: { viewport: { width: 390, height: 844 } } },
  ],
  webServer: {
    command: 'python3 serve.py --port 8000 --host 127.0.0.1',
    url: 'http://127.0.0.1:8000',
    reuseExistingServer: !process.env.CI,
  },
});
