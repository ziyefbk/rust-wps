import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import path from 'node:path';

for(const kind of ['text','image'])test(`canvas ${kind} resize cancels, commits once, keeps ratio and exports`,async({page},info)=>{
 const bounds=[914400,914400,1828800,914400];
 const text={bounds_emu:bounds,text:'尺寸文字',font_size_hundredths_pt:2400,bold:false,italic:false,color_rgb:[0,0,0]};
 const picture={bounds_emu:bounds,data_url:'data:image/png;base64,'+readFileSync('tests/fixtures/slide-image.png').toString('base64'),alt:'尺寸图片'};
 const model={version:1,size_emu:[10972800,6172200],slides:[{text_boxes:kind==='text'?[text]:[],...(kind==='image'?{images:[picture]}:{})}]};
 const object=value=>kind==='text'?value?.slides[0].text_boxes[0]:value?.slides[0].images[0];let draft,saved,writes=0;
 await nativeBridge(page,async(command,args)=>{
  if(command==='load_presentation_draft')return model;
  if(command==='save_presentation_draft'){draft=args.model;writes++;return;}
  if(command==='save_new_presentation'){
   const file=info.outputPath('resize.pptx'),json=info.outputPath('resize.json');mkdirSync(path.dirname(file),{recursive:true});writeFileSync(json,JSON.stringify(args.model));const exe=path.resolve('target/debug/rust-wps.exe');execFileSync(exe,['create-pptx',json,file],{windowsHide:true});saved=JSON.parse(execFileSync(exe,['read-pptx-model',file],{encoding:'utf8',windowsHide:true}));return {path:file};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#restore-presentation').click();await expect.poll(()=>writes).toBeGreaterThan(0);const before=writes;
 const group=()=>page.locator('#slide-preview g[data-shape-id="1"]');const handle=()=>group().locator('[data-resize-handle]');
 await group().focus();await page.keyboard.press('Space');await expect(handle()).toHaveCount(1);
 async function begin(){await handle().scrollIntoViewIfNeeded();const r=await handle().boundingBox(),p={x:r.x+r.width/2,y:r.y+r.height/2};await page.mouse.move(p.x,p.y);await page.mouse.down();return p;}
 let p=await begin();await page.mouse.move(p.x+40,p.y+20,{steps:4});await expect(group().locator('[data-resize-preview]')).toHaveCount(1);await page.waitForTimeout(800);expect(writes).toBe(before);expect(object(draft).bounds_emu).toEqual(bounds);
 await page.keyboard.press('Escape');await page.mouse.up();await expect(group().locator('[data-resize-preview]')).toHaveCount(0);await expect(page.locator('#pptx-undo')).toBeDisabled();
 p=await begin();const scale=await page.locator('#slide-canvas').evaluate(svg=>svg.getScreenCTM().a);await page.mouse.move(p.x+40,p.y+20,{steps:4});await page.mouse.up();
 const expected=[bounds[2]+40/scale*9525,bounds[3]+20/scale*9525];await expect.poll(()=>{const b=object(draft)?.bounds_emu;return b&&Math.abs(b[2]-expected[0])<=1&&Math.abs(b[3]-expected[1])<=1;}).toBe(true);expect(object(draft).bounds_emu.slice(0,2)).toEqual(bounds.slice(0,2));
 await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-undo')).toBeDisabled();await expect.poll(()=>object(draft)?.bounds_emu).toEqual(bounds);
 p=await begin();await page.keyboard.down('Shift');await page.mouse.move(p.x+40,p.y+3,{steps:4});await page.mouse.up();await page.keyboard.up('Shift');
 await expect.poll(()=>object(draft)?.bounds_emu[2]).toBeGreaterThan(bounds[2]);const resized=structuredClone(object(draft));expect(Math.abs(resized.bounds_emu[2]-2*resized.bounds_emu[3])).toBeLessThanOrEqual(1);
 await page.locator('#pptx-editor-save').click();await expect(page.locator('#notice')).toContainText('已另存 PPTX');expect(object(saved)).toEqual(resized);expect({...resized,bounds_emu:bounds}).toEqual(object(model));
 await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-undo')).toBeDisabled();await expect.poll(()=>object(draft)?.bounds_emu).toEqual(bounds);
 p=await begin();await page.mouse.move(5000,5000,{steps:4});await page.mouse.up();await expect.poll(()=>object(draft)?.bounds_emu).toEqual([bounds[0],bounds[1],model.size_emu[0]-bounds[0],model.size_emu[1]-bounds[1]]);
 await page.locator('#pptx-undo').click();await expect.poll(()=>object(draft)?.bounds_emu).toEqual(bounds);p=await begin();await page.mouse.move(-5000,-5000,{steps:4});await page.mouse.up();await expect.poll(()=>object(draft)?.bounds_emu).toEqual([bounds[0],bounds[1],1,1]);
});
