import {test,expect} from '@playwright/test';
import {loadTsModule} from './load-ts.mjs';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import path from 'node:path';
const {PresentationHistory}=await loadTsModule('ui/presentation-history.ts');
const snap=(json,slide=0,box=0)=>({json,slide,box});
test('presentation history bounds retained UTF8 snapshots, keeps selection and clears redo only on a change',()=>{
  const h=new PresentationHistory(2,100);
  h.commit(snap('a',1,2),snap('b'));h.commit(snap('b'),snap('c'));h.commit(snap('c'),snap('d'));
  expect(h.undo(snap('d'))).toEqual(snap('c'));expect(h.undo(snap('c'))).toEqual(snap('b'));expect(h.canUndo).toBe(false);
  expect(h.commit(snap('b'),snap('b'))).toBe(false);expect(h.canRedo).toBe(true);
  expect(h.redo(snap('b'))).toEqual(snap('c'));h.commit(snap('c'),snap('e'));expect(h.canRedo).toBe(false);
  const small=new PresentationHistory(100,5);small.commit(snap('中文'),snap('x'));expect(small.canUndo).toBe(false);
  const selection=new PresentationHistory();selection.commit(snap('before',3,4),snap('after'));expect(selection.undo(snap('after'))).toEqual(snap('before',3,4));
});
test('presentation undo restores deleted pages, styles and positions; redo exports and drafts the restored model',async({page},info)=>{
  let saved,draft;
  await nativeBridge(page,async(command,args)=>{
    if(command==='save_presentation_draft'){draft=structuredClone(args.model);return;}
    if(command==='save_new_presentation'){
      const json=info.outputPath('history.json'),output=info.outputPath('history.pptx');mkdirSync(path.dirname(json),{recursive:true});writeFileSync(json,JSON.stringify(args.model));
      const exe=path.resolve('target/debug/rust-wps.exe'),opts={encoding:'utf8',windowsHide:true};execFileSync(exe,['create-pptx',json,output],opts);saved=JSON.parse(execFileSync(exe,['read-pptx-model',output],opts));return {path:output};
    }
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#new-presentation').click();await expect(page.locator('#pptx-undo')).toBeDisabled();
  await page.locator('#pptx-box-text').fill('回退中文');await page.locator('#pptx-box-italic').check();
  await page.locator('#pptx-box-bound-0').fill('2');await page.locator('#pptx-box-position').click();
  await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-bound-0')).toHaveValue('2.54');
  await page.locator('#pptx-redo').click();await expect(page.locator('#pptx-box-bound-0')).toHaveValue('2');
  await page.locator('#pptx-add-slide').click();await page.locator('#pptx-add-box').click();await page.locator('#pptx-box-text').fill('第二页');await page.locator('#pptx-delete-slide').click();
  await page.locator('#pptx-undo').click();await expect(page.locator('#slide-picker option')).toHaveCount(2);await expect(page.locator('#pptx-box-text')).toHaveValue('第二页');
  await page.locator('#pptx-box-text').press('Control+z');await expect(page.locator('#pptx-box-text')).toHaveValue('新文本框');
  await page.locator('#pptx-box-text').press('Control+Shift+z');await expect(page.locator('#pptx-box-text')).toHaveValue('第二页');
  await page.locator('#pptx-editor-save').click();await expect(page.getByRole('status')).toContainText('已另存 PPTX');
  expect(saved.slides).toHaveLength(2);expect(saved.slides[0].text_boxes[0].italic).toBe(true);expect(saved.slides[0].text_boxes[0].bounds_emu[0]).toBe(720000);
  await page.locator('#pptx-undo').click();await expect(page.locator('#presentation-edit-state')).toContainText('未另存');
  await page.locator('#pptx-redo').click();await expect.poll(()=>draft).toEqual(saved);
  page.once('dialog',()=>{throw new Error('Redo to saved content must not prompt for unsaved changes');});await page.locator('#new-document').click();await expect(page.locator('#presentation')).toBeHidden();await expect(page.locator('[aria-label="文档编辑区"]')).toBeVisible();
});

test('keyboard undo keeps typing focus and a new edit discards redo',async({page})=>{
  await nativeBridge(page,async(command)=>{if(command==='save_presentation_draft')return;throw new Error(command);});
  await page.goto('/');await page.locator('#new-presentation').click();await page.locator('#pptx-box-text').fill('first');await page.locator('#pptx-box-text').fill('second');
  await page.locator('#pptx-box-text').press('Control+z');await expect(page.locator('#pptx-box-text')).toHaveValue('first');await expect(page.locator('#pptx-box-text')).toBeFocused();
  await page.keyboard.insertText('!');await expect(page.locator('#pptx-box-text')).toHaveValue('first!');await expect(page.locator('#pptx-redo')).toBeDisabled();
  await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-text')).toHaveValue('first');
});
