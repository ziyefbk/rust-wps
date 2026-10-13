import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {writeFileSync,mkdirSync} from 'node:fs';
import path from 'node:path';

test('superscript and subscript exclude each other and survive DOCX, independent reader and drafts',async({page},info)=>{
 let draft,imported,independent;
 await nativeBridge(page,async(command,args)=>{
  if(command==='save_draft'){draft=args.doc;return;}
  if(command==='load_draft')return draft;
  if(command==='save_new_document'){
   const file=info.outputPath('scripts.docx'),json=info.outputPath('scripts.json');mkdirSync(path.dirname(file),{recursive:true});writeFileSync(json,JSON.stringify(args.doc));const exe=path.resolve('target/debug/rust-wps.exe');const report=JSON.parse(execFileSync(exe,['create-docx',json,file],{encoding:'utf8',windowsHide:true}));imported=JSON.parse(execFileSync(exe,['import-docx',file],{encoding:'utf8',windowsHide:true})).document;
   const python=path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');independent=JSON.parse(execFileSync(python,['-c',`
import sys,json
from docx import Document
from docx.oxml.ns import qn
document=Document(sys.argv[1])
default_color=document.styles.element.find(qn('w:docDefaults')).find(qn('w:rPrDefault')).find(qn('w:rPr')).find(qn('w:color')).get(qn('w:val'))
print(json.dumps({'runs':[[r.text,r.font.superscript,r.font.subscript] for r in document.paragraphs[0].runs],'defaultColor':default_color}))
`,file],{encoding:'utf8',windowsHide:true}));return {path:file,report};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#new-document').click();await page.locator('.tiptap').click();await page.keyboard.insertText('H2O x2');
 await page.evaluate(()=>{const t=document.querySelector('.tiptap p').firstChild;window.getSelection().setBaseAndExtent(t,1,t,2);document.getElementById('edit-subscript').click();});await expect(page.locator('.tiptap sub')).toHaveText('2');
 await page.evaluate(()=>{const t=document.querySelector('.tiptap p').lastChild;window.getSelection().setBaseAndExtent(t,t.textContent.length-1,t,t.textContent.length);document.getElementById('edit-superscript').click();});await expect(page.locator('.tiptap sup')).toHaveText('2');await expect(page.locator('#edit-superscript')).toHaveAttribute('aria-pressed','true');
 await page.locator('#edit-subscript').click();await expect(page.locator('.tiptap sup')).toHaveCount(0);await expect(page.locator('.tiptap sub')).toHaveCount(2);await expect(page.locator('#edit-superscript')).toHaveAttribute('aria-pressed','false');
 expect(await page.evaluate(()=>{const b=document.getElementById('edit-undo');b.focus();b.click();return document.activeElement===document.querySelector('.tiptap');})).toBe(true);await expect(page.locator('.tiptap sup')).toHaveText('2');await expect(page.locator('.tiptap sub')).toHaveCount(1);
 await expect.poll(()=>draft?.content[0]?.content).toEqual([{type:'text',text:'H'},{type:'text',marks:[{type:'subscript'}],text:'2'},{type:'text',text:'O x'},{type:'text',marks:[{type:'superscript'}],text:'2'}]);
 await page.locator('#edit-save').click();await expect(page.locator('#notice')).toContainText('已导出 DOCX');expect(imported.content[0].content).toEqual(draft.content[0].content.map(node=>({...node,marks:[...(node.marks??[]),{type:'fontSize',attrs:{halfPoints:22}},{type:'textColor',attrs:{rgb:'000000'}},{type:'fontFamily',attrs:{name:'Arial'}}]})));expect(independent.runs).toEqual([['H',null,null],['2',false,true],['O x',null,null],['2',true,false]]);expect(independent.defaultColor).toBe('000000');
 await page.reload();await page.locator('#restore-draft').click();await expect(page.locator('.tiptap sub')).toHaveText('2');await expect(page.locator('.tiptap sup')).toHaveText('2');await expect(page.locator('.tiptap')).toHaveText('H2O x2');
});


test('immediate typing after toolbar undo keeps the intended paragraph and caret',async({page})=>{
 await nativeBridge(page,async(command)=>{if(command==='save_draft')return;throw new Error(command);});
 await page.goto('/');await page.locator('#new-document').click();await page.locator('.tiptap').click();await page.keyboard.insertText('甲乙');
 await page.evaluate(()=>{const t=document.querySelector('.tiptap p').firstChild;window.getSelection().setBaseAndExtent(t,0,t,1);document.getElementById('edit-superscript').click();});await expect(page.locator('.tiptap sup')).toHaveText('甲');
 expect(await page.evaluate(()=>{const b=document.getElementById('edit-undo');b.focus();b.click();return document.activeElement===document.querySelector('.tiptap');})).toBe(true);
 await page.evaluate(()=>{const root=document.querySelector('.tiptap'),t=root.querySelector('p').lastChild;window.getSelection().setBaseAndExtent(t,t.textContent.length,t,t.textContent.length);root.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',code:'Enter',bubbles:true}));});await page.keyboard.insertText('紧接输入');await expect(page.locator('.tiptap p')).toHaveText(['甲乙','紧接输入']);
 await page.waitForTimeout(100);await page.keyboard.insertText('后续');await expect(page.locator('.tiptap p')).toHaveText(['甲乙','紧接输入后续']);
});


for(const key of ['Backspace','Delete'])test(`pending DOM selection is consumed before ${key} joins paragraphs`,async({page})=>{
 await nativeBridge(page,async(command)=>{if(command==='save_draft')return;throw new Error(command);});
 await page.goto('/');await page.locator('#new-document').click();await page.locator('.tiptap').click();await page.keyboard.insertText('甲乙');await page.keyboard.press('Enter');await page.keyboard.insertText('丙丁');
 await page.evaluate(key=>{const root=document.querySelector('.tiptap'),paragraphs=root.querySelectorAll('p'),t=paragraphs[key==='Backspace'?1:0].firstChild,offset=key==='Backspace'?0:t.textContent.length;window.getSelection().setBaseAndExtent(t,offset,t,offset);root.dispatchEvent(new KeyboardEvent('keydown',{key,code:key,bubbles:true,keyCode:key==='Backspace'?8:46}));},key);
 await expect(page.locator('.tiptap p')).toHaveText(['甲乙丙丁']);
});
