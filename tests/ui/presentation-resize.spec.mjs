import {test,expect} from '@playwright/test';
import {loadTsModule} from './load-ts.mjs';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {mkdirSync,writeFileSync,readFileSync} from 'node:fs';
import path from 'node:path';
const {resizePresentation}=await loadTsModule('ui/presentation-resize.ts');
const base=()=>JSON.parse(readFileSync('docs/examples/presentation-v1.json','utf8'));
test('uniform fit preserves source, picture aspect and page metadata',()=>{
 const model=base();model.slides[0].notes='保留';model.slides[0].hidden=true;model.slides[0].images=[{bounds_emu:[0,0,914400,457200],data_url:'test',alt:'图'}];
 const before=structuredClone(model),result=resizePresentation(model,[9144000,6858000]);
 expect(model).toEqual(before);expect(result.slides[0].notes).toBe('保留');expect(result.slides[0].hidden).toBe(true);
 const [x,y,w,h]=result.slides[0].images[0].bounds_emu;expect(x).toBe(0);expect(y).toBe(857250);expect(w/h).toBe(2);
 expect(()=>resizePresentation(model,[0,1])).toThrow();
});
test('size switch validates through Rust, undoes, drafts and exports independent page dimensions',async({page},info)=>{
 let draft,saved,pdfPage,sequence=0;const exe=path.resolve('target/debug/rust-wps.exe'),options={encoding:'utf8',windowsHide:true};
 const create=model=>{const json=info.outputPath(`${sequence}.json`),output=info.outputPath(`${sequence++}.pptx`);mkdirSync(path.dirname(output),{recursive:true});writeFileSync(json,JSON.stringify(model));execFileSync(exe,['create-pptx',json,output],options);return output;};
 await nativeBridge(page,async(command,args)=>{
  if(command==='save_presentation_draft'){draft=structuredClone(args.model);return;}
  if(command==='validate_presentation'){create(args.model);return;}
  if(command==='export_current_pdf'){pdfPage=args.page;return null;}
  if(command==='save_new_presentation'){
   const output=create(args.model);saved=JSON.parse(execFileSync(exe,['read-pptx-model',output],options));
   const python='C:/Users/28275/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe';
   expect(JSON.parse(execFileSync(python,['-c','from pptx import Presentation;import json,sys;p=Presentation(sys.argv[1]);print(json.dumps([p.slide_width,p.slide_height]))',output],options))).toEqual([9144000,6858000]);return {path:output};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#new-presentation').click();
 await page.locator('#pptx-page-size').selectOption('standard');await page.locator('#pptx-apply-page-size').click();
 await expect(page.locator('#slide-canvas')).toHaveAttribute('viewBox','0 0 960 720');
 await page.locator('#pptx-undo').click();await expect(page.locator('#slide-canvas')).toHaveAttribute('viewBox','0 0 1152 648');
 await page.locator('#pptx-redo').click();await expect(page.locator('#slide-canvas')).toHaveAttribute('viewBox','0 0 960 720');
 await expect.poll(()=>draft?.size_emu).toEqual([9144000,6858000]);
 await page.locator('#pptx-export-pdf').click();await expect.poll(()=>pdfPage).toEqual({width_inches:10,height_inches:7.5,expected_pages:1});
 await page.locator('#pptx-editor-save').click();await expect(page.locator('#presentation-edit-state')).toContainText('已另存');expect(saved.size_emu).toEqual([9144000,6858000]);
});
