import { test, expect } from '@playwright/test';
import { loadTsModule } from './load-ts.mjs';
import { nativeBridge } from './bridge.mjs';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import path from 'node:path';
const { WorkbookEditor } = await loadTsModule('ui/workbook-model.ts');
const range = (top, left, bottom = top, right = left) => ({ top, left, bottom, right });
const initial = () => ({ version: 1, sheets: [{ name: 'Sheet1', cells: [{ row: 0, column: 0, content: { type: 'number', value: 0.125 } }, { row: 0, column: 1, content: { type: 'formula', value: 'A1*2' } }] }] });

test('range formats preserve values and sparse blanks through undo and clear', () => {
  const editor = new WorkbookEditor(initial()), before = editor.snapshot();
  editor.setNumberFormat(0, range(0, 0, 1, 1), '0.00%');
  expect(editor.snapshot().sheets[0].cells).toHaveLength(4);
  expect(editor.getCell(0, 0, 0).content.value).toBe(0.125);
  expect(editor.getCell(0, 0, 1).content.value).toBe('A1*2');
  expect(editor.getCell(0, 1, 1).content.type).toBe('empty');
  editor.undo(); expect(editor.snapshot()).toEqual(before); expect(editor.dirty).toBe(false);
  editor.redo(); editor.setCell(0, 0, 0, 'empty', '');
  expect(editor.getCell(0, 0, 0).number_format).toBe('0.00%');
  editor.setNumberFormat(0, range(0, 0, 1, 1), null);
  expect(editor.snapshot().sheets[0].cells).toHaveLength(1);
  expect(editor.getCell(0, 0, 1).content.value).toBe('A1*2');
  editor.undo(); expect(editor.snapshot().sheets[0].cells).toHaveLength(4);
  expect(editor.setNumberFormat(0, range(0, 0, 1, 1), '0.00%')).toBe(false);
});

test('invalid and over-budget formats reject the whole transaction', () => {
  const editor = new WorkbookEditor(initial()), before = editor.snapshot();
  for (const format of ['', '\0', '中'.repeat(342)]) expect(() => editor.setNumberFormat(0, range(0, 0), format)).toThrow();
  for (const rectangle of [range(-1, 0), range(0, 0, 10000, 0), range(0, 16384), range(1, 0, 0, 0)]) expect(() => editor.setNumberFormat(0, rectangle, '0')).toThrow();
  expect(() => editor.setNumberFormat(0, range(0, 0, 4999, 0), 'x'.repeat(1024))).toThrow();
  expect(editor.snapshot()).toEqual(before); expect(editor.canUndo).toBe(false);
  editor.setNumberFormat(0, range(1048575, 16383), '0.00');
  expect(editor.snapshot().sheets[0].cells).toHaveLength(3);
});

test('format codes export through Rust and independent openpyxl without changing raw numbers', () => {
  const editor = new WorkbookEditor(initial());
  editor.setNumberFormat(0, range(0, 0, 1, 1), '0.00%');
  editor.setCell(0, 2, 0, 'number', '45292'); editor.setNumberFormat(0, range(2, 0), 'yyyy-mm-dd');
  const directory = mkdtempSync(path.resolve('tmp/spreadsheets/formats-'));
  const input = path.join(directory, 'model.json'), output = path.join(directory, 'formats.xlsx');
  writeFileSync(input, JSON.stringify(editor.snapshot()));
  const exe = path.resolve('target/debug/rust-wps.exe');
  execFileSync(exe, ['create-xlsx', input, output], { windowsHide: true });
  const restored = JSON.parse(execFileSync(exe, ['read-xlsx-model', output], { encoding: 'utf8', windowsHide: true }));
  expect(restored.sheets[0].cells.find(cell => cell.row === 2).content.value).toBe(45292);
  const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const report = JSON.parse(execFileSync(python, ['-c', 'import openpyxl,json,sys; s=openpyxl.load_workbook(sys.argv[1]).active; print(json.dumps({"value":s["A1"].value,"formula":s["B1"].value,"formats":[s[a].number_format for a in ["A1","B1","A2","B2","A3"]],"date":str(s["A3"].value)}))', output], { encoding: 'utf8', windowsHide: true }));
  expect(report).toEqual({ value: 0.125, formula: '=A1*2', formats: ['0.00%', '0.00%', '0.00%', '0.00%', 'yyyy-mm-dd'], date: '2024-01-01 00:00:00' });
});

test('format toolbar applies a rectangular selection, autosaves and undoes', async ({ page }) => {
  let draft;
  await nativeBridge(page, async (command, args) => { if (command === 'save_workbook_draft') { draft = args.model; return; } throw new Error(command); });
  await page.goto('/'); await page.locator('#new-workbook').click();
  await page.locator('[data-address="A1"]').click();
  await page.locator('[data-address="B2"]').click({ modifiers: ['Shift'] });
  await page.locator('#sheet-format-preset').selectOption('0.00%');
  await page.locator('#sheet-format-apply').click();
  await expect(page.locator('[role="gridcell"][aria-selected="true"]')).toHaveCount(4);
  await expect.poll(() => draft?.sheets[0].cells.length).toBe(4);
  expect(draft.sheets[0].cells.every(cell => cell.number_format === '0.00%')).toBe(true);
  await expect(page.locator('#sheet-cell-info')).toContainText('网格暂显示原值');
  await page.screenshot({ path: 'tmp/number-format-toolbar.png' });
  await page.locator('#sheet-undo').click();
  await expect.poll(() => draft?.sheets[0].cells.length).toBe(0);
  await page.locator('#sheet-redo').click();
  await page.locator('[data-address="A1"]').click();
  await page.locator('[data-address="B2"]').click({ modifiers: ['Shift'] });
  await page.locator('#sheet-format-preset').selectOption(''); await page.locator('#sheet-format-apply').click();
  await expect.poll(() => draft?.sheets[0].cells.length).toBe(0);
});
