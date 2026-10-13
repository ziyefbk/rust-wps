import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';

for(const format of ['ppt','pptx'])test(`${format} master static text is readable separately from slide text`,async({page})=>{
  const source=path.resolve(`tests/fixtures/apache-poi/WithMaster.${format}`);
  const presentation=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),[`inspect-${format}`,source],{encoding:'utf8',windowsHide:true}));
  await nativeBridge(page,async command=>{
    if(command==='open_document')return {name:`WithMaster.${format}`,path:source,inspection:{format,text:'',bytes:100,warnings:presentation.warnings},presentation};
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#open').click();
  await expect(page.locator('#presentation-master-text')).toHaveText('This text comes from the Master Slide\n');
  await expect(page.locator('#slide-text')).not.toContainText('Master Slide');
  await page.locator('#slide-picker').selectOption('1');
  await expect(page.locator('#presentation-master-text')).toContainText('Master Slide');
  await expect(page.locator('#presentation-master-text')).not.toContainText('Master Title');
});

test('legacy PPT opens real ordered text and retains selected page after failure',async({page})=>{
  const source=path.resolve('tests/fixtures/apache-poi/basic_test_ppt_file.ppt');
  const presentation=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['inspect-ppt',source],{encoding:'utf8',windowsHide:true}));
  let calls=0;
  await nativeBridge(page,async command=>{
    if(command==='open_document'){
      if(++calls===2)throw new Error('损坏 PPT');
      return {name:'basic.ppt',path:source,inspection:{format:'ppt',text:'',bytes:100,warnings:presentation.warnings},presentation};
    }
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#open').click();
  await expect(page.locator('#format')).toHaveText('PPT');
  await expect(page.locator('#legacy-ppt-notice')).toContainText('仅逐页文本阅读');
  await expect(page.locator('#slide-text')).toContainText('This is on page 1');
  await expect(page.locator('#slide-notes')).toContainText('These are the notes for page 1');
  await expect(page.locator('#pptx-replace-save')).toHaveCount(0);
  await expect(page.locator('#pptx-play')).toBeDisabled();
  await expect(page.locator('#pptx-export-pdf')).toBeDisabled();
  await expect(page.locator('#slide-canvas')).toHaveCount(0);
  await page.locator('#slide-picker').selectOption('1');
  await expect(page.locator('#slide-text')).toContainText('This is page two');
  await expect(page.locator('#slide-notes')).toContainText('These are the notes on page two');
  await expect(page.locator('#slide-text')).not.toContainText('notes');
  await page.locator('#open').click();
  await expect(page.getByRole('status')).toContainText('损坏 PPT');
  await expect(page.locator('#slide-picker')).toHaveValue('1');
  await expect(page.locator('#slide-text')).toContainText('This is page two');
});
