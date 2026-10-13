import {test,expect} from '@playwright/test';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {writeFileSync,mkdirSync} from 'node:fs';
import path from 'node:path';

test('clear character styles acts on pending partial selection and preserves links and block structure',async({page},info)=>{
 const original={type:'doc',content:[{type:'heading',attrs:{level:2,textAlign:'center'},content:[{type:'text',text:'前中后',marks:[{type:'bold'},{type:'italic'},{type:'strike'},{type:'superscript'},{type:'link',attrs:{href:'https://example.com/'}}]}]},{type:'paragraph',content:[{type:'text',text:'代码',marks:[{type:'code'}]}]}]};
 let draft,imported;
 await nativeBridge(page,async(command,args)=>{
  if(command==='load_draft')return original;
  if(command==='save_draft'){draft=args.doc;return;}
  if(command==='save_new_document'){
   const file=info.outputPath('clear.docx'),json=info.outputPath('clear.json');mkdirSync(path.dirname(file),{recursive:true});writeFileSync(json,JSON.stringify(args.doc));const exe=path.resolve('target/debug/rust-wps.exe');const report=JSON.parse(execFileSync(exe,['create-docx',json,file],{encoding:'utf8',windowsHide:true}));imported=JSON.parse(execFileSync(exe,['import-docx',file],{encoding:'utf8',windowsHide:true})).document;return {path:file,report};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#restore-draft').click();await expect(page.locator('.tiptap h2 sup')).toHaveText('前中后');
 expect(await page.evaluate(()=>{const root=document.querySelector('.tiptap h2'),walker=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);const t=walker.nextNode();window.getSelection().setBaseAndExtent(t,1,t,2);const button=document.getElementById('edit-clear-marks');button.focus();button.click();return document.activeElement===document.querySelector('.tiptap');})).toBe(true);
 await expect.poll(()=>draft?.content[0]?.content.length).toBe(3);
 const content=draft.content[0].content;expect(content.map(n=>n.text)).toEqual(['前','中','后']);expect(content[1].marks.map(m=>m.type)).toEqual(['link']);expect(content[1].marks[0].attrs.href).toBe('https://example.com/');expect(content[0].marks.some(m=>m.type==='superscript')).toBe(true);expect(content[2].marks.some(m=>m.type==='bold')).toBe(true);expect(draft.content[0].attrs).toMatchObject({level:2,textAlign:'center'});expect(draft.content[1].content[0].marks).toEqual([{type:'code'}]);
 await page.locator('#edit-undo').click();await expect(page.locator('.tiptap h2 sup')).toHaveText('前中后');await page.locator('#edit-redo').click();await expect(page.locator('.tiptap h2 sup')).toHaveCount(2);
 await page.locator('#edit-save').click();await expect(page.locator('#notice')).toContainText('已导出 DOCX');const middle=imported.content[0].content.find(n=>n.text==='中');expect(middle.marks.some(m=>m.type==='link'&&m.attrs.href==='https://example.com/')).toBe(true);expect(middle.marks.some(m=>['bold','italic','strike','superscript','subscript'].includes(m.type))).toBe(false);expect(imported.content[0].attrs).toMatchObject({level:2,textAlign:'center'});
});


test('clear at a caret affects only future input and a plain caret creates no history or draft',async({page})=>{
 let writes=0,draft;
 await nativeBridge(page,async(command,args)=>{if(command==='load_draft')return {type:'doc',content:[{type:'paragraph',content:[{type:'text',text:'保留',marks:[{type:'bold'}]}]}]};if(command==='save_draft'){writes++;draft=args.doc;return;}throw new Error(command);});
 await page.goto('/');await page.locator('#restore-draft').click();await expect(page.locator('.tiptap strong')).toHaveText('保留');
 await page.evaluate(()=>{const t=document.querySelector('.tiptap strong').firstChild;window.getSelection().setBaseAndExtent(t,2,t,2);document.getElementById('edit-clear-marks').click();});await page.keyboard.insertText('新');
 await expect(page.locator('.tiptap strong')).toHaveText('保留');await expect(page.locator('.tiptap')).toHaveText('保留新');await expect.poll(()=>draft?.content[0]?.content).toEqual([{type:'text',marks:[{type:'bold'}],text:'保留'},{type:'text',text:'新'}]);
 const before=writes;await page.locator('#edit-clear-marks').click();await page.waitForTimeout(800);expect(writes).toBe(before);
 await page.locator('#edit-undo').click();await expect(page.locator('.tiptap')).toHaveText('保留');await expect(page.locator('#edit-undo')).toBeDisabled();
});
