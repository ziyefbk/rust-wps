// Opt-in native WebView2 test. Build with --features native-smoke first.
// Uses the pinned, local DOC corpus fixture and a fresh draft/output directory.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import path from 'node:path';

const source = path.resolve('tmp/corpus/SampleDoc.doc');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const sourceBytes = readFileSync(source);
const sourceHash = hash(sourceBytes);
const corpus = JSON.parse(readFileSync('scripts/corpus.json', 'utf8'));
const fixture = corpus.fixtures.find(item => item.name === 'SampleDoc.doc');
if (!fixture || sourceBytes.length !== fixture.bytes || sourceHash !== fixture.sha256) {
  throw new Error('SampleDoc.doc does not match the pinned corpus manifest');
}

mkdirSync('tmp/native-doc-pages', { recursive: true });
const directory = mkdtempSync(path.resolve('tmp/native-doc-pages/run-'));
const input = path.join(directory, 'SampleDoc.doc');
const output = path.join(directory, 'DOC 手动分页.pdf');
const draftPath = path.join(directory, 'draft-v1.json');
writeFileSync(input, sourceBytes, { flag: 'wx' });

const reservation = createServer();
await new Promise((resolve, reject) => {
  reservation.once('error', reject);
  reservation.listen(0, '127.0.0.1', resolve);
});
const port = reservation.address().port;
await new Promise((resolve, reject) => reservation.close(error => error ? reject(error) : resolve()));

const child = spawn(path.resolve('src-tauri/target/debug/rust-wps-desktop.exe'), [], {
  windowsHide: true,
  stdio: ['ignore', 'pipe', 'pipe'],
  env: {
    ...process.env,
    RUST_WPS_SMOKE_PDF_OUTPUT: output,
    RUST_WPS_SMOKE_OPEN_INPUT: input,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
  },
});
let processError;
let processLog = '';
child.on('error', error => { processError = error; });
for (const stream of [child.stdout, child.stderr]) {
  // Drain both streams so a noisy worker cannot block the native app.
  stream.on('data', data => { processLog = (processLog + data.toString()).slice(-65536); });
}
const childClosed = new Promise(resolve => child.once('close', resolve));

function pageBreakCount(document) {
  const stack = [document];
  let count = 0;
  while (stack.length) {
    const node = stack.pop();
    if (node.type === 'pageBreak') count += 1;
    if (Array.isArray(node.content)) stack.push(...node.content);
  }
  return count;
}

