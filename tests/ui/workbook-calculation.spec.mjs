import { test, expect } from '@playwright/test';
import { nativeBridge } from './bridge.mjs';
import { loadTsModule } from './load-ts.mjs';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import path from 'node:path';
const { validateCalculation } = await loadTsModule('ui/workbook-calculation.ts');
const cli = path.resolve('target/debug/rust-wps.exe');
const model = () => ({ version: 1, sheets: [{ name: 'Data', cells: [
  { row: 0, column: 0, content: { type: 'number', value: 2 } },
  { row: 0, column: 1, content: { type: 'formula', value: 'A1*3' }, number_format: '0.00' },
  { row: 0, column: 2, content: { type: 'formula', value: 'UNKNOWN(1,0)' } },
  { row: 0, column: 3, content: { type: 'formula', value: 'D1' } },
  { row: 0, column: 4, content: { type: 'formula', value: 'SUM(A1:A100001)' } },
] }] });
function calculate(model) {
  const directory = mkdtempSync(path.resolve('tmp/spreadsheets/calculate-ui-'));
  const file = path.join(directory, 'model.json'); writeFileSync(file, JSON.stringify(model));
  return JSON.parse(execFileSync(cli, ['calculate-workbook', file], { encoding: 'utf8', windowsHide: true }));
}

