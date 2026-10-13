import {test,expect} from '@playwright/test';
import {loadTsModule} from './load-ts.mjs';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {mkdirSync} from 'node:fs';
import path from 'node:path';
const {wrapTextRuns}=await loadTsModule('ui/presentation-wrap.ts');
const text=result=>result.lines.map(line=>line.map(run=>run.text).join(''));
const measure=value=>[...new Intl.Segmenter(undefined,{granularity:'grapheme'}).segment(value)].length;

test('word and CJK wrapping preserves content and style without whitespace-only extra lines',()=>{
 const source=[{text:'hello world',bold:true},{text:'中文换行',italic:true}];
 const before=structuredClone(source),wrapped=wrapTextRuns(source,5,measure);
 expect(text(wrapped)).toEqual(['hello ','world','中文换行']);expect(wrapped.truncated).toBe(false);expect(source).toEqual(before);
 expect(wrapped.lines[2].every(run=>run.italic)).toBe(true);
 expect(text(wrapTextRuns([{text:'abcdefghi'}],3,measure))).toEqual(['abc','def','ghi']);
});
test('hard blank lines, combining marks and joined emoji remain intact',()=>{
 expect(text(wrapTextRuns([{text:'a\n\nb\n'}],5,measure))).toEqual(['a','','b','']);
 expect(text(wrapTextRuns([{text:'e\u0301👩‍💻e\u0301'}],1,measure))).toEqual(['e\u0301','👩‍💻','e\u0301']);
});
test('shared work and line budgets report truncation without broken graphemes',()=>{
 const budget={units:4,runs:2};const first=wrapTextRuns([{text:'😀😀X'}],5,measure,budget);expect(text(first)).toEqual(['😀😀']);expect(first.truncated).toBe(true);
 expect(wrapTextRuns([{text:'next'}],5,measure,budget).truncated).toBe(true);
 expect(wrapTextRuns([{text:'a\nb\nc'}],5,measure,undefined,2).truncated).toBe(true);
 expect(wrapTextRuns([{text:'x'}],0,measure).truncated).toBe(true);
 expect(wrapTextRuns([{text:'x'}],10,()=>NaN).truncated).toBe(true);
});
test('editing wraps long Chinese text and blocks clipped PDF without changing content',async({page})=>{
 let draft,exports=0;
 await nativeBridge(page,async(command,args)=>{if(command==='save_presentation_draft'){draft=structuredClone(args.model);return;}if(command==='export_current_pdf'){exports++;return null;}throw new Error(command);});
 await page.goto('/');await page.locator('#new-presentation').click();const content='中文自动换行验证'.repeat(10);
 await page.locator('#pptx-box-text').fill(content);
 const bounds=['1','1','8','10'];for(let i=0;i<4;i++)await page.locator(`#pptx-box-bound-${i}`).fill(bounds[i]);await page.locator('#pptx-box-position').click();
 await expect.poll(()=>page.locator('#slide-canvas text > tspan').count()).toBeGreaterThan(1);
 await expect(page.locator('#slide-canvas')).not.toHaveAttribute('data-text-overflow','true');
 await page.locator('#pptx-export-pdf').click();await expect.poll(()=>exports).toBe(1);
 await page.locator('#pptx-box-bound-3').fill('0.1');await page.locator('#pptx-box-position').click();await expect(page.locator('#slide-canvas')).toHaveAttribute('data-text-overflow','true');
 await page.locator('#pptx-export-pdf').click();await expect(page.locator('#notice')).toContainText('文字超出文本框高度');expect(exports).toBe(1);
 await expect(page.locator('#pptx-box-text')).toHaveValue(content);await expect.poll(()=>draft?.slides[0].text_boxes[0].text).toBe(content);await expect(page.locator('#print-document')).toBeEmpty();
});

test('descenders beyond the frame block PDF even when the baseline fits',async({page})=>{
 let exports=0,draft;
 await nativeBridge(page,async(command,args)=>{if(command==='save_presentation_draft'){draft=structuredClone(args.model);return;}if(command==='export_current_pdf'){exports++;return null;}throw new Error(command);});
 await page.goto('/');await page.locator('#new-presentation').click();await page.locator('#pptx-box-text').fill('gypq');
 await page.locator('#pptx-box-bound-3').fill('0.85');await page.locator('#pptx-box-position').click();
 const geometry=await page.locator('#slide-canvas text').evaluate(text=>{const frame=text.parentElement.querySelector('rect'),line=text.querySelector(':scope > tspan');return {baseline:Number(line.getAttribute('y')),bottom:Number(frame.getAttribute('y'))+Number(frame.getAttribute('height'))};});
 expect(geometry.baseline).toBeLessThan(geometry.bottom);
 await expect(page.locator('#slide-canvas')).toHaveAttribute('data-text-overflow','true');
 await page.locator('#pptx-export-pdf').click();await expect(page.locator('#notice')).toContainText('文字超出文本框高度');expect(exports).toBe(0);
 await expect.poll(()=>draft?.slides[0].text_boxes[0].text).toBe('gypq');await expect(page.locator('#print-document')).toBeEmpty();
 await page.locator('#pptx-box-bound-3').fill('1.5');await page.locator('#pptx-box-position').click();
 await expect(page.locator('#slide-canvas')).not.toHaveAttribute('data-text-overflow','true');
 await page.locator('#pptx-export-pdf').click();await expect.poll(()=>exports).toBe(1);
});

test('actual wrapped PDF retains all non-whitespace text and slide dimensions',async({page},info)=>{
 const content='自动换行 中文 text '.repeat(4);let output;
 const presentation={size_emu:[9144000,6858000],unreadable_parts:[],slides:[{id:256,name:'',hidden:false,text:content,parse_error:null,shapes:[{id:1,name:'wrapped',kind:'shape',bounds_emu:[914400,914400,1828800,3657600],text:content,runs:[{text:content,font_size_hundredths_pt:1800}],children:[]}]}]};
 await nativeBridge(page,async command=>{
  if(command==='open_document')return {name:'wrap.pptx',path:'wrap.pptx',inspection:{format:'pptx',text:'',bytes:1,warnings:[]},presentation};
  if(command==='export_current_pdf'){
   await expect.poll(()=>page.locator('#print-document svg text > tspan').count()).toBeGreaterThan(1);
   output=info.outputPath('wrapped.pdf');mkdirSync(path.dirname(output),{recursive:true});await page.pdf({path:output,preferCSSPageSize:true,printBackground:true});return {path:output};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#open').click();await page.locator('#pptx-export-pdf').click();await expect(page.locator('#notice')).toContainText('已另存近似 PDF');
 const actual=JSON.parse(execFileSync('C:/Users/28275/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe',['-c','from pypdf import PdfReader;import json,sys;r=PdfReader(sys.argv[1]);print(json.dumps({"text":[p.extract_text() for p in r.pages],"size":[float(r.pages[0].mediabox.width),float(r.pages[0].mediabox.height)]}))',output],{encoding:'utf8'}));
 expect(actual.text).toHaveLength(1);expect(actual.text[0].replace(/\s/gu,'')).toBe(content.replace(/\s/gu,''));expect(actual.size).toEqual([720,540]);
 await expect(page.locator('#slide-text')).toHaveText(content);await expect(page.locator('#print-document')).toBeEmpty();
});
