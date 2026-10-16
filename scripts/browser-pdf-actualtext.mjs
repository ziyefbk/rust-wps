// Real Chromium, actual main.ts/PDF.js, explicitly simulated native transport.
// The identified candidate CLI supplies unchanged inspection reports for the
// workflow PDFs. Provisional-recovery controls are separately labeled below.
import assert from 'node:assert/strict';
import { chromium, expect } from '@playwright/test';
import { createServer } from 'vite';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, statfsSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { nativeBridge } from '../tests/ui/bridge.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..'); process.chdir(root);
const option = name => { const index = process.argv.indexOf(`--${name}`); return index < 0 ? null : process.argv[index + 1]; };
assert(option('cli') && option('cli-sha256') && option('preview-cli') && option('preview-cli-sha256') && option('semantic-manifest'), 'Pass exact --cli/--cli-sha256, --preview-cli/--preview-cli-sha256, and --semantic-manifest; no old-binary fallback');
const hash = file => createHash('sha256').update(readFileSync(file)).digest('hex');
const cli = path.resolve(option('cli')), expectedCliHash = option('cli-sha256');
const previewCli = path.resolve(option('preview-cli')), expectedPreviewHash = option('preview-cli-sha256');
assert.equal(hash(previewCli), expectedPreviewHash, 'Candidate preview adapter identity mismatch');
assert.equal(hash(cli), expectedCliHash, 'Candidate CLI identity mismatch');
const available = statfsSync(root); assert(available.bavail * available.bsize > 1024 ** 3, 'Keep the 1 GiB resource floor');
mkdirSync('tmp/browser-pdf-actualtext', { recursive: true });
const directory = option('output') ? path.resolve(option('output')) : mkdtempSync(path.resolve('tmp/browser-pdf-actualtext/run-'));
if (option('output')) mkdirSync(directory, { recursive: false });
const sourceDirectory = path.resolve(option('workflow-source-dir') ?? 'tmp/browser-word-workflow/run-yMJlKi');
const output = name => path.join(directory, name);
const writeJson = (name, value) => writeFileSync(output(name), `${JSON.stringify(value, null, 2)}\n`, { flag: 'wx' });
const report = { started: new Date().toISOString(), directory, cli: { path: cli, sha256: hash(cli) }, previewCli: { path: previewCli, sha256: hash(previewCli) },
  sourceCode: ['ui/main.ts', 'ui/pdf-text.ts', 'ui/index.html', 'tests/ui/bridge.mjs', 'scripts/browser-pdf-actualtext.mjs'].map(file => ({ file, sha256: hash(file) })),
  browser: 'Real headless Chromium, actual UI source served by Vite', pdfjsVersion: JSON.parse(readFileSync('node_modules/pdfjs-dist/package.json', 'utf8')).version, nativeIpc: false, systemDialogs: false,
  limitations: ['Native transport and session token issuance are explicit test adapters', 'Only the old transport missing-evidence envelope is simulated; unsupported/encrypted inspection comes from the pinned production preview API', 'No native WebView, Windows printing, IME, system clipboard or general Unicode normalization claim'],
  protectedSources: [], calls: [], cases: [], pageErrors: [], passed: false };
