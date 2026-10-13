import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import path from 'node:path';
test('third-party explicit vertical anchor reaches read-only preview and PDF',async({page},info)=>{
 const file=info.outputPath('external-bottom.pptx');mkdirSync(path.dirname(file),{recursive:true});
 execFileSync('C:/Users/28275/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe',['-c','from pptx import Presentation;from pptx.util import Inches,Pt;from pptx.enum.text import MSO_ANCHOR;import sys;p=Presentation();s=p.slides.add_slide(p.slide_layouts[6]);b=s.shapes.add_textbox(Inches(1),Inches(1),Inches(5),Inches(3));b.text="external gypq";b.text_frame.paragraphs[0].runs[0].font.size=Pt(24);b.text_frame.vertical_anchor=MSO_ANCHOR.BOTTOM;p.save(sys.argv[1])',file],{windowsHide:true});
 const report=JSON.parse(execFileSync(path.resolve('target/debug/rust-wps.exe'),['inspect-pptx',file],{encoding:'utf8',windowsHide:true}));expect(report.slides[0].shapes[0].vertical_alignment).toBe('b');
 let pdf=0,baseline;
 await nativeBridge(page,async command=>{
  if(command==='open_document')return {name:'external.pptx',path:file,inspection:{format:'pptx',bytes:1,text:'',warnings:[]},presentation:report};
  if(command==='export_current_pdf'){pdf++;expect(Number(await page.locator('#print-document svg text > tspan').first().getAttribute('y'))).toBe(baseline);return null;}
  throw new Error(command);
 });
 await page.goto('/');await page.locator('#open').click();baseline=Number(await page.locator('#slide-canvas text > tspan').first().getAttribute('y'));
 expect(baseline).toBeGreaterThan(300);await expect(page.locator('#slide-canvas')).not.toHaveAttribute('data-text-overflow','true');
 await page.locator('#pptx-export-pdf').click();await expect.poll(()=>pdf).toBe(1);
});

test('vertical alignment moves wrapped text, undoes, drafts and exports PPTX and PDF snapshots',async({page},info)=>{
 let draft,saved,pdf=0,bottom;
 await nativeBridge(page,async(command,args)=>{
  if(command==='save_presentation_draft'){draft=args.model;return;}
  if(command==='export_current_pdf'){pdf++;expect(Number(await page.locator('#print-document svg text > tspan').first().getAttribute('y'))).toBe(bottom);return null;}
  if(command==='save_new_presentation'){
   const json=info.outputPath('model.json'),file=info.outputPath('vertical.pptx');mkdirSync(path.dirname(file),{recursive:true});writeFileSync(json,JSON.stringify(args.model));const cli=path.resolve('target/debug/rust-wps.exe'),options={encoding:'utf8',windowsHide:true};
   execFileSync(cli,['create-pptx',json,file],options);saved=JSON.parse(execFileSync(cli,['read-pptx-model',file],options));
   const actual=execFileSync('C:/Users/28275/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe',['-c','from pptx import Presentation;import sys;p=Presentation(sys.argv[1]);print(int(p.slides[0].shapes[0].text_frame.vertical_anchor))',file],options).trim();expect(actual).toBe('4');return {path:file};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#new-presentation').click();await page.locator('#pptx-box-text').fill('中文 gypq\n第二行');
 await page.locator('#pptx-box-bound-3').fill('6');await page.locator('#pptx-box-position').click();
 const baseline=()=>page.locator('#slide-canvas text > tspan').first().getAttribute('y').then(Number);
 const top=await baseline();await page.locator('#pptx-box-vertical').selectOption('middle');const middle=await baseline();expect(middle).toBeGreaterThan(top);
 await expect.poll(()=>draft?.slides[0].text_boxes[0].vertical_alignment).toBe('middle');
 await page.locator('#pptx-undo').click();await expect(page.locator('#pptx-box-vertical')).toHaveValue('top');expect(await baseline()).toBe(top);
 await page.locator('#pptx-redo').click();expect(await baseline()).toBe(middle);
 await page.locator('#pptx-box-vertical').selectOption('bottom');bottom=await baseline();expect(bottom).toBeGreaterThan(middle);
 await expect(page.locator('#slide-canvas')).not.toHaveAttribute('data-text-overflow','true');
 await page.locator('#pptx-export-pdf').click();await expect.poll(()=>pdf).toBe(1);
 await page.locator('#pptx-editor-save').click();await expect(page.locator('#presentation-edit-state')).toContainText('已另存');expect(saved.slides[0].text_boxes[0].vertical_alignment).toBe('bottom');
 await page.locator('#pptx-box-bound-3').fill('0.1');await page.locator('#pptx-box-position').click();await page.locator('#pptx-export-pdf').click();await expect(page.locator('#notice')).toContainText('文字超出文本框高度');expect(pdf).toBe(1);
});
