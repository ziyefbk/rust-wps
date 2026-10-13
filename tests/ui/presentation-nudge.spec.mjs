import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import path from 'node:path';

for(const kind of ['text','image'])test(`canvas ${kind} arrow movement clamps, retains focus, exports and undoes per key`,async({page},info)=>{
 const bounds=[0,0,1828800,914400];
 const text={bounds_emu:bounds,text:'键盘文字',font_size_hundredths_pt:2400,bold:false,italic:false,color_rgb:[0,0,0]};
 const image={bounds_emu:bounds,data_url:'data:image/png;base64,'+readFileSync('tests/fixtures/slide-image.png').toString('base64'),alt:'键盘图片'};
 const model={version:1,size_emu:[10972800,6172200],slides:[{text_boxes:kind==='text'?[text]:[],...(kind==='image'?{images:[image]}:{})}]};
 let draft,saved,writes=0;
 const object=value=>kind==='text'?value?.slides[0].text_boxes[0]:value?.slides[0].images[0];
 await nativeBridge(page,async(command,args)=>{
  if(command==='load_presentation_draft')return model;
  if(command==='save_presentation_draft'){draft=args.model;writes++;return;}
  if(command==='save_new_presentation'){
   const file=info.outputPath('nudge.pptx'),json=info.outputPath('nudge.json');mkdirSync(path.dirname(file),{recursive:true});writeFileSync(json,JSON.stringify(args.model));const exe=path.resolve('target/debug/rust-wps.exe');execFileSync(exe,['create-pptx',json,file],{windowsHide:true});saved=JSON.parse(execFileSync(exe,['read-pptx-model',file],{encoding:'utf8',windowsHide:true}));return {path:file};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#restore-presentation').click();await expect.poll(()=>writes).toBeGreaterThan(0);const initialWrites=writes;
 const group=()=>page.locator('#slide-preview g[data-shape-id="1"]');await group().focus();
 await page.keyboard.press('ArrowLeft');await page.keyboard.press('Shift+ArrowUp');await group().dispatchEvent('keydown',{key:'ArrowRight',isComposing:true});
 await page.waitForTimeout(800);expect(writes).toBe(initialWrites);await expect(page.locator('#pptx-undo')).toBeDisabled();await expect(group()).toBeFocused();
 await page.keyboard.press('ArrowRight');await expect(group()).toBeFocused();await page.keyboard.press('Shift+ArrowDown');await expect(group()).toBeFocused();await expect(group()).toHaveAttribute('aria-pressed','true');
 await expect.poll(()=>object(draft)?.bounds_emu).toEqual([9525,95250,...bounds.slice(2)]);
 await page.locator('#pptx-editor-save').click();await expect(page.locator('#notice')).toContainText('已另存 PPTX');expect(object(saved)).toEqual({...object(model),bounds_emu:[9525,95250,...bounds.slice(2)]});
 await page.locator('#pptx-undo').click();await expect.poll(()=>object(draft)?.bounds_emu).toEqual([9525,0,...bounds.slice(2)]);
 await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-undo')).toBeDisabled();await expect.poll(()=>object(draft)?.bounds_emu).toEqual(bounds);
 await group().focus();await page.keyboard.press('Enter');const input=page.locator(kind==='text'?'#pptx-box-text':'#pptx-image-alt');await expect(input).toBeFocused();
 const before=writes;await input.evaluate(el=>el.setSelectionRange(0,0));await page.keyboard.press('ArrowRight');expect(await input.evaluate(el=>el.selectionStart)).toBe(1);await page.waitForTimeout(800);expect(writes).toBe(before);await expect(page.locator('#pptx-undo')).toBeDisabled();
});
