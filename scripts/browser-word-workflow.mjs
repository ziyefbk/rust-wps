// Real Chromium + production UI/TipTap, with explicitly simulated Tauri host.
// CLI handles actual DOCX inspection/import/export/trusted-model reopening.
// Drafts use an isolated JSON-on-disk TEST adapter, not the Rust draft backend.
// No native GUI, native IPC, system dialog/clipboard/IME or Windows PDF claim.
import assert from 'node:assert/strict';
import { chromium, expect } from '@playwright/test';
import { createServer } from 'vite';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { nativeBridge } from '../tests/ui/bridge.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
process.chdir(root);
function option(name) { const i = process.argv.indexOf(`--${name}`); return i < 0 ? null : process.argv[i + 1]; }
const cli = path.resolve(option('cli') ?? '');
const source = path.resolve(option('source') ?? '');
const oracle = path.resolve(option('expected') ?? '');
const expectedCliHash = option('cli-sha256');
if (!option('cli') || !option('source') || !option('expected') || !expectedCliHash) {
  throw new Error('Required: --cli FILE --cli-sha256 SHA256 --source independent.docx --expected independent.expected.json');
}
const hash = file => createHash('sha256').update(readFileSync(file)).digest('hex');
assert.equal(hash(cli), expectedCliHash, 'CLI must match the explicitly chosen binary');
const sourceExpected = JSON.parse(readFileSync(oracle, 'utf8'));
assert.equal(hash(source), sourceExpected.sha256, 'Independent source must match its frozen oracle');
const genuine = path.resolve('tests/fixtures/docx-cell-no-fill/Bug60337.docx');
assert.equal(hash(genuine), '87f983bdd93b3b5b8469104a5b09ccff41e0f5c144060e13fd4db07bfd2e39da');
mkdirSync('tmp/browser-word-workflow', { recursive: true });
const directory = mkdtempSync(path.resolve('tmp/browser-word-workflow/run-'));
const draftPath = path.join(directory, 'test-adapter-draft.json');
const protectedInputs = [cli, source, oracle, genuine].map(file => ({ file, sha256: hash(file) }));
function filesUnder(dir) { return readdirSync(dir).flatMap(name => { const file = path.join(dir, name); return statSync(file).isDirectory() ? filesUnder(file) : [file]; }); }
const testedSourceFiles = [...filesUnder(path.resolve('ui')), path.resolve('scripts/browser-word-workflow.mjs'), path.resolve('scripts/browser-word-workflow-check.py'), path.resolve('tests/ui/bridge.mjs'), path.resolve('tests/ui/word-save-readonly.spec.mjs'), path.resolve('tests/ui/word-save-readonly.config.mjs')].map(file => ({ file: path.relative(root, file), sha256: hash(file) }));
const report = { directory, started: new Date().toISOString(), browser: 'Real headless Chromium',
  nativeGui: false, nativeTauriIpc: false, systemDialogs: false, chineseIme: false, systemClipboard: false,
  frontend: 'Actual repository UI source modules served with Vite development transforms on loopback; no editor substitution; not a production dist bundle',
  docxBackend: 'Explicitly identified production CLI, invoked through test Tauri transport',
  draftBackend: 'Isolated persisted JSON test adapter; not production Rust draft validation/atomicity',
  pdfBackend: 'Actual production Print snapshot and Chromium page.pdf; not Windows direct export or system printing',
  protectedInputs, testedSourceFiles, cases: [], calls: [], pageErrors: [], dialogs: [], unexpectedDialogs: [], strictPdfFailures: [], functionalFailures: [], productPdfTextFailures: [], limitations: [
    'Independent authored Chinese fixture is not a genuine third-party Chinese document',
    'Real external corpus sample is an English Apache POI table',
    'Text inserted with Playwright insertText; not real Chinese IME composition',
    'Fresh Chromium processes exercise persisted test draft recovery, not native desktop shutdown',
    'Font availability and Chromium/LibreOffice layout do not establish Microsoft Word/WPS fidelity',
  ] };