test('automatic calculation coalesces edits and serializes a fresh calculation after a stale response',async({page})=>{
 const source=model();source.sheets[0].cells=source.sheets[0].cells.slice(0,2);
 let finish,calls=0,active=0,maximum=0,first;
 await nativeBridge(page,async(command,args)=>{
  if(command==='load_workbook_draft')return source;
  if(command==='save_workbook_draft')return;
  if(command==='calculate_workbook'){
   calls++;active++;maximum=Math.max(maximum,active);
   if(calls===1){first=args.model;return new Promise(resolve=>{finish=()=>{active--;resolve(calculate(first));};});}
   const result=calculate(args.model);active--;return result;
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#restore-workbook').click();await page.locator('#sheet-auto-calculate').check();
 await expect.poll(()=>Boolean(finish)).toBe(true);
 await page.locator('[data-address="A1"]').click();await page.locator('#sheet-formula').fill('4');await page.locator('#sheet-apply').click();
 await page.locator('#sheet-formula').fill('7');await page.locator('#sheet-apply').click();
 finish();await expect(page.locator('[data-address="B1"]')).toHaveText('21');expect(calls).toBe(2);expect(maximum).toBe(1);
 await page.locator('#sheet-auto-calculate').uncheck();await page.locator('#sheet-formula').fill('8');await page.locator('#sheet-apply').click();
 await page.waitForTimeout(800);expect(calls).toBe(2);await expect(page.locator('[data-address="B1"]')).toHaveText('');
 await page.locator('#sheet-calculate').click();await expect(page.locator('[data-address="B1"]')).toHaveText('24');
});

test('automatic calculation stops after a failure and resumes on the next edit',async({page})=>{
 const source=model();source.sheets[0].cells=source.sheets[0].cells.slice(0,2);let calls=0;
 await nativeBridge(page,async(command,args)=>{if(command==='load_workbook_draft')return source;if(command==='save_workbook_draft')return;if(command==='calculate_workbook'){if(++calls===1)throw new Error('worker unavailable');return calculate(args.model);}throw new Error(command);});
 await page.goto('/');await page.locator('#restore-workbook').click();await page.locator('#sheet-auto-calculate').check();
 await expect(page.locator('#sheet-calculation-state')).toHaveText('计算失败');await page.waitForTimeout(800);expect(calls).toBe(1);
 await page.locator('[data-address="A1"]').click();await page.locator('#sheet-formula').fill('5');await page.locator('#sheet-apply').click();
 await expect(page.locator('[data-address="B1"]')).toHaveText('15');expect(calls).toBe(2);
});

test('leaving cancels a pending automatic calculation',async({page})=>{
 let calls=0;
 page.on('dialog',dialog=>dialog.accept());
 await nativeBridge(page,async(command)=>{if(command==='load_workbook_draft')return model();if(command==='save_workbook_draft')return;if(command==='calculate_workbook'){calls++;throw new Error('unexpected calculation');}throw new Error(command);});
 await page.goto('/');await page.locator('#restore-workbook').click();await page.locator('#sheet-auto-calculate').check();await page.locator('#new-document').click();
 await expect(page.locator('#spreadsheet')).toBeHidden();await page.waitForTimeout(800);expect(calls).toBe(0);
});

test('scalar math updates through automatic calculation with dependencies and numeric errors',async({page})=>{
 const source={version:1,sheets:[{name:'Math',cells:[
  {row:0,column:0,content:{type:'number',value:-8.9}},
  {row:0,column:1,content:{type:'formula',value:'INT(A1)'}},
  {row:0,column:2,content:{type:'formula',value:'SQRT(ABS(B1))'}},
  {row:0,column:3,content:{type:'formula',value:'SIGN(A1)'}},
  {row:0,column:4,content:{type:'formula',value:'SQRT(A1)'}},
 ]}]};
 await nativeBridge(page,async(command,args)=>{if(command==='load_workbook_draft')return source;if(command==='save_workbook_draft')return;if(command==='calculate_workbook')return calculate(args.model);throw new Error(command);});
 await page.goto('/');await page.locator('#restore-workbook').click();await page.locator('#sheet-auto-calculate').check();
 await expect(page.locator('[data-address="B1"]')).toHaveText('-9');await expect(page.locator('[data-address="C1"]')).toHaveText('3');
 await expect(page.locator('[data-address="D1"]')).toHaveText('-1');await expect(page.locator('[data-address="E1"]')).toHaveText('#NUM!');
 await page.locator('[data-address="A1"]').click();await page.locator('#sheet-formula').fill('16');await page.locator('#sheet-apply').click();
 await expect(page.locator('[data-address="B1"]')).toHaveText('16');await expect(page.locator('[data-address="C1"]')).toHaveText('4');
 await expect(page.locator('[data-address="D1"]')).toHaveText('1');await expect(page.locator('[data-address="E1"]')).toHaveText('4');
});

test('report validation rejects incomplete, duplicate, stale and malformed values atomically', () => {
  const source = model(), report = calculate(source);
  expect(validateCalculation(report, source)).toEqual(report);
  const cases = [];
  let bad = structuredClone(report); bad.cells.pop(); cases.push(bad);
  bad = structuredClone(report); bad.cells[0].formula = 'A1+3'; cases.push(bad);
  bad = structuredClone(report); bad.cells[1] = bad.cells[0]; cases.push(bad);
  bad = structuredClone(report); bad.cells[0].value.value = Infinity; cases.push(bad);
  bad = structuredClone(report); bad.complete = true; cases.push(bad);
  bad = structuredClone(report); bad.cells[1].failure.kind = 'random'; cases.push(bad);
  bad = structuredClone(report); bad.cells[0].failure = { kind: 'error', code: '#REF!' }; cases.push(bad);
  for (const item of cases) expect(() => validateCalculation(item, source)).toThrow();
});

test('desktop command displays Rust results and failures, formats them and preserves uncached export', async ({ page }) => {
  const directory = mkdtempSync(path.resolve('tmp/spreadsheets/calculate-export-')), output = path.join(directory, 'output.xlsx');
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_workbook_draft') return model();
    if (command === 'save_workbook_draft') return;
    if (command === 'calculate_workbook') return calculate(args.model);
    if (command === 'save_new_workbook') { const input = path.join(directory, 'export.json'); writeFileSync(input, JSON.stringify(args.model)); execFileSync(cli, ['create-xlsx', input, output], { windowsHide: true }); return { path: output }; }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-workbook').click(); await page.locator('#sheet-calculate').click();
  await expect(page.locator('[data-address="B1"]')).toHaveText('6');
  await expect(page.locator('[data-address="C1"]')).toHaveText('未支持');
  await expect(page.locator('[data-address="D1"]')).toHaveText('循环引用');
  await expect(page.locator('[data-address="E1"]')).toHaveText('超出计算上限');
  await expect(page.locator('#sheet-calculation-state')).toHaveText('本次部分计算');
  await page.locator('[data-address="B1"]').click(); await expect(page.locator('#sheet-formula')).toHaveValue('=A1*3');
  await expect(page.locator('#sheet-cell-info')).toContainText('本次计算 6');
  await page.locator('#sheet-formatted').check(); await expect(page.locator('[data-address="B1"]')).toHaveText('6.00');
  await page.screenshot({ path: 'tmp/workbook-calculation.png' });
  await page.locator('#sheet-save').click(); await expect(page.locator('#sheet-cell-info')).toContainText('已导出 XLSX');
  const exported = JSON.parse(execFileSync(cli, ['inspect-xlsx', output], { encoding: 'utf8', windowsHide: true }));
  expect(exported.sheets[0].cells.filter(cell => cell.formula).every(cell => cell.formula_cache === 'absent')).toBe(true);
  expect(exported.sheets[0].cells.find(cell => cell.column === 1).formula).toBe('A1*3');
  await page.locator('[data-address="A1"]').click(); await page.locator('#sheet-formula').fill('4'); await page.locator('#sheet-apply').click();
  await expect(page.locator('[data-address="B1"]')).toHaveText('');
  await expect(page.locator('#sheet-calculation-state')).toHaveText('已过期，需重算');
  await page.locator('#sheet-calculate').click(); await expect(page.locator('[data-address="B1"]')).toHaveText('12.00');
});

test('editing during calculation discards late results and blocks leaving until the worker finishes', async ({ page }) => {
  let finish, received;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_workbook_draft') return model();
    if (command === 'save_workbook_draft') return;
    if (command === 'calculate_workbook') { received = args.model; return new Promise(resolve => { finish = resolve; }); }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-workbook').click(); await page.locator('#sheet-calculate').click();
  await expect.poll(() => Boolean(finish)).toBe(true); await expect(page.locator('#sheet-calculate')).toBeDisabled();
  await page.locator('#new-document').click(); await expect(page.locator('#sheet-cell-info')).toContainText('公式计算尚未完成');
  await page.locator('[data-address="A1"]').click(); await page.locator('#sheet-formula').fill('7'); await page.locator('#sheet-apply').click();
  finish(calculate(received));
  await expect(page.locator('#sheet-cell-info')).toContainText('已丢弃过期');
  await expect(page.locator('[data-address="A1"]')).toHaveText('7'); await expect(page.locator('[data-address="B1"]')).toHaveText('');
  await expect(page.locator('#sheet-calculate')).toBeEnabled();
});

test('worker errors retain content and malformed results never become visible', async ({ page }) => {
  let attempt = 0;
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_workbook_draft') return model();
    if (command === 'save_workbook_draft') return;
    if (command === 'calculate_workbook') { if (!attempt++) throw new Error('解析超时'); const result = calculate(args.model); result.cells[0].formula = 'stale'; return result; }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-workbook').click(); await page.locator('#sheet-calculate').click();
  await expect(page.locator('#sheet-cell-info')).toContainText('解析超时');
  await page.locator('#sheet-calculate').click(); await expect(page.locator('#sheet-cell-info')).toContainText('版本不符');
  await expect(page.locator('[data-address="B1"]')).toHaveText(''); await expect(page.locator('[data-address="A1"]')).toHaveText('2');
  await expect(page.locator('#sheet-calculate')).toBeEnabled();
});

test('conditional formula selects text or numeric results after a model edit', async ({ page }) => {
  const source = model(); source.sheets[0].cells = source.sheets[0].cells.slice(0, 2);
  source.sheets[0].cells[1].content.value = 'IF(A1>0,"positive",1/0)';
  await nativeBridge(page, async (command, args) => {
    if (command === 'load_workbook_draft') return source;
    if (command === 'save_workbook_draft') return;
    if (command === 'calculate_workbook') return calculate(args.model);
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-workbook').click(); await page.locator('#sheet-calculate').click();
  await expect(page.locator('[data-address="B1"]')).toHaveText('positive');
  await page.locator('[data-address="A1"]').click(); await page.locator('#sheet-formula').fill('-2'); await page.locator('#sheet-apply').click();
  await expect(page.locator('[data-address="B1"]')).toHaveText('');
  await page.locator('#sheet-calculate').click(); await expect(page.locator('[data-address="B1"]')).toHaveText('#DIV/0!');
});
