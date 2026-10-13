import {test,expect} from '@playwright/test';
import {Schema} from '@tiptap/pm/model';
import {EditorState} from '@tiptap/pm/state';
import {loadTsModule} from './load-ts.mjs';
import {nativeBridge} from './bridge.mjs';
import {execFileSync} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import path from 'node:path';
const {searchWord}=await loadTsModule('ui/word-search.ts');
const {createWordReplacement}=await loadTsModule('ui/word-replace.ts');
const schema=new Schema({nodes:{doc:{content:'paragraph+'},paragraph:{content:'text*'},text:{}},marks:{bold:{},italic:{}}});
const paragraph=text=>schema.node('paragraph',null,text?schema.text(text):null);

test('replacement checks full draft size and permits shrinking excessive node counts',()=>{
 const oversized=EditorState.create({doc:schema.node('doc',null,Array.from({length:5000},()=>paragraph('a')))});
 expect(()=>createWordReplacement(oversized,[{from:1,to:2}],'a','b')).toThrow('文档结构');
 const shrunk=createWordReplacement(oversized,[{from:1,to:2}],'a','');expect(shrunk.doc.childCount).toBe(5000);expect(shrunk.doc.child(0).textContent).toBe('');expect(oversized.doc.child(0).textContent).toBe('a');
 const images=new Schema({nodes:{doc:{content:'block+'},paragraph:{group:'block',content:'text*'},text:{},image:{group:'block',attrs:{src:{}}}}});
 const state=EditorState.create({doc:images.node('doc',null,[images.node('paragraph',null,images.text('a')),images.node('image',{src:'data:image/png;base64,'+'A'.repeat(32*1024*1024-1024)})])});
 expect(Buffer.byteLength(JSON.stringify({version:1,document:state.doc.toJSON()}))).toBeLessThan(32*1024*1024);
 expect(()=>createWordReplacement(state,[{from:1,to:2}],'a','x'.repeat(4096))).toThrow('32 MiB');expect(state.doc.child(0).textContent).toBe('a');
});

test('replacement validates UTF8 growth and stale ranges before changing the state',()=>{
 const original=schema.node('doc',null,[paragraph('a'.repeat(2*1024*1024))]);const state=EditorState.create({doc:original});
 expect(()=>createWordReplacement(state,[{from:1,to:2}],'a','中')).toThrow('2 MiB');expect(state.doc).toBe(original);
 const near=EditorState.create({doc:schema.node('doc',null,[paragraph('a'.repeat(2*1024*1024-2))])});
 expect(createWordReplacement(near,[{from:1,to:2}],'a','中').doc.textContent.startsWith('中a')).toBe(true);
 const oversized=EditorState.create({doc:schema.node('doc',null,[paragraph('a'.repeat(2*1024*1024+1))])});expect(createWordReplacement(oversized,[{from:1,to:2}],'a','').doc.textContent.length).toBe(2*1024*1024);
 for(const value of ['\ufffe','\uffff','\ud800','bad\u0001','x'.repeat(4097)])expect(()=>createWordReplacement(state,[{from:1,to:2}],'a',value)).toThrow('4096');
 const short=EditorState.create({doc:schema.node('doc',null,[paragraph('aa'),paragraph('aa')])});
 expect(()=>createWordReplacement(short,[{from:1,to:2},{from:1,to:2}],'a','b')).toThrow('失效');
 expect(()=>createWordReplacement(short,[{from:1,to:2}],'z','b')).toThrow('失效');
 expect(createWordReplacement(short,searchWord(short.doc,'a').matches,'a','a')).toBeNull();
 const transaction=createWordReplacement(short,searchWord(short.doc,'a').matches,'a','aa');expect(transaction.doc.textContent).toBe('aaaaaaaa');expect(short.doc.textContent).toBe('aaaa');
});

