// Build the CLI and desktop with native-smoke before running. No model injection:
// real typing, Ctrl+E code formatting, and toolbar edits produce the draft.
import assert from 'node:assert/strict';
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import path from 'node:path';

const family = 'Courier New';
const probeTexts = ['FontBodyProbe', 'FontCodeProbe'];
const rgb = '1A6B3C';
const halfPoints = 27;
const installedFontPath = path.join(process.env.WINDIR ?? 'C:\\Windows', 'Fonts', 'cour.ttf');
assert.ok(existsSync(installedFontPath), `This probe requires installed Courier New: ${installedFontPath}`);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const installedFontHash = hash(readFileSync(installedFontPath));
mkdirSync('tmp/native-word-font-family', { recursive: true });
const directory = mkdtempSync(path.resolve('tmp/native-word-font-family/run-'));
const output = path.join(directory, 'font-family.pdf');
const docxPath = path.join(directory, 'font-family.docx');
const draftPath = path.join(directory, 'draft-v1.json');
const modelPath = path.join(directory, 'model.json');
const python = path.join(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe');
const saveReport = (name, value) => writeFileSync(path.join(directory, name), `${JSON.stringify(value, null, 2)}\n`, { flag: 'wx' });
const reservation = createServer();
await new Promise((resolve, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', resolve); });
const port = reservation.address().port;
await new Promise((resolve, reject) => reservation.close(error => error ? reject(error) : resolve()));
const child = spawn(path.resolve('src-tauri/target/debug/rust-wps-desktop.exe'), [], {
  windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, RUST_WPS_SMOKE_PDF_OUTPUT: output, RUST_WPS_SMOKE_OPEN_INPUT: docxPath,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1` },
});
let processError, log = '', browser, page;
child.on('error', error => { processError = error; });
for (const stream of [child.stdout, child.stderr]) stream.on('data', data => { log = (log + data.toString()).slice(-65536); });
const childClosed = new Promise(resolve => child.once('close', resolve));
function checkDraft(draft, expectedFamily = family) {
  assert.equal(draft.version, 2);
  assert.equal(draft.document.attrs?.styleProfile, 'standard-v2');
  assert.equal(draft.document.content.length, 2);
  for (const [index, paragraph] of draft.document.content.entries()) {
    assert.equal(paragraph.type, 'paragraph');
    assert.ok(paragraph.content?.length > 0);
    assert.equal(paragraph.content.map(node => node.text ?? '').join(''), probeTexts[index]);
    for (const node of paragraph.content) {
      assert.equal(node.type, 'text');
      const marks = node.marks ?? [];
      const fonts = marks.filter(mark => mark.type === 'fontFamily');
      assert.equal(fonts.length, expectedFamily === null ? 0 : 1);
      if (expectedFamily !== null) assert.deepEqual(fonts[0].attrs, { name: expectedFamily });
      assert.deepEqual(marks.filter(mark => mark.type === 'fontSize').map(mark => mark.attrs), [{ halfPoints }]);
      assert.deepEqual(marks.filter(mark => mark.type === 'textColor').map(mark => mark.attrs), [{ rgb }]);
      assert.equal(marks.filter(mark => mark.type === 'code').length, index === 1 ? 1 : 0);
    }
  }
}
async function waitForDraft(expectedFamily = family) {
  await expect.poll(() => {
    try { checkDraft(JSON.parse(readFileSync(draftPath, 'utf8')), expectedFamily); return true; }
    catch { return false; }
  }, { timeout: 10000 }).toBe(true);
}
async function checkEditor() {
  const editor = page.locator('.tiptap');
  await expect(editor).toHaveAttribute('data-style-profile', 'standard-v2');
  await expect(editor.locator(':scope > p')).toHaveText(probeTexts);
  await expect(editor.locator('code')).toHaveText(probeTexts[1]);
  await expect(editor.locator('code [data-font-family]')).toHaveText(probeTexts[1]);
  const spans = editor.locator('[data-font-family]');
  await expect(spans).toHaveCount(2);
  const css = await spans.evaluateAll(elements => elements.map(element => {
    const style = getComputedStyle(element);
    return { text: element.textContent, name: element.getAttribute('data-font-family'),
      fontFamily: style.fontFamily, fontSize: style.fontSize, color: style.color,
      insideCode: element.closest('code') !== null };
  }));
  assert.deepEqual(css.map(span => span.text), probeTexts);
  for (const [index, span] of css.entries()) {
    assert.equal(span.name, family);
    assert.equal(span.fontFamily.replace(/^"|"$/g, ''), family);
    assert.equal(span.fontSize, '18px');
    assert.equal(span.color, 'rgb(26, 107, 60)');
    assert.equal(span.insideCode, index === 1);
  }
  return css;
}
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
  page.on('dialog', async dialog => {
    if (dialog.type() === 'confirm' && dialog.message().includes('保存本地草稿后离开')) await dialog.accept();
    else { log += `\nUnexpected dialog: ${dialog.type()} ${dialog.message()}\n`; await dialog.dismiss(); }
  });
  await page.waitForLoadState('domcontentloaded');
  await page.locator('#new-document').click();
  await page.locator('.tiptap').click();
  await page.keyboard.type(probeTexts[0]);
  await page.keyboard.press('Enter');
  await page.keyboard.press('Control+e');
  await page.keyboard.type(probeTexts[1]);
  await expect(page.locator('.tiptap code')).toHaveText(probeTexts[1]);
  await page.keyboard.press('Control+a');
  await page.locator('#edit-font-size').fill('13.5');
  await page.locator('#edit-font-size').press('Enter');
  await page.locator('#edit-text-color').fill('#1a6b3c');
  await page.locator('#edit-text-color').press('Enter');
  await waitForDraft(null);
  await page.locator('#edit-font-family').fill(family);
  await page.locator('#edit-font-family').press('Enter');
  await waitForDraft();
  await checkEditor();
  await page.locator('#edit-undo').click();
  await expect(page.locator('.tiptap [data-font-family]')).toHaveCount(0);
  await waitForDraft(null);
  await page.locator('#edit-redo').click();
  await waitForDraft();
  const editorBeforeReopen = await checkEditor();
  const draft = JSON.parse(readFileSync(draftPath, 'utf8'));
  checkDraft(draft);
  writeFileSync(modelPath, JSON.stringify(draft.document), { flag: 'wx' });
  execFileSync(path.resolve('target/debug/rust-wps.exe'), ['create-docx', modelPath, docxPath],
    { windowsHide: true, timeout: 30000 });
  const docxHash = hash(readFileSync(docxPath));
  const docx = JSON.parse(execFileSync(python, ['-c', [
    'import sys,json', 'from docx import Document', 'from docx.oxml.ns import qn',
    'doc=Document(sys.argv[1]);rows=[]',
    'assert [p.text for p in doc.paragraphs]==["FontBodyProbe","FontCodeProbe"]',
    'for paragraph in doc.paragraphs:',
    '    for run in paragraph.runs:',
    '        rpr=run._r.rPr;fonts=rpr.findall(qn("w:rFonts"))',
    '        assert len(fonts)==1,"Duplicate rFonts: explicit family must replace the code default"',
    '        slots={name:fonts[0].get(qn("w:"+name)) for name in ["ascii","hAnsi","eastAsia","cs"]}',
    '        assert all(value=="Courier New" for value in slots.values()),slots',
    '        assert len(fonts[0].attrib)==4,dict(fonts[0].attrib)',
    '        assert run.font.size.pt==13.5 and rpr.find(qn("w:szCs")).get(qn("w:val"))=="27"',
    '        assert str(run.font.color.rgb)=="1A6B3C" and len(rpr.findall(qn("w:color")))==1',
    '        rows.append({"text":run.text,"font_slots":slots,"points":run.font.size.pt,"sizeCsHalfPoints":27,"rgb":str(run.font.color.rgb),"rFontsCount":len(fonts)})',
    'print(json.dumps({"runs":rows},ensure_ascii=True))',
  ].join('\n'), docxPath], { encoding: 'utf8', windowsHide: true, timeout: 30000 }));
  saveReport('docx-inspection.json', docx);
  await page.locator('#open').click();
  await expect(page.locator('#title')).toHaveText(path.basename(docxPath), { timeout: 35000 });
  await expect(page.locator('#draft-state')).toHaveText('文档已打开');
  const editorAfterReopen = await checkEditor();
  const draftBeforePrint = readFileSync(draftPath);
  checkDraft(JSON.parse(draftBeforePrint.toString('utf8')));
  assert.equal(hash(readFileSync(docxPath)), docxHash);
  await page.screenshot({ path: path.join(directory, 'editor.png'), fullPage: true });
  await page.locator('#edit-export-pdf').click();
  await page.getByRole('status').filter({ hasText: '已另存 PDF' }).waitFor({ timeout: 70000 });
  const pdf = JSON.parse(execFileSync(python, ['-c', [
    'import sys,json,math', 'from pypdf import PdfReader',
    'reader=PdfReader(sys.argv[1]);pages=[];samples=[];fill=[0.0,0.0,0.0];stack=[];page_index=0',
    'def operand(op,args,cm,tm):',
    '    global fill',
    '    if op==b"q":stack.append(fill[:])',
    '    elif op==b"Q":fill=stack.pop()',
    '    elif op==b"rg":fill=[float(value) for value in args]',
    '    elif op==b"g":fill=[float(args[0])]*3',
    '    elif op==b"k":',
    '        c,m,y,k=map(float,args);fill=[1-min(1,value+k) for value in [c,m,y]]',
    'def visit(text,cm,tm,font,size):',
    '    for marker in ["FontBodyProbe","FontCodeProbe"]:',
    '        if marker in "".join(text.split()):',
    '            scale=math.hypot(tm[2]*cm[0]+tm[3]*cm[2],tm[2]*cm[1]+tm[3]*cm[3])',
    '            samples.append({"marker":marker,"text":text,"page":page_index,"font_size":size,"points":size*scale,"cm":list(cm),"tm":list(tm),"fill_rgb":fill[:],"BaseFont":str(font.get("/BaseFont","")) if font else None})',
    'for page_index,page in enumerate(reader.pages):',
    '    fill=[0.0,0.0,0.0];stack=[];fonts=[];font_map=page["/Resources"].get("/Font",{})',
    '    if hasattr(font_map,"get_object"):font_map=font_map.get_object()',
    '    for key,reference in font_map.items():',
    '        font=reference.get_object();descendants=[item.get_object() for item in font.get("/DescendantFonts",[])]',
    '        fonts.append({"key":str(key),"BaseFont":str(font.get("/BaseFont","")),"Subtype":str(font.get("/Subtype","")),"descendant_fonts":[str(item.get("/BaseFont","")) for item in descendants]})',
    '    text=page.extract_text(visitor_text=visit,visitor_operand_before=operand) or ""',
    '    pages.append({"width_points":float(page.mediabox.width),"height_points":float(page.mediabox.height),"text":text,"font_resources":fonts})',
    'print(json.dumps({"pages":pages,"font_samples":samples},ensure_ascii=True))',
  ].join('\n'), output], { encoding: 'utf8', windowsHide: true, timeout: 30000 }));
  saveReport('pdf-inspection.json', pdf);
  assert.equal(pdf.pages.length, 1);
  assert.ok(Math.abs(pdf.pages[0].width_points - 210 * 72 / 25.4) <= 1);
  assert.ok(Math.abs(pdf.pages[0].height_points - 297 * 72 / 25.4) <= 1);
  // Remove only the PDF subset tag. Do not accept a CSS family declaration,
  // font availability check, or a fallback resource as proof of actual usage.
  const baseFontName = value => (value ?? '').replace(/^\//, '').replace(/^[A-Z]{6}\+/, '');
  const expectedBaseFont = 'CourierNewPSMT';
  assert.ok(pdf.pages[0].font_resources.some(font => [font.BaseFont, ...font.descendant_fonts]
    .some(name => baseFontName(name) === expectedBaseFont)), JSON.stringify(pdf.pages[0].font_resources));
  const expectedRgb = [26 / 255, 107 / 255, 60 / 255];
  for (const marker of probeTexts) {
    const samples = pdf.font_samples.filter(sample => sample.marker === marker);
    assert.ok(samples.length > 0, `Missing PDF probe ${marker}`);
    for (const sample of samples) {
      assert.equal(baseFontName(sample.BaseFont), expectedBaseFont, JSON.stringify(sample));
      assert.ok(Math.abs(sample.points - 13.5) <= 0.15, JSON.stringify(sample));
      assert.ok(sample.fill_rgb.length === 3 && sample.fill_rgb.every((value, index) => Math.abs(value - expectedRgb[index]) < 1 / 255), JSON.stringify(sample));
    }
  }
  assert.ok(readFileSync(draftPath).equals(draftBeforePrint), 'Printing changed the document draft');
  assert.equal(hash(readFileSync(docxPath)), docxHash, 'Opening or printing changed the DOCX');
  const report = { nativeWebView2: true, actualTypingAndToolbarEdits: true, family, installedFontPath, installedFontHash,
    explicitFamilyOverridesCode: true, undoRedoPreservesSizeColorCodeAndText: true,
    workerReopenPreservesFontSizeColorAndCode: true, printLeavesDraftUnchanged: true, sourceDocxUnchanged: true,
    actualDraftHalfPoints: halfPoints, actualDraftRgb: rgb, expectedPdfBaseFont: expectedBaseFont,
    docxHash, directory, docxPath, output, editorBeforeReopen, editorAfterReopen, docx, pdf,
    limitation: 'Installed Courier New on two horizontal Latin probes; not arbitrary font availability, CJK glyph fallback, full shaping, or Word layout equivalence.' };
  saveReport('report.json', report);
  console.log(JSON.stringify(report, null, 2));
} catch (error) {
  if (page) {
    const state = await page.evaluate(() => ({ status: document.querySelector('#status')?.textContent,
      notice: document.querySelector('#notice')?.textContent, title: document.querySelector('#title')?.textContent,
      familyInput: document.querySelector('#edit-font-family')?.value,
      editor: document.querySelector('.tiptap')?.outerHTML })).catch(failure => ({ error: String(failure) }));
    saveReport('failure-state.json', state);
    console.error(JSON.stringify(state));
    await page.screenshot({ path: path.join(directory, 'failure.png'), fullPage: true }).catch(() => {});
  }
  throw error;
} finally {
  try { if (browser) await browser.close().catch(() => {}); }
  finally {
    if (child.exitCode === null && child.signalCode === null && !processError) child.kill();
    await childClosed;
    writeFileSync(path.join(directory, 'native-process.log'), log);
  }
}
