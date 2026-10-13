import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';
test('PPTX notes follow selected slide, remain inert and clear on a page without notes',async({page})=>{
  const source=path.resolve('tests/fixtures/notes-slides.pptx');
  const presentation=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['inspect-pptx',source],{encoding:'utf8',windowsHide:true}));
  await nativeBridge(page,async command=>{
    if(command==='open_document')return {name:'notes.pptx',path:source,inspection:{format:'pptx',text:'',bytes:100,warnings:[]},presentation};
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#open').click();
  await expect(page.locator('#slide-notes')).toHaveText('第一页备注 <test>');
  await expect(page.locator('#slide-notes test')).toHaveCount(0);
  await expect(page.locator('#slide-text')).not.toContainText('备注');
  await page.locator('#slide-picker').selectOption('1');
  await expect(page.locator('#slide-notes')).toHaveText('第二页备注');
  await page.locator('#slide-picker').selectOption('2');
  await expect(page.locator('#slide-notes')).toBeHidden();
  await expect(page.locator('#slide-notes')).toBeEmpty();
});
