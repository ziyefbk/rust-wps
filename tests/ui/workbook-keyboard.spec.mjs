import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import path from 'node:path';
const cli=path.resolve('target/debug/rust-wps.exe');

test('keyboard entry navigates, drafts and exports committed Chinese, numeric and formula cells',async({page},info)=>{
 let draft,saved;
 await nativeBridge(page,async(command,args)=>{
  if(command==='save_workbook_draft'){draft=structuredClone(args.model);return;}
  if(command==='save_new_workbook'){
   const input=info.outputPath('model.json'),output=info.outputPath('keyboard.xlsx');mkdirSync(path.dirname(input),{recursive:true});writeFileSync(input,JSON.stringify(args.model));
   execFileSync(cli,['create-xlsx',input,output],{windowsHide:true});saved=JSON.parse(execFileSync(cli,['read-xlsx-model',output],{encoding:'utf8',windowsHide:true}));return {path:output};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#new-workbook').click();
 await page.locator('#sheet-formula').fill('连续中文录入');await page.locator('#sheet-formula').press('Enter');
 await expect(page.locator('#sheet-address')).toHaveValue('A2');await expect(page.locator('#sheet-formula')).toBeFocused();
 await page.locator('#sheet-input-type').selectOption('number');await page.locator('#sheet-formula').fill('24');await page.locator('#sheet-formula').press('Tab');
 await expect(page.locator('#sheet-address')).toHaveValue('B2');
 await page.locator('#sheet-input-type').selectOption('formula');await page.locator('#sheet-formula').fill('=A2*2');await page.locator('#sheet-formula').press('Shift+Enter');
 await expect(page.locator('#sheet-address')).toHaveValue('B1');
 await page.locator('#sheet-formula').fill('向左录入');await page.locator('#sheet-formula').press('Shift+Tab');await expect(page.locator('#sheet-address')).toHaveValue('A1');
 const expected=[{row:0,column:0,content:{type:'text',value:'连续中文录入'}},{row:0,column:1,content:{type:'text',value:'向左录入'}},{row:1,column:0,content:{type:'number',value:24}},{row:1,column:1,content:{type:'formula',value:'A2*2'}}];
 await expect.poll(()=>draft?.sheets[0].cells).toEqual(expected.map(cell=>({...cell,number_format:null})));
 await page.locator('#sheet-save').click();await expect(page.locator('#sheet-cell-info')).toContainText('已导出 XLSX');expect(saved.sheets[0].cells).toEqual(expected.map(cell=>({...cell,number_format:null})));
 await page.locator('#sheet-viewport').focus();await page.keyboard.press('Control+z');await expect(page.locator('[data-address="B1"]')).toHaveText('');
});

test('invalid input and IME confirmation never commit or move; Escape restores the cell',async({page})=>{
 let draft;
 const model={version:1,sheets:[{name:'Data',cells:[{row:0,column:0,content:{type:'number',value:2}}]}]};
 await nativeBridge(page,async(command,args)=>{if(command==='load_workbook_draft')return model;if(command==='save_workbook_draft'){draft=args.model;return;}throw new Error(command);});
 await page.goto('/');await page.locator('#restore-workbook').click();await page.locator('[data-address="A1"]').click();await page.keyboard.press('F2');await expect(page.locator('#sheet-formula')).toBeFocused();
 await page.locator('#sheet-formula').fill('invalid');await page.locator('#sheet-formula').press('Enter');await expect(page.locator('#sheet-address')).toHaveValue('A1');await expect(page.locator('#sheet-formula')).toHaveValue('invalid');
 await expect(page.locator('[data-address="A1"]')).toHaveText('2');
 await page.locator('#sheet-formula').fill('23');
 for(const options of [{isComposing:true},{keyCode:229}]){
  const canceled=await page.locator('#sheet-formula').evaluate((input,options)=>!input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true,...options})),options);expect(canceled).toBe(false);
 }
 await expect(page.locator('#sheet-address')).toHaveValue('A1');await expect(page.locator('[data-address="A1"]')).toHaveText('2');
 await page.locator('#sheet-formula').press('Escape');await expect(page.locator('#sheet-formula')).toHaveValue('2');await expect(page.locator('#sheet-viewport')).toBeFocused();
 await expect.poll(()=>draft?.sheets[0].cells[0].content.value).toBe(2);
 await page.locator('[data-address="A1"]').dblclick();await expect(page.locator('#sheet-formula')).toBeFocused();
});