let browser, server, page, current, next, failOpen = false, counter = 0;
try {
  const canonical = text => text.replace(/\s/gu, '');
  const inspect = (file, label) => {
    assert.equal(hash(cli), expectedCliHash, 'Candidate changed during browser verification');
    const result = spawnSync(cli, ['inspect', file], { encoding: 'utf8', timeout: 30000, maxBuffer: 4 * 1024 * 1024 });
    writeFileSync(output(`${label}.stdout.json`), result.stdout ?? '', { flag: 'wx' });
    writeFileSync(output(`${label}.stderr.txt`), result.stderr ?? '', { flag: 'wx' });
    assert.equal(result.status, 0, `${label}: ${result.stderr ?? result.error}`);
    return JSON.parse(result.stdout);
  };
  const preview = (file, label) => {
    assert.equal(hash(previewCli), expectedPreviewHash);
    const result = spawnSync(previewCli, [file], { encoding: 'utf8', timeout: 30000, maxBuffer: 4 * 1024 * 1024 });
    writeFileSync(output(`${label}.preview.stdout.json`), result.stdout ?? '', { flag: 'wx' });
    writeFileSync(output(`${label}.preview.stderr.txt`), result.stderr ?? '', { flag: 'wx' });
    assert.equal(result.status, 0, `${label}: ${result.stderr ?? result.error}`);
    const receipt = JSON.parse(result.stdout);
    assert.equal(receipt.pdf_identity.bytes, readFileSync(file).length);
    assert.equal(Buffer.from(receipt.pdf_identity.sha256).toString('hex'), hash(file));
    return receipt.inspection;
  };
  const semanticManifest = path.resolve(option('semantic-manifest'));
  report.protectedSources.push({ file: semanticManifest, sha256: hash(semanticManifest) });
  const semanticCases = JSON.parse(readFileSync(semanticManifest, 'utf8')).cases;
  const semanticCase = name => {
    const fixture = semanticCases.find(fixture => fixture.name === name); assert(fixture, `Missing independent semantic fixture ${name}`);
    assert.equal(hash(fixture.path), fixture.sha256, `Independent fixture changed: ${name}`);
    report.protectedSources.push({ file: fixture.path, sha256: fixture.sha256 }); return fixture;
  };
  const emptySource = semanticCase('all-empty-replacement'), unsupportedSource = semanticCase('utf16-odd');
  const radicalSource = semanticCase('true-radical-preserved'), literalRadicalSource = semanticCase('literal-radical-no-actual');
  const radicalInspection = inspect(radicalSource.path, 'true-radical-preserved'), literalRadicalInspection = inspect(literalRadicalSource.path, 'literal-radical-no-actual');
  assert.equal(radicalInspection.pdf_actual_text, 'applied'); assert.equal(canonical(radicalInspection.text), canonical(radicalSource.expected_text));
  assert.equal(literalRadicalInspection.pdf_actual_text, 'not_detected'); assert.equal(canonical(literalRadicalInspection.text), canonical(literalRadicalSource.expected_text));
  const emptyInspection = inspect(emptySource.path, 'all-empty-replacement');
  assert.equal(emptyInspection.pdf_actual_text, 'applied'); assert.equal(emptyInspection.text, emptySource.expected_text);
  const unsupportedInspection = preview(unsupportedSource.path, 'utf16-odd');
  assert.equal(unsupportedInspection.pdf_actual_text, 'unsupported'); assert(unsupportedInspection.text_error);
  const encryptedFile = path.resolve('tests/fixtures/encrypted-text.pdf');
  report.protectedSources.push({ file: encryptedFile, sha256: hash(encryptedFile) });
  const encryptedInspection = preview(encryptedFile, 'encrypted-text');
  assert.equal(encryptedInspection.pdf_actual_text, 'unknown'); assert(encryptedInspection.text_error);
  const fixtureNames = ['new-document-browser-print', 'minutes-browser-print', 'genuine-table-browser-print'];
  const fixtures = fixtureNames.map(name => {
    const file = path.join(sourceDirectory, `${name}.pdf`), oracle = path.join(sourceDirectory, `${name}.expected.json`);
    report.protectedSources.push({ file, sha256: hash(file) }, { file: oracle, sha256: hash(oracle) });
    const inspection = inspect(file, name), expected = JSON.parse(readFileSync(oracle, 'utf8'));
    assert.equal(canonical(inspection.text), canonical(expected.full_text), `${name}: candidate must correct full semantic text before UI testing`);
    const expectedStatus = name === 'genuine-table-browser-print' ? 'not_detected' : 'applied';
    assert.equal(inspection.pdf_actual_text, expectedStatus, `${name}: source evidence must match independently inspected content`);
    return { name, file, inspection, expected };
  });
  // A tiny ordinary PDF control is intentionally untagged and contains no
  // ActualText. Its strict inspection still comes from the candidate backend.
  function ordinaryPdf() {
    const content = 'BT /F1 12 Tf 20 100 Td (Ordinary recovery text) Tj ET';
    const objects = ['<< /Type /Catalog /Pages 2 0 R >>', '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
      '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 150] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>',
      '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>', `<< /Length ${Buffer.byteLength(content)} >>\nstream\n${content}\nendstream`];
    let pdf = '%PDF-1.4\n'; const offsets = [0];
    for (let i = 0; i < objects.length; i++) { offsets.push(Buffer.byteLength(pdf)); pdf += `${i + 1} 0 obj\n${objects[i]}\nendobj\n`; }
    const xref = Buffer.byteLength(pdf); pdf += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
    for (const offset of offsets.slice(1)) pdf += `${String(offset).padStart(10, '0')} 00000 n \n`;
    return `${pdf}trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  }
  const ordinaryFile = output('ordinary-untagged-control.pdf'); writeFileSync(ordinaryFile, ordinaryPdf(), { flag: 'wx' });
  const ordinaryInspection = inspect(ordinaryFile, 'ordinary-control'); assert.equal(ordinaryInspection.pdf_actual_text, 'not_detected');
  const sources = new Map([...fixtures.map(f => [f.file, hash(f.file)]), [ordinaryFile, hash(ordinaryFile)], [emptySource.path, hash(emptySource.path)], [unsupportedSource.path, hash(unsupportedSource.path)], [encryptedFile, hash(encryptedFile)], [radicalSource.path, hash(radicalSource.path)], [literalRadicalSource.path, hash(literalRadicalSource.path)]]);
  async function transport(command, args) {
    report.calls.push({ command, args });
    if (command === 'open_document') {
      if (failOpen) { failOpen = false; throw new Error('Simulated file-open failure'); }
      if (!next) return null;
      current = next; next = null;
      current.token = String(++counter);
      return { session_token: current.token, name: path.basename(current.file), path: current.file, inspection: structuredClone(current.inspection) };
    }
    if (command === 'read_current_pdf') {
      assert(current); assert.equal(args.sessionToken, current.token); assert.equal(hash(current.file), sources.get(current.file));
      return [...readFileSync(current.file)];
    }
    throw new Error(`Unexpected test-host command: ${command}`);
  }
  async function open(fixture) {
    next = { file: fixture.file, inspection: structuredClone(fixture.inspection) };
    await page.locator('#open').click();
    if (fixture.password) {
      await expect(page.locator('#pdf-password')).toBeVisible();
      await page.getByLabel('PDF 密码', { exact: true }).fill(fixture.password);
      await page.getByLabel('PDF 密码', { exact: true }).press('Enter');
    }
    await expect(page.locator('#open')).toBeEnabled();
    await expect(page.locator('#page-position')).toHaveText(`1 / ${fixture.expected?.pages ?? 1}`);
    await expect(page.locator('canvas.pdf-page')).toBeVisible();
    await page.locator('#text-view').click();
  }
  async function exactText(expected) {
    await expect.poll(async () => canonical(await page.locator('#text-content').innerText())).toBe(canonical(expected));
  }
  async function search(query, expectedCount = 1) {
    await page.locator('#search').fill(query);
    await expect(page.locator('#text-content mark')).toHaveCount(expectedCount);
    const marks = await page.locator('#text-content mark').allTextContents();
    assert(marks.every(mark => mark === query), 'Literal search highlights unchanged source text');
  }
  function passed(name, details = {}) { report.cases.push({ name, passed: true, ...details }); console.log(JSON.stringify({ passed: name, ...details })); }
  server = await createServer({ configFile: path.join(root, 'vite.config.ts'), server: { host: '127.0.0.1', port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer.address(); assert(address && typeof address !== 'string');
  browser = await chromium.launch({ executablePath: option('browser') ?? '/usr/bin/chromium', headless: true,
    args: ['--no-sandbox', '--disable-dev-shm-usage'], env: { ...process.env, XDG_CONFIG_HOME: output('chromium-config') } });
  report.browserVersion = browser.version();
  page = await browser.newPage({ viewport: { width: 1380, height: 1000 } }); page.setDefaultTimeout(15000);
  page.on('pageerror', error => report.pageErrors.push(String(error)));
  await nativeBridge(page, transport); await page.goto(`http://127.0.0.1:${address.port}`);
  for (const fixture of fixtures.filter(fixture => fixture.inspection.pdf_actual_text === 'applied')) {
    await open(fixture); await exactText(fixture.expected.full_text);
    await expect(page.locator('#pdf-semantic-hint')).toContainText('受支持的 ActualText');
    await expect(page.locator('#extract-pdf-text')).toBeDisabled();
    const query = fixture.name.startsWith('new-') ? '日常文档' : fixture.name.startsWith('minutes-') ? '参会人员' : '已核对';
    await search(query);
    // Native disabled .click() and synthetic delivery both exercise actual
    // bindings; the latter proves the handler's independent protection.
    for (let i = 0; i < 3; i++) {
      await page.evaluate(() => { const button = document.getElementById('extract-pdf-text'); button.click(); button.dispatchEvent(new MouseEvent('click', { bubbles: true })); });
      await expect(page.locator('#open')).toBeEnabled(); await exactText(fixture.expected.full_text); await search(query);
    }
    await page.screenshot({ path: output(`${fixture.name}-search.png`), fullPage: true });
    writeJson(`${fixture.name}-browser.json`, { text: await page.locator('#text-content').innerText(), query, marks: await page.locator('#text-content mark').allTextContents(), hint: await page.locator('#pdf-semantic-hint').innerText() });
    passed(`Applied backend text remains searchable after repeated extraction attempts: ${fixture.name}`);
  }
  const beforeCancel = await page.locator('#text-content').innerText(), beforeTitle = await page.locator('#title').innerText();
  await page.locator('#open').click(); await expect(page.locator('#open')).toBeEnabled(); await exactText(beforeCancel);
  failOpen = true; await page.locator('#open').click(); await expect(page.locator('#notice')).toContainText('Simulated file-open failure');
  await exactText(beforeCancel); await expect(page.locator('#title')).toHaveText(beforeTitle);
  await expect(page.locator('#pdf-semantic-hint')).toContainText('受支持的 ActualText');
  passed('Canceled and failed opens preserve the applied source text and evidence');

  await open({ file: emptySource.path, inspection: emptyInspection });
  await expect(page.locator('#text-content')).toContainText('源 ActualText 提取后的正文为空');
  await expect(page.locator('#text-content')).not.toContainText('DISAPPEAR');
  await expect(page.locator('#search')).toBeDisabled(); await expect(page.locator('#extract-pdf-text')).toBeDisabled();
  await page.evaluate(() => document.getElementById('extract-pdf-text').dispatchEvent(new MouseEvent('click', { bubbles: true })));
  await expect(page.locator('#open')).toBeEnabled(); await expect(page.locator('#text-content')).not.toContainText('DISAPPEAR');
  await expect(page.locator('#pdf-semantic-hint')).toContainText('受支持的 ActualText');
  await page.screenshot({ path: output('intentional-empty-actualtext.png'), fullPage: true });
  passed('Actual candidate intentional-empty ActualText remains empty after page rendering and manual extraction delivery');

  await open({ file: radicalSource.path, inspection: radicalInspection }); await exactText(radicalSource.expected_text);
  await search('文'); await search('⽂'); await expect(page.locator('#extract-pdf-text')).toBeDisabled();
  passed('Applied ActualText changes only its own occurrence; a genuine neighboring Kangxi radical remains separately searchable');
  await open({ file: literalRadicalSource.path, inspection: literalRadicalInspection }); await exactText(literalRadicalSource.expected_text);
  await page.locator('#extract-pdf-text').click(); await expect(page.locator('#open')).toBeEnabled();
  await exactText(literalRadicalSource.expected_text); await search('文', 0); await search('⽂');
  passed('Ordinary PDF containing a genuine Kangxi radical is never Unicode-normalized by re-extraction or search');

  const tableFixture = fixtures.find(fixture => fixture.name === 'genuine-table-browser-print');
  await open(tableFixture); await exactText(tableFixture.expected.full_text);
  await expect(page.locator('#pdf-semantic-hint')).toBeHidden(); await expect(page.locator('#extract-pdf-text')).toBeEnabled();
  await page.locator('#extract-pdf-text').click(); await expect(page.locator('#open')).toBeEnabled();
  await exactText(tableFixture.expected.full_text); await search('已核对');
  await page.screenshot({ path: output('genuine-table-browser-print-search.png'), fullPage: true });
  passed('Workflow table with no ActualText preserves ordinary re-extraction and exact Chinese search');

  await open({ file: ordinaryFile, inspection: ordinaryInspection });
  await expect(page.locator('#pdf-semantic-hint')).toBeHidden(); await expect(page.locator('#extract-pdf-text')).toBeEnabled();
  await page.locator('#extract-pdf-text').click(); await expect(page.locator('#open')).toBeEnabled();
  await exactText('Ordinary recovery text'); await search('recovery');
  passed('not_detected ordinary PDF still permits normal PDF.js extraction and search');

  const legacyInspection = { ...ordinaryInspection, text: '', text_error: 'Simulated older transport failure' }; delete legacyInspection.pdf_actual_text;
  const recoveryCases = [
    { name: 'Actual unsupported semantic preview', file: unsupportedSource.path, inspection: unsupportedInspection, expectedText: 'A', query: 'A', count: 1 },
    { name: 'Actual encrypted unknown semantic preview', file: encryptedFile, inspection: encryptedInspection,
      expected: { pages: 2 }, password: 'test-password', expectedText: 'First page searchable secret\n\nSecond page searchable text', query: 'searchable', count: 2 },
    { name: 'Simulated older transport missing evidence', file: ordinaryFile, inspection: legacyInspection, expectedText: 'Ordinary recovery text', query: 'recovery', count: 1 },
  ];
  for (const fixture of recoveryCases) {
    await open(fixture); await exactText(fixture.expectedText);
    await expect(page.locator('#pdf-semantic-hint')).toContainText('临时字体正文');
    await expect(page.locator('#notice')).toContainText('临时字体正文');
    await page.locator('#extract-pdf-text').click(); await expect(page.locator('#open')).toBeEnabled();
    await exactText(fixture.expectedText); await expect(page.locator('#pdf-semantic-hint')).toContainText('不能保证搜索或复制');
    // Stop in the same event turn while the real PDF.js page request is in
    // flight. The completed earlier text must survive the empty canceled retry.
    await page.evaluate(() => { document.getElementById('extract-pdf-text').click(); document.getElementById('stop-pdf-text').click(); });
    await expect(page.locator('#open')).toBeEnabled(); await expect(page.locator('#notice')).toContainText('已停止');
    await exactText(fixture.expectedText); await search(fixture.query, fixture.count);
    await expect(page.locator('#pdf-semantic-hint')).toContainText('临时字体正文');
    await expect(page.locator('#stop-pdf-text')).toBeHidden();
    passed(`${fixture.name}: real PDF.js recovery, repeated extraction, cancellation and persistent warning`);
  }
  await open(fixtures[0]); await exactText(fixtures[0].expected.full_text); await search('日常文档');
  await expect(page.locator('#pdf-semantic-hint')).toContainText('受支持的 ActualText');
  await expect(page.locator('#pdf-semantic-hint')).not.toContainText('临时'); await expect(page.locator('#extract-pdf-text')).toBeDisabled();
  await page.screenshot({ path: output('source-switch-back-search.png'), fullPage: true });
  passed('Source switching back to applied evidence clears old provisional warning and preserves exact Chinese search');
  assert.deepEqual(report.pageErrors, [], 'No uncaught browser errors');
  for (const source of report.protectedSources) assert.equal(hash(source.file), source.sha256, `Protected source changed: ${source.file}`);
  assert.equal(hash(cli), expectedCliHash); assert.equal(hash(previewCli), expectedPreviewHash); report.passed = true;
} catch (error) { report.error = String(error?.stack ?? error); throw error; }
finally {
  await browser?.close(); await server?.close(); report.finished = new Date().toISOString();
  writeJson('report.json', report); console.log(JSON.stringify({ report: output('report.json'), passed: report.passed, cases: report.cases.length }));
}
