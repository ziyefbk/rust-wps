import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {writeFileSync,mkdirSync} from 'node:fs';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';
test('authored hidden pages undo, draft, export and remain editable while slideshow skips them',async({page},info)=>{
 const exe=path.resolve('target/debug/rust-wps.exe'),options={encoding:'utf8',windowsHide:true};let draft,saved;
 await nativeBridge(page,async(command,args)=>{
  if(command==='save_presentation_draft'){draft=structuredClone(args.model);return;}
  if(command==='save_new_presentation'){
   const output=info.outputPath('hidden.pptx');mkdirSync(path.dirname(output),{recursive:true});const json=info.outputPath('hidden.json');writeFileSync(json,JSON.stringify(args.model));execFileSync(exe,['create-pptx',json,output],options);
   saved=JSON.parse(execFileSync(exe,['read-pptx-model',output],options));
   const python='C:/Users/28275/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe';
   expect(JSON.parse(execFileSync(python,['-c','from pptx import Presentation;import sys,json;p=Presentation(sys.argv[1]);print(json.dumps([s._element.get("show","1") for s in p.slides]))',output],options))).toEqual(['0','1']);return {path:output};
  }
  throw new Error(command);
 });
 await page.goto('/');await page.locator('#new-presentation').click();
 await page.locator('#pptx-box-text').fill('隐藏页仍可编辑');await page.locator('#pptx-edit-notes').fill('保留备注');
 await page.locator('#pptx-toggle-hidden').click();await expect(page.locator('#slide-status')).toContainText('隐藏页');
 await expect(page.locator('#pptx-play')).toBeDisabled();await expect(page.locator('#pptx-export-pdf')).toBeDisabled();
 await page.locator('#pptx-undo').click();await expect(page.locator('#slide-status')).toContainText('普通页');
 await page.locator('#pptx-redo').click();await expect(page.locator('#slide-status')).toContainText('隐藏页');
 await page.locator('#pptx-add-slide').click();await page.locator('#pptx-add-box').click();await page.locator('#pptx-box-text').fill('可见页');
 await expect.poll(()=>draft?.slides[0].hidden).toBe(true);
 await page.locator('#pptx-editor-save').click();await expect(page.locator('#presentation-edit-state')).toContainText('已另存');
 expect(saved.slides[0].hidden).toBe(true);expect(saved.slides[0].notes).toBe('保留备注');
 await page.locator('#slide-picker').selectOption('0');await expect(page.locator('#pptx-box-text')).toHaveValue('隐藏页仍可编辑');
 await page.locator('#pptx-play').click();await expect(page.locator('#presentation-show')).toContainText('可见页');await expect(page.locator('#presentation-show')).not.toContainText('隐藏页仍可编辑');await page.keyboard.press('Escape');
});
