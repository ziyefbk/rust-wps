// Build with --features native-smoke. Exercises real WebView2, draft IPC,
// Rust DOCX output, and physical PDF font size/text color read by pypdf.
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import path from 'node:path';

mkdirSync('tmp/native-word-size', { recursive: true });
const directory = mkdtempSync(path.resolve('tmp/native-word-size/run-'));
const output = path.join(directory, 'font-size.pdf');
const draftPath = path.join(directory, 'draft-v1.json');
const reservation = createServer();
await new Promise((resolve, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', resolve); });
const port = reservation.address().port;
await new Promise((resolve, reject) => reservation.close(error => error ? reject(error) : resolve()));
const child = spawn(path.resolve('src-tauri/target/debug/rust-wps-desktop.exe'), [], {
  windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, RUST_WPS_SMOKE_PDF_OUTPUT: output,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let processError, log = '', browser, page;
child.on('error', error => { processError = error; });
for (const stream of [child.stdout, child.stderr]) stream.on('data', data => { log = (log + data.toString()).slice(-65536); });
const childClosed = new Promise(resolve => child.once('close', resolve));
const halfPoints = model => model.content[0]?.content[0]?.marks?.find(mark => mark.type === 'fontSize')?.attrs.halfPoints;
const textColor = model => model.content[0]?.content[0]?.marks?.find(mark => mark.type === 'textColor')?.attrs.rgb;
try {
  const deadline = Date.now() + 20000;
  while (!browser && Date.now() < deadline) {
    if (processError) throw processError;
    if (child.exitCode !== null || child.signalCode !== null) throw new Error(`Native app exited: ${child.exitCode ?? child.signalCode}`);
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); }
    catch { await new Promise(resolve => setTimeout(resolve, 300)); }
  }
  if (!browser) throw new Error('Native WebView2 CDP did not become ready');
  const context = browser.contexts()[0];
  page = context.pages()[0] ?? await context.waitForEvent('page', { timeout: 10000 });
  page.on('pageerror', error => { log += `\nPage error: ${error.stack ?? error}\n`; });
  await page.waitForLoadState('domcontentloaded');
  await page.locator('#new-document').click();
  await page.locator('.tiptap').fill('SizeProbe 字号验证 العربية');
  await page.locator('.tiptap').click(); await page.keyboard.press('Control+a');
  await page.locator('#edit-font-size').fill('13.5'); await page.locator('#edit-font-size').press('Enter');
  await expect(page.locator('.tiptap [data-font-size="27"]')).toHaveText('SizeProbe 字号验证 العربية');
  await expect(page.locator('.tiptap [data-font-size="27"]')).toHaveCSS('font-size', '18px');
  await expect.poll(() => { try { return halfPoints(JSON.parse(readFileSync(draftPath, 'utf8')).document); } catch { return null; } }, { timeout: 10000 }).toBe(27);
  await page.locator('#edit-undo').click();
  await expect(page.locator('.tiptap [data-font-size]')).toHaveCount(0);
  await expect(page.locator('.tiptap')).toHaveText('SizeProbe 字号验证 العربية');
  await page.locator('#edit-redo').click();
  await expect.poll(() => { try { return halfPoints(JSON.parse(readFileSync(draftPath, 'utf8')).document); } catch { return null; } }, { timeout: 10000 }).toBe(27);
  await page.locator('#edit-text-color').fill('#1a6b3c'); await page.locator('#edit-text-color').press('Enter');
  await expect(page.locator('.tiptap [data-text-color="1A6B3C"]')).toHaveCSS('color', 'rgb(26, 107, 60)');
  await expect.poll(() => { try { return textColor(JSON.parse(readFileSync(draftPath, 'utf8')).document); } catch { return null; } }, { timeout: 10000 }).toBe('1A6B3C');
  await page.locator('#edit-undo').click(); await expect(page.locator('.tiptap [data-text-color]')).toHaveCount(0);
  await expect(page.locator('.tiptap [data-font-size="27"]')).toHaveText('SizeProbe 字号验证 العربية');
  await page.locator('#edit-redo').click();
  await expect.poll(() => { try { return textColor(JSON.parse(readFileSync(draftPath, 'utf8')).document); } catch { return null; } }, { timeout: 10000 }).toBe('1A6B3C');
  const draftBeforePrint = readFileSync(draftPath);
  await page.screenshot({ path: path.join(directory, 'editor.png'), fullPage: true });
  const modelPath = path.join(directory, 'model.json'), docxPath = path.join(directory, 'font-size.docx');
  writeFileSync(modelPath, JSON.stringify(JSON.parse(draftBeforePrint.toString('utf8')).document), { flag: 'wx' });
  execFileSync(path.resolve('target/debug/rust-wps.exe'), ['create-docx', modelPath, docxPath], { windowsHide: true, timeout: 30000 });
  const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
  const docx = JSON.parse(execFileSync(python, ['-c', [
    'import sys,json', 'from docx import Document', 'from docx.oxml.ns import qn',
    'runs=Document(sys.argv[1]).paragraphs[0].runs',
    'assert "".join(r.text for r in runs)=="SizeProbe 字号验证 العربية"',
    'assert all(r.font.size.pt==13.5 and r._r.rPr.find(qn("w:szCs")).get(qn("w:val"))=="27" for r in runs)',
    'assert all(str(r.font.color.rgb)=="1A6B3C" and len(r._r.rPr.findall(qn("w:color")))==1 for r in runs)',
    'print(json.dumps({"points":13.5,"sizeCsHalfPoints":27,"rgb":"1A6B3C","runs":len(runs)}))',
  ].join('\n'), docxPath], { encoding: 'utf8', windowsHide: true, timeout: 30000 }));
  await page.locator('#edit-export-pdf').click();
  await page.getByRole('status').filter({ hasText: '已另存 PDF' }).waitFor({ timeout: 70000 });
  const pdf = JSON.parse(execFileSync(python, ['-c', [
    'import sys,json,math', 'from pypdf import PdfReader',
    'reader=PdfReader(sys.argv[1]); samples=[]; fill=[0.0,0.0,0.0]; stack=[]',
    'def operand(op,args,cm,tm):',
    '    global fill',
    '    if op==b"q": stack.append(fill[:])',
    '    elif op==b"Q": fill=stack.pop()',
    '    elif op==b"rg": fill=[float(v) for v in args]',
    '    elif op==b"g": fill=[float(args[0])]*3',
    '    elif op==b"k":',
    '        c,m,y,k=map(float,args);fill=[1-min(1,v+k) for v in [c,m,y]]',
    'def visit(text,cm,tm,font,size):',
    '    if "SizeProbe" in "".join(text.split()):',
    '        # Transform one text-space vertical unit through tm then cm.',
    '        scale=math.hypot(tm[2]*cm[0]+tm[3]*cm[2],tm[2]*cm[1]+tm[3]*cm[3])',
    '        samples.append({"text":text,"font_size":size,"cm":cm,"tm":tm,"points":size*scale,"fill_rgb":fill[:]})',
    'text="\\n".join(p.extract_text(visitor_text=visit,visitor_operand_before=operand) for p in reader.pages)',
    'assert len(reader.pages)==1',
    'assert samples and all(abs(s["points"]-13.5)<0.15 for s in samples),samples',
    'expected=[26/255,107/255,60/255]',
    'assert all(all(abs(a-b)<1/255 for a,b in zip(s["fill_rgb"],expected)) for s in samples),samples',
    'print(json.dumps({"pages":len(reader.pages),"text":text,"font_samples":samples},ensure_ascii=True))',
  ].join('\n'), output], { encoding: 'utf8', windowsHide: true, timeout: 30000 }));
  if (!readFileSync(draftPath).equals(draftBeforePrint)) throw new Error('Printing changed the document draft');
  const report = { nativeWebView2: true, actualDraftHalfPoints: 27, actualDraftRgb: '1A6B3C', undoRedoPreservesText: true,
    printLeavesDraftUnchanged: true, docxPath, output, docx, pdf,
    limitation: 'Physical size and solid RGB fill on horizontal Latin text; not full color management, font substitution, script shaping, or Word layout equivalence.' };
  writeFileSync(path.join(directory, 'report.json'), `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' });
  console.log(JSON.stringify(report, null, 2));
} catch (error) {
  if (page) {
    const state = await page.evaluate(() => ({ status: document.querySelector('#status')?.textContent,
      notice: document.querySelector('#notice')?.textContent, text: document.querySelector('.tiptap')?.textContent,
      input: document.querySelector('#edit-font-size')?.value,
      color: document.querySelector('#edit-text-color')?.value })).catch(failure => ({ error: String(failure) }));
    writeFileSync(path.join(directory, 'failure-state.json'), JSON.stringify(state, null, 2));
    console.error(JSON.stringify(state));
  }
  throw error;
} finally {
  try { if (browser) await browser.close().catch(() => {}); }
  finally {
    if (child.exitCode === null && child.signalCode === null && !processError) child.kill();
    await childClosed; writeFileSync(path.join(directory, 'native-process.log'), log);
  }
}
