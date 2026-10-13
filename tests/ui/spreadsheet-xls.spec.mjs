import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';
test('legacy XLS opens read-only, distinguishes formula cache and navigates the BIFF8 far cell',async({page})=>{
  const source=path.resolve('tests/fixtures/basic-biff8.xls'), before=readFileSync(source);
  const workbook=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['inspect-xls',source],{encoding:'utf8',windowsHide:true}));
  await nativeBridge(page,async(command)=>{
    if(command==='open_document')return {name:'basic-biff8.xls',path:source,inspection:{format:'xls',text:'',bytes:before.length,warnings:workbook.warnings},workbook,workbook_model:null};
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#open').click();
  await expect(page.locator('#format')).toHaveText('XLS');
  await expect(page.getByRole('status')).toContainText('公式仅显示源缓存');
  await expect(page.locator('#sheet-edit-toolbar')).toBeHidden();await expect(page.locator('#sheet-import')).toBeHidden();
  await page.locator('[data-address="B1"]').click();
  await expect(page.locator('[data-address="B1"]')).toHaveText('25');
  await expect(page.locator('#sheet-formula')).toHaveValue('25');
  await expect(page.locator('#sheet-formula')).toHaveAttribute('readonly','');
  await expect(page.locator('#sheet-cell-info')).toContainText('XLS 公式表达式未恢复，仅源缓存 present');
  await page.locator('[data-address="C2"]').click();await expect(page.locator('#sheet-cell-info')).toContainText('源缓存 absent');
  await page.locator('#sheet-formatted').check();await expect(page.locator('[data-address="A2"]')).toHaveText('2024-01-01');
  await page.locator('#sheet-address').fill('IV65536');await page.locator('#sheet-address').press('Enter');
  await expect(page.locator('[data-address="IV65536"]')).toHaveText('Far');
  await page.locator('#sheet-picker').selectOption('1');await expect(page.locator('#sheet-status')).toContainText('VeryHidden');
  expect(readFileSync(source)).toEqual(before);
});