test('all replacements preserve per-match styles and tables, undo together and export through Rust',async({page},info)=>{
 const source={type:'doc',content:[{type:'heading',attrs:{level:2,textAlign:'center'},content:[{type:'text',text:'前中',marks:[{type:'bold'}]},{type:'text',text:'文后',marks:[{type:'italic'}]}]},{type:'paragraph',content:[{type:'text',text:'中文 中文'}]},{type:'table',content:[{type:'tableRow',content:[{type:'tableCell',content:[{type:'paragraph',content:[{type:'text',text:'中文'}]}]}]}]},{type:'paragraph',content:[{type:'text',text:'中'},{type:'hardBreak'},{type:'text',text:'文'}]}]};
 let draft,writes=0,exported,imported;
 await nativeBridge(page,async(command,args)=>{
  if(command==='load_draft')return source;if(command==='save_draft'){draft=args.doc;writes++;return;}
  if(command==='save_new_document'){
   const json=info.outputPath('replacement.json');exported=info.outputPath('replacement.docx');mkdirSync(path.dirname(json),{recursive:true});writeFileSync(json,JSON.stringify(args.doc));const exe=path.resolve('target/debug/rust-wps.exe');const report=JSON.parse(execFileSync(exe,['create-docx',json,exported],{encoding:'utf8',windowsHide:true}));imported=JSON.parse(execFileSync(exe,['import-docx',exported],{encoding:'utf8',windowsHide:true})).document;return {path:exported,report};
  }throw new Error(command);
 });
 await page.goto('/');await page.locator('#restore-draft').click();await page.locator('#edit-find').fill('中文');await expect(page.locator('#edit-find-state')).toHaveText('共4 处');
 await page.locator('#edit-replacement').fill('中文中文');await page.locator('#edit-replace-all').click();await expect(page.locator('#notice')).toContainText('已替换 4 处');await expect(page.locator('#edit-find-state')).toHaveText('共8 处');
 await expect(page.locator('.tiptap h2 strong')).toHaveText('前中文中文');await expect(page.locator('.tiptap h2 em')).toHaveText('后');await expect(page.locator('.tiptap td')).toHaveText('中文中文');await expect(page.locator('.tiptap p').last().locator('br')).toHaveCount(1);await expect.poll(()=>writes).toBe(1);
 const after=draft;expect(after.content[0].attrs.textAlign).toBe('center');
 await page.locator('#edit-undo').click();await expect(page.locator('.tiptap h2')).toHaveText('前中文后');await expect(page.locator('.tiptap td')).toHaveText('中文');await expect(page.locator('#edit-undo')).toBeDisabled();
 await page.locator('#edit-redo').click();await expect(page.locator('.tiptap td')).toHaveText('中文中文');await page.locator('#edit-save').click();await expect(page.locator('#notice')).toContainText('已导出 DOCX');expect(imported.content[0].content[0]).toMatchObject({text:'前中文中文',marks:[{type:'bold'},{type:'fontSize',attrs:{halfPoints:30}},{type:'fontFamily',attrs:{name:'Microsoft YaHei'}}]});
 const python=path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
 execFileSync(python,['-c','from docx import Document; import sys; d=Document(sys.argv[1]); p=d.paragraphs[0]; assert p.text=="前中文中文后"; assert p.runs[0].bold and p.runs[-1].italic; assert d.tables[0].cell(0,0).text=="中文中文"',exported],{windowsHide:true});
 await page.locator('#edit-replacement').fill('');await page.locator('#edit-replace-all').click();await expect(page.locator('.tiptap h2')).toHaveText('前后');await expect(page.locator('.tiptap td')).toHaveText('');await page.locator('#edit-undo').click();await expect(page.locator('.tiptap h2')).toHaveText('前中文中文后');
});

test('incomplete scan blocks all replacement without changing document or drafts',async({page})=>{
 const original='x '.repeat(1000);let writes=0;
 await nativeBridge(page,async(command)=>{if(command==='load_draft')return {type:'doc',content:[{type:'paragraph',content:[{type:'text',text:original}]}]};if(command==='save_draft'){writes++;return;}throw new Error(command);});
 await page.goto('/');await page.locator('#restore-draft').click();await page.locator('#edit-find').fill('x');await page.locator('#edit-replacement').fill('y');await expect(page.locator('#edit-find-state')).toContainText('扫描不完整');await expect(page.locator('#edit-replace-all')).toBeDisabled();
 await page.locator('#edit-replace-all').evaluate(button=>{button.disabled=false;button.click();});await expect(page.locator('#notice')).toContainText('无法全部替换');expect(await page.locator('.tiptap p').textContent()).toBe(original);await page.waitForTimeout(800);expect(writes).toBe(0);await expect(page.locator('#edit-undo')).toBeDisabled();
});