const state = { nextOpen: null, nextSave: null, current: null, draftWrites: 0, failDraft: false, expectedDialogs: [], holdSave: false, pendingSave: null, destroyed: false };
const allowedInputs = new Set([source, genuine]);
let counter = 0, browser, page, server;
const saveJson = (name, value) => writeFileSync(path.join(directory, name), `${JSON.stringify(value, null, 2)}\n`, { flag: 'wx' });
function command(executable, args, label, allowFailure = false) {
  const result = spawnSync(executable, args, { encoding: 'utf8', timeout: 30000, maxBuffer: 4 * 1024 * 1024 });
  const prefix = `${String(++counter).padStart(3, '0')}-${label}`;
  writeFileSync(path.join(directory, `${prefix}.stdout`), result.stdout ?? '', { flag: 'wx' });
  writeFileSync(path.join(directory, `${prefix}.stderr`), result.stderr ?? '', { flag: 'wx' });
  report.calls.push({ executable, args, label, status: result.status, error: result.error?.message, evidence: prefix });
  if (result.error || (result.status !== 0 && !allowFailure)) throw new Error((result.stderr || result.error?.message || `${label} exited ${result.status}`).trim());
  return result.stdout;
}
const cliJson = (args, label) => JSON.parse(command(cli, args, label));
function independent(mode, file, expected, name) {
  const expectedPath = path.join(directory, `${name}.expected.json`);
  saveJson(`${name}.expected.json`, expected);
  const result = JSON.parse(command('python3', [path.resolve('scripts/browser-word-workflow-check.py'), mode, file, '--expected', expectedPath], `${name}-independent`, mode === 'pdf'));
  if (mode === 'pdf' && !result.passed) report.strictPdfFailures.push({ name, errors: result.errors });
  saveJson(`${name}.inspection.json`, result);
  return result;
}
function casePassed(name, detail = {}) {
  report.cases.push({ name, passed: true, ...detail });
  console.log(JSON.stringify({ passed: name, ...detail }));
}
function output(name) { assert.equal(path.basename(name), name); return path.join(directory, name); }
function readDraft() { return JSON.parse(readFileSync(draftPath, 'utf8')).document; }
async function durableContains(text) {
  await expect.poll(() => existsSync(draftPath) ? JSON.stringify(readDraft()) : '', { timeout: 10000 }).toContain(text);
}
async function transport(commandName, args) {
  report.calls.push({ transport: commandName, documentSnapshot: args?.doc ? createHash('sha256').update(JSON.stringify(args.doc)).digest('hex') : undefined });
  if (commandName === 'save_draft') {
    if (state.failDraft) throw new Error('Workflow test: isolated draft write failure');
    const temp = output(`draft-write-${++state.draftWrites}.json`);
    writeFileSync(temp, JSON.stringify({ testAdapter: true, document: args.doc }), { flag: 'wx' });
    renameSync(temp, draftPath);
    return;
  }
  if (commandName === 'load_draft') return existsSync(draftPath) ? readDraft() : null;
  if (commandName === 'open_document') {
    const selected = state.nextOpen; state.nextOpen = null;
    if (!selected) return null;
    assert.ok(allowedInputs.has(selected), 'Only explicit source or run output may be opened');
    const inspection = cliJson(['inspect', selected], 'open-inspect');
    let authored = null;
    try { authored = cliJson(['read-docx-model', selected], 'open-trusted-model'); }
    catch (error) { if (!String(error).includes('No verified editable document model')) throw error; }
    state.current = selected;
    return { name: path.basename(selected), path: selected, inspection, authored };
  }
  if (commandName === 'import_current') {
    assert.ok(allowedInputs.has(state.current));
    const imported = cliJson(['import-docx', state.current], 'external-import');
    saveJson(`import-${counter}.json`, imported);
    return imported;
  }
  if (commandName === 'save_new_document') {
    const selected = state.nextSave; state.nextSave = null;
    if (state.holdSave) await new Promise(resolve => { state.pendingSave = resolve; });
    if (!selected) return null;
    assert.equal(path.dirname(selected), directory, 'All output must remain in unique run directory');
    const model = output(`save-snapshot-${counter + 1}.json`);
    writeFileSync(model, JSON.stringify(args.doc), { flag: 'wx' });
    const exported = cliJson(['create-docx', model, selected], 'save-docx');
    allowedInputs.add(selected);
    return { path: selected, report: exported };
  }
  if (commandName === 'plugin:window|destroy') { state.destroyed = true; return; }
  if (commandName === 'export_current_pdf') throw new Error('Browser test host has no native direct PDF export; use Print');
  throw new Error(`Unexpected test-host command: ${commandName}`);
}
async function freshBrowser() {
  if (browser) await browser.close();
  browser = await chromium.launch({ executablePath: option('browser') ?? '/usr/bin/chromium', headless: true,
    args: ['--no-sandbox', '--disable-dev-shm-usage'],
    env: { ...process.env, XDG_CONFIG_HOME: output('chromium-config') } });
  const context = await browser.newContext({ viewport: { width: 1380, height: 1000 } });
  page = await context.newPage();
  page.setDefaultTimeout(15000);
  page.on('pageerror', error => report.pageErrors.push(String(error)));
  page.on('dialog', async dialog => {
    const expected = state.expectedDialogs.shift();
    report.dialogs.push({ type: dialog.type(), message: dialog.message(), expected: expected ?? null });
    if (!expected || dialog.type() !== 'confirm' || !dialog.message().includes('尚未导出 DOCX')) {
      report.unexpectedDialogs.push({ type: dialog.type(), message: dialog.message() }); await dialog.dismiss(); return;
    }
    if (expected.accept) await dialog.accept(); else await dialog.dismiss();
  });
  await nativeBridge(page, transport);
  await page.addInitScript(() => { window.print = () => { window.workflowPrintCalls = (window.workflowPrintCalls ?? 0) + 1; }; });
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/`);
  await page.waitForLoadState('networkidle');
  return page;
}
async function confirmLeave(action, accept) {
  state.expectedDialogs.push({ accept }); await action();
  await expect.poll(() => state.expectedDialogs.length).toBe(0);
}
async function open(file) {
  state.nextOpen = file;
  await page.locator('#open').click();
  await expect(page.locator('#title')).toHaveText(path.basename(file));
}
async function save(file) {
  state.nextSave = file;
  await page.locator('#edit-save').click();
  await expect(page.locator('#notice')).toContainText(`已导出 DOCX：${file}`);
  await expect(page.locator('#edit-save')).toBeEnabled();
}
async function importSource(file, testCancel = false) {
  await open(file);
  await page.locator('#import-document').click();
  await expect(page.getByRole('dialog')).toBeVisible();
  if (testCancel) {
    await page.getByRole('button', { name: '取消', exact: true }).click();
    await expect(page.locator('.tiptap')).toHaveCount(0);
    await expect(page.locator('#text-content')).toContainText(sourceExpected.paragraphs[0]);
    await page.locator('#import-document').click();
    await expect(page.getByRole('dialog')).toBeVisible();
  }
  await page.getByRole('button', { name: '接受变化并编辑副本', exact: true }).click();
  await expect(page.locator('#editor-content .tiptap')).toBeVisible();
}
async function printPdf(name, expected, mutateAfterSnapshot = false) {
  const before = existsSync(draftPath) ? hash(draftPath) : null;
  await page.locator('#edit-print').click();
  await expect.poll(() => page.evaluate(() => window.workflowPrintCalls ?? 0)).toBeGreaterThan(0);
  await expect(page.locator('#notice')).toContainText('打印副本');
  if (mutateAfterSnapshot) {
    const last = page.locator('#editor-content .tiptap > p').last();
    await last.click(); await page.keyboard.press('Control+End'); await page.keyboard.insertText('打印后新增内容');
    await expect(page.locator('#print-document')).not.toContainText('打印后新增内容');
  } else if (before) assert.equal(hash(draftPath), before, 'Print must not mutate the durable draft');
  await page.screenshot({ path: output(`${name}-editor.png`), fullPage: true });
  await page.emulateMedia({ media: 'print' });
  await expect(page.locator('.app')).toBeHidden();
  await page.pdf({ path: output(`${name}.pdf`), preferCSSPageSize: true, printBackground: true, displayHeaderFooter: false });
  await page.emulateMedia({ media: 'screen' });
  const result = independent('pdf', output(`${name}.pdf`), { ...expected, full_text: expected.contains.join('\n') }, name);
  const reopened = cliJson(['inspect', output(`${name}.pdf`)], `${name}-product-reopen`);
  saveJson(`${name}.product-inspection.json`, reopened);
  const canonical = text => text.replace(/\s/g, '');
  if (canonical(reopened.text) !== canonical(expected.contains.join('\n'))) {
    report.productPdfTextFailures.push({ name, expected: expected.contains.join('\n'), actual: reopened.text, route: 'Actual release42 CLI inspect of browser-created PDF' });
  }
  command('pdftoppm', ['-png', '-r', '90', output(`${name}.pdf`), output(`${name}-page`)], `${name}-raster`);
  return result;
}
function directoryBytes(directory) { return readdirSync(directory).reduce((sum, name) => { const f = path.join(directory, name); const st = statSync(f); return sum + (st.isDirectory() ? directoryBytes(f) : st.size); }, 0); }

console.log(JSON.stringify({ directory, cli, cliSha256: hash(cli), source, oracle }));
try {
  independent('docx', source, { nonempty_paragraphs: sourceExpected.paragraphs, tables: [], page_breaks: 0, page_points: [595.28, 841.89], paragraph_styles: ['Heading1', 'Normal', 'Normal'], margins_points: [72,72,72,72] }, 'source-minutes');
  server = await createServer({ configFile: path.resolve('vite.config.ts'), server: { host: '127.0.0.1', port: 1427, strictPort: true }, clearScreen: false, logLevel: 'warn' });
  await server.listen();
  await freshBrowser();
  report.browserVersion = browser.version();

  await importSource(source, true);
  const minutes = page.locator('#editor-content .tiptap');
  await expect(minutes.locator('h1')).toHaveText(sourceExpected.paragraphs[0]);
  const last = minutes.locator(':scope > p').last();
  await last.click(); await page.keyboard.press('Control+End'); await page.keyboard.insertText(' 会议结论已确认。');
  await expect(last).toContainText('会议结论已确认。');
  await page.locator('#edit-undo').click(); await expect(last).not.toContainText('会议结论已确认。');
  await page.locator('#edit-redo').click(); await expect(last).toContainText('会议结论已确认。');
  await durableContains('会议结论已确认。');
  const minutesText = [...sourceExpected.paragraphs]; minutesText[2] += ' 会议结论已确认。';
  const minutesOutput = output('会议纪要-修改.docx');
  await save(minutesOutput);
  independent('docx', minutesOutput, { nonempty_paragraphs: minutesText, tables: [], page_breaks: 0, page_points: [595.28, 841.89], paragraph_styles: ['Heading1', 'Normal', 'Normal'], margins_points: [72,72,72,72] }, 'minutes-output');
  await freshBrowser(); await open(minutesOutput);
  await expect(page.locator('#draft-state')).toHaveText('文档已打开');
  await expect(page.locator('#editor-content .tiptap > p').last()).toHaveText(minutesText[2]);
  const minutesPdf = await printPdf('minutes-browser-print', { pages: 1, page_points: [595.28, 841.89], contains: minutesText });
  casePassed('External independently authored Chinese minutes: Open/review/cancel/edit/undo/redo/Save/fresh-browser reopen; browser print checked separately', { file: minutesOutput, strictPdfPassed: minutesPdf.passed });

  const editor = page.locator('#editor-content .tiptap');
  await editor.locator(':scope > p').last().click(); await page.keyboard.press('Control+End'); await page.keyboard.insertText(' 未导出恢复标记');
  await durableContains('未导出恢复标记');
  const durable = readFileSync(draftPath);
  await confirmLeave(() => page.locator('#new-document').click(), false);
  await expect(editor).toContainText('未导出恢复标记');
  state.nextOpen = null; await confirmLeave(() => page.locator('#open').click(), true);
  await expect(editor).toContainText('未导出恢复标记');
  const corrupt = output('corrupt.docx'); writeFileSync(corrupt, 'not a ZIP', { flag: 'wx' }); allowedInputs.add(corrupt);
  state.nextOpen = corrupt; await confirmLeave(() => page.locator('#open').click(), true);
  await expect(page.locator('#notice')).toHaveClass(/error/);
  await expect(editor).toContainText('未导出恢复标记');
  state.nextSave = null; await page.locator('#edit-save').click();
  await expect(page.locator('#edit-save')).toBeEnabled(); await expect(editor).toContainText('未导出恢复标记');
  const priorOutputHash = hash(minutesOutput);
  state.nextSave = minutesOutput; await page.locator('#edit-save').click();
  await expect(page.locator('#notice')).toContainText('DOCX 导出失败');
  assert.equal(hash(minutesOutput), priorOutputHash, 'Existing output must remain byte-identical');
  await expect(editor).toContainText('未导出恢复标记');
  casePassed('Canceled/corrupt Open, canceled Save and real CLI overwrite refusal preserve dirty editor and existing output');

  state.failDraft = true;
  await editor.locator(':scope > p').last().click(); await page.keyboard.press('Control+End'); await page.keyboard.insertText(' 写入失败仍保留');
  await expect(page.locator('#draft-state')).toContainText('草稿保存失败', { timeout: 10000 });
  assert.ok(readFileSync(draftPath).equals(durable), 'Failed draft write must retain previous durable draft');
  await confirmLeave(() => page.locator('#new-document').click(), true);
  await expect(page.locator('#notice')).toContainText('已保留当前文档');
  await expect(editor).toContainText('写入失败仍保留');
  state.failDraft = false;
  await editor.locator(':scope > p').last().click(); await page.keyboard.press('Control+End'); await page.keyboard.insertText(' 重试已保存');
  await durableContains('重试已保存');
  const recoveredModel = readDraft();
  await freshBrowser(); await page.locator('#restore-draft').click();
  await expect(page.locator('#editor-content .tiptap')).toContainText('未导出恢复标记 写入失败仍保留 重试已保存');
  const recovered = output('会议纪要-恢复.docx'); await save(recovered);
  const recoveredRead = cliJson(['read-docx-model', recovered], 'recovered-model');
  assert.deepEqual(recoveredRead, recoveredModel);
  const recoveredText = [...minutesText]; recoveredText[2] += ' 未导出恢复标记 写入失败仍保留 重试已保存';
  independent('docx', recovered, { nonempty_paragraphs: recoveredText, tables: [], page_breaks: 0 }, 'recovered-output');
  casePassed('Disk-backed test draft survives fresh browser process; injected disk failure blocks navigation and retry recovers newest text');

  // release43 contract: visibly read-only while Save is pending, not concurrent editing.
  // Baseline run-Fom5G6 retained the prior editable-view/silent-drop failure.
  const recoveringEditor = page.locator('#editor-content .tiptap');
  const latestText = [...recoveredText];
  for (const outcome of ['success', 'cancel', 'overwrite-error']) {
    const marker = ` ${outcome}前的修改`;
    await recoveringEditor.locator(':scope > p').last().click(); await page.keyboard.press('Control+End'); await page.keyboard.insertText(marker);
    latestText[2] += marker;
    await durableContains(marker);
    const beforeText = await recoveringEditor.innerText();
    const beforeDraft = readFileSync(draftPath);
    const destination = outcome === 'success' ? output('会议纪要-保存快照.docx') : outcome === 'cancel' ? null : minutesOutput;
    state.nextSave = destination; state.holdSave = true;
    const savesBefore = report.calls.filter(call => call.transport === 'save_new_document').length;
    await page.locator('#edit-save').click();
    await expect.poll(() => typeof state.pendingSave).toBe('function');
    await expect(recoveringEditor).toHaveAttribute('contenteditable', 'false');
    await expect(page.locator('#notice')).toContainText('编辑区暂时只读');
    for (const id of ['edit-save','edit-bold','edit-undo','edit-page-break','edit-style','edit-align']) await expect(page.locator(`#${id}`)).toBeDisabled();
    await recoveringEditor.locator(':scope > p').last().click(); await page.keyboard.press('Control+End'); await page.keyboard.insertText(' 不应接收的只读输入');
    assert.equal(await recoveringEditor.innerText(), beforeText);
    await page.screenshot({ path: output(`save-${outcome}-readonly.png`), fullPage: true });
    await page.keyboard.press('Control+s');
    await page.locator('#new-document').click();
    await expect(recoveringEditor).toHaveAttribute('contenteditable', 'false');
    await page.locator('#open').click();
    await expect(recoveringEditor).toHaveAttribute('contenteditable', 'false');
    await expect(page.locator('#notice')).toContainText('请等待导出完成');
    await page.evaluate(() => window.requestNativeClose());
    assert.equal(state.destroyed, false, 'Simulated host close must be blocked during Save');
    assert.equal(report.calls.filter(call => call.transport === 'save_new_document').length, savesBefore + 1);
    assert.ok(readFileSync(draftPath).equals(beforeDraft), 'Read-only input must not mutate durable draft');
    state.holdSave = false; state.pendingSave(); state.pendingSave = null;
    await expect(recoveringEditor).toHaveAttribute('contenteditable', 'true');
    await expect(page.locator('#edit-save')).toBeEnabled();
    await expect(page.locator('#edit-bold')).toBeEnabled();
    assert.equal(await recoveringEditor.innerText(), beforeText);
    if (outcome === 'success') {
      await expect(page.locator('#notice')).toContainText('已导出 DOCX');
      independent('docx', destination, { nonempty_paragraphs: latestText, tables: [], page_breaks: 0 }, 'delayed-save-output');
    } else if (outcome === 'cancel') {
      await expect(page.locator('#notice')).toContainText('已取消 DOCX 导出');
      await expect(page.locator('#draft-state')).not.toHaveText('DOCX 已导出');
    } else {
      await expect(page.locator('#notice')).toContainText('DOCX 导出失败');
      assert.equal(hash(minutesOutput), priorOutputHash);
      await expect(page.locator('#draft-state')).not.toHaveText('DOCX 已导出');
    }
    const resumed = ` ${outcome}后可继续输入`;
    await recoveringEditor.locator(':scope > p').last().click(); await page.keyboard.press('Control+End'); await page.keyboard.insertText(resumed);
    await expect(recoveringEditor).toContainText(resumed); latestText[2] += resumed;
    casePassed(`Delayed Save ${outcome}: visible read-only editor/toolbar, unchanged snapshot/draft, duplicate Save/navigation/close blocked, editing restored`);
  }
  const latest = output('会议纪要-最新.docx'); await save(latest);
  independent('docx', latest, { nonempty_paragraphs: latestText, tables: [], page_breaks: 0 }, 'latest-save-output');

  await importSource(genuine);
  const table = page.locator('#editor-content .tiptap table');
  await expect(table.locator('tr')).toHaveCount(3);
  await expect(table.locator('tr').first().locator('th,td')).toHaveCount(5);
  const cell = table.locator('tr').nth(1).locator('th,td').nth(1);
  await cell.click(); await page.keyboard.press('End'); await page.keyboard.insertText(' 已核对');
  await page.locator('#edit-undo').click(); await expect(cell).toHaveText('10');
  await page.locator('#edit-redo').click(); await expect(cell).toHaveText('10 已核对');
  await durableContains('已核对');
  const tableOutput = output('genuine-table-edited.docx'); await save(tableOutput);
  const expectedTable = [[['', 'Branden', 'Nicolaas', 'Sam', 'Simon'], ['Task I', '10 已核对', '10', '10', '10'], ['Task II', '20', '5', '5', '15']]];
  independent('docx', tableOutput, { nonempty_paragraphs: [], tables: expectedTable, page_breaks: 0, page_points: [612, 792] }, 'genuine-table-output');
  await freshBrowser(); await open(tableOutput);
  await expect(page.locator('#editor-content .tiptap table')).toContainText('10 已核对');
  const tablePdf = await printPdf('genuine-table-browser-print', { pages: 1, page_points: [612, 792], contains: expectedTable.flat(2).filter(Boolean) });
  casePassed('Genuine English Apache POI table: Open/import/edit cell/undo/redo/Save/reopen/Chinese added text; browser print checked separately', { file: tableOutput, strictPdfPassed: tablePdf.passed });

  await page.locator('#new-document').click();
  const fresh = page.locator('#editor-content .tiptap');
  await fresh.click(); await page.keyboard.insertText('第一章 中文日常文档');
  await page.keyboard.press('Enter'); await page.keyboard.insertText('本段用于验证中文、标点与数字：预算 1,250.50 元，完成率 80%。');
  await page.keyboard.press('Enter'); await page.locator('#edit-bold').click(); await page.keyboard.insertText('需要强调的结论');
  await page.locator('#edit-bold').click();
  await expect(fresh.locator('strong')).toHaveText('需要强调的结论');
  await page.locator('#edit-page-break').click(); await page.keyboard.insertText('第二章 保存与打印');
  await expect(fresh.locator('[data-page-break]')).toHaveCount(1);
  await page.keyboard.press('Enter'); await page.keyboard.insertText('末段检查：所有原件保持不变，导出副本可重新打开。');
  await durableContains('末段检查');
  const newOutput = output('新建中文文档.docx'); await save(newOutput);
  const newText = ['第一章 中文日常文档', '本段用于验证中文、标点与数字：预算 1,250.50 元，完成率 80%。', '需要强调的结论', '第二章 保存与打印', '末段检查：所有原件保持不变，导出副本可重新打开。'];
  independent('docx', newOutput, { nonempty_paragraphs: newText, tables: [], page_breaks: 1, page_points: [595.28, 841.89], bold_run_text: ['需要强调的结论'] }, 'new-output');
  await freshBrowser(); await open(newOutput);
  await expect(page.locator('#editor-content .tiptap strong')).toHaveText('需要强调的结论');
  const newPdf = await printPdf('new-document-browser-print', { pages: 2, page_points: [595.28, 841.89], contains: newText, excludes: ['打印后新增内容'], page_markers: [newText[0], newText[3]] }, true);
  await expect(page.locator('#draft-state')).not.toHaveText('DOCX 已导出');
  await durableContains('打印后新增内容');
  casePassed('New Chinese document: real bold toolbar/manual page break/Save/reopen/two-page browser print/immutable snapshot', { strictPdfPassed: newPdf.passed });
  const dirtyBeforeDirect = await page.locator('#editor-content .tiptap').innerText();
  await page.locator('#edit-export-pdf').click();
  await expect(page.locator('#notice')).toContainText('Browser test host has no native direct PDF export');
  assert.equal(await page.locator('#editor-content .tiptap').innerText(), dirtyBeforeDirect);
  await expect(page.locator('#print-document')).toBeEmpty();
  await expect(page.locator('#draft-state')).not.toHaveText('DOCX 已导出');
  casePassed('Unsupported direct PDF test-host response preserves dirty text and clears print snapshot');
  assert.deepEqual(report.pageErrors, [], 'No unhandled browser page errors');
  assert.deepEqual(report.unexpectedDialogs, [], 'Unexpected prompts must never be silently accepted');
  assert.deepEqual(state.expectedDialogs, [], 'Every expected prompt must have occurred');
  report.functionalFlowsPassed = report.functionalFailures.length === 0;
  report.passed = report.functionalFlowsPassed && report.strictPdfFailures.length === 0 && report.productPdfTextFailures.length === 0;
} catch (error) {
  report.passed = false; report.error = error.stack ?? String(error);
  if (page && !page.isClosed()) {
    report.failureState = await page.evaluate(() => ({ title: document.querySelector('#title')?.textContent, notice: document.querySelector('#notice')?.textContent, draft: document.querySelector('#draft-state')?.textContent, editor: document.querySelector('#editor-content')?.innerHTML })).catch(() => null);
    await page.screenshot({ path: output('failure.png'), fullPage: true }).catch(() => {});
  }
  console.error(report.error);
} finally {
  for (const item of protectedInputs) {
    item.unchanged = hash(item.file) === item.sha256;
    if (!item.unchanged) report.passed = false;
  }
  for (const item of testedSourceFiles) {
    item.unchanged = hash(item.file) === item.sha256;
    if (!item.unchanged) { report.passed = false; report.sourceChangedDuringRun = true; }
  }
  if (browser) await browser.close();
  if (server) await server.close();
  report.artifactBytes = directoryBytes(directory);
  if (report.artifactBytes > 150 * 1024 * 1024) { report.passed = false; report.budgetExceeded = true; }
  report.finished = new Date().toISOString();
  saveJson('report.json', report);
  console.log(JSON.stringify({ directory, passed: report.passed, cases: report.cases.length, artifactBytes: report.artifactBytes }));
}
process.exitCode = report.passed ? 0 : 1;
