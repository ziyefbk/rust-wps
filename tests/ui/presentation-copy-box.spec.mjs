import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import path from 'node:path';

test('text box copy preserves formatting, edits independently, undoes and exports',async({page},info)=>{
 let draft,saved,sequence=0;
 const exe=path.resolve('target/debug/rust-wps.exe'),options={encoding:'utf8',windowsHide:true};
 const create=model=>{const json=info.outputPath(`${sequence}.json`),output=info.outputPath(`${sequence++}.pptx`);mkdirSync(path.dirname(output),{recursive:true});writeFileSync(json,JSON.stringify(model));execFileSync(exe,['create-pptx',json,output],options);return output;};
 await nativeBridge(page,async(command,args)=>{
  if(command==='save_presentation_draft'){draft=structuredClone(args.model);return;}
  if(command==='validate_presentation'){create(args.model);return;}
  if(command==='save_new_presentation'){
   const output=create(args.model);saved=JSON.parse(execFileSync(exe,['read-pptx-model',output],options));
   const python='C:/Users/28275/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe';
   const actual=JSON.parse(execFileSync(python,['-c','from pptx import Presentation;import json,sys;p=Presentation(sys.argv[1]);print(json.dumps([s.text for s in p.slides[0].shapes]))',output],options));
   expect(actual).toEqual(['中文原框','独立副本']);return {path:output};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#new-presentation').click();
 await page.locator('#pptx-box-text').fill('中文原框');await page.locator('#pptx-box-bold').check();
 await page.locator('#pptx-box-size').fill('32');await page.locator('#pptx-box-size').dispatchEvent('change');
 await expect.poll(()=>draft?.slides[0].text_boxes[0].font_size_hundredths_pt).toBe(3200);
 const original=structuredClone(draft.slides[0].text_boxes[0]);
 await page.locator('#pptx-copy-box').click();await expect(page.locator('#pptx-box-picker')).toHaveValue('1');
 await expect.poll(()=>draft?.slides[0].text_boxes.length).toBe(2);expect(draft.slides[0].text_boxes).toEqual([original,original]);
 await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-picker option')).toHaveCount(1);
 await page.locator('#pptx-redo').click();await expect(page.locator('#pptx-box-picker option')).toHaveCount(2);
 await page.locator('#pptx-box-picker').selectOption('1');await page.locator('#pptx-box-text').fill('独立副本');
 await page.locator('#pptx-editor-save').click();await expect(page.locator('#presentation-edit-state')).toContainText('已另存');
 expect(saved.slides[0].text_boxes[0]).toEqual(original);expect(saved.slides[0].text_boxes[1]).toEqual({...original,text:'独立副本'});
});

test('copy validation failure preserves model and history',async({page})=>{
 let draft;
 await nativeBridge(page,async(command,args)=>{
  if(command==='save_presentation_draft'){draft=structuredClone(args.model);return;}
  if(command==='validate_presentation')throw new Error('Presentation text exceeds 1 MiB');
  throw new Error(command);
 });
 await page.goto('/');await page.locator('#new-presentation').click();
 await page.locator('#pptx-box-text').fill('校验失败保留');await expect.poll(()=>draft?.slides[0].text_boxes[0].text).toBe('校验失败保留');
 const before=structuredClone(draft);
 await page.locator('#pptx-copy-box').click();await expect(page.locator('#notice')).toContainText('1 MiB');
 await expect(page.locator('#pptx-box-picker option')).toHaveCount(1);expect(draft).toEqual(before);
 await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-text')).not.toHaveValue('校验失败保留');
});
