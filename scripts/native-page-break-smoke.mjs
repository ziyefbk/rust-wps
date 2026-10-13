// Real WebView2/Tauri PDF export; requires a native-smoke feature build.
import { chromium } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import path from 'node:path';

mkdirSync('tmp/native-pdf', { recursive: true });
const directory = mkdtempSync(path.resolve('tmp/native-pdf/page-breaks-'));
const executable = path.resolve('src-tauri/target/debug/rust-wps-desktop.exe');
const cli = path.resolve('target/debug/rust-wps.exe');
const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
const results = [];
for (const [name, sequence, expected] of [
  ['normal', ['First page', 'break', 'Second page'], ['First page', 'Second page']],
  ['leading', ['break', 'Second page'], ['', 'Second page']],
  ['consecutive', ['First page', 'break', 'break', 'Third page'], ['First page', '', 'Third page']],
  ['trailing', ['First page', 'break'], ['First page', '']],
]) {
  const model = path.join(directory, `${name}.json`);
  const input = path.join(directory, `${name}.docx`);
  const output = path.join(directory, `${name}.pdf`);
  writeFileSync(model, JSON.stringify({ type: 'doc', content: sequence.map(text => text === 'break' ? { type: 'pageBreak' } : { type: 'paragraph', content: [{ type: 'text', text }] }) }));
  execFileSync(cli, ['create-docx', model, input]);
  const originalInput = readFileSync(input);
  const reservation = createServer();
  await new Promise(resolve => reservation.listen(0, '127.0.0.1', resolve));
  const port = reservation.address().port;
  await new Promise(resolve => reservation.close(resolve));
  const child = spawn(executable, [], { windowsHide: true, stdio: 'ignore', env: {
    ...process.env, RUST_WPS_SMOKE_OPEN_INPUT: input, RUST_WPS_SMOKE_PDF_OUTPUT: output,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
  } });
  let browser;
  try {
    const deadline = Date.now() + 20000;
    while (!browser && Date.now() < deadline) {
      if (child.exitCode !== null) throw new Error(`Native app exited: ${child.exitCode}`);
      try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); }
      catch { await new Promise(resolve => setTimeout(resolve, 300)); }
    }
    if (!browser) throw new Error('Native CDP startup timed out');
    const context = browser.contexts()[0];
    const page = context.pages()[0] ?? await context.waitForEvent('page', { timeout: 10000 });
    await page.locator('#open').click();
    await page.locator('#editor-content .tiptap').waitFor({ timeout: 35000 });
    const breaks = await page.locator('#editor-content [data-page-break]').count();
    if (breaks !== sequence.filter(text => text === 'break').length) throw new Error(`${name}: wrong editor break count`);
    await page.locator('#edit-export-pdf').click();
    await page.getByRole('status').filter({ hasText: '已另存 PDF' }).waitFor({ timeout: 70000 });
    const report = JSON.parse(execFileSync(python, ['-c', 'import sys,json; from pypdf import PdfReader; r=PdfReader(sys.argv[1]); print(json.dumps({"pages":[p.extract_text() for p in r.pages],"sizes":[[float(p.mediabox.width),float(p.mediabox.height)] for p in r.pages]}))', output], { encoding: 'utf8' }));
    const canonical = text => text.normalize('NFKC').replace(/\s+/g, '');
    if (report.pages.length !== expected.length || report.pages.some((text, i) => canonical(text) !== canonical(expected[i]))) throw new Error(`${name}: unexpected page text ${JSON.stringify(report.pages)}`);
    if (report.sizes.some(([w, h]) => Math.abs(w - 595.28) > 1 || Math.abs(h - 841.89) > 1)) throw new Error(`${name}: non-A4 page`);
    if (!readFileSync(input).equals(originalInput)) throw new Error(`${name}: source was modified`);
    results.push({ name, output, sourcePreserved: true, ...report });
  } finally {
    if (browser) await browser.close().catch(() => {});
    child.kill();
  }
}
console.log(JSON.stringify({ nativeWebView2: true, actualTauriPdfExport: true, comparison: 'NFKC and whitespace normalization per page; raw text retained', results }, null, 2));
