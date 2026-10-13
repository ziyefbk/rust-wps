import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import path from 'node:path';
const base=()=>{const m=JSON.parse(readFileSync('docs/examples/presentation-v1.json','utf8'));const box=m.slides[0].text_boxes[0];m.slides=[{text_boxes:[{...box,text:'原页 A'}],images:[{bounds_emu:[0,0,914400,457200],data_url:'data:image/png;base64,'+readFileSync('tests/fixtures/slide-image.png').toString('base64'),alt:'原图'}]},{text_boxes:[{...box,text:'原页 B'}]},{text_boxes:[]}];return m;};
test('slides duplicate independently, move with undo and retain order and images in real PPTX and draft restore',async({page},info)=>{
  let draft=base(),saved,output,sequence=0;
  const create=model=>{const json=info.outputPath(`${sequence}.json`),target=info.outputPath(`${sequence++}.pptx`);mkdirSync(path.dirname(json),{recursive:true});writeFileSync(json,JSON.stringify(model));const exe=path.resolve('target/debug/rust-wps.exe'),opts={encoding:'utf8',windowsHide:true};execFileSync(exe,['create-pptx',json,target],opts);return {model:JSON.parse(execFileSync(exe,['read-pptx-model',target],opts)),path:target};};
  await nativeBridge(page,async(command,args)=>{
    if(command==='load_presentation_draft')return draft;
    if(command==='save_presentation_draft'){draft=structuredClone(args.model);return;}
    if(command==='validate_presentation'){create(args.model);return;}
    if(command==='save_new_presentation'){const result=create(args.model);saved=result.model;output=result.path;return {path:output};}
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#restore-presentation').click();await expect(page.locator('#pptx-slide-up')).toBeDisabled();
  await page.locator('#pptx-copy-slide').click();await expect(page.locator('#slide-picker option')).toHaveCount(4);await expect(page.locator('#slide-picker')).toHaveValue('1');
  await page.locator('#pptx-box-text').fill('独立副本');await page.locator('#pptx-image-bound-0').fill('2');await page.locator('#pptx-image-position').click();
  await page.locator('#pptx-slide-down').click();await expect(page.locator('#slide-picker')).toHaveValue('2');await expect(page.locator('#pptx-box-text')).toHaveValue('独立副本');
  await page.locator('#pptx-slide-up').click();await page.locator('#pptx-undo').click();await expect(page.locator('#slide-picker')).toHaveValue('2');await page.locator('#pptx-redo').click();await expect(page.locator('#slide-picker')).toHaveValue('1');
  await page.locator('#pptx-slide-up').click();await expect(page.locator('#pptx-slide-up')).toBeDisabled();await page.locator('#slide-picker').selectOption('3');await expect(page.locator('#pptx-slide-down')).toBeDisabled();
  await page.locator('#pptx-editor-save').click();await expect(page.getByRole('status')).toContainText('已另存 PPTX');
  expect(saved.slides.map(s=>s.text_boxes[0]?.text??'')).toEqual(['独立副本','原页 A','原页 B','']);expect(saved.slides[0].images[0].bounds_emu[0]).toBe(720000);expect(saved.slides[1].images[0].bounds_emu[0]).toBe(0);await expect.poll(()=>draft).toEqual(saved);
  const python=path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');const independent=JSON.parse(execFileSync(python,['-c','import sys,json,base64;from pptx import Presentation;p=Presentation(sys.argv[1]);print(json.dumps([[s.shapes[0].text if len(s.shapes) else "",[[int(x.left),base64.b64encode(x.image.blob).decode()] for x in s.shapes if x.shape_type==13]] for s in p.slides],ensure_ascii=True))',output],{encoding:'utf8'}));expect(independent.map(s=>s[0])).toEqual(['独立副本','原页 A','原页 B','']);expect(independent[0][1][0][0]).toBe(720000);expect(independent[1][1][0][0]).toBe(0);expect(independent[0][1][0][1]).toBe(independent[1][1][0][1]);expect(independent[0][1][0][1]).toBe(readFileSync('tests/fixtures/slide-image.png').toString('base64'));
  await page.locator('#pptx-play').click();await page.keyboard.press('Home');await expect(page.locator('#pptx-show-canvas')).toContainText('独立副本');expect(Number(await page.locator('#pptx-show-canvas image').getAttribute('x'))).toBeGreaterThan(0);await page.keyboard.press('ArrowRight');await expect(page.locator('#pptx-show-canvas')).toContainText('原页 A');await expect(page.locator('#pptx-show-canvas image')).toHaveAttribute('x','0');await page.keyboard.press('Escape');
  await page.reload();await page.locator('#restore-presentation').click();await expect(page.locator('#pptx-box-text')).toHaveValue('独立副本');await page.locator('#slide-picker').selectOption('1');await expect(page.locator('#pptx-box-text')).toHaveValue('原页 A');await expect(page.locator('#pptx-image-bound-0')).toHaveValue('0');
});
test('failed slide duplication preserves model and history and pauses native close',async({page})=>{
  let finish,started=false;
  await nativeBridge(page,async(command)=>{if(command==='save_presentation_draft')return;if(command==='validate_presentation'){started=true;await new Promise(resolve=>finish=resolve);throw new Error('Presentation pictures exceed 2 MiB');}throw new Error(command);});
  await page.goto('/');await page.locator('#new-presentation').click();await page.locator('#pptx-box-text').fill('保留内容');await page.locator('#pptx-copy-slide').click();await expect.poll(()=>started).toBe(true);await expect(page.locator('#pptx-box-text')).toBeDisabled();await page.evaluate(()=>window.requestNativeClose());await expect(page.getByRole('status')).toContainText('操作尚未完成');finish();await expect(page.getByRole('status')).toContainText('2 MiB');await expect(page.locator('#slide-picker option')).toHaveCount(1);await expect(page.locator('#pptx-box-text')).toHaveValue('保留内容');await expect(page.locator('#pptx-box-text')).toBeEnabled();await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-text')).toHaveValue('新演示文稿');
});
