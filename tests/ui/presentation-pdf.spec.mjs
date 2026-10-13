import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {readFileSync,mkdirSync} from 'node:fs';
import path from 'node:path';
const picture='data:image/png;base64,'+readFileSync('tests/fixtures/slide-image.png').toString('base64');
const shape=text=>({id:1,name:'文字',kind:'shape',bounds_emu:[914400,914400,7315200,1828800],text,runs:[{text,font_size_hundredths_pt:2400}],children:[]});
const report=()=>({size_emu:[10972800,6172200],unreadable_parts:[],slides:['','中文演示 First','','','Final','','hidden'].map((text,i)=>({id:i+256,name:'',hidden:i===6,text,parse_error:null,shapes:text?[shape(text),...(i===1?[{id:2,name:'图片',kind:'picture',bounds_emu:[0,0,914400,457200],text:'',children:[],image:picture}]:[])]:[]}))});
test('presentation PDF has slide-sized pages, preserves all blank pages, embeds images and omits hidden slides',async({page},info)=>{
  let layout,pdf;
  await nativeBridge(page,async(command,args)=>{
    if(command==='save_draft')return;
    if(command==='open_document')return {name:'pdf.pptx',path:'pdf.pptx',inspection:{format:'pptx',text:'',bytes:1,warnings:[]},presentation:report()};
    if(command==='export_current_pdf'){layout=args.page;pdf=info.outputPath('slides.pdf');mkdirSync(path.dirname(pdf),{recursive:true});await page.pdf({path:pdf,preferCSSPageSize:true,printBackground:true});return {path:pdf};}
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#open').click();await page.locator('#pptx-export-pdf').click();await expect(page.getByRole('status')).toContainText('已另存近似 PDF');
  expect(layout).toEqual({width_inches:12,height_inches:6.75,expected_pages:6});
  const python=path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const result=JSON.parse(execFileSync(python,['-c','import sys,json;from pypdf import PdfReader;r=PdfReader(sys.argv[1]);print(json.dumps({"text":[p.extract_text() for p in r.pages],"sizes":[[float(p.mediabox.width),float(p.mediabox.height)] for p in r.pages],"images":[len(p.images) for p in r.pages]},ensure_ascii=True))',pdf],{encoding:'utf8'}));
  expect(result.text).toHaveLength(6);expect(result.text.map(t=>t.trim())).toEqual(['','中文演示 First','','','Final','']);expect(result.images[1]).toBe(1);for(const [w,h] of result.sizes){expect(w).toBeCloseTo(864,0);expect(h).toBeCloseTo(486,0);}
  await expect(page.locator('#print-document')).toBeEmpty();await expect(page.locator('#presentation-print-style')).toHaveCount(0);await expect(page.locator('#pptx-export-pdf')).toBeEnabled();
  await page.evaluate(()=>window.print=()=>{});await page.locator('#new-document').click();await page.locator('#editor-content .tiptap').fill('Word after slides');await page.locator('#edit-print').click();await expect(page.getByRole('status')).toContainText('已准备 A4');
  const word=info.outputPath('word-after-slides.pdf');await page.pdf({path:word,preferCSSPageSize:true});const dimensions=JSON.parse(execFileSync(python,['-c','import sys,json;from pypdf import PdfReader;p=PdfReader(sys.argv[1]).pages[0];print(json.dumps([float(p.mediabox.width),float(p.mediabox.height)]))',word],{encoding:'utf8'}));expect(dimensions[0]).toBeCloseTo(595,0);expect(dimensions[1]).toBeCloseTo(842,0);
});
for(const outcome of ['success','cancel','error'])test(`authored presentation PDF ${outcome} pauses close and preserves edits and undo`,async({page})=>{
  let finish,started=false,snapshot;
  await nativeBridge(page,async(command,args)=>{
    if(command==='save_presentation_draft')return;
    if(command==='export_current_pdf'){started=true;snapshot=await page.locator('#print-document').textContent();await new Promise(resolve=>finish=resolve);if(outcome==='error')throw new Error('disk full');return outcome==='cancel'?null:{path:'slides.pdf'};}
    throw new Error(command);
  });
  await page.goto('/');await page.locator('#new-presentation').click();await page.locator('#pptx-box-text').fill('最新中文');await page.locator('#pptx-export-pdf').click();await expect.poll(()=>started).toBe(true);expect(snapshot).toContain('最新中文');await expect(page.locator('#pptx-box-text')).toBeDisabled();
  await page.evaluate(()=>window.requestNativeClose());await expect(page.getByRole('status')).toContainText('操作尚未完成');finish();await expect(page.locator('#pptx-box-text')).toBeEnabled();await expect(page.locator('#print-document')).toBeEmpty();await expect(page.locator('#pptx-box-text')).toHaveValue('最新中文');
  await expect(page.getByRole('status')).toContainText(outcome==='success'?'已另存近似 PDF':outcome==='cancel'?'已取消 PDF':'disk full');await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-text')).toHaveValue('新演示文稿');
});
test('presentation PDF rejects unreadable pages and corrupt images before calling native export',async({page})=>{
  let exports=0,opens=0;
  await nativeBridge(page,async(command)=>{if(command==='open_document'){const model=report();if(opens++===0)model.slides[1].parse_error='broken';else model.slides[1].shapes[1].image='data:image/png;base64,aGVsbG8=';return {name:'bad.pptx',path:'bad.pptx',inspection:{format:'pptx',text:'',bytes:1,warnings:[]},presentation:model};}if(command==='export_current_pdf'){exports++;return null;}throw new Error(command);});
  await page.goto('/');await page.locator('#open').click();await page.locator('#pptx-export-pdf').click();await expect(page.getByRole('status')).toContainText('读取失败');expect(exports).toBe(0);await expect(page.locator('#print-document')).toBeEmpty();
  await page.locator('#open').click();await page.locator('#pptx-export-pdf').click();await expect(page.getByRole('status')).toContainText('操作未完成');expect(exports).toBe(0);await expect(page.locator('#presentation-print-style')).toHaveCount(0);await expect(page.locator('#pptx-export-pdf')).toBeEnabled();
});
