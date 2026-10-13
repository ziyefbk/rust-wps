import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {writeFileSync,mkdirSync} from 'node:fs';
import path from 'node:path';

test('strike toolbar synchronizes pending selection and preserves Chinese runs in DOCX and drafts',async({page},info)=>{
 let draft,imported,independent;
 await nativeBridge(page,async(command,args)=>{
  if(command==='save_draft'){draft=args.doc;return;}
  if(command==='load_draft')return draft;
  if(command==='save_new_document'){
   const file=info.outputPath('strike.docx'),json=info.outputPath('strike.json');mkdirSync(path.dirname(file),{recursive:true});writeFileSync(json,JSON.stringify(args.doc));const exe=path.resolve('target/debug/rust-wps.exe');const report=JSON.parse(execFileSync(exe,['create-docx',json,file],{encoding:'utf8',windowsHide:true}));imported=JSON.parse(execFileSync(exe,['import-docx',file],{encoding:'utf8',windowsHide:true})).document;
   const python=path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');independent=JSON.parse(execFileSync(python,['-c',`
import sys,json
from docx import Document
from docx.oxml.ns import qn
document=Document(sys.argv[1])
default_color=document.styles.element.find(qn('w:docDefaults')).find(qn('w:rPrDefault')).find(qn('w:rPr')).find(qn('w:color')).get(qn('w:val'))
print(json.dumps({'runs':[[r.text,r.font.strike] for r in document.paragraphs[0].runs],'defaultColor':default_color},ensure_ascii=True))
`,file],{encoding:'utf8',windowsHide:true}));return {path:file,report};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#new-document').click();await page.locator('.tiptap').click();await page.keyboard.insertText('删除保留');
 // Select and click in one task before native selectionchange is delivered.
 expect(await page.evaluate(()=>{const t=document.querySelector('.tiptap p').firstChild;window.getSelection().setBaseAndExtent(t,0,t,2);document.getElementById('edit-strike').click();return document.activeElement===document.querySelector('.tiptap');})).toBe(true);
 await expect(page.locator('.tiptap s')).toHaveText('删除');await expect(page.locator('#edit-strike')).toHaveAttribute('aria-pressed','true');await expect(page.locator('.tiptap')).toHaveText('删除保留');
 await expect.poll(()=>draft?.content[0]?.content).toEqual([{type:'text',marks:[{type:'strike'}],text:'删除'},{type:'text',text:'保留'}]);
 await page.locator('#edit-undo').click();await expect(page.locator('.tiptap s')).toHaveCount(0);await expect(page.locator('.tiptap')).toHaveText('删除保留');
 await page.locator('#edit-redo').click();await expect(page.locator('.tiptap s')).toHaveText('删除');
 await page.locator('#edit-save').click();await expect(page.locator('#notice')).toContainText('已导出 DOCX');expect(imported.content[0].content).toEqual([{type:'text',marks:[{type:'strike'},{type:'fontSize',attrs:{halfPoints:22}},{type:'textColor',attrs:{rgb:'000000'}},{type:'fontFamily',attrs:{name:'Microsoft YaHei'}}],text:'删除'},{type:'text',marks:[{type:'fontSize',attrs:{halfPoints:22}},{type:'textColor',attrs:{rgb:'000000'}},{type:'fontFamily',attrs:{name:'Microsoft YaHei'}}],text:'保留'}]);
 expect(independent.runs).toEqual([['删除',true],['保留',null]]);expect(independent.defaultColor).toBe('000000');
 await page.reload();await page.locator('#restore-draft').click();await expect(page.locator('.tiptap s')).toHaveText('删除');
 await page.evaluate(()=>{const t=document.querySelector('.tiptap s').firstChild;window.getSelection().setBaseAndExtent(t,0,t,t.textContent.length);document.getElementById('edit-strike').click();});await expect(page.locator('.tiptap s')).toHaveCount(0);await expect(page.locator('.tiptap')).toHaveText('删除保留');
});
