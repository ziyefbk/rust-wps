import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import path from 'node:path';

test('text alignment previews, undoes, drafts and survives actual PPTX and PDF snapshots',async({page},info)=>{
 let draft,saved,pdf=0;const options={encoding:'utf8',windowsHide:true};
 await nativeBridge(page,async(command,args)=>{
  if(command==='save_presentation_draft'){draft=structuredClone(args.model);return;}
  if(command==='export_current_pdf'){pdf++;await expect(page.locator('#print-document svg text').first()).toHaveAttribute('text-anchor','end');return null;}
  if(command==='save_new_presentation'){
   const json=info.outputPath('aligned.json'),output=info.outputPath('aligned.pptx');mkdirSync(path.dirname(output),{recursive:true});writeFileSync(json,JSON.stringify(args.model));const exe=path.resolve('target/debug/rust-wps.exe');execFileSync(exe,['create-pptx',json,output],options);saved=JSON.parse(execFileSync(exe,['read-pptx-model',output],options));
   const actual=execFileSync('C:/Users/28275/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe',['-c','from pptx import Presentation;import sys;p=Presentation(sys.argv[1]);print(int(p.slides[0].shapes[0].text_frame.paragraphs[0].alignment))',output],options).trim();expect(actual).toBe('3');return {path:output};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#new-presentation').click();await page.locator('#pptx-box-text').fill('中文对齐\n第二行');
 await page.locator('#pptx-box-alignment').selectOption('center');await expect(page.locator('#slide-canvas text').first()).toHaveAttribute('text-anchor','middle');
 await expect.poll(()=>draft?.slides[0].text_boxes[0].alignment).toBe('center');
 const box=draft.slides[0].text_boxes[0];await expect(page.locator('#slide-canvas text').first()).toHaveAttribute('x',String((box.bounds_emu[0]+box.bounds_emu[2]/2)/9525));
 await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-alignment')).toHaveValue('left');await expect(page.locator('#slide-canvas text').first()).toHaveAttribute('text-anchor','start');
 await page.locator('#pptx-redo').click();await expect(page.locator('#pptx-box-alignment')).toHaveValue('center');
 await page.locator('#pptx-box-alignment').selectOption('right');await page.locator('#pptx-export-pdf').click();await expect.poll(()=>pdf).toBe(1);
 await page.locator('#pptx-editor-save').click();await expect(page.locator('#presentation-edit-state')).toContainText('已另存');expect(saved.slides[0].text_boxes[0].alignment).toBe('right');
});
