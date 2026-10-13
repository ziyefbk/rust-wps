import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import path from 'node:path';
const png=readFileSync('tests/fixtures/slide-image.png');
test('presentation images validate via Rust, preview, undo, draft, export and restore with matching bytes',async({page},info)=>{
  let draft,saved,sequence=0;
  const create=model=>{const name=String(sequence++),json=info.outputPath(name+'.json'),output=info.outputPath(name+'.pptx');mkdirSync(path.dirname(json),{recursive:true});writeFileSync(json,JSON.stringify(model));const opts={encoding:'utf8',windowsHide:true},exe=path.resolve('target/debug/rust-wps.exe');execFileSync(exe,['create-pptx',json,output],opts);return {path:output,model:JSON.parse(execFileSync(exe,['read-pptx-model',output],opts))};};
  await nativeBridge(page,async(command,args)=>{
    if(command==='inspect_presentation_picture'){create({version:1,size_emu:[10972800,6172200],slides:[{text_boxes:[],images:[{bounds_emu:[0,0,914400,457200],data_url:args.dataUrl,alt:''}]}]});return [32,16];}
    if(command==='validate_presentation'){create(args.model);return;}
    if(command==='save_presentation_draft'){draft=structuredClone(args.model);return;}
    if(command==='load_presentation_draft')return draft;
    if(command==='save_new_presentation'){const result=create(args.model);saved=result.model;return {path:result.path};}
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#new-presentation').click();await page.locator('#pptx-image-file').setInputFiles({name:'示意.png',mimeType:'image/png',buffer:png});
  await expect(page.locator('#slide-canvas image')).toHaveCount(1);expect(await page.locator('#slide-canvas image').getAttribute('href')).toBe('data:image/png;base64,'+png.toString('base64'));
  await page.locator('#pptx-image-alt').fill('中文 & <图片>');await page.locator('#pptx-image-alt').press('Tab');
  await page.locator('#pptx-image-bound-0').fill('2');await page.locator('#pptx-image-position').click();
  await page.locator('#pptx-image-delete').click();await expect(page.locator('#slide-canvas image')).toHaveCount(0);
  await page.locator('#pptx-undo').click();await expect(page.locator('#slide-canvas image')).toHaveCount(1);await expect(page.locator('#pptx-image-alt')).toHaveValue('中文 & <图片>');
  await page.locator('#pptx-editor-save').click();await expect(page.getByRole('status')).toContainText('已另存 PPTX');
  expect(saved.slides[0].images[0].bounds_emu[0]).toBe(720000);expect(saved.slides[0].images[0].data_url).toBe('data:image/png;base64,'+png.toString('base64'));
  await expect.poll(()=>draft).toEqual(saved);await page.reload();await page.locator('#restore-presentation').click();await expect(page.locator('#slide-canvas image')).toHaveCount(1);await expect(page.locator('#pptx-image-bound-0')).toHaveValue('2');
});
test('picture inspection pauses close; failure and invalid position retain the model',async({page})=>{
  let finish,inspections=0;
  await nativeBridge(page,async(command)=>{
    if(command==='save_presentation_draft')return;
    if(command==='validate_presentation')return;
    if(command==='inspect_presentation_picture'){inspections++;if(inspections===1){await new Promise(resolve=>finish=resolve);throw new Error('invalid PNG');}return [32,16];}
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#new-presentation').click();await page.locator('#pptx-image-file').setInputFiles({name:'bad.png',mimeType:'image/png',buffer:Buffer.from('bad')});
  await expect.poll(()=>inspections).toBe(1);await page.evaluate(()=>window.requestNativeClose());await expect(page.getByRole('status')).toContainText('操作尚未完成');finish();await expect(page.getByRole('status')).toContainText('invalid PNG');await expect(page.locator('#slide-canvas image')).toHaveCount(0);await expect(page.locator('#pptx-undo')).toBeDisabled();
  await page.locator('#pptx-image-file').setInputFiles({name:'good.png',mimeType:'image/png',buffer:png});await expect(page.locator('#slide-canvas image')).toHaveCount(1);
  await page.locator('#pptx-image-bound-2').fill('1000');await page.locator('#pptx-image-position').click();await expect(page.getByRole('status')).toContainText('必须完全位于');
  await page.locator('#pptx-undo').click();await expect(page.locator('#slide-canvas image')).toHaveCount(0);
});


for(const [width,height,expected] of [[720000,1440000,[720000,360000]],[2160000,360000,[720000,360000]]])test(`restore picture ratio fits ${width}x${height}, retains bytes and exports`,async({page},info)=>{
 const data='data:image/png;base64,'+png.toString('base64');
 const original=[720000,360000,width,height];
 const model={version:1,size_emu:[10972800,6172200],slides:[{text_boxes:[],images:[{bounds_emu:original,data_url:data,alt:'比例测试'}]}]};
 let draft,saved;
 await nativeBridge(page,async(command,args)=>{
  if(command==='load_presentation_draft')return model;
  if(command==='inspect_presentation_picture')return [32,16];
  if(command==='validate_presentation')return;
  if(command==='save_presentation_draft'){draft=args.model;return;}
  if(command==='save_new_presentation'){
   const file=info.outputPath('ratio.pptx'),json=info.outputPath('ratio.json');mkdirSync(path.dirname(file),{recursive:true});writeFileSync(json,JSON.stringify(args.model));const exe=path.resolve('target/debug/rust-wps.exe');execFileSync(exe,['create-pptx',json,file],{windowsHide:true});saved=JSON.parse(execFileSync(exe,['read-pptx-model',file],{encoding:'utf8',windowsHide:true}));return {path:file};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#restore-presentation').click();await page.locator('#pptx-image-restore-ratio').click();await expect(page.locator('#notice')).toContainText('已在当前图片框内恢复');
 await expect(page.locator('#pptx-image-bound-2')).toHaveValue(String(expected[0]/360000));await expect(page.locator('#pptx-image-bound-3')).toHaveValue(String(expected[1]/360000));
 await expect.poll(()=>draft?.slides[0].images[0].bounds_emu).toEqual([...original.slice(0,2),...expected]);
 await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-image-bound-2')).toHaveValue(String(width/360000));await expect(page.locator('#pptx-image-bound-3')).toHaveValue(String(height/360000));await expect(page.locator('#pptx-undo')).toBeDisabled();
 await page.locator('#pptx-redo').click();await page.locator('#pptx-editor-save').click();await expect(page.locator('#notice')).toContainText('已另存 PPTX');
 expect(saved.slides[0].images[0]).toEqual({bounds_emu:[...original.slice(0,2),...expected],data_url:data,alt:'比例测试'});
});

test('failed picture ratio inspection preserves draft and history',async({page})=>{
 const model={version:1,size_emu:[10972800,6172200],slides:[{text_boxes:[],images:[{bounds_emu:[0,0,720000,720000],data_url:'data:image/png;base64,'+png.toString('base64'),alt:''}]}]};
 let writes=0;
 await nativeBridge(page,async(command)=>{if(command==='load_presentation_draft')return model;if(command==='save_presentation_draft'){writes++;return;}if(command==='inspect_presentation_picture')throw new Error('图片解析失败');throw new Error(command);});
 await page.goto('/');await page.locator('#restore-presentation').click();await expect.poll(()=>writes).toBeGreaterThan(0);const before=writes;
 await page.locator('#pptx-image-restore-ratio').click();await expect(page.locator('#notice')).toContainText('图片解析失败');await page.waitForTimeout(800);expect(writes).toBe(before);await expect(page.locator('#pptx-image-bound-3')).toHaveValue('2');await expect(page.locator('#pptx-undo')).toBeDisabled();
});
