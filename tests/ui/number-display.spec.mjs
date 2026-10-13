import { test, expect } from '@playwright/test';
import { loadTsModule } from './load-ts.mjs';
import { nativeBridge } from './bridge.mjs';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import path from 'node:path';
const { formatNumber: format } = await loadTsModule('ui/number-display.ts');

test('numeric presets round and group without converting stored content', () => {
  for (const [value, code, text] of [[1234.5, '#,##0.00', '1,234.50'], [0.125, '0.00%', '12.50%'], [-0.125, '0%', '-13%'], [1.005, '0.00', '1.01'], [-1.005, '0.00', '-1.01'], [12.8, '0', '13'], [-0, '0.00', '0.00'], [12, '@', '12']]) expect(format(value, code)).toEqual({ text });
  expect(format(1e308, '0%').fallback).toContain('超出');
  expect(format(123, '[Red]0.00').text).toBe('123'); expect(format(123, '[Red]0.00').fallback).toBeTruthy();
});

test('date and time presets handle epochs, leap compatibility and rounding', () => {
  for (const [value, epoch, text] of [[1, false, '1900-01-01'], [59, false, '1900-02-28'], [60, false, '1900-02-29'], [61, false, '1900-03-01'], [45292.75, false, '2024-01-01'], [0, true, '1904-01-01'], [43830, true, '2024-01-01'], [2958465, false, '9999-12-31']]) expect(format(value, 'yyyy-mm-dd', epoch)).toEqual({ text });
  expect(format(0, 'yyyy-mm-dd').fallback).toBeTruthy();
  expect(format(-1, 'yyyy-mm-dd').fallback).toBeTruthy();
  expect(format(2958465, 'yyyy-mm-dd', true).fallback).toBeTruthy();
  expect(format(0.5, 'hh:mm:ss').text).toBe('12:00:00');
  expect(format(1 + 1 / 86400, 'hh:mm:ss').text).toBe('00:00:01');
  expect(format(1 - 0.1 / 86400, 'hh:mm:ss').text).toBe('00:00:00');
});

test('normal dates independently agree with openpyxl in both date systems', () => {
  const samples = [[1, false], [59, false], [61, false], [1462, false], [45292, false], [2958465, false], [0, true], [43830, true]];
  const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const dates = JSON.parse(execFileSync(python, ['-c', 'import json,sys,datetime; from openpyxl.utils.datetime import from_excel,CALENDAR_MAC_1904,CALENDAR_WINDOWS_1900; a=json.loads(sys.stdin.read()); print(json.dumps([(from_excel(n,CALENDAR_MAC_1904 if e else CALENDAR_WINDOWS_1900).date().isoformat() if n else "1904-01-01") for n,e in a]))'], { input: JSON.stringify(samples), encoding: 'utf8', windowsHide: true }));
  expect(samples.map(([n, epoch]) => format(n, 'yyyy-mm-dd', epoch).text)).toEqual(dates);
});

test('formatted view keeps formula input and typed clipboard raw and flags unsupported codes', async ({ page }) => {
  let draft;
  await nativeBridge(page, async (command, args) => { if (command === 'save_workbook_draft') { draft = args.model; return; } throw new Error(command); });
  await page.goto('/'); await page.locator('#new-workbook').click();
  await page.locator('#sheet-input-type').selectOption('number'); await page.locator('#sheet-formula').fill('0.125'); await page.locator('#sheet-apply').click();
  await page.locator('#sheet-format-preset').selectOption('0.00%'); await page.locator('#sheet-format-apply').click();
  await expect(page.locator('[data-address="A1"]')).toHaveText('0.125');
  await page.locator('#sheet-formatted').check();
  await expect(page.locator('[data-address="A1"]')).toHaveText('12.50%');
  await page.screenshot({ path: 'tmp/number-display.png' });
  await expect(page.locator('#sheet-formula')).toHaveValue('0.125');
  await page.locator('#sheet-formula').fill('0.5');
  await page.locator('#sheet-formatted').uncheck(); await page.locator('#sheet-formatted').check();
  await expect(page.locator('#sheet-formula')).toHaveValue('0.5');
  await page.locator('#sheet-formula').fill('0.125');
  const clipboard = await page.locator('#sheet-viewport').evaluate(node => {
    const data = new DataTransfer(); node.dispatchEvent(new ClipboardEvent('copy', { clipboardData: data, bubbles: true }));
    return { text: data.getData('text/plain'), types: [...data.types].map(type => [type, data.getData(type)]) };
  });
  expect(clipboard.text).toBe('12.50%');
  expect(clipboard.types.find(([type]) => type !== 'text/plain')[1]).toContain('0.125');
  await page.locator('#sheet-format-code').fill('[Red]0.00'); await page.locator('#sheet-format-apply').click();
  await expect(page.locator('[data-address="A1"]')).toHaveText('0.125');
  await expect(page.locator('[data-address="A1"]')).toHaveAttribute('data-format-fallback', 'true');
  await page.locator('[data-address="A1"]').click();
  await expect(page.locator('#sheet-cell-info')).toContainText('此格式尚未支持');
  await expect.poll(() => draft?.sheets[0].cells[0].content.value).toBe(0.125);
  await page.locator('#sheet-formatted').uncheck();
  await expect(page.locator('[data-address="A1"]')).not.toHaveAttribute('data-format-fallback', 'true');
});

