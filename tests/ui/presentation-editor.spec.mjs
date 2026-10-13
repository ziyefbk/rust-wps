import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import {writeFileSync,mkdirSync} from 'node:fs';
import path from 'node:path';
import {nativeBridge} from './bridge.mjs';
test('new presentation edits text and style, manages slides, exports through Rust and reopens editable',async({page},info)=>{
  const exe=path.resolve('target/debug/rust-wps.exe'), options={encoding:'utf8',windowsHide:true};let savedModel,output;
  await nativeBridge(page,async(command,args)=>{
    if(command==='save_presentation_draft')return;
    if(command==='save_new_presentation'){
      output=info.outputPath('created.pptx');mkdirSync(path.dirname(output),{recursive:true});
      const json=info.outputPath('model.json');writeFileSync(json,JSON.stringify(args.model));
      execFileSync(exe,['create-pptx',json,output],options);
      savedModel=JSON.parse(execFileSync(exe,['read-pptx-model',output],options));return {path:output};
    }
    if(command==='open_document')return {name:'created.pptx',path:output,inspection:{format:'pptx',text:'',bytes:1,warnings:[]},presentation:JSON.parse(execFileSync(exe,['inspect-pptx',output],options)),presentation_model:savedModel};
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#new-presentation').click();
  await page.locator('#pptx-box-text').fill('中文演示\n第二行');await page.locator('#pptx-box-size').fill('30');await page.locator('#pptx-box-size').press('Tab');
  await page.locator('#pptx-box-italic').check();await expect(page.locator('#slide-canvas text').first()).toContainText('中文演示');
  await page.locator('#pptx-box-bound-0').fill('2');await page.locator('#pptx-box-position').click();
  await page.locator('#presentation').screenshot({path:'test-results/pptx-editor.png'});
  await page.locator('#pptx-add-slide').click();await expect(page.locator('#slide-picker option')).toHaveCount(2);
  await page.locator('#pptx-add-box').click();await page.locator('#pptx-box-text').fill('第二页');
  await page.locator('#pptx-editor-save').click();await expect(page.getByRole('status')).toContainText('已另存 PPTX');
  expect(savedModel.slides[0].text_boxes[0].font_size_hundredths_pt).toBe(3000);expect(savedModel.slides[0].text_boxes[0].italic).toBe(true);
  expect(savedModel.slides[0].text_boxes[0].bounds_emu[0]).toBe(720000);
  await page.locator('#open').click();await expect(page.locator('#presentation-editor')).toBeVisible();
  await expect(page.locator('#pptx-box-text')).toHaveValue('中文演示\n第二行');
  await page.locator('#slide-picker').selectOption('1');await expect(page.locator('#pptx-box-text')).toHaveValue('第二页');
  await page.locator('#pptx-delete-slide').click();await expect(page.locator('#slide-picker option')).toHaveCount(1);await expect(page.locator('#pptx-delete-slide')).toBeDisabled();
});
test('unsaved presentation refuses leaving on cancel and save failure retains content',async({page})=>{
  await nativeBridge(page,async(command)=>{if(command==='save_presentation_draft')return;if(command==='save_new_presentation')throw new Error('disk failure');throw new Error(command);});
  await page.goto('/');await page.locator('#new-presentation').click();await page.locator('#pptx-box-text').fill('保留修改');
  page.on('dialog',dialog=>dialog.dismiss());await page.locator('#new-document').click();await expect(page.locator('#pptx-box-text')).toHaveValue('保留修改');
  await page.locator('#pptx-editor-save').click();await expect(page.getByRole('status')).toContainText('disk failure');await expect(page.locator('#pptx-box-text')).toHaveValue('保留修改');
  await expect(page.locator('#presentation-edit-state')).toContainText('未另存');
});
