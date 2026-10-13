// Real Tauri/WebView2 printing. Build with --features native-smoke before running.
import {chromium,expect} from '@playwright/test';
import {once} from 'node:events';
import {spawn,execFileSync} from 'node:child_process';
import {mkdirSync,mkdtempSync,readFileSync,writeFileSync,existsSync} from 'node:fs';
import {createServer} from 'node:net';
import path from 'node:path';
import assert from 'node:assert/strict';
mkdirSync('tmp/native-pdf',{recursive:true});const directory=mkdtempSync(path.resolve('tmp/native-pdf/presentation-'));
const source=path.join(directory,'slides.pptx'),output=path.join(directory,'slides.pdf'),json=path.join(directory,'slides.json');
const model=JSON.parse(readFileSync('docs/examples/presentation-v1.json','utf8')),box=model.slides[0].text_boxes[0];
model.slides=[{text_boxes:[]},{text_boxes:[{...box,text:'原生演示 PDF 测试'}],images:[{bounds_emu:[0,0,914400,457200],data_url:'data:image/png;base64,'+readFileSync('tests/fixtures/slide-image.png').toString('base64'),alt:'图片'}]},{text_boxes:[]},{text_boxes:[]},{text_boxes:[{...box,text:'Final slide'}]},{text_boxes:[]}];
const wrappedText='原生窄框自动换行验证'.repeat(3);
model.slides[4].text_boxes.push({...box,bounds_emu:[914400,2743200,1828800,2743200],text:wrappedText,vertical_alignment:'bottom'});
writeFileSync(json,JSON.stringify(model));execFileSync(path.resolve('target/debug/rust-wps.exe'),['create-pptx',json,source],{windowsHide:true});const original=readFileSync(source);
const server=createServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const port=server.address().port;await new Promise(resolve=>server.close(resolve));
const child=spawn(path.resolve('src-tauri/target/debug/rust-wps-desktop.exe'),[],{windowsHide:true,stdio:'ignore',env:{...process.env,RUST_WPS_SMOKE_OPEN_INPUT:source,RUST_WPS_SMOKE_PPTX_OUTPUT:path.join(directory,'dragged.pptx'),RUST_WPS_SMOKE_PDF_OUTPUT:output,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`}});
let browser;
try{
  const deadline=Date.now()+20000;while(!browser&&Date.now()<deadline){if(child.exitCode!==null)throw new Error('Native app exited');try{browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`,{timeout:1000});}catch{await new Promise(resolve=>setTimeout(resolve,300));}}
  if(!browser)throw new Error('Native CDP startup timed out');const context=browser.contexts()[0],page=context.pages()[0]??await context.waitForEvent('page',{timeout:10000});
  await page.locator('#open').click();await page.locator('#presentation-editor').waitFor({timeout:35000});await page.locator('#slide-picker').selectOption('1');await page.locator('#pptx-box-text').fill('原生演示 PDF 最新内容');
  for(const [shapeId,kind,originalBounds,keepRatio] of [[1,'text_boxes',model.slides[1].text_boxes[0].bounds_emu,false],[2,'images',model.slides[1].images[0].bounds_emu,true]]){
    const group=()=>page.locator(`#slide-preview g[data-shape-id="${shapeId}"]`);await group().focus();await page.keyboard.press('Space');
    const handle=group().locator('[data-resize-handle]');await handle.scrollIntoViewIfNeeded();const r=await handle.boundingBox(),start={x:r.x+r.width/2,y:r.y+r.height/2};
    await page.mouse.move(start.x,start.y);await page.mouse.down();if(keepRatio)await page.keyboard.down('Shift');await page.mouse.move(start.x+24,start.y+8,{steps:4});await expect(group().locator('[data-resize-preview]')).toHaveCount(1);await page.mouse.up();if(keepRatio)await page.keyboard.up('Shift');
    await expect.poll(()=>{try{const b=JSON.parse(readFileSync(path.join(directory,'presentation-draft-v1.json'),'utf8')).slides[1][kind][0].bounds_emu;return b[0]===originalBounds[0]&&b[1]===originalBounds[1]&&b[2]>originalBounds[2]&&b[3]>originalBounds[3]&&(!keepRatio||Math.abs(b[2]*originalBounds[3]-b[3]*originalBounds[2])<=(originalBounds[2]+originalBounds[3])/2);}catch{return false;}},{timeout:10000}).toBe(true);
    await page.locator('#pptx-undo').click();await expect.poll(()=>{try{return JSON.parse(readFileSync(path.join(directory,'presentation-draft-v1.json'),'utf8')).slides[1][kind][0].bounds_emu;}catch{return null;}},{timeout:10000}).toEqual(originalBounds);
  }
  const textBounds=model.slides[1].text_boxes[0].bounds_emu;
  await page.locator('#slide-preview g[data-shape-id="1"]').focus();await page.keyboard.press('ArrowRight');await page.keyboard.press('Shift+ArrowDown');await expect(page.locator('#slide-preview g[data-shape-id="1"]')).toBeFocused();
  await expect.poll(()=>{try{return JSON.parse(readFileSync(path.join(directory,'presentation-draft-v1.json'),'utf8')).slides[1].text_boxes[0].bounds_emu;}catch{return null;}},{timeout:10000}).toEqual([textBounds[0]+9525,textBounds[1]+95250,...textBounds.slice(2)]);
  await page.locator('#pptx-undo').click();await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-bound-0')).toHaveValue(String(textBounds[0]/360000));
  await page.locator('#slide-preview g[data-shape-id="2"]').focus();await page.keyboard.press('ArrowRight');await expect(page.locator('#slide-preview g[data-shape-id="2"]')).toBeFocused();
  await expect.poll(()=>{try{return JSON.parse(readFileSync(path.join(directory,'presentation-draft-v1.json'),'utf8')).slides[1].images[0].bounds_emu;}catch{return null;}},{timeout:10000}).toEqual([9525,0,914400,457200]);
  await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-image-bound-0')).toHaveValue('0');
  const pictureGroup=()=>page.locator('#slide-preview g[data-shape-id="2"]');
  await pictureGroup().scrollIntoViewIfNeeded();const pictureRect=await pictureGroup().boundingBox(),pictureStart={x:pictureRect.x+pictureRect.width/2,y:pictureRect.y+pictureRect.height/2};
  const pictureScale=await page.locator('#slide-canvas').evaluate(svg=>svg.getScreenCTM().a);
  await page.mouse.move(pictureStart.x,pictureStart.y);await page.mouse.down();await page.mouse.move(pictureStart.x+24,pictureStart.y+12,{steps:3});await page.mouse.up();
  const movedPicture=[Math.round(24/pictureScale*9525),Math.round(12/pictureScale*9525),914400,457200];
  await expect.poll(()=>{try{const actual=JSON.parse(readFileSync(path.join(directory,'presentation-draft-v1.json'),'utf8')).slides[1].images[0].bounds_emu;return Math.abs(actual[0]-movedPicture[0])<=1&&Math.abs(actual[1]-movedPicture[1])<=1&&actual[2]===movedPicture[2]&&actual[3]===movedPicture[3];}catch{return false;}},{timeout:10000}).toBe(true);
  await expect(pictureGroup()).toHaveAttribute('aria-pressed','true');await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-image-bound-0')).toHaveValue('0');
  await page.locator('#pptx-image-bound-3').fill('2.54');await page.locator('#pptx-image-position').click();
  await page.locator('#pptx-image-restore-ratio').click();await expect(page.locator('#pptx-image-bound-3')).toHaveValue('1.27');
  await expect.poll(()=>{try{return JSON.parse(readFileSync(path.join(directory,'presentation-draft-v1.json'),'utf8')).slides[1].images[0].bounds_emu;}catch{return null;}},{timeout:10000}).toEqual([0,0,914400,457200]);
  await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-image-bound-3')).toHaveValue('2.54');
  await page.locator('#pptx-redo').click();await expect(page.locator('#pptx-image-bound-3')).toHaveValue('1.27');
  await page.locator('#pptx-undo').click();await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-image-bound-3')).toHaveValue('1.27');
  await page.locator('#pptx-box-text').fill('gypq');
  await page.locator('#pptx-box-bound-3').fill('0.85');await page.locator('#pptx-box-position').click();
  await page.locator('#pptx-export-pdf').click();await expect(page.locator('#notice')).toContainText('文字超出文本框高度');
  assert.equal(existsSync(output),false);assert.equal(await page.locator('#print-document').textContent(),'');
  assert.equal(await page.locator('#pptx-box-text').inputValue(),'gypq');
  const geometry=await page.locator('#slide-canvas text').evaluate(text=>{const rect=text.parentElement.querySelector('rect'),line=text.querySelector(':scope > tspan');return {baseline:Number(line.getAttribute('y')),bottom:Number(rect.getAttribute('y'))+Number(rect.getAttribute('height'))};});assert.ok(geometry.baseline<geometry.bottom);
  const draft=path.join(directory,'presentation-draft-v1.json');
  await expect.poll(()=>{try{const value=JSON.parse(readFileSync(draft,'utf8'));return value.slides[1].text_boxes[0].text==='gypq'&&value.slides[1].text_boxes[0].bounds_emu[3]===306000;}catch{return false;}},{timeout:10000}).toBe(true);
  await page.locator('#pptx-undo').click();await page.locator('#pptx-undo').click();
  assert.equal(await page.locator('#pptx-box-text').inputValue(),'原生演示 PDF 最新内容');
  await page.locator('#slide-picker').selectOption('4');await expect.poll(()=>page.locator('#slide-canvas text').nth(1).locator(':scope > tspan').count()).toBeGreaterThan(1);
  await page.locator('#pptx-box-picker').selectOption('1');await expect(page.locator('#pptx-box-vertical')).toHaveValue('bottom');
  const firstBaseline=()=>page.locator('#slide-canvas text').nth(1).locator(':scope > tspan').first().getAttribute('y').then(Number);
  const bottomBaseline=await firstBaseline();await page.locator('#pptx-box-vertical').selectOption('middle');assert.ok(await firstBaseline()<bottomBaseline);
  await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-vertical')).toHaveValue('bottom');assert.equal(await firstBaseline(),bottomBaseline);
  const lowerEdge=await page.locator('#slide-canvas text').nth(1).evaluate(text=>{const rect=text.parentElement.querySelector('rect'),line=text.querySelector(':scope > tspan:last-child');return Number(rect.getAttribute('y'))+Number(rect.getAttribute('height'))-Number(line.getAttribute('y'));});assert.ok(lowerEdge>=-1e-6&&lowerEdge<20);
  await expect.poll(()=>{try{const value=JSON.parse(readFileSync(draft,'utf8'));return value.slides[4].text_boxes[1].vertical_alignment==='bottom'&&value.slides[1].text_boxes[0].text==='原生演示 PDF 最新内容';}catch{return false;}},{timeout:10000}).toBe(true);
  // Exercise actual WebView pointer capture, draft IPC and Rust package export.
  const originalBounds=model.slides[4].text_boxes[1].bounds_emu;
  const target=()=>page.locator('#slide-preview g[data-shape-id="2"]');
  async function beginDrag(){await target().scrollIntoViewIfNeeded();const b=await target().boundingBox();const start={x:b.x+b.width/2,y:b.y+b.height/2};await page.mouse.move(start.x,start.y);await page.mouse.down();return start;}
  let start=await beginDrag();await page.mouse.move(start.x+32,start.y-18,{steps:4});await expect(target()).toHaveAttribute('transform',/translate/);await page.keyboard.press('Escape');await page.mouse.up();await expect(target()).not.toHaveAttribute('transform');
  assert.deepEqual(JSON.parse(readFileSync(draft,'utf8')).slides[4].text_boxes[1].bounds_emu,originalBounds);
  await target().evaluate(g=>g.addEventListener('pointerdown',e=>g.dataset.pointerId=String(e.pointerId),{once:true}));
  start=await beginDrag();await page.mouse.move(start.x+20,start.y-10,{steps:3});await expect(target()).toHaveAttribute('transform',/translate/);
  await target().evaluate(g=>g.releasePointerCapture(Number(g.dataset.pointerId)));await page.mouse.move(start.x+21,start.y-10);await expect(target()).not.toHaveAttribute('transform');await page.mouse.up();
  assert.deepEqual(JSON.parse(readFileSync(draft,'utf8')).slides[4].text_boxes[1].bounds_emu,originalBounds);
  start=await beginDrag();const scale=await page.locator('#slide-canvas').evaluate(svg=>svg.getScreenCTM().a);await page.mouse.move(start.x+32,start.y-18,{steps:4});await page.mouse.up();
  const moved=[Math.round(originalBounds[0]+32/scale*9525),Math.round(originalBounds[1]-18/scale*9525),...originalBounds.slice(2)];
  await expect.poll(()=>JSON.parse(readFileSync(draft,'utf8')).slides[4].text_boxes[1].bounds_emu,{timeout:10000}).toEqual(moved);
  await page.locator('#pptx-editor-save').click();await expect(page.locator('#presentation-edit-state')).toContainText('已另存');
  const exported=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['read-pptx-model',path.join(directory,'dragged.pptx')],{encoding:'utf8',windowsHide:true}));assert.deepEqual(exported.slides[4].text_boxes[1].bounds_emu,moved);assert.deepEqual(exported.slides[1].images,model.slides[1].images);
  const independentPython=path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const independentBounds=JSON.parse(execFileSync(independentPython,['-c','import sys,json;from pptx import Presentation;p=Presentation(sys.argv[1]);s=p.slides[4].shapes[1];print(json.dumps([s.left,s.top,s.width,s.height]))',path.join(directory,'dragged.pptx')],{encoding:'utf8',windowsHide:true}));assert.deepEqual(independentBounds,moved);
  const pptxBytes=readFileSync(path.join(directory,'dragged.pptx'));

  await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-bound-0')).toHaveValue(String(originalBounds[0]/360000));
  await expect.poll(()=>JSON.parse(readFileSync(draft,'utf8')).slides[4].text_boxes[1].bounds_emu,{timeout:10000}).toEqual(originalBounds);
  await page.locator('#pptx-editor-save').click();await expect(page.locator('#notice')).toContainText('Output exists; choose a new file');assert.deepEqual(readFileSync(path.join(directory,'dragged.pptx')),pptxBytes);

  await page.locator('#slide-picker').selectOption('1');
  await page.locator('#pptx-export-pdf').click();await page.waitForFunction(()=>document.querySelector('#status')?.textContent==='就绪'&&document.querySelector('#notice')?.textContent.includes('PDF'),{},{timeout:70000});
  const notice=await page.locator('#notice').textContent();if(!notice.includes('已另存近似 PDF'))throw new Error(notice);
  const python=path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const result=JSON.parse(execFileSync(python,['-c','import sys,json;from pypdf import PdfReader;r=PdfReader(sys.argv[1]);print(json.dumps({"text":[p.extract_text() for p in r.pages],"sizes":[[float(p.mediabox.width),float(p.mediabox.height)] for p in r.pages],"images":[len(p.images) for p in r.pages]},ensure_ascii=True))',output],{encoding:'utf8',windowsHide:true}));
  const canonical=t=>t.normalize('NFKC').replace(/\s+/g,'');assert.deepEqual(result.text.map(canonical),['','原生演示PDF最新内容','','',canonical('Final slide'+wrappedText),'']);assert.ok(result.sizes.every(([w,h])=>Math.abs(w-864)<1&&Math.abs(h-486)<1));assert.equal(result.images[1],1);assert.deepEqual(readFileSync(source),original);
  const bytes=readFileSync(output);await page.locator('#pptx-export-pdf').click();await page.getByRole('status').filter({hasText:'目标文件已存在'}).waitFor();assert.deepEqual(readFileSync(output),bytes);assert.equal(await page.locator('#print-document').textContent(),'');await page.locator('#pptx-undo').click();assert.equal(await page.locator('#pptx-box-text').inputValue(),'原生演示 PDF 测试');
  console.log(JSON.stringify({nativeWebView2PresentationPdf:true,nativeDragCancelUndoDraftAndPptx:true,nativeCaptureLoss:true,nativePictureRatioRestore:true,nativePictureDrag:true,nativeObjectArrowMovement:true,nativeObjectResize:true,independentPptxBounds:true,pptxNoOverwrite:true,verticalBottomAlignment:true,verticalUndoAndDraft:true,wrappedTextPreserved:true,overflowBlocksPublication:true,descenderOverflow:true,draftPreserved:true,sourcePreserved:true,noOverwrite:true,undoPreserved:true,output,...result}));
}finally{if(browser)await browser.close().catch(()=>{});if(child.exitCode===null){const exited=once(child,'exit');child.kill();await exited;}}
