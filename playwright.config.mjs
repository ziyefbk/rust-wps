import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests/ui', timeout: 30000, workers: 1,
  use: { channel: 'msedge', headless: true, viewport: { width: 1180, height: 820 }, baseURL: 'http://127.0.0.1:1421' },
  webServer: { command: 'npx vite preview --host 127.0.0.1 --port 1421 --strictPort', url: 'http://127.0.0.1:1421', reuseExistingServer: false },
  outputDir: 'tmp/ui-tests', reporter: 'list',
});
