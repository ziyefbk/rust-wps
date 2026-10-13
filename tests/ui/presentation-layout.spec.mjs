import {test,expect} from '@playwright/test';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
import {mkdirSync,readFileSync} from 'node:fs';
import {nativeBridge} from './bridge.mjs';

for(const level of ['layout','master'])for(const ambiguous of [false,true])test(`${level} placeholders render and export only unique positions (${ambiguous})`,async({page},info)=>{
 const source=path.resolve(`tests/fixtures/${level}-placeholders${ambiguous?'-ambiguous':''}.pptx`);
 const presentation=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['inspect-pptx',source],{encoding:'utf8',windowsHide:true}));
 let pdfCalls=0,pdfPath,pdfPage;const before=readFileSync(source);
 await nativeBridge(page,async(command,args)=>{
  if(command==='open_document')return {name:'layout.pptx',path:source,inspection:{format:'pptx',text:'',bytes:100,warnings:[]},presentation};
  if(command==='export_current_pdf'){
   pdfCalls++;pdfPage=args.page;pdfPath=info.outputPath('layout.pdf');mkdirSync(path.dirname(pdfPath),{recursive:true});
   await expect(page.locator('#print-document svg text')).toHaveCount(3);
   await page.pdf({path:pdfPath,preferCSSPageSize:true,printBackground:true});return {path:pdfPath};
  }
  throw new Error(command);
 });
 await page.goto('/');await page.locator('#open').click();
 await expect(page.locator('#slide-canvas text')).toHaveCount(ambiguous?2:3);
 if(!ambiguous){
  const rect=page.locator('#slide-canvas g[data-shape-id="2"] > rect').first();
  await expect(rect).toHaveAttribute('x',String((level==='layout'?685800:457200)/9525));
  await expect(rect).toHaveAttribute('y',String((level==='layout'?2130425:274638)/9525));
 }
 await expect(page.locator('#slide-preview-info')).toContainText(ambiguous?'1 个占位符':'2 个占位符');
 if(level==='master')await expect(page.locator('#slide-preview-info')).toContainText(`其中 ${ambiguous?1:2} 个来自母版`);
 await expect(page.locator('#slide-text')).toContainText('Inherited title');
 await page.locator('#pptx-export-pdf').click();
 if(ambiguous){await expect(page.locator('#notice')).toContainText('超出近似 PDF 绘制范围');expect(pdfCalls).toBe(0);}
 else{
  await expect(page.locator('#notice')).toContainText('已另存近似 PDF');expect(pdfCalls).toBe(1);
  expect(pdfPage).toEqual({width_inches:10,height_inches:7.5,expected_pages:1});
  const actual=JSON.parse(execFileSync('C:/Users/28275/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe',['-c','from pypdf import PdfReader;import sys,json;r=PdfReader(sys.argv[1]);print(json.dumps({"text":[p.extract_text() for p in r.pages],"size":[float(r.pages[0].mediabox.width),float(r.pages[0].mediabox.height)]}))',pdfPath],{encoding:'utf8'}));
  expect(actual.text).toHaveLength(1);for(const text of ['Inherited title','Inherited subtitle','Direct position'])expect(actual.text[0]).toContain(text);
  expect(actual.size).toEqual([720,540]);
 }
 expect(readFileSync(source)).toEqual(before);
 await expect(page.locator('#print-document')).toBeEmpty();await expect(page.locator('#presentation-print-style')).toHaveCount(0);
});

for(const level of ['layout','master'])test(`invalid ${level} bounds and invalid direct geometry cannot bypass PDF validation`,async({page})=>{
 const source=path.resolve(`tests/fixtures/${level}-placeholders.pptx`);
 const report=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['inspect-pptx',source],{encoding:'utf8',windowsHide:true}));
 let opens=0,exports=0;
 const invalid=[[],[0,0,-1,100],[0,0,1.5,100],[0,0,100]];
 await nativeBridge(page,async command=>{
  if(command==='open_document'){
   const presentation=structuredClone(report),shape=presentation.slides[0].shapes[0];
   if(opens<invalid.length){shape[`${level}_bounds_emu`]=invalid[opens];if(level==='layout')shape.master_bounds_emu=[0,0,100,100];}else shape.bounds_emu=[0,0,-1,100];
   opens++;return {name:'invalid.pptx',path:source,inspection:{format:'pptx',text:'',bytes:100,warnings:[]},presentation};
  }
  if(command==='export_current_pdf'){exports++;return null;}
  throw new Error(command);
 });
 await page.goto('/');
 for(let i=0;i<=invalid.length;i++){
  await page.locator('#open').click();await page.locator('#pptx-export-pdf').click();
  await expect(page.locator('#notice')).toContainText('超出近似 PDF 绘制范围');
  await expect(page.locator('#print-document')).toBeEmpty();
 }
 expect(exports).toBe(0);
});
