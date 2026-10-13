import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import path from 'node:path';
test('pointer drag commits once, cancels without model changes, clamps to page and exports exact coordinates',async({page},info)=>{
 const original=[914400,914400,2743200,1828800];
 const source={version:1,size_emu:[10972800,6172200],slides:[{text_boxes:[{bounds_emu:original,text:'拖动中文',font_size_hundredths_pt:2400,bold:false,italic:false,color_rgb:[0,0,0]}]}]};
 let draft,saved;
 await nativeBridge(page,async(command,args)=>{
  if(command==='load_presentation_draft')return source;
  if(command==='save_presentation_draft'){draft=args.model;return;}
  if(command==='save_new_presentation'){
   const json=info.outputPath('model.json'),file=info.outputPath('dragged.pptx');mkdirSync(path.dirname(file),{recursive:true});writeFileSync(json,JSON.stringify(args.model));const cli=path.resolve('target/debug/rust-wps.exe');execFileSync(cli,['create-pptx',json,file],{windowsHide:true});saved=JSON.parse(execFileSync(cli,['read-pptx-model',file],{encoding:'utf8',windowsHide:true}));return {path:file};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#restore-presentation').click();await expect.poll(()=>draft?.slides[0].text_boxes[0].bounds_emu).toEqual(original);
 const group=()=>page.locator('#slide-preview g[data-shape-id="1"]');
 async function start(){await group().scrollIntoViewIfNeeded();const rect=await group().boundingBox();const point={x:rect.x+rect.width/2,y:rect.y+rect.height/2};await page.mouse.move(point.x,point.y);await page.mouse.down();return point;}
 let point=await start();await page.mouse.move(point.x+50,point.y+25,{steps:5});await expect(group()).toHaveAttribute('transform',/translate/);
 await page.waitForTimeout(800);expect(draft.slides[0].text_boxes[0].bounds_emu).toEqual(original);
 await page.keyboard.press('Escape');await page.mouse.up();await expect(group()).not.toHaveAttribute('transform');expect(draft.slides[0].text_boxes[0].bounds_emu).toEqual(original);
 point=await start();const scale=await page.locator('#slide-canvas').evaluate(svg=>svg.getScreenCTM().a);
 await page.mouse.move(point.x+50,point.y+25,{steps:5});await page.mouse.up();
 const moved=[Math.round(original[0]+50/scale*9525),Math.round(original[1]+25/scale*9525),...original.slice(2)];
 await expect.poll(()=>draft?.slides[0].text_boxes[0].bounds_emu).toEqual(moved);
 await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-bound-0')).toHaveValue(String(original[0]/360000));await expect(page.locator('#pptx-undo')).toBeDisabled();
 await page.locator('#pptx-redo').click();await expect(page.locator('#pptx-box-bound-0')).toHaveValue(String(moved[0]/360000));
 point=await start();await page.mouse.move(-100,-100,{steps:3});await page.mouse.up();await expect.poll(()=>draft?.slides[0].text_boxes[0].bounds_emu).toEqual([0,0,...original.slice(2)]);
 await page.locator('#pptx-editor-save').click();await expect(page.locator('#presentation-edit-state')).toContainText('已另存');expect(saved.slides[0].text_boxes[0].bounds_emu).toEqual([0,0,...original.slice(2)]);
});


for(const interruption of ['pointercancel','lostpointercapture','unrelated-pointer'])test(`drag interruption ${interruption} preserves draft and history`,async({page})=>{
 const bounds=[914400,914400,2743200,1828800];
 const model={version:1,size_emu:[10972800,6172200],slides:[{text_boxes:[{bounds_emu:bounds,text:'中断拖拽',font_size_hundredths_pt:2400,bold:false,italic:false,color_rgb:[0,0,0]}]}]};
 let draft,writes=0;
 await nativeBridge(page,async(command,args)=>{if(command==='load_presentation_draft')return model;if(command==='save_presentation_draft'){draft=args.model;writes++;return;}throw new Error(command);});
 await page.goto('/');await page.locator('#restore-presentation').click();await expect.poll(()=>writes).toBeGreaterThan(0);const initialWrites=writes;
 const group=()=>page.locator('#slide-preview g[data-shape-id="1"]');
 await group().scrollIntoViewIfNeeded();let rect=await group().boundingBox();
 await group().evaluate(g=>g.addEventListener('pointerdown',event=>g.dataset.pointerId=String(event.pointerId),{once:true}));
 await page.mouse.move(rect.x+rect.width/2,rect.y+rect.height/2);await page.mouse.down();await page.mouse.move(rect.x+rect.width/2+40,rect.y+rect.height/2+20,{steps:4});await expect(group()).toHaveAttribute('transform',/translate/);
 if(interruption==='lostpointercapture'){await group().evaluate(g=>g.releasePointerCapture(Number(g.dataset.pointerId)));await page.mouse.move(rect.x+rect.width/2+41,rect.y+rect.height/2+20);}
 else await group().evaluate((g,kind)=>g.dispatchEvent(new PointerEvent('pointercancel',{bubbles:true,pointerId:Number(g.dataset.pointerId)+(kind==='unrelated-pointer'?1:0)})),interruption);
 if(interruption==='unrelated-pointer'){
   await expect(group()).toHaveAttribute('transform',/translate/);await page.keyboard.press('Escape');
 }
 await expect(group()).not.toHaveAttribute('transform');await page.mouse.up();await page.waitForTimeout(800);
 expect(writes).toBe(initialWrites);expect(draft.slides[0].text_boxes[0].bounds_emu).toEqual(bounds);await expect(page.locator('#pptx-undo')).toBeDisabled();
 // A cancelled gesture must not poison the following real pointer gesture.
 rect=await group().boundingBox();await page.mouse.move(rect.x+rect.width/2,rect.y+rect.height/2);await page.mouse.down();await page.mouse.move(rect.x+rect.width/2+25,rect.y+rect.height/2+10,{steps:3});await page.mouse.up();
 await expect.poll(()=>writes).toBeGreaterThan(initialWrites);expect(draft.slides[0].text_boxes[0].bounds_emu).not.toEqual(bounds);await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-undo')).toBeDisabled();
});
