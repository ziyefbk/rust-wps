import { test, expect } from '@playwright/test';
import { loadTsModule } from './load-ts.mjs';
import { nativeBridge } from './bridge.mjs';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import path from 'node:path';
const { WorkbookEditor } = await loadTsModule('ui/workbook-model.ts');
const cell = (row, column, type, value, format) => ({ row, column, content: type === 'empty' ? { type } : { type, value }, ...(format ? { number_format: format } : {}) });
const initial = () => ({ version: 1, sheets: [{ name: 'Data', cells: [
  cell(0,0,'text','key'), cell(0,1,'text','value'),
  cell(1,0,'number',3,'0.00'), cell(1,1,'formula','A2+$A$1'),
  cell(2,0,'number',1), cell(2,1,'text','first'),
  cell(3,0,'number',1), cell(3,1,'text','second'),
  cell(4,1,'empty',undefined,'0%'), cell(0,3,'formula','A2'),
] }] });
const range = { top: 0, left: 0, bottom: 4, right: 1 };

test('stable rectangle sorting moves formats, sparse blanks and relative formulas in one undo', () => {
  const editor = new WorkbookEditor(initial()), before = editor.snapshot();
  expect(editor.sortRange(0, range, 0, false, true)).toBe(true);
  expect(editor.getCell(0,0,0).content.value).toBe('key');
  expect(editor.getCell(0,1,1).content.value).toBe('first');
  expect(editor.getCell(0,2,1).content.value).toBe('second');
  expect(editor.getCell(0,3,0).number_format).toBe('0.00');
  expect(editor.getCell(0,3,1).content.value).toBe('A4+$A$1');
  expect(editor.getCell(0,4,0)).toBeUndefined();
  expect(editor.getCell(0,4,1).number_format).toBe('0%');
  expect(editor.getCell(0,0,3).content.value).toBe('A2');
  const sorted = editor.snapshot();
  expect(editor.sortRange(0,range,0,false,true)).toBe(false);
  editor.undo(); expect(editor.snapshot()).toEqual(before); expect(editor.dirty).toBe(false);
  editor.redo(); expect(editor.snapshot()).toEqual(sorted);
  editor.sortRange(0,range,0,true,true);
  expect(editor.getCell(0,1,0).content.value).toBe(3);
  expect(editor.getCell(0,4,0)).toBeUndefined();
});

test('sort rejects invalid bounds and missing formula keys without changing content or history', () => {
  const editor = new WorkbookEditor(initial()), before = editor.snapshot();
  for (const [r,c] of [[range,2],[{...range,top:-1},0],[{...range,bottom:10000},0],[{...range,right:16384},0]]) expect(() => editor.sortRange(0,r,c)).toThrow();
  expect(() => editor.sortRange(0,range,1,false,true)).toThrow('先计算');
  expect(editor.snapshot()).toEqual(before); expect(editor.canUndo).toBe(false);
  const bad = new WorkbookEditor({ version:1,sheets:[{name:'Bad',cells:[cell(0,0,'number',2),cell(1,0,'number',1),cell(0,1,'formula','"unclosed')]}] });
  const snapshot = bad.snapshot();
  expect(() => bad.sortRange(0,{top:0,left:0,bottom:1,right:1},0)).toThrow();
  expect(bad.snapshot()).toEqual(snapshot); expect(bad.canUndo).toBe(false);
});

test('formula keys use fresh typed values and mixed types have a deterministic order', () => {
  const editor = new WorkbookEditor({version:1,sheets:[{name:'Data',cells:[
    cell(0,0,'formula','1+1'),cell(1,0,'number',1),cell(2,0,'text','10'),cell(3,0,'boolean',false),cell(5,0,'text',''),
  ]}]});
  editor.sortRange(0,{top:0,left:0,bottom:5,right:0},0,false,false,new Map([[0,{type:'number',value:2}]]));
  expect(editor.snapshot().sheets[0].cells.map(c=>c.content)).toEqual([
    {type:'number',value:1},{type:'formula',value:'1+1'},{type:'text',value:''},{type:'text',value:'10'},{type:'boolean',value:false},
  ]);
});

