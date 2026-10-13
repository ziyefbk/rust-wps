import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import path from 'node:path';

test('canvas image selection and drag preserve image bytes, other objects and one-step undo',async({page},info)=>{
 const data='data:image/png;base64,'+readFileSync('tests/fixtures/slide-image.png').toString('base64');
 const first={bounds_emu:[914400,914400,1828800,914400],data_url:data,alt:'第一张'};
 const second={bounds_emu:[5486400,914400,1828800,914400],data_url:data,alt:'第二张'};
 const text={bounds_emu:[0,3657600,2743200,914400],text:'文字保持',font_size_hundredths_pt:2400,bold:false,italic:false,color_rgb:[0,0,0]};
 const model={version:1,size_emu:[10972800,6172200],slides:[{text_boxes:[text],images:[first,second]}]};
 let draft,saved,writes=0;
 await nativeBridge(page,async(command,args)=>{
  if(command==='load_presentation_draft')return model;
  if(command==='save_presentation_draft'){draft=args.model;writes++;return;}
  if(command==='save_new_presentation'){
   const file=info.outputPath('images.pptx'),json=info.outputPath('images.json');mkdirSync(path.dirname(file),{recursive:true});writeFileSync(json,JSON.stringify(args.model));const exe=path.resolve('target/debug/rust-wps.exe');execFileSync(exe,['create-pptx',json,file],{windowsHide:true});saved=JSON.parse(execFileSync(exe,['read-pptx-model',file],{encoding:'utf8',windowsHide:true}));return {path:file};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#restore-presentation').click();await expect.poll(()=>writes).toBeGreaterThan(0);const initialWrites=writes;
 const image=()=>page.locator('#slide-preview g[data-shape-id="3"]');
 await image().click();await expect(page.locator('#pptx-image-picker')).toHaveValue('1');await expect(image()).toHaveAttribute('aria-pressed','true');await expect(page.locator('#slide-preview g[data-shape-id="1"]')).toHaveAttribute('aria-pressed','false');
 await image().focus();await page.keyboard.press('Enter');await expect(page.locator('#pptx-image-alt')).toBeFocused();await expect(page.locator('#pptx-image-alt')).toHaveValue('第二张');
 await page.waitForTimeout(800);expect(writes).toBe(initialWrites);
 async function begin(){await image().scrollIntoViewIfNeeded();const r=await image().boundingBox(),p={x:r.x+r.width/2,y:r.y+r.height/2};await page.mouse.move(p.x,p.y);await page.mouse.down();return p;}
 let p=await begin();await page.mouse.move(p.x+30,p.y+20,{steps:3});await expect(image()).toHaveAttribute('transform',/translate/);await page.keyboard.press('Escape');await page.mouse.up();await expect(image()).not.toHaveAttribute('transform');await expect(page.locator('#pptx-undo')).toBeDisabled();
 p=await begin();const scale=await page.locator('#slide-canvas').evaluate(svg=>svg.getScreenCTM().a);await page.mouse.move(p.x+30,p.y+20,{steps:3});await page.mouse.up();
 const moved=[Math.round(second.bounds_emu[0]+30/scale*9525),Math.round(second.bounds_emu[1]+20/scale*9525),...second.bounds_emu.slice(2)];
 await expect.poll(()=>draft?.slides[0].images[1].bounds_emu).toEqual(moved);await expect(page.locator('#pptx-image-picker')).toHaveValue('1');await expect(image()).toHaveAttribute('aria-pressed','true');
 await page.locator('#pptx-editor-save').click();await expect(page.locator('#notice')).toContainText('已另存 PPTX');expect(saved.slides[0].images).toEqual([first,{...second,bounds_emu:moved}]);expect(saved.slides[0].text_boxes).toEqual([text]);
 await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-undo')).toBeDisabled();await expect.poll(()=>draft?.slides[0].images).toEqual([first,second]);
 await page.locator('#slide-preview g[data-shape-id="1"]').click();await expect(page.locator('#pptx-box-text')).toHaveValue('文字保持');await expect(page.locator('#slide-preview g[data-shape-id="1"]')).toHaveAttribute('aria-pressed','true');
});