let browser;
let page;
try {
  const deadline = Date.now() + 20000;
  while (!browser && Date.now() < deadline) {
    if (processError) throw processError;
    if (child.exitCode !== null || child.signalCode !== null) {
      throw new Error(`Native app exited: ${child.exitCode ?? child.signalCode}\n${processLog}`);
    }
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 });
    } catch {
      await new Promise(resolve => setTimeout(resolve, 300));
    }
  }
  if (!browser) throw new Error(`Native WebView2 CDP did not become ready\n${processLog}`);
  const context = browser.contexts()[0];
  page = context.pages()[0] ?? await context.waitForEvent('page', { timeout: 10000 });
  page.on('pageerror', error => { processLog += `\nPage error: ${error.stack ?? error}\n`; });
  // The controls exist in initial HTML before the deferred module installs
  // click handlers. DOMContentLoaded waits for those modules to execute.
  await page.waitForLoadState('domcontentloaded');
  await page.locator('#open').click();
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === '就绪'
    && (document.querySelector('#text-content')?.textContent.includes('I am a test document')
      || document.querySelector('#notice')?.classList.contains('error')),
  undefined, { timeout: 30000 });
  if (await page.locator('#notice').evaluate(node => node.classList.contains('error'))) {
    throw new Error(await page.locator('#notice').textContent());
  }
  await expect(page.locator('#text-content')).toContainText('This is page two');
  await page.locator('#import-document').click();
  await expect(page.getByRole('dialog')).toBeVisible({ timeout: 30000 });
  await expect(page.locator('#import-risks')).toContainText('已保留受支持的正文手动分页');
  await page.getByRole('button', { name: '接受变化并编辑副本', exact: true }).click();
  await expect(page.locator('#title')).toHaveText('SampleDoc.doc · 编辑副本');
  await expect(page.locator('#editor-content [data-page-break]')).toHaveCount(1);
  await expect(page.locator('#editor-content .tiptap')).toContainText('This is page 1');
  await expect(page.locator('#editor-content .tiptap')).toContainText('This is page two');
  await expect.poll(() => {
    try { return pageBreakCount(JSON.parse(readFileSync(draftPath, 'utf8')).document); }
    catch { return null; }
  }, { timeout: 10000 }).toBe(1);
  const draftBeforePrint = readFileSync(draftPath);

  await page.locator('#edit-export-pdf').click();
  await page.getByRole('status').filter({ hasText: '已另存 PDF' }).waitFor({ timeout: 70000 });
  const pdfBytes = readFileSync(output);
  if (!pdfBytes.subarray(0, 5).equals(Buffer.from('%PDF-'))) throw new Error('Invalid PDF signature');
  const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const pdf = JSON.parse(execFileSync(python, ['-c', [
    'import json, sys',
    'from pypdf import PdfReader',
    'reader = PdfReader(sys.argv[1])',
    'print(json.dumps({"pages": [{"width": float(p.mediabox.width), "height": float(p.mediabox.height), "text": p.extract_text() or ""} for p in reader.pages]}, ensure_ascii=True))',
  ].join('\n'), output], { encoding: 'utf8', windowsHide: true, timeout: 30000 }));
  writeFileSync(path.join(directory, 'pdf-extracted.json'), `${JSON.stringify(pdf, null, 2)}\n`, { flag: 'wx' });
  if (pdf.pages.length !== 2) throw new Error(`Expected 2 printed pages; got ${pdf.pages.length}`);
  // python-docx inspection of the paired SampleDoc.docx confirms the original
  // markers are "This is page 1" and "This is page two". Only whitespace and
  // Unicode compatibility normalization are allowed in PDF text comparison.
  const canonical = pdf.pages.map(item => item.text.normalize('NFKC').replace(/\s+/g, ''));
  if (!canonical[0].includes('Thisispage1') || canonical[0].includes('Thisispagetwo')) {
    throw new Error('First PDF page does not contain only the expected page marker');
  }
  if (!canonical[1].includes('Thisispagetwo') || canonical[1].includes('Thisispage1')) {
    throw new Error('Second PDF page does not contain only the expected page marker');
  }
  for (const item of pdf.pages) {
    if (Math.abs(item.width - 595.28) > 1 || Math.abs(item.height - 841.89) > 1) {
      throw new Error('Unexpected PDF page geometry');
    }
  }
  if (!readFileSync(draftPath).equals(draftBeforePrint)) throw new Error('Printing changed the imported draft');
  if (hash(readFileSync(source)) !== sourceHash || hash(readFileSync(input)) !== sourceHash) {
    throw new Error('Opening, importing or printing changed a DOC source file');
  }

  const report = {
    nativeWebView2: true,
    realDocWorkerAndImport: true,
    editorPageBreaks: 1,
    persistedDraftPageBreaks: 1,
    pdfPageMarkersSeparated: true,
    draftUnchangedByPrinting: true,
    originalAndIsolatedSourceUnchanged: true,
    sourceHash,
    input,
    output,
    comparison: 'pypdf; NFKC and whitespace normalization; raw page text retained below',
    ...pdf,
  };
  writeFileSync(path.join(directory, 'report.json'), `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' });
  console.log(JSON.stringify(report, null, 2));
} catch (error) {
  if (page) {
    const state = await page.evaluate(() => ({
      status: document.querySelector('#status')?.textContent,
      notice: document.querySelector('#notice')?.textContent,
      title: document.querySelector('#title')?.textContent,
      text: document.querySelector('#text-content')?.textContent,
      readyState: document.readyState,
    })).catch(failure => ({ error: String(failure) }));
    writeFileSync(path.join(directory, 'failure-state.json'), JSON.stringify(state, null, 2));
    console.error(JSON.stringify(state));
  }
  throw error;
} finally {
  try {
    if (browser) await browser.close().catch(() => {});
  } finally {
    // Kill only the app this test spawned and wait for its streams to close.
    if (child.exitCode === null && child.signalCode === null && !processError) child.kill();
    await childClosed;
    writeFileSync(path.join(directory, 'native-process.log'), processLog);
  }
}
