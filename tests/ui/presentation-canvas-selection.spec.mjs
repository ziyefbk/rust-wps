import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import path from 'node:path';
test('canvas selection is inert, keyboard accessible and edits the correct exported text box',async({page},info)=>{
 const box=text=>({bounds_emu:[914400,914400,2743200,1828800],text,font_size_hundredths_pt:2400,bold:false,italic:false,color_rgb:[0,0,0]});
 const source={version:1,size_emu:[10972800,6172200],slides:[{text_boxes:[box('第一个'),{...box('第二个'),bounds_emu:[5486400,914400,2743200,1828800]}]}]};
 let drafts=0,saved,pdf=0;
 await nativeBridge(page,async(command,args)=>{
  if(command==='load_presentation_draft')return source;
  if(command==='save_presentation_draft'){drafts++;return;}
  if(command==='export_current_pdf'){pdf++;await expect(page.locator('#print-document [role="button"]')).toHaveCount(0);await expect(page.locator('#print-document rect[stroke="#275ea8"]')).toHaveCount(0);return null;}
  if(command==='save_new_presentation'){
   const json=info.outputPath('model.json'),file=info.outputPath('selected.pptx');mkdirSync(path.dirname(file),{recursive:true});writeFileSync(json,JSON.stringify(args.model));const cli=path.resolve('target/debug/rust-wps.exe');execFileSync(cli,['create-pptx',json,file],{windowsHide:true});saved=JSON.parse(execFileSync(cli,['read-pptx-model',file],{encoding:'utf8',windowsHide:true}));return {path:file};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#restore-presentation').click();await expect.poll(()=>drafts).toBeGreaterThan(0);const before=drafts;
 const second=()=>page.locator('#slide-preview g[data-shape-id="2"]');
 await second().click();await expect(page.locator('#pptx-box-picker')).toHaveValue('1');await expect(page.locator('#pptx-box-text')).toHaveValue('第二个');await expect(second()).toHaveAttribute('aria-pressed','true');
 await page.waitForTimeout(800);expect(drafts).toBe(before);
 await page.locator('#slide-preview g[data-shape-id="1"]').focus();await page.keyboard.press('Space');await expect(page.locator('#pptx-box-picker')).toHaveValue('0');await expect(page.locator('#slide-preview g[data-shape-id="1"]')).toBeFocused();
 await page.locator('#slide-preview g[data-shape-id="1"]').focus();await page.keyboard.press('Enter');await expect(page.locator('#pptx-box-picker')).toHaveValue('0');await expect(page.locator('#pptx-box-text')).toBeFocused();
 await second().click();await second().dblclick();await expect(page.locator('#pptx-box-text')).toBeFocused();await page.locator('#pptx-box-text').fill('画布选中的新中文');
 await expect(second()).toHaveAttribute('aria-pressed','true');await page.locator('#pptx-export-pdf').click();await expect.poll(()=>pdf).toBe(1);
 await page.locator('#pptx-editor-save').click();await expect(page.locator('#presentation-edit-state')).toContainText('已另存');expect(saved.slides[0].text_boxes.map(box=>box.text)).toEqual(['第一个','画布选中的新中文']);
 await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-text')).toHaveValue('第二个');
});
