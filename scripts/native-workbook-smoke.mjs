// Real WebView2, IPC, worker, draft and XLSX export. Dialog paths are test-only.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync, unlinkSync, rmdirSync } from 'node:fs';
import { createServer } from 'node:net';
import path from 'node:path';
mkdirSync('tmp/native-workbook', { recursive: true });
const directory = mkdtempSync(path.resolve('tmp/native-workbook/run-'));
const executable = path.resolve('src-tauri/target/debug/rust-wps-desktop.exe');
const cli = path.resolve('target/debug/rust-wps.exe');
const input = path.join(directory, 'source.xlsx'), output = path.join(directory, 'saved.xlsx');
const seed=JSON.parse(readFileSync('docs/examples/workbook-v1.json','utf8'));seed.sheets.push({name:'隐藏资料',hidden:true,cells:[{row:999,column:25,content:{type:'text',value:'跨表定位中文'}}]});
const seedPath=path.join(directory,'seed.json');writeFileSync(seedPath,JSON.stringify(seed));execFileSync(cli,['create-xlsx',seedPath,input]);
const sourceBytes = readFileSync(input);
const expectedModel = JSON.parse(execFileSync(cli, ['read-xlsx-model', input], { encoding: 'utf8' }));
expectedModel.sheets[0].cells.find(cell => cell.row === 1 && cell.column === 1).content.value = 24.75;
const reservation = createServer();
await new Promise(resolve => reservation.listen(0, '127.0.0.1', resolve));
const port = reservation.address().port;
await new Promise(resolve => reservation.close(resolve));
const child = spawn(executable, [], { windowsHide: true, stdio: 'ignore', env: {
  ...process.env, RUST_WPS_SMOKE_OPEN_INPUT: input, RUST_WPS_SMOKE_XLSX_OUTPUT: output,
  RUST_WPS_SMOKE_PDF_OUTPUT: path.join(directory, 'unused.pdf'),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
} });
let browser;
try {
  const deadline = Date.now() + 25000;
  while (!browser && Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Native app exited: ${child.exitCode}`);
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); }
    catch { await new Promise(resolve => setTimeout(resolve, 300)); }
  }
  if (!browser) throw new Error('Native CDP startup timed out');
  const context = browser.contexts()[0], page = context.pages()[0] ?? await context.waitForEvent('page');
  page.on('dialog', dialog => dialog.accept());
  await page.locator('#open').click();
  await expect(page.locator('#sheet-edit-toolbar')).toBeVisible({ timeout: 35000 });
  await page.locator('#sheet-viewport').focus();await page.keyboard.press('Control+f');await expect(page.locator('#sheet-search')).toBeFocused();
  await page.locator('#sheet-search').fill('定位中文');await page.locator('#sheet-search').press('Enter');
  await expect(page.locator('#sheet-picker')).toHaveValue('1');await expect(page.locator('#sheet-address')).toHaveValue('Z1000');await expect(page.locator('#sheet-search-state')).toContainText('隐藏');
  await page.locator('#sheet-formula').fill('不应提交');await page.locator('#sheet-formula').press('Escape');await expect(page.locator('#sheet-formula')).toHaveValue('跨表定位中文');await expect(page.locator('#sheet-viewport')).toBeFocused();
  await page.locator('#sheet-search').fill('=sum');await page.locator('#sheet-search').press('Enter');await expect(page.locator('#sheet-picker')).toHaveValue('0');await expect(page.locator('#sheet-address')).toHaveValue('B4');
  await page.locator('#sheet-calculate').click();
  await expect(page.locator('[data-address="B4"]')).toHaveText('1280', { timeout: 35000 });
  await page.locator('[data-address="B4"]').click();
  await expect(page.locator('#sheet-formula')).toHaveValue('=SUM(B2:B3)');
  await page.locator('#sheet-formatted').check();
  await expect(page.locator('[data-address="B4"]')).toHaveText('1,280.00');
  await page.locator('#sheet-formatted').uncheck();
  await page.locator('#sheet-auto-calculate').check();
  await page.locator('#sheet-search').fill('1200');await page.locator('#sheet-search').press('Enter');await expect(page.locator('#sheet-address')).toHaveValue('B2');
  await page.locator('#sheet-viewport').focus();await page.keyboard.press('F2');await expect(page.locator('#sheet-formula')).toBeFocused();
  await page.getByLabel('公式或原始值').fill('11');await page.locator('#sheet-formula').press('Tab');await expect(page.locator('#sheet-address')).toHaveValue('C2');
  await expect(page.locator('#sheet-search-state')).toContainText('0 个匹配');await expect(page.locator('#sheet-search-next')).toBeDisabled();
  await page.locator('[data-address="B2"]').dblclick();await page.getByLabel('公式或原始值').fill('24.75');await page.locator('#sheet-formula').press('Enter');await expect(page.locator('#sheet-address')).toHaveValue('B3');
  await expect(page.locator('[data-address="B2"]')).toHaveText('24.75');
  await expect(page.locator('[data-address="B4"]')).toHaveText('');
  await expect(page.locator('#sheet-calculation-state')).toHaveText('已过期，需重算');
  await expect(page.locator('[data-address="B4"]')).toHaveText('104.75', { timeout: 35000 });
  await page.locator('#sheet-auto-calculate').uncheck();
  await page.locator('[data-address="B2"]').click();
  await page.getByLabel('公式或原始值').fill('25.75');await page.locator('#sheet-apply').click();
  await page.waitForTimeout(800);await expect(page.locator('[data-address="B4"]')).toHaveText('');
  await page.locator('#sheet-undo').click();await expect(page.locator('[data-address="B2"]')).toHaveText('24.75');
  await page.locator('#sheet-auto-calculate').check();await expect(page.locator('[data-address="B4"]')).toHaveText('104.75', { timeout: 35000 });
  const draftPath = path.join(directory, 'workbook-draft-v1.json');
  await expect.poll(() => { try { return JSON.parse(readFileSync(draftPath, 'utf8')).sheets[0].cells.find(cell => cell.row === 1 && cell.column === 1).content.value; } catch { return null; } }, { timeout: 10000 }).toBe(24.75);
  await page.locator('#sheet-save').click();
  await expect(page.locator('#sheet-cell-info')).toContainText('已导出 XLSX', { timeout: 35000 });
  const savedBytes = readFileSync(output);
  const python=path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const independent=JSON.parse(execFileSync(python,['-c','import openpyxl,json,sys;p=sys.argv[1];w=openpyxl.load_workbook(p);c=openpyxl.load_workbook(p,data_only=True);s=w.worksheets[0];print(json.dumps({"value":s["B2"].value,"formula":s["B4"].value,"cache":c.worksheets[0]["B4"].value}))',output],{encoding:'utf8',windowsHide:true}));
  expect(independent).toEqual({value:24.75,formula:'=SUM(B2:B3)',cache:null});
  const saved = JSON.parse(execFileSync(cli, ['read-xlsx-model', output], { encoding: 'utf8' }));
  expect(saved.sheets[0].cells.find(cell => cell.row === 1 && cell.column === 1).content.value).toBe(24.75);
  expect(saved).toEqual(expectedModel);
  const savedReport = JSON.parse(execFileSync(cli, ['inspect-xlsx', output], { encoding: 'utf8' }));
  expect(savedReport.sheets[0].cells.find(cell => cell.formula).formula_cache).toBe('absent');
  await page.locator('#sheet-save').click();
  await expect(page.locator('#sheet-cell-info')).toContainText('Output already exists', { timeout: 35000 });
  expect(readFileSync(output)).toEqual(savedBytes);
  // Reopen the exported file through the actual executable's worker protocol.
  const reopened = JSON.parse(execFileSync(executable, ['--document-worker'], { input: JSON.stringify({ operation: 'open', path: output }), encoding: 'utf8', windowsHide: true }));
  expect(reopened.document.workbook_model).toEqual(saved);
  await page.locator('#sheet-auto-calculate').uncheck();
  // The desktop source stays untouched; reopening it shows its original value.
  await page.locator('#open').click();
  await expect(page.locator('[data-address="B2"]')).toHaveText('1200');
  await expect(page.locator('[data-address="B4"]')).toHaveText('');
  await page.locator('#restore-workbook').click();
  await expect(page.locator('[data-address="B2"]')).toHaveText('24.75');
  await page.locator('[data-address="A1"]').click();
  await page.getByLabel('公式或原始值').fill('关闭前最终修改'); await page.locator('#sheet-apply').click();
  // An empty directory at the draft destination forces atomic publication to fail.
  // This fault is confined to this run's isolated test directory.
  unlinkSync(draftPath); mkdirSync(draftPath);
  await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'main' }));
  await expect(page.locator('#sheet-cell-info')).toContainText('已保留工作簿', { timeout: 10000 });
  expect(child.exitCode).toBe(null);
  await expect(page.locator('[data-address="A1"]')).toHaveText('关闭前最终修改');
  rmdirSync(draftPath);
  const closed = new Promise((resolve, reject) => { child.once('exit', code => code === 0 ? resolve() : reject(new Error(`Exit ${code}`))); });
  await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'main' })).catch(error => { if (!/closed/i.test(String(error))) throw error; });
  let closeTimer;
  try { await Promise.race([closed, new Promise((_, reject) => { closeTimer = setTimeout(() => reject(new Error('Native close timed out')), 15000); })]); }
  finally { clearTimeout(closeTimer); }
  const finalDraft = JSON.parse(readFileSync(draftPath, 'utf8'));
  expect(finalDraft.sheets[0].cells.find(cell => cell.row === 0 && cell.column === 0).content.value).toBe('关闭前最终修改');
  expect(readFileSync(input)).toEqual(sourceBytes);
  const report = { nativeWebView2: true, realIpcAndWorker: true, draftPersistedAndRestored: true,
    nativeSearchHiddenAndFormula:true, nativeKeyboardCommitAndCancel:true, nativeSearchInvalidation:true, realCalculationWorker: true, automaticRecalculation: true, disabledAutomaticStaysStale: true, undoAndReenableRecalculate: true, independentOpenpyxl: true, formulaResultAndNumberFormat: true, editInvalidatesCalculation: true, exportRetainsUncachedFormula: true,
    xlsxSavedAndReopened: true, overwriteRejected: true, failedDraftPreventsNativeClose: true, nativeCloseFlushedLatestEdit: true,
    originalSourcePreserved: true, nativeFileDialogsBypassedByTestFeature: true, directory };
  writeFileSync(path.join(directory, 'report.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
} finally {
  if (browser) await browser.close().catch(() => {});
  if (child.exitCode === null) {const exited=once(child,'exit');child.kill();await exited;}
}