test('read-only formatted view uses workbook date system and retains raw formula-bar values', async ({ page }) => {
  const report = { date1904: true, recalculated: false, warnings: [], sheets: [{ name: 'Dates', visibility: 'Visible', kind: 'Worksheet', merged_ranges: [], cells: [{ address: 'A1', row: 0, column: 0, value: { type: 'number', value: 0 }, number_format: 'yyyy-mm-dd', formula: null, formula_cache: null }] }] };
  await nativeBridge(page, async command => { if (command === 'open_document') return { name: 'dates.xlsx', path: 'dates.xlsx', inspection: { format: 'xlsx', bytes: 1, text: '', warnings: [] }, workbook: report }; throw new Error(command); });
  await page.goto('/'); await page.locator('#open').click();
  await expect(page.locator('#sheet-format-toolbar')).toBeHidden();
  await page.locator('#sheet-formatted').check();
  await expect(page.locator('[data-address="A1"]')).toHaveText('1904-01-01');
  await expect(page.locator('#sheet-formula')).toHaveValue('0');
  await expect(page.locator('#sheet-formula')).toHaveAttribute('readonly', '');
});

test('editable 1904 dates retain epoch through edit, sheet undo, draft and actual export', async ({ page }) => {
  const source = { version:1, date1904:true, sheets:[{name:'Dates',cells:[{row:0,column:0,content:{type:'number',value:0},number_format:'yyyy-mm-dd'}]}] };
  const directory = mkdtempSync(path.resolve('tmp/spreadsheets/date1904-ui-'));
  const output = path.join(directory,'dates.xlsx');
  let draft;
  await nativeBridge(page, async (command,args) => {
    if(command==='load_workbook_draft') return source;
    if(command==='save_workbook_draft') {draft=args.model;return;}
    if(command==='save_new_workbook') {
      const input=path.join(directory,'model.json'); writeFileSync(input,JSON.stringify(args.model));
      execFileSync(path.resolve('target/debug/rust-wps.exe'),['create-xlsx',input,output],{windowsHide:true});
      return {path:output};
    }
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#restore-workbook').click();await page.locator('#sheet-formatted').check();
  await expect(page.locator('[data-address="A1"]')).toHaveText('1904-01-01');
  await page.locator('#sheet-formula').fill('1');await page.locator('#sheet-apply').click();
  await expect(page.locator('[data-address="A1"]')).toHaveText('1904-01-02');
  await expect(page.locator('#sheet-status')).toContainText('1904 日期系统');
  await page.locator('#sheet-add').click();await page.locator('#sheet-undo').click();
  await expect(page.locator('[data-address="A1"]')).toHaveText('1904-01-02');
  await expect.poll(()=>draft?.date1904).toBe(true);
  await page.locator('#sheet-save').click();await expect(page.locator('#sheet-cell-info')).toContainText('已导出 XLSX');
  const python = path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const actual=JSON.parse(execFileSync(python,['-c','import openpyxl,json,sys; b=openpyxl.load_workbook(sys.argv[1]); print(json.dumps([str(b.epoch),str(b.active["A1"].value)]))',output],{encoding:'utf8',windowsHide:true}));
  expect(actual).toEqual(['1904-01-01 00:00:00','1904-01-02 00:00:00']);
});
