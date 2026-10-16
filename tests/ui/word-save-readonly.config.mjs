import { defineConfig } from '@playwright/test';
import { mkdtempSync, mkdirSync } from 'node:fs';
import path from 'node:path';
mkdirSync('tmp/word-save-readonly', {recursive:true});
const output = process.env.WORD_SAVE_READONLY_RUN ?? mkdtempSync(path.resolve('tmp/word-save-readonly/run-'));
process.env.WORD_SAVE_READONLY_RUN = output;
export default defineConfig({
  testDir: '.', testMatch: /word-save-readonly\.spec\.mjs$/, timeout:30000, workers:1,
  use: { headless:true, viewport:{width:1380,height:1000}, baseURL:'http://127.0.0.1:1428',
    launchOptions:{executablePath:process.env.CHROMIUM_PATH ?? '/usr/bin/chromium', args:['--no-sandbox','--disable-dev-shm-usage'], env:{...process.env,XDG_CONFIG_HOME:path.join(output,'chromium-config')}} },
  webServer:{ cwd:path.resolve('.'), command:'node node_modules/vite/bin/vite.js --host 127.0.0.1 --port 1428 --strictPort', url:'http://127.0.0.1:1428',reuseExistingServer:false },
  outputDir:path.join(output,'results'), reporter:[['list'],['json',{outputFile:path.join(output,'report.json')}]],
});
