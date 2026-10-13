// Opt-in native WebView2 test. Build with --features native-smoke first.
// The feature only bypasses the save dialog when an explicit test output is set.
import { chromium, expect } from '@playwright/test';
import { once } from 'node:events';
import { spawn, execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { createServer } from 'node:net';

mkdirSync('tmp/native-pdf', { recursive: true });
const directory = mkdtempSync(path.resolve('tmp/native-pdf/run-'));
const output = path.join(directory, '原生导出.pdf');
const input = path.join(directory, 'input.docx');
writeFileSync(input, readFileSync('tmp/corpus/SampleDoc.docx'));
const reservation = createServer();
await new Promise(resolve => reservation.listen(0, '127.0.0.1', resolve));
const port = reservation.address().port;
await new Promise(resolve => reservation.close(resolve));
const child = spawn(path.resolve('src-tauri/target/debug/rust-wps-desktop.exe'), [], {
  windowsHide: true, stdio: 'pipe', env: { ...process.env, RUST_WPS_SMOKE_PDF_OUTPUT: output, RUST_WPS_SMOKE_OPEN_INPUT: input, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let browser;
try {
  // Poll the actual process and CDP readiness, with a bounded startup deadline.
  const deadline = Date.now() + 20000;
  while (!browser && Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Native app exited: ${child.exitCode}`);
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); }
    catch { await new Promise(resolve => setTimeout(resolve, 300)); }
  }
  if (!browser) throw new Error('Native WebView2 CDP did not become ready');
  const context = browser.contexts()[0];
  let page = context.pages()[0];
  if (!page) page = await context.waitForEvent('page', { timeout: 10000 });
  await page.locator('#open').click();
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === '就绪' && (document.querySelector('#text-content')?.textContent.includes('I am a test document') || document.querySelector('#notice')?.classList.contains('error')));
  if (await page.locator('#notice').evaluate(node => node.classList.contains('error'))) throw new Error(await page.locator('#notice').textContent());
  await page.locator('#text-content').filter({ hasText: 'I am a test document' }).waitFor();
  await page.locator('#import-document').click();
  await page.getByRole('button', { name: '接受变化并编辑副本' }).click();
  await page.locator('#editor-content .tiptap').fill('原生 PDF 中文导出验证');
  await page.evaluate(()=>{const t=document.querySelector('.tiptap p').firstChild;window.getSelection().setBaseAndExtent(t,0,t,2);document.getElementById('edit-strike').click();});
  await expect(page.locator('.tiptap s')).toHaveText('原生');await expect(page.locator('#edit-strike')).toHaveAttribute('aria-pressed','true');
  await page.locator('#edit-undo').click();await expect(page.locator('.tiptap s')).toHaveCount(0);await page.locator('#edit-redo').click();await expect(page.locator('.tiptap s')).toHaveText('原生');
  await expect.poll(()=>{try{return JSON.parse(readFileSync(path.join(directory,'draft-v1.json'),'utf8')).document.content[0].content[0];}catch{return null;}},{timeout:10000}).toEqual({type:'text',marks:[{type:'strike'}],text:'原生'});
  await page.evaluate(()=>{const t=document.querySelector('.tiptap s').firstChild;window.getSelection().setBaseAndExtent(t,0,t,1);document.getElementById('edit-superscript').click();});
  await expect(page.locator('.tiptap sup')).toHaveText('原');await page.locator('#edit-subscript').click();await expect(page.locator('.tiptap sup')).toHaveCount(0);await expect(page.locator('.tiptap sub')).toHaveText('原');
  await expect.poll(()=>{try{const n=JSON.parse(readFileSync(path.join(directory,'draft-v1.json'),'utf8')).document.content[0].content[0];return n.text==='原'&&n.marks.some(m=>m.type==='subscript')&&!n.marks.some(m=>m.type==='superscript');}catch{return false;}},{timeout:10000}).toBe(true);
  await page.locator('#edit-undo').click();await expect(page.locator('.tiptap sup')).toHaveText('原');await page.locator('#edit-undo').click();await expect(page.locator('.tiptap sup')).toHaveCount(0);await expect(page.locator('.tiptap sub')).toHaveCount(0);
  await page.evaluate(()=>{const t=document.querySelector('.tiptap s').firstChild;window.getSelection().setBaseAndExtent(t,0,t,1);document.getElementById('edit-clear-marks').click();});
  await expect(page.locator('.tiptap s')).toHaveText('生');
  await expect.poll(()=>{try{const n=JSON.parse(readFileSync(path.join(directory,'draft-v1.json'),'utf8')).document.content[0].content[0];return n.text==='原'&&(!n.marks||n.marks.length===0);}catch{return false;}},{timeout:10000}).toBe(true);
  await page.locator('#edit-undo').click();await expect(page.locator('.tiptap s')).toHaveText('原生');
  await page.keyboard.press('End');
  await page.keyboard.press('Enter');
  await page.keyboard.insertText('Native WebView2 PrintToPdf success.');
  await expect(page.locator('.tiptap p')).toHaveText(['原生 PDF 中文导出验证','Native WebView2 PrintToPdf success.']);
  await page.locator('#edit-table').click();
  await page.locator('#editor-content th').first().click(); await page.keyboard.insertText('测试表格');
  await expect.poll(()=>{try{const data=readFileSync(path.join(directory,'draft-v1.json'),'utf8');return data.includes('测试表格')&&data.includes('Native WebView2 PrintToPdf success.');}catch{return false;}},{timeout:10000}).toBe(true);
  const beforeSearch=readFileSync(path.join(directory,'draft-v1.json'));
  await page.keyboard.press('Control+f');await expect(page.locator('#edit-find')).toBeFocused();await page.locator('#edit-find').fill('测试表格');await page.locator('#edit-find').press('Enter');await expect(page.locator('#edit-find-state')).toHaveText('1/1 处');{
    if(await page.evaluate(()=>window.getSelection().toString())!=='测试表格')throw new Error('Editor search selected wrong table range');
  }
  await page.locator('#edit-find').fill('原生');await page.locator('#edit-find').press('Enter');if(await page.evaluate(()=>window.getSelection().toString())!=='原生')throw new Error('Editor search selected wrong styled range');
  await page.waitForTimeout(800);if(!readFileSync(path.join(directory,'draft-v1.json')).equals(beforeSearch))throw new Error('Search changed the document draft');
  await page.locator('#edit-replacement').fill('原生替换😀');await page.locator('#edit-replace-current').click();
  await expect(page.locator('.tiptap s')).toHaveText('原生替换😀');
  await expect.poll(()=>{try{return JSON.parse(readFileSync(path.join(directory,'draft-v1.json'),'utf8')).document.content[0].content[0];}catch{return null;}},{timeout:10000}).toEqual({type:'text',marks:[{type:'strike'}],text:'原生替换😀'});
  const replacementModel=path.join(directory,'replacement.json'),replacementDocx=path.join(directory,'replacement.docx');
  writeFileSync(replacementModel,JSON.stringify(JSON.parse(readFileSync(path.join(directory,'draft-v1.json'),'utf8')).document));
  execFileSync(path.resolve('target/debug/rust-wps.exe'),['create-docx',replacementModel,replacementDocx],{windowsHide:true});
  execFileSync(path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe'),['-c','import sys; from docx import Document; d=Document(sys.argv[1]); r=d.paragraphs[0].runs[0]; assert r.text=="原生替换😀" and r.font.strike; assert d.tables[0].cell(0,0).text=="测试表格"',replacementDocx],{windowsHide:true});
  await page.keyboard.insertText('紧接输入');await expect(page.locator('.tiptap p').first()).toContainText('原生替换😀紧接输入');
  await page.locator('#edit-undo').click();await expect(page.locator('.tiptap p').first()).toHaveText('原生替换😀 PDF 中文导出验证');
  await page.locator('#edit-undo').click();await expect(page.locator('.tiptap p').first()).toHaveText('原生 PDF 中文导出验证');await expect(page.locator('.tiptap s')).toHaveText('原生');
  await expect.poll(()=>{try{return JSON.parse(readFileSync(path.join(directory,'draft-v1.json'),'utf8')).document;}catch{return null;}},{timeout:10000}).toEqual(JSON.parse(beforeSearch.toString('utf8')).document);
  await page.locator('#editor-content th').first().click();await page.keyboard.press('End');await page.keyboard.insertText(' 原生');
  await page.locator('#edit-find').fill('原生');await expect(page.locator('#edit-find-state')).toHaveText('共2 处');await page.locator('#edit-replacement').fill('批量😀');await page.locator('#edit-replace-all').click();
  await expect(page.locator('#notice')).toContainText('已替换 2 处');await expect(page.locator('.tiptap s')).toHaveText('批量😀');await expect(page.locator('#editor-content th').first()).toHaveText('测试表格 批量😀');
  await expect.poll(()=>{try{const document=JSON.parse(readFileSync(path.join(directory,'draft-v1.json'),'utf8')).document;return JSON.stringify(document).split('批量😀').length-1;}catch{return 0;}},{timeout:10000}).toBe(2);
  const allModel=path.join(directory,'all-replacement.json'),allDocx=path.join(directory,'all-replacement.docx');
  writeFileSync(allModel,JSON.stringify(JSON.parse(readFileSync(path.join(directory,'draft-v1.json'),'utf8')).document));
  execFileSync(path.resolve('target/debug/rust-wps.exe'),['create-docx',allModel,allDocx],{windowsHide:true});
  execFileSync(path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe'),['-c','import sys; from docx import Document; d=Document(sys.argv[1]); r=d.paragraphs[0].runs[0]; assert r.text=="批量😀" and r.font.strike; assert d.tables[0].cell(0,0).text=="测试表格 批量😀"',allDocx],{windowsHide:true});
  await page.locator('#edit-undo').click();await expect(page.locator('.tiptap s')).toHaveText('原生');await expect(page.locator('#editor-content th').first()).toHaveText('测试表格 原生');
  await page.locator('#edit-undo').click();await expect(page.locator('#editor-content th').first()).toHaveText('测试表格');
  await expect.poll(()=>{try{return JSON.parse(readFileSync(path.join(directory,'draft-v1.json'),'utf8')).document;}catch{return null;}},{timeout:10000}).toEqual(JSON.parse(beforeSearch.toString('utf8')).document);
  await page.locator('#edit-find').press('Escape');
  await page.locator('#edit-export-pdf').click();
  await page.getByRole('status').filter({ hasText: '已另存 PDF' }).waitFor({ timeout: 70000 });
  const original = readFileSync(output);
  if (!original.subarray(0, 5).equals(Buffer.from('%PDF-'))) throw new Error('Invalid PDF signature');
  await page.locator('#edit-export-pdf').click();
  await page.getByRole('status').filter({ hasText: '目标文件已存在' }).waitFor();
  if (!readFileSync(output).equals(original)) throw new Error('Existing output was overwritten');
  const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const report = JSON.parse(execFileSync(python, ['-c', 'import sys,json; from pypdf import PdfReader; r=PdfReader(sys.argv[1]); print(json.dumps({"pages":len(r.pages),"width":float(r.pages[0].mediabox.width),"height":float(r.pages[0].mediabox.height),"text":"\\n".join(p.extract_text() for p in r.pages)},ensure_ascii=True))', output], { encoding: 'utf8' }));
  // Windows font ToUnicode maps can use compatibility radicals; pypdf can
  // also insert spaces within kerned Latin words. Preserve raw evidence and
  // compare explicitly normalized text, rather than claiming byte identity.
  const canonical = report.text.normalize('NFKC').replace(/\s+/g, '');
  for (const text of ['原生PDF中文导出验证', 'NativeWebView2', '测试表格']) if (!canonical.includes(text)) throw new Error(`Missing printed text: ${text}`);
  if (Math.abs(report.width - 595.28) > 1 || Math.abs(report.height - 841.89) > 1) throw new Error('Unexpected page geometry');
  // Corrupt only this run's input copy. The parser error must not replace the
  // current editor; confirmation UI itself is covered by separate tests.
  writeFileSync(input, 'PK\x03\x04invalid');
  await page.evaluate(() => { window.confirm = () => true; });
  await page.locator('#open').click();
  await page.waitForFunction(() => document.querySelector('#status')?.textContent === '就绪' && document.querySelector('#notice')?.textContent.includes('Invalid DOCX ZIP'));
  if (!(await page.locator('#editor-content').textContent()).includes('原生 PDF 中文导出验证')) throw new Error('Parser failure replaced current editor');
  await page.locator('#editor-content .tiptap p').first().click(); await page.keyboard.press('End'); await page.keyboard.insertText(' 故障后继续编辑');
  if (!(await page.locator('#editor-content').textContent()).includes('故障后继续编辑')) throw new Error('Editor did not recover from parser error');
  console.log(JSON.stringify({ nativeWebView2: true, strikeSelectionUndoDraft: true, scriptMarksExclusiveUndoDraft: true, clearMarksSelectionUndoDraft: true, editorSearchNoDraft: true, editorReplacementDraftUndoAndIndependentDocx: true, editorReplaceAllDraftUndoAndIndependentDocx:true, allDocx, replacementDocx, isolatedOpenAndImport: true, parserErrorPreservesEditor: true, preservedExistingOutput: true, comparison: 'NFKC and whitespace normalization; raw text retained below', output, ...report }, null, 2));
} finally {
  if (browser) await browser.close().catch(() => {});
  if(child.exitCode===null){const exited=once(child,'exit');child.kill();await exited;}
}
