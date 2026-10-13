import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
const cli=path.resolve('target/debug/rust-wps.exe'), source=path.resolve('tests/fixtures/external-basic.xlsx');
const run=command=>JSON.parse(execFileSync(cli,[command,source],{encoding:'utf8',windowsHide:true}));
const opened=()=>({name:'external-basic.xlsx',path:source,inspection:{format:'xlsx',text:'',bytes:5000,warnings:[]},workbook:run('inspect-xlsx'),workbook_model:null});

test('external XLSX import reviews real audit, cancels and then starts an unsaved editable copy',async({page})=>{
  let draft;
  await nativeBridge(page,async(command,args)=>{
    if(command==='open_document')return opened();
    if(command==='import_workbook_current')return run('import-xlsx');
    if(command==='save_workbook_draft'){draft=args.model;return;}
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#open').click();
  await expect(page.locator('#sheet-edit-toolbar')).toBeHidden();
  await page.locator('#sheet-import').click();
  await expect(page.locator('#import-description')).toContainText('XLSX');
  await expect(page.locator('#import-risks')).toContainText('源公式缓存丢弃');
  await page.getByRole('button',{name:'取消',exact:true}).click();
  await expect(page.locator('#sheet-edit-toolbar')).toBeHidden();
  await expect(page.locator('[data-address="A2"]')).toHaveText('001');
  expect(draft).toBeUndefined();
  await page.locator('#sheet-import').click();
  await page.getByRole('button',{name:'接受变化并编辑副本'}).click();
  await expect(page.locator('#sheet-edit-toolbar')).toBeVisible();
  await expect(page.locator('#sheet-edit-state')).toContainText('有未导出的修改');
  await expect(page.locator('#title')).toHaveText('external-basic.xlsx · 编辑副本');
  await expect(page.locator('#sheet-import')).toBeHidden();
  await expect.poll(()=>draft?.sheets[0].cells.find(c=>c.row===1&&c.column===2)?.content.value).toBe('B2*2');
  await page.locator('[data-address="B2"]').click();await page.locator('#sheet-formula').fill('24');await page.locator('#sheet-apply').click();
  await expect.poll(()=>draft?.sheets[0].cells.find(c=>c.row===1&&c.column===1)?.content.value).toBe(24);
});

test('blocked import shows reasons and never enables acceptance',async({page})=>{
  await nativeBridge(page,async(command)=>{
    if(command==='open_document')return opened();
    if(command==='import_workbook_current')return {model:null,risks:['原文件保持不变'],blockers:['数组公式不能恢复']};
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#open').click();await page.locator('#sheet-import').click();
  await expect(page.locator('#import-risks')).toContainText('不能转换：数组公式');
  await expect(page.getByRole('button',{name:'接受变化并编辑副本'})).toBeDisabled();
  await page.getByRole('button',{name:'取消',exact:true}).click();
  await expect(page.locator('[data-address="B2"]')).toHaveText('12.5');
  await expect(page.locator('#sheet-edit-toolbar')).toBeHidden();
});

test('failed workbook import preserves the reading view and allows retry',async({page})=>{
  await nativeBridge(page,async(command)=>{
    if(command==='open_document')return opened();
    if(command==='import_workbook_current')throw new Error('Workbook audit failed');
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#open').click();await page.locator('#sheet-import').click();
  await expect(page.getByRole('status')).toContainText('Workbook audit failed');
  await expect(page.locator('[data-address="A2"]')).toHaveText('001');
  await expect(page.locator('#sheet-import')).toBeEnabled();
  await expect(page.locator('#sheet-edit-toolbar')).toBeHidden();
});