test('sorted XLSX retains row data, formula and formats in independent openpyxl read', () => {
  const editor = new WorkbookEditor(initial()); editor.sortRange(0,range,0,false,true);
  const directory = mkdtempSync(path.resolve('tmp/spreadsheets/sort-'));
  const input = path.join(directory,'model.json'), output = path.join(directory,'sorted.xlsx');
  writeFileSync(input,JSON.stringify(editor.snapshot()));
  execFileSync(path.resolve('target/debug/rust-wps.exe'),['create-xlsx',input,output],{windowsHide:true});
  const python = path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const actual = JSON.parse(execFileSync(python,['-c','import openpyxl,json,sys; s=openpyxl.load_workbook(sys.argv[1]).active; print(json.dumps({"keys":[s.cell(r,1).value for r in range(1,6)],"formula":s["B4"].value,"format":s["A4"].number_format,"blankFormat":s["B5"].number_format,"outside":s["D1"].value}))',output],{encoding:'utf8',windowsHide:true}));
  expect(actual).toEqual({keys:['key',1,1,3,null],formula:'=A4+$A$1',format:'0.00',blankFormat:'0%',outside:'=A2'});
});

test('sort toolbar preserves selection, saves draft and undoes the whole rectangle', async ({page}) => {
  let draft;
  await nativeBridge(page,async(command,args)=>{
    if(command==='load_workbook_draft') return initial();
    if(command==='save_workbook_draft') {draft=args.model;return;}
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-workbook').click();
  await page.locator('[data-address="A1"]').click(); await page.locator('[data-address="B5"]').click({modifiers:['Shift']});
  await page.locator('#sheet-sort-header').check(); await page.locator('#sheet-sort-asc').click();
  await expect(page.locator('[data-address="B2"]')).toHaveText('first');
  await expect(page.locator('[role="gridcell"][aria-selected="true"]')).toHaveCount(10);
  await page.screenshot({path:'tmp/workbook-sort.png'});
  await expect.poll(()=>draft?.sheets[0].cells.find(c=>c.row===3&&c.column===1)?.content.value).toBe('A4+$A$1');
  await page.locator('#sheet-undo').click(); await expect(page.locator('[data-address="A2"]')).toHaveText('3');
  await expect.poll(()=>draft?.sheets[0].cells.find(c=>c.row===1&&c.column===1)?.content.value).toBe('A2+$A$1');
});

test('formula sorting requires calculation and invalidates computed keys after moving rows', async ({page}) => {
  const source = {version:1,sheets:[{name:'Data',cells:[cell(0,0,'formula','3'),cell(0,1,'text','three'),cell(1,0,'formula','1'),cell(1,1,'text','one')]}]};
  await nativeBridge(page,async(command,args)=>{
    if(command==='load_workbook_draft') return source;
    if(command==='save_workbook_draft') return;
    if(command==='calculate_workbook') {
      const directory=mkdtempSync(path.resolve('tmp/spreadsheets/sort-calc-')), file=path.join(directory,'model.json');
      writeFileSync(file,JSON.stringify(args.model));
      return JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['calculate-workbook',file],{encoding:'utf8',windowsHide:true}));
    }
    throw new Error(command);
  });
  await page.goto('/'); await page.locator('#restore-workbook').click();
  await page.locator('[data-address="A1"]').click(); await page.locator('[data-address="B2"]').click({modifiers:['Shift']});
  await page.locator('#sheet-sort-asc').click(); await expect(page.locator('#sheet-cell-info')).toContainText('先计算');
  await expect(page.locator('[data-address="B1"]')).toHaveText('three');
  await page.locator('#sheet-calculate').click(); await expect(page.locator('[data-address="A1"]')).toHaveText('3');
  await page.locator('#sheet-sort-asc').click(); await expect(page.locator('[data-address="B1"]')).toHaveText('one');
  await expect(page.locator('[data-address="A1"]')).toHaveText('');
  await expect(page.locator('#sheet-calculation-state')).toHaveText('已过期，需重算');
  await page.locator('#sheet-sort-desc').click(); await expect(page.locator('#sheet-cell-info')).toContainText('先计算');
  await expect(page.locator('[data-address="B1"]')).toHaveText('one');
  await page.locator('#sheet-undo').click(); await expect(page.locator('[data-address="B1"]')).toHaveText('three');
});
