import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { readFileSync, mkdtempSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { nativeBridge } from './bridge.mjs';

function fixture() {
  const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const generated = JSON.parse(execFileSync(python, ['scripts/check-spreadsheet.py'], { encoding: 'utf8' }));
  return { name: '中文工作簿.xlsx', path: path.join(path.dirname(generated.report), '中文工作簿.xlsx'), inspection: { format: 'xlsx', text: '', bytes: 5000, warnings: [] }, workbook: JSON.parse(readFileSync(generated.report, 'utf8')) };
}

test('sparse XLSX grid switches hidden sheets, shows formulas and reaches the far cell', async ({ page }) => {
  const opened = fixture(); let fail = false;
  await nativeBridge(page, async command => {
    if (command === 'open_document') { if (fail) throw new Error('Invalid XLSX ZIP'); return opened; }
    throw new Error(`Unexpected ${command}`);
  });
  await page.goto('/'); await page.locator('#open').click();
  await expect(page.getByLabel('只读工作簿')).toBeVisible();
  await expect(page.locator('[data-address="A1"]')).toHaveText('中文表格');
  await expect(page.locator('[data-address="B1"]')).toHaveText('12.5');
  await page.locator('[data-address="E1"]').click();
  await expect(page.getByLabel('公式或原始值')).toHaveValue('=B1*2');
  await expect(page.locator('#sheet-cell-info')).toContainText('empty_or_absent');
  await expect(page.getByLabel('公式或原始值')).toHaveAttribute('readonly', '');
  await page.getByLabel('单元格地址').fill('XFD1048576'); await page.getByLabel('单元格地址').press('Enter');
  await expect(page.locator('[data-address="XFD1048576"]')).toHaveText('远端单元格');
  expect(await page.locator('[role="gridcell"]').count()).toBeLessThanOrEqual(1800);
  await page.getByLabel('工作表网格').press('ArrowLeft');
  await expect(page.getByLabel('单元格地址')).toHaveValue('XFC1048576');
  await page.getByLabel('工作表网格').press('ArrowRight');
  await expect(page.getByLabel('公式或原始值')).toHaveValue('远端单元格');
  await page.getByLabel('工作表', { exact: true }).selectOption('1');
  await expect(page.locator('[data-address="A1"]')).toHaveText('隐藏内容仍须读取');
  await expect(page.locator('#sheet-status')).toContainText('1904 日期系统');
  fail = true; await page.locator('#open').click();
  await expect(page.getByRole('status')).toContainText('Invalid XLSX ZIP');
  await expect(page.locator('[data-address="A1"]')).toHaveText('隐藏内容仍须读取');
  await page.getByLabel('单元格地址').fill('XFE1'); await page.getByLabel('单元格地址').press('Enter');
  await expect(page.locator('#sheet-cell-info')).toContainText('超出工作表范围');
});

test('workbook text is inert and switching to a new Word document removes the grid', async ({ page }) => {
  const opened = fixture();
  opened.workbook.sheets[0].cells[0].value.value = '<img src=x onerror="window.injected=true">';
  await nativeBridge(page, async command => { if (command === 'open_document') return opened; if (command === 'save_draft') return; });
  await page.goto('/'); await page.locator('#open').click();
  await expect(page.locator('[data-address="A1"]')).toHaveText('<img src=x onerror="window.injected=true">');
  await expect(page.locator('#sheet-layer img')).toHaveCount(0);
  await page.locator('#new-document').click();
  await expect(page.locator('#spreadsheet')).toBeHidden();
  await expect(page.locator('#sheet-layer')).toBeEmpty();
  await expect(page.locator('#editor-content .tiptap')).toBeVisible();
});

function authoredFixture() {
  const directory = mkdtempSync(path.resolve('tmp/spreadsheets/ui-edit-'));
  const exe = path.resolve('target/debug/rust-wps.exe'), file = path.join(directory, 'source.xlsx');
  const workbook = JSON.parse(execFileSync(exe, ['create-xlsx', 'docs/examples/workbook-v1.json', file], { encoding: 'utf8', windowsHide: true }));
  const workbook_model = JSON.parse(execFileSync(exe, ['read-xlsx-model', file], { encoding: 'utf8', windowsHide: true }));
  return { directory, exe, opened: { name: '预算.xlsx', path: file, workbook, workbook_model, inspection: { format: 'xlsx', text: '', bytes: 5000, warnings: [] } } };
}

test('verified workbook edits, undoes, autosaves and exports through the real Rust writer', async ({ page }) => {
  const { directory, exe, opened } = authoredFixture(), source = readFileSync(opened.path); let draft = null, exported = null;
  await nativeBridge(page, async (command, args) => {
    if (command === 'open_document') return opened;
    if (command === 'save_workbook_draft') { draft = args.model; return; }
    if (command === 'load_workbook_draft') return draft;
    if (command === 'save_new_workbook') {
      const input = path.join(directory, 'edited.json'), output = path.join(directory, 'edited.xlsx');
      writeFileSync(input, JSON.stringify(args.model));
      execFileSync(exe, ['create-xlsx', input, output], { windowsHide: true });
      exported = JSON.parse(execFileSync(exe, ['read-xlsx-model', output], { encoding: 'utf8', windowsHide: true }));
      return { path: output };
    }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#open').click();
  await expect(page.locator('#sheet-edit-toolbar')).toBeVisible();
  await page.locator('[data-address="B2"]').click();
  await page.getByLabel('公式或原始值').fill('24.75'); await page.locator('#sheet-apply').click();
  await expect(page.locator('[data-address="B2"]')).toHaveText('24.75');
  await page.screenshot({ path: 'tmp/workbook-editor.png' });
  await page.locator('#sheet-undo').click(); await expect(page.locator('[data-address="B2"]')).toHaveText('1200');
  await page.locator('#sheet-redo').click(); await expect(page.locator('[data-address="B2"]')).toHaveText('24.75');
  await expect.poll(() => draft?.sheets[0].cells.find(cell => cell.row === 1 && cell.column === 1).content.value).toBe(24.75);
  await page.locator('#sheet-save').click();
  await expect(page.locator('#sheet-cell-info')).toContainText('已导出 XLSX');
  expect(exported.sheets[0].cells.find(cell => cell.row === 1 && cell.column === 1).number_format).toBe('#,##0.00');
  expect(readFileSync(opened.path)).toEqual(source);
  await expect(page.locator('#sheet-edit-state')).not.toContainText('有未导出的修改');
  await page.locator('#open').click();
  await expect(page.locator('[data-address="B2"]')).toHaveText('1200');
  await page.locator('#restore-workbook').click();
  await expect(page.locator('[data-address="B2"]')).toHaveText('24.75');
});

test('workbook draft failure prevents switching to Word and retains edits', async ({ page }) => {
  const { opened } = authoredFixture(); let fail = true;
  await nativeBridge(page, async command => {
    if (command === 'open_document') return opened;
    if (command === 'save_workbook_draft') { if (fail) throw new Error('disk unavailable'); return; }
    if (command === 'save_draft') return;
    throw new Error(command);
  });
  page.on('dialog', dialog => dialog.accept());
  await page.goto('/'); await page.locator('#open').click();
  await page.getByLabel('公式或原始值').fill('修改后的项目'); await page.locator('#sheet-apply').click();
  await page.locator('#new-document').click();
  await expect(page.locator('#sheet-cell-info')).toContainText('已保留工作簿');
  await expect(page.locator('[data-address="A1"]')).toHaveText('修改后的项目');
  await expect(page.locator('#editor-content .tiptap')).toHaveCount(0);
  fail = false; await page.locator('#new-document').click();
  await expect(page.locator('#spreadsheet')).toBeHidden();
  await expect(page.locator('#editor-content .tiptap')).toBeVisible();
});

test('new workbook draft restores separately from Word and native close flushes the latest edit', async ({ page }) => {
  let draft = null, destroyed = false;
  await nativeBridge(page, async (command, args) => {
    if (command === 'save_workbook_draft') { draft = args.model; return; }
    if (command === 'load_workbook_draft') return draft;
    if (command === 'save_draft') return;
    if (command === 'plugin:window|destroy') { destroyed = true; return; }
    throw new Error(command);
  });
  page.on('dialog', dialog => dialog.accept());
  await page.goto('/'); await page.locator('#new-workbook').click();
  await expect(page.getByLabel('工作簿编辑器')).toBeVisible();
  await page.getByLabel('公式或原始值').fill('独立工作簿草稿'); await page.locator('#sheet-apply').click();
  await expect(page.locator('#sheet-status')).toContainText('1 个实际单元格');
  await page.locator('#new-document').click();
  await expect(page.locator('#editor-content .tiptap')).toBeVisible();
  await page.locator('#restore-workbook').click();
  await expect(page.locator('[data-address="A1"]')).toHaveText('独立工作簿草稿');
  await page.getByLabel('公式或原始值').fill('关闭前的最后修改'); await page.locator('#sheet-apply').click();
  await page.evaluate(() => window.requestNativeClose());
  await expect.poll(() => destroyed).toBe(true);
  expect(draft.sheets[0].cells[0].content.value).toBe('关闭前的最后修改');
});

test('rectangular clipboard pastes atomically, preserves internal types and exports all cells', async ({ page }) => {
  const { opened, directory, exe } = authoredFixture(); let draft;
  await nativeBridge(page, async (command, args) => {
    if (command === 'open_document') return opened;
    if (command === 'save_workbook_draft') { draft = args.model; return; }
    if (command === 'save_new_workbook') {
      const input = path.join(directory, 'clipboard.json'), output = path.join(directory, 'clipboard.xlsx');
      writeFileSync(input, JSON.stringify(args.model));
      execFileSync(exe, ['create-xlsx', input, output], { windowsHide: true }); return { path: output };
    }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#open').click();
  await page.locator('[data-address="A1"]').click();
  await page.locator('[data-address="B2"]').click({ modifiers: ['Shift'] });
  await expect(page.locator('[role="gridcell"][aria-selected="true"]')).toHaveCount(4);
  const copied = await page.evaluate(() => {
    const data = new DataTransfer(); document.getElementById('sheet-viewport').dispatchEvent(new ClipboardEvent('copy', { clipboardData: data, bubbles: true, cancelable: true }));
    return { text: data.getData('text/plain'), typed: data.getData('application/x-rust-wps-cells+json') };
  });
  expect(copied.text).toBe('项目\t金额\r\n设备\t1200');
  await page.locator('[data-address="C5"]').click();
  await page.evaluate(copied => {
    const data = new DataTransfer(); data.setData('text/plain', copied.text); data.setData('application/x-rust-wps-cells+json', copied.typed);
    document.getElementById('sheet-viewport').dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
  }, copied);
  await expect(page.locator('[data-address="D6"]')).toHaveText('1200');
  await expect(page.locator('[role="gridcell"][aria-selected="true"]')).toHaveCount(4);
  await page.locator('#sheet-undo').click(); await expect(page.locator('[data-address="C5"]')).toHaveText(''); await expect(page.locator('[data-address="D6"]')).toHaveText('');
  await page.locator('#sheet-redo').click(); await expect(page.locator('[data-address="D6"]')).toHaveText('1200');
  await page.locator('[data-address="C5"]').click(); await page.locator('[data-address="D6"]').click({ modifiers: ['Shift'] });
  await page.getByLabel('工作表网格').press('Delete');
  await expect(page.locator('[data-address="D6"]')).toHaveText('');
  await page.getByLabel('工作表网格').press('Control+z'); await expect(page.locator('[data-address="D6"]')).toHaveText('1200');
  await expect.poll(() => draft?.sheets[0].cells.find(cell => cell.row === 5 && cell.column === 3)?.content.type).toBe('number');
  await page.locator('[data-address="E5"]').click();
  await page.evaluate(() => {
    const data = new DataTransfer(); data.setData('text/plain', '001\t"两行\n中文"\r\n=SUM(A1:A2)\t尾部');
    document.getElementById('sheet-viewport').dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
  });
  await expect(page.locator('[data-address="E5"]')).toHaveText('001');
  await expect(page.locator('[data-address="F5"]')).toHaveText('两行\n中文');
  await page.locator('#sheet-save').click(); await expect(page.locator('#sheet-cell-info')).toContainText('已导出 XLSX');
  const restored = JSON.parse(execFileSync(exe, ['read-xlsx-model', path.join(directory, 'clipboard.xlsx')], { encoding: 'utf8', windowsHide: true }));
  expect(restored.sheets[0].cells.find(cell => cell.row === 5 && cell.column === 4).content).toEqual({ type: 'text', value: '=SUM(A1:A2)' });
  await page.getByLabel('单元格地址').fill('XFD1048576'); await page.getByLabel('单元格地址').press('Enter');
  await page.evaluate(() => {
    const data = new DataTransfer(); data.setData('text/plain', 'first\toutside');
    document.getElementById('sheet-viewport').dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
  });
  await expect(page.locator('#sheet-cell-info')).toContainText('超出工作表范围');
  await expect(page.locator('[data-address="XFD1048576"]')).toHaveText('');
});

test('internal formula copy adjusts relative references and preserves anchors through Rust export', async ({ page }) => {
  const { opened, directory, exe } = authoredFixture();
  await nativeBridge(page, async (command, args) => {
    if (command === 'open_document') return opened;
    if (command === 'save_workbook_draft') return;
    if (command === 'save_new_workbook') {
      const input = path.join(directory, 'formula.json'), output = path.join(directory, 'formula.xlsx');
      writeFileSync(input, JSON.stringify(args.model)); execFileSync(exe, ['create-xlsx', input, output], { windowsHide: true }); return { path: output };
    }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#open').click();
  await page.locator('[data-address="B4"]').click();
  await page.getByLabel('公式或原始值').fill('=SUM(B2:B3)+$A1+B$2+$C$3+IF(A1="中文 A1",1,0)'); await page.locator('#sheet-apply').click();
  const copied = await page.evaluate(() => {
    const data = new DataTransfer(); document.getElementById('sheet-viewport').dispatchEvent(new ClipboardEvent('copy', { clipboardData: data, bubbles: true, cancelable: true }));
    return data.getData('application/x-rust-wps-cells+json');
  });
  await page.locator('[data-address="D6"]').click();
  await page.evaluate(text => {
    const data = new DataTransfer(); data.setData('application/x-rust-wps-cells+json', text);
    document.getElementById('sheet-viewport').dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
  }, copied);
  const expected = 'SUM(D4:D5)+$A3+D$2+$C$3+IF(C3="中文 A1",1,0)';
  await expect(page.getByLabel('公式或原始值')).toHaveValue(`=${expected}`);
  await page.locator('#sheet-undo').click(); await expect(page.getByLabel('公式或原始值')).toHaveValue('');
  await page.locator('#sheet-redo').click(); await expect(page.getByLabel('公式或原始值')).toHaveValue(`=${expected}`);
  await page.evaluate(() => {
    const data = new DataTransfer(); data.setData('application/x-rust-wps-cells+json', JSON.stringify({ version: 2, origin: { row: 0, column: 0 }, rows: [[{ type: 'text', value: 'partial change' }, { type: 'formula', value: '"unterminated' }]] }));
    document.getElementById('sheet-viewport').dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
  });
  await expect(page.locator('#sheet-cell-info')).toContainText('公式引号未闭合');
  await expect(page.getByLabel('公式或原始值')).toHaveValue(`=${expected}`);
  await page.locator('#sheet-save').click(); await expect(page.locator('#sheet-cell-info')).toContainText('已导出 XLSX');
  const restored = JSON.parse(execFileSync(exe, ['read-xlsx-model', path.join(directory, 'formula.xlsx')], { encoding: 'utf8', windowsHide: true }));
  expect(restored.sheets[0].cells.find(cell => cell.row === 5 && cell.column === 3).content).toEqual({ type: 'formula', value: expected });
});
