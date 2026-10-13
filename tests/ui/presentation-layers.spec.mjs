import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import path from 'node:path';

test('text layer order follows selection, history, draft and actual PPTX shape order',async({page},info)=>{
 let draft,saved;const options={encoding:'utf8',windowsHide:true};
 await nativeBridge(page,async(command,args)=>{
  if(command==='save_presentation_draft'){draft=structuredClone(args.model);return;}
  if(command==='save_new_presentation'){
   const json=info.outputPath('layers.json'),output=info.outputPath('layers.pptx');mkdirSync(path.dirname(output),{recursive:true});writeFileSync(json,JSON.stringify(args.model));
   const exe=path.resolve('target/debug/rust-wps.exe');execFileSync(exe,['create-pptx',json,output],options);
   saved=JSON.parse(execFileSync(exe,['read-pptx-model',output],options));
   const actual=JSON.parse(execFileSync('C:/Users/28275/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe',['-c','from pptx import Presentation;import json,sys;p=Presentation(sys.argv[1]);print(json.dumps([s.text for s in p.slides[0].shapes]))',output],options));
   expect(actual).toEqual(['上层文字','底层文字']);return {path:output};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#new-presentation').click();
 await expect(page.locator('#pptx-box-back')).toBeDisabled();await expect(page.locator('#pptx-box-front')).toBeDisabled();
 await page.locator('#pptx-box-text').fill('底层文字');await page.locator('#pptx-add-box').click();await page.locator('#pptx-box-text').fill('上层文字');
 await expect(page.locator('#pptx-box-front')).toBeDisabled();
 await page.locator('#pptx-box-back').click();await expect(page.locator('#pptx-box-picker')).toHaveValue('0');await expect(page.locator('#pptx-box-text')).toHaveValue('上层文字');
  await expect.poll(()=>draft?.slides[0].text_boxes.map(b=>b.text)).toEqual(['上层文字','底层文字']);
  await expect(page.locator('#slide-canvas text')).toHaveText(['上层文字','底层文字']);
 const ordered=structuredClone(draft);
  await page.locator('#pptx-undo').click();await expect(page.locator('#slide-text')).toHaveText('底层文字\n上层文字');
  await expect(page.locator('#slide-canvas text')).toHaveText(['底层文字','上层文字']);
 await page.locator('#pptx-redo').click();await expect(page.locator('#slide-text')).toHaveText('上层文字\n底层文字');
 await page.locator('#pptx-box-picker').selectOption('0');await page.locator('#pptx-box-front').click();await expect(page.locator('#pptx-box-picker')).toHaveValue('1');
 await page.locator('#pptx-undo').click();
 await page.locator('#pptx-editor-save').click();await expect(page.locator('#presentation-edit-state')).toContainText('已另存');expect(saved).toEqual(ordered);
});
