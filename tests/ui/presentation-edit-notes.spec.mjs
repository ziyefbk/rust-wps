import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {writeFileSync,mkdirSync} from 'node:fs';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';
test('authored notes edit, undo, draft, export and reopen without appearing on slide',async({page},info)=>{
  const exe=path.resolve('target/debug/rust-wps.exe'),options={encoding:'utf8',windowsHide:true};let draft,saved,output;
  await nativeBridge(page,async(command,args)=>{
    if(command==='save_presentation_draft'){draft=structuredClone(args.model);return;}
    if(command==='save_new_presentation'){
      output=info.outputPath('notes.pptx');mkdirSync(path.dirname(output),{recursive:true});
      const json=info.outputPath('notes.json');writeFileSync(json,JSON.stringify(args.model));
      execFileSync(exe,['create-pptx',json,output],options);saved=JSON.parse(execFileSync(exe,['read-pptx-model',output],options));return {path:output};
    }
    if(command==='open_document')return {name:'notes.pptx',path:output,inspection:{format:'pptx',text:'',bytes:1,warnings:[]},presentation:JSON.parse(execFileSync(exe,['inspect-pptx',output],options)),presentation_model:saved};
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#new-presentation').click();
  const value='中文备注 <test>\n\n末行\n';
  await page.locator('#pptx-edit-notes').fill(value);
  await expect(page.locator('#slide-notes')).toHaveText(value);
  await expect(page.locator('#slide-canvas')).not.toContainText('中文备注');
  await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-edit-notes')).toHaveValue('');
  await page.locator('#pptx-redo').click();await expect(page.locator('#pptx-edit-notes')).toHaveValue(value);
  await expect.poll(()=>draft?.slides[0].notes).toBe(value);
  await page.locator('#pptx-editor-save').click();await expect(page.locator('#presentation-edit-state')).toContainText('已另存');
  expect(saved.slides[0].notes).toBe(value);
  const python='C:/Users/28275/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe';
  const result=execFileSync(python,['-c','import sys,json;from pptx import Presentation;p=Presentation(sys.argv[1]);print(json.dumps(p.slides[0].notes_slide.notes_text_frame.text,ensure_ascii=True))',output],options);
  expect(JSON.parse(result)).toBe(value);
  await page.locator('#open').click();await expect(page.locator('#pptx-edit-notes')).toHaveValue(value);
});
