// Build the CLI and desktop with --features native-smoke before running.
// Uses actual WebView2 editing, Save/draft IPC, worker reopen, and native PDF output.
// File paths are smoke-only overrides; no native dialog/IME/clipboard claims.
import assert from 'node:assert/strict';
import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import path from 'node:path';

assert.equal(process.platform, 'win32', 'This acceptance requires Windows WebView2');
const profile = 'standard-v2';
const probes = [
  { tag: 'p', text: 'BodyProbe 中文', marker: 'BodyProbe', points: 11 },
  { tag: 'h1', text: 'HeadingOneProbe', marker: 'HeadingOneProbe', points: 18 },
  { tag: 'h2', text: 'HeadingTwoProbe', marker: 'HeadingTwoProbe', points: 15 },
  { tag: 'h3', text: 'HeadingThreeProbe', marker: 'HeadingThreeProbe', points: 13 },
];
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
mkdirSync('tmp/native-word-style', { recursive: true });
const directory = mkdtempSync(path.resolve('tmp/native-word-style/run-'));
const output = path.join(directory, 'standard-v2.pdf');
const docxPath = path.join(directory, 'standard-v2.docx');
const draftPath = path.join(directory, 'draft-v1.json');
const python = process.env.RUST_WPS_SMOKE_PYTHON || 'python';
const desktop = path.resolve('src-tauri/target/debug/rust-wps-desktop.exe');
const cli = path.resolve('target/debug/rust-wps.exe');
const runtime = {
  commit: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(),
  node: process.version,
  rust: execFileSync('rustc', ['--version', '--verbose'], { encoding: 'utf8' }).trim(),
  python: JSON.parse(execFileSync(python, ['-c', 'import sys,json,docx,pypdf;print(json.dumps(dict(version=sys.version,executable=sys.executable,docx=docx.__version__,pypdf=pypdf.__version__)))'], { encoding: 'utf8', timeout: 30000 })),
  desktopSha256: hash(readFileSync(desktop)), cliSha256: hash(readFileSync(cli)),
};
console.log(JSON.stringify({ runtime, directory }));
const reservation = createServer();
await new Promise((resolve, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', resolve); });
const port = reservation.address().port;
await new Promise((resolve, reject) => reservation.close(error => error ? reject(error) : resolve()));
const child = spawn(desktop, [], {
  windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: {
    ...process.env,
    RUST_WPS_SMOKE_PDF_OUTPUT: output,
    RUST_WPS_SMOKE_DOCX_OUTPUT: docxPath,
    WEBVIEW2_USER_DATA_FOLDER: path.join(directory, 'webview2-profile'),
    RUST_WPS_SMOKE_OPEN_INPUT: docxPath,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
  },
});
let processError, log = '', browser, page, cdp;
const startup = { initialReady: false, diagnosticReload: false, events: [] };
const recordRuntime = (kind, detail) => {
  if (startup.events.length < 300) startup.events.push({ kind, time: new Date().toISOString(), detail: String(detail).slice(0, 4000) });
};
child.on('error', error => { processError = error; });
for (const stream of [child.stdout, child.stderr]) stream.on('data', data => { log = (log + data.toString()).slice(-65536); });
const childClosed = new Promise(resolve => child.once('close', resolve));
const saveReport = (name, value) => writeFileSync(path.join(directory, name), `${JSON.stringify(value, null, 2)}\n`, { flag: 'wx' });
const textOf = node => node.text ?? (node.content ?? []).map(textOf).join('');
function checkDraft(draft) {
  assert.equal(draft.version, 2);
  assert.equal(draft.document.attrs?.styleProfile, profile);
  const blocks = draft.document.content;
  // StarterKit enables TrailingNode by default. After a final heading it
  // appends one empty paragraph; it does not add explicit typography marks.
  assert.ok(blocks.length === 4 || blocks.length === 5, `Unexpected block count: ${blocks.length}`);
  if (blocks.length === 5) {
    assert.equal(blocks[4].type, 'paragraph');
    assert.equal(blocks[4].content?.length ?? 0, 0, 'The optional trailing paragraph must be empty');
  }
  assert.deepEqual(blocks.slice(0, 4).map(textOf), probes.map(probe => probe.text));
  assert.deepEqual(blocks.slice(0, 4).map(node => [node.type, node.attrs?.level ?? null]),
    [['paragraph', null], ['heading', 1], ['heading', 2], ['heading', 3]]);
  const stack = [draft.document];
  while (stack.length) {
    const node = stack.pop();
    assert.ok(!(node.marks ?? []).some(mark => ['fontSize', 'textColor'].includes(mark.type)),
      'The default-style probe must have no explicit font-size or text-color marks');
    stack.push(...(node.content ?? []));
  }
}
async function checkEditor() {
  const editor = page.locator('.tiptap');
  await expect(editor).toHaveAttribute('data-style-profile', profile);
  await expect(page.locator('#authoring-sheet')).toHaveAttribute('data-style-profile', profile);
  const blocks = editor.locator(':scope > *');
  const count = await blocks.count();
  assert.ok(count === 4 || count === 5, `Unexpected editor block count: ${count}`);
  assert.deepEqual(await blocks.evaluateAll(elements => elements.slice(0, 4).map(element => element.tagName)),
    ['P', 'H1', 'H2', 'H3']);
  if (count === 5) {
    assert.ok(await blocks.nth(4).evaluate(element => element.tagName === 'P' && element.textContent === ''
      && [...element.children].every(child => child.tagName === 'BR' && child.classList.contains('ProseMirror-trailingBreak'))
      && element.children.length <= 1), 'Only one empty trailing paragraph is allowed');
  }
  await expect(editor.locator('[data-font-size], [data-text-color]')).toHaveCount(0);
  const measured = [];
  for (const [index, probe] of probes.entries()) {
    const block = blocks.nth(index);
    await expect(block).toHaveText(probe.text);
    const css = await block.evaluate(element => {
      const style = getComputedStyle(element);
      return { fontSize: style.fontSize, points: parseFloat(style.fontSize) * 72 / 96,
        color: style.color, fontFamily: style.fontFamily, fontWeight: style.fontWeight };
    });
    assert.ok(Math.abs(css.points - probe.points) < 0.01, JSON.stringify({ probe, css }));
    assert.equal(css.color, 'rgb(0, 0, 0)');
    assert.equal(css.fontFamily.split(',')[0].replaceAll('"', '').trim(), 'Arial');
    assert.equal(Number(css.fontWeight), probe.tag === 'p' ? 400 : 700);
    measured.push({ ...probe, ...css });
  }
  return measured;
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
  runtime.webview2 = browser.version();
  const context = browser.contexts()[0];
  page = context.pages()[0] ?? await context.waitForEvent('page', { timeout: 10000 });
  page.on('pageerror', error => recordRuntime('pageerror', error.stack ?? error));
  page.on('console', message => recordRuntime(`console:${message.type()}`, message.text()));
  page.on('requestfailed', request => recordRuntime('requestfailed', `${request.url()} ${request.failure()?.errorText}`));
  page.on('response', response => {
    const type = response.request().resourceType();
    if (type === 'document' || type === 'script' || response.status() >= 400) {
      const headers = response.headers();
      recordRuntime('response', JSON.stringify({ url: response.url(), status: response.status(), type,
        contentType: headers['content-type'], csp: headers['content-security-policy'] }));
    }
  });
  cdp = await context.newCDPSession(page);
  cdp.on('Log.entryAdded', ({ entry }) => recordRuntime(`cdp-log:${entry.level}`, JSON.stringify(entry)));
  cdp.on('Runtime.exceptionThrown', ({ exceptionDetails }) => recordRuntime('cdp-exception', JSON.stringify(exceptionDetails)));
  await cdp.send('Log.enable'); // Also surfaces entries buffered before CDP attached.
  await cdp.send('Runtime.enable');
  await page.addInitScript(() => {
    window.__nativeSmokeStartup = [];
    const record = event => { if (window.__nativeSmokeStartup.length < 100) window.__nativeSmokeStartup.push(event); };
    window.addEventListener('error', event => record({ kind: 'error', message: event.message?.slice(0, 4000), filename: event.filename, line: event.lineno }));
    window.addEventListener('unhandledrejection', event => record({ kind: 'unhandledrejection', message: String(event.reason).slice(0, 4000) }));
    window.addEventListener('securitypolicyviolation', event => record({ kind: 'csp', directive: event.effectiveDirective, blockedURI: event.blockedURI }));
  });
  const bootState = () => page.evaluate(() => ({
    url: location.href, readyState: document.readyState,
    scripts: [...document.scripts].map(script => ({ src: script.src, type: script.type })),
    cspMeta: [...document.querySelectorAll('meta[http-equiv="Content-Security-Policy"]')].map(meta => meta.content),
    resources: performance.getEntriesByType('resource').slice(0, 100).map(entry => ({ name: entry.name, duration: entry.duration, initiatorType: entry.initiatorType })),
    pdfjsVersion: globalThis.pdfjsLib?.version ?? null,
    tauriInvoke: typeof window.__TAURI_INTERNALS__?.invoke,
    apiSupport: { promiseTry: typeof Promise.try, promiseWithResolvers: typeof Promise.withResolvers,
      mapGetOrInsertComputed: typeof Map.prototype.getOrInsertComputed,
      uint8FromBase64: typeof Uint8Array.fromBase64, urlParse: typeof URL.parse },
    startupEvents: window.__nativeSmokeStartup ?? [],
  }));
  const hasNewDocumentHandler = async () => {
    const { result } = await cdp.send('Runtime.evaluate', { expression: 'document.querySelector("#new-document")', objectGroup: 'smoke-readiness' });
    try {
      if (!result.objectId || result.subtype === 'null') return false;
      const { listeners } = await cdp.send('DOMDebugger.getEventListeners', { objectId: result.objectId });
      return listeners.some(listener => listener.type === 'click');
    } finally { await cdp.send('Runtime.releaseObjectGroup', { objectGroup: 'smoke-readiness' }); }
  };
  // This test owns an isolated draft. Accept only the app's unsaved-draft
  // confirmation when opening the DOCX generated below.
  page.on('dialog', async dialog => {
    if (dialog.type() === 'confirm' && dialog.message().includes('保存本地草稿后离开')) await dialog.accept();
    else { log += `\nUnexpected dialog: ${dialog.type()} ${dialog.message()}\n`; await dialog.dismiss(); }
  });
  await page.locator('#new-document').waitFor({ state: 'attached', timeout: 15000 });
  await page.waitForLoadState('domcontentloaded');
  // Static HTML renders this button enabled before application handlers exist.
  // DOMContentLoaded waits for the entry module; require its actual click handler.
  startup.initialState = await bootState();
  startup.initialReady = await hasNewDocumentHandler();
  if (!startup.initialReady) {
    // No edit has happened. Replay the same real app URL once with diagnostic
    // listeners installed before scripts, solely to capture missed boot errors.
    // Recovery here NEVER converts the original failed launch into a pass.
    startup.diagnosticReload = true;
    await page.reload({ waitUntil: 'load', timeout: 15000 });
    startup.reloadedState = await bootState();
    startup.reloadedReady = await hasNewDocumentHandler();
    throw new Error('Initial native application did not bind New Document; diagnostic reload captured startup evidence (not an acceptance retry)');
  }
  assert.equal(startup.initialState.tauriInvoke, 'function', 'Native Tauri invoke bridge is missing');
  await page.locator('#new-document').click();
  await page.locator('.tiptap').click();
  await page.keyboard.type('BodyProbe ');
  await page.keyboard.insertText('中文');
  for (let level = 1; level <= 3; level++) {
    await page.keyboard.press('Enter');
    await page.locator('#edit-style').selectOption(String(level));
    await page.keyboard.type(probes[level].text);
  }
  const editorBeforeReopen = await checkEditor();
  await expect.poll(() => {
    try { const draft = JSON.parse(readFileSync(draftPath, 'utf8')); checkDraft(draft); return true; }
    catch { return false; }
  }, { timeout: 10000 }).toBe(true);
  const firstDraft = readFileSync(draftPath);
  const draft = JSON.parse(firstDraft.toString('utf8'));
  checkDraft(draft);
  await page.locator('#edit-save').click();
  await expect(page.locator('#notice')).toContainText('已导出 DOCX：', { timeout: 35000 });
  await expect(page.locator('#draft-state')).toHaveText('DOCX 已导出');
  await expect(page.locator('.tiptap')).toHaveAttribute('contenteditable', 'true');
  const docxHash = hash(readFileSync(docxPath));
  // Exercise the same real Save handler twice. The second export must reject
  // the existing destination rather than silently overwrite or truncate it.
  await page.locator('#edit-save').click();
  await expect(page.locator('#notice')).toContainText('DOCX 导出失败：', { timeout: 35000 });
  await expect(page.locator('#notice')).toContainText('Output already exists; choose a new file');
  await expect(page.locator('.tiptap')).toHaveAttribute('contenteditable', 'true');
  await expect(page.locator('#edit-save')).toBeEnabled();
  assert.equal(hash(readFileSync(docxPath)), docxHash, 'Repeated Save overwrote the DOCX');
  await checkEditor();
  const docx = JSON.parse(execFileSync(python, ['-c', [
    'import sys,json,zipfile',
    'from docx import Document',
    'from docx.oxml.ns import qn',
    'doc=Document(sys.argv[1]); styles=doc.styles._element',
    'defaults=styles.find(qn("w:docDefaults")).find(qn("w:rPrDefault")).find(qn("w:rPr"))',
    'fonts=defaults.find(qn("w:rFonts"))',
    'font_names={key:fonts.get(qn("w:"+key)) for key in ["ascii","hAnsi","eastAsia","cs"]}',
    'assert font_names=={"ascii":"Arial","hAnsi":"Arial","eastAsia":"Microsoft YaHei","cs":"Arial"},font_names',
    'default_size={tag:defaults.find(qn("w:"+tag)).get(qn("w:val")) for tag in ["sz","szCs"]}',
    'assert default_size=={"sz":"22","szCs":"22"},default_size',
    'color=defaults.find(qn("w:color")).get(qn("w:val"));assert color=="000000",color',
    'assert doc.styles["Normal"]._element.get(qn("w:default"))=="1"',
    'heading_sizes={}',
    'for level,size in [(1,"36"),(2,"30"),(3,"26")]:',
    '    style=doc.styles["Heading "+str(level)];rpr=style._element.find(qn("w:rPr"))',
    '    sizes={tag:rpr.find(qn("w:"+tag)).get(qn("w:val")) for tag in ["sz","szCs"]}',
    '    assert sizes=={"sz":size,"szCs":size},sizes',
    '    assert style.base_style.style_id=="Normal"',
    '    heading_sizes[style.style_id]=sizes',
    'paragraphs=[p.text for p in doc.paragraphs]',
    'assert len(paragraphs) in [4,5],paragraphs',
    'assert paragraphs[:4]==["BodyProbe 中文","HeadingOneProbe","HeadingTwoProbe","HeadingThreeProbe"],paragraphs',
    'assert [p.style.style_id for p in doc.paragraphs[:4]]==["Normal","Heading1","Heading2","Heading3"]',
    'if len(paragraphs)==5:',
    '    assert paragraphs[4]=="" and doc.paragraphs[4].style.style_id=="Normal" and not doc.paragraphs[4].runs',
    'for p in doc.paragraphs:',
    '    for run in p.runs:',
    '        assert run._r.rPr is None or all(run._r.rPr.find(qn("w:"+tag)) is None for tag in ["sz","szCs","color"])',
    'sections=[]',
    'for section in doc.sections:',
    '    margins={key:getattr(section,key+"_margin").mm for key in ["top","bottom","left","right"]}',
    '    assert all(abs(value-25.4)<0.01 for value in margins.values()),margins',
    '    assert abs(section.page_width.mm-210)<0.1 and abs(section.page_height.mm-297)<0.1',
    '    sections.append({"width_mm":section.page_width.mm,"height_mm":section.page_height.mm,"margins_mm":margins})',
    'with zipfile.ZipFile(sys.argv[1]) as archive:',
    '    model=json.loads(archive.read("rust-wps/model-v2.json"))',
    '    assert model["version"]==2 and model["document"]["attrs"]["styleProfile"]=="standard-v2"',
    'print(json.dumps({"default_fonts":font_names,"default_size_half_points":default_size,"color":color,"heading_sizes":heading_sizes,"sections":sections,"paragraphs":paragraphs,"embedded_model_version":model["version"]},ensure_ascii=True))',
  ].join('\n'), docxPath], { encoding: 'utf8', windowsHide: true, timeout: 30000 }));
  saveReport('docx-inspection.json', docx);

  await page.locator('#open').click();
  await expect(page.locator('#title')).toHaveText(path.basename(docxPath), { timeout: 35000 });
  await expect(page.locator('#draft-state')).toHaveText('文档已打开');
  const editorAfterReopen = await checkEditor();
  assert.equal(hash(readFileSync(docxPath)), docxHash, 'Worker reopen changed the DOCX');
  const draftBeforePrint = readFileSync(draftPath);
  checkDraft(JSON.parse(draftBeforePrint.toString('utf8')));
  await page.screenshot({ path: path.join(directory, 'editor.png'), fullPage: true });
  await page.locator('#edit-export-pdf').click();
  await page.getByRole('status').filter({ hasText: '已另存 PDF' }).waitFor({ timeout: 70000 });
  const pdf = JSON.parse(execFileSync(python, ['-c', [
    'import sys,json,math',
    'from pypdf import PdfReader',
    'reader=PdfReader(sys.argv[1]);samples=[];pages=[];fill=[0.0,0.0,0.0];stack=[];page_index=0',
    'markers=["BodyProbe","HeadingOneProbe","HeadingTwoProbe","HeadingThreeProbe"]',
    'def operand(op,args,cm,tm):',
    '    global fill',
    '    if op==b"q": stack.append(fill[:])',
    '    elif op==b"Q": fill=stack.pop()',
    '    elif op==b"rg": fill=[float(v) for v in args]',
    '    elif op==b"g": fill=[float(args[0])]*3',
    '    elif op==b"k":',
    '        c,m,y,k=map(float,args);fill=[1-min(1,v+k) for v in [c,m,y]]',
    'def visit(text,cm,tm,font,size):',
    '    for marker in markers:',
    '        if marker in "".join(text.split()):',
    '            scale=math.hypot(tm[2]*cm[0]+tm[3]*cm[2],tm[2]*cm[1]+tm[3]*cm[3])',
    '            x=tm[4]*cm[0]+tm[5]*cm[2]+cm[4]',
    '            y=tm[4]*cm[1]+tm[5]*cm[3]+cm[5]',
    '            samples.append({"marker":marker,"page":page_index,"text":text,"font_size":size,"cm":list(cm),"tm":list(tm),"points":size*scale,"origin_points":[x,y],"fill_rgb":fill[:],"BaseFont":str(font.get("/BaseFont","")) if font else None})',
    'for page_index,page in enumerate(reader.pages):',
    '    fill=[0.0,0.0,0.0];stack=[];resources=[]',
    '    font_map=page["/Resources"].get("/Font",{})',
    '    if hasattr(font_map,"get_object"):font_map=font_map.get_object()',
    '    for key,reference in font_map.items():',
    '        font=reference.get_object()',
    '        resources.append({"key":str(key),"BaseFont":str(font.get("/BaseFont","")),"Subtype":str(font.get("/Subtype","")),"descendant_fonts":[str(item.get_object().get("/BaseFont","")) for item in font.get("/DescendantFonts",[])]})',
    '    text=page.extract_text(visitor_text=visit,visitor_operand_before=operand) or ""',
    '    pages.append({"width_points":float(page.mediabox.width),"height_points":float(page.mediabox.height),"text":text,"font_resources":resources})',
    'print(json.dumps({"pages":pages,"font_samples":samples},ensure_ascii=True))',
  ].join('\n'), output], { encoding: 'utf8', windowsHide: true, timeout: 30000 }));
  // Persist measurements before assertions so an actual rendering discrepancy
  // retains raw matrices/font names; never widen thresholds to hide it.
  saveReport('pdf-inspection.json', pdf);
  assert.equal(pdf.pages.length, 1);
  assert.ok(Math.abs(pdf.pages[0].width_points - 210 * 72 / 25.4) <= 1, JSON.stringify(pdf.pages[0]));
  assert.ok(Math.abs(pdf.pages[0].height_points - 297 * 72 / 25.4) <= 1, JSON.stringify(pdf.pages[0]));
  assert.ok(pdf.pages[0].text.includes('中文'), 'The Chinese body text is missing from extracted PDF text');
  const resources = pdf.pages[0].font_resources;
  assert.ok(resources.some(font => [font.BaseFont, ...font.descendant_fonts].some(name => /arial/i.test(name))), JSON.stringify(resources));
  for (const probe of probes) {
    const samples = pdf.font_samples.filter(sample => sample.marker === probe.marker);
    assert.ok(samples.length > 0, `Missing PDF probe ${probe.marker}`);
    for (const sample of samples) {
      assert.ok(Math.abs(sample.points - probe.points) <= 0.15, JSON.stringify({ expected: probe.points, sample }));
      assert.ok(sample.fill_rgb.length === 3 && sample.fill_rgb.every(value => Math.abs(value) <= 1e-6), JSON.stringify(sample));
      assert.match(sample.BaseFont ?? '', /arial/i, JSON.stringify(sample));
      if (probe.tag === 'p') assert.ok(Math.abs(sample.origin_points[0] - 72) <= 1, JSON.stringify(sample));
    }
  }
  assert.ok(readFileSync(draftPath).equals(draftBeforePrint), 'Printing changed the document draft');
  assert.equal(hash(readFileSync(docxPath)), docxHash, 'Opening or printing changed the DOCX');
  const report = {
    nativeWebView2: true, nativeSaveIpc: true, repeatSaveRejectsOverwrite: true,
    inputViaKeyboardAndStyleDropdown: true, profile, runtime, startup,
    draftVersion: 2, trailingEmptyParagraphs: draft.document.content.length - probes.length,
    noExplicitFontSizeOrTextColor: true, workerReopenPreservesProfile: true,
    printLeavesDraftUnchanged: true, sourceDocxUnchanged: true, docxHash,
    thresholds: { fontSizePoints: 0.15, positionPoints: 1 },
    directory, docxPath, output, editorBeforeReopen, editorAfterReopen, docx, pdf,
    limitation: 'Smoke build with file-picker overrides and isolated draft/profile; no OS dialogs, IME, clipboard, installer or general fidelity acceptance. One-page default style probe; Latin font identity/size, solid black fill and body left margin. Not full Word pagination, shaping or font-substitution equivalence.',
  };
  saveReport('report.json', report);
  console.log(JSON.stringify(report, null, 2));
} catch (error) {
  if (page) {
    const state = await page.evaluate(() => ({ status: document.querySelector('#status')?.textContent,
      notice: document.querySelector('#notice')?.textContent, title: document.querySelector('#title')?.textContent,
      draftState: document.querySelector('#draft-state')?.textContent,
      editor: document.querySelector('.tiptap')?.outerHTML, url: location.href, readyState: document.readyState,
      startupEvents: window.__nativeSmokeStartup ?? [] })).catch(failure => ({ error: String(failure) }));
    saveReport('failure-state.json', state);
    console.error(JSON.stringify(state));
    await page.screenshot({ path: path.join(directory, 'failure.png'), fullPage: true }).catch(() => {});
  }
  throw error;
} finally {
  // Only terminate our owned app tree, never all WebView2/app processes.
  // Kill the tree before disconnecting CDP so WebView children remain owned.
  if (child.pid && child.exitCode === null && child.signalCode === null && !processError) {
    try { execFileSync('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, timeout: 10000 }); }
    catch (error) { log += `\nOwned process cleanup: ${String(error)}\n`; }
  }
  const bounded = async (promise, ms) => {
    let timer;
    try { await Promise.race([promise, new Promise(resolve => { timer = setTimeout(resolve, ms); })]); }
    finally { clearTimeout(timer); }
  };
  if (browser) await bounded(browser.close().catch(() => {}), 5000);
  await bounded(childClosed, 5000);
  if (child.exitCode === null && child.signalCode === null && !processError) {
    child.stdout?.destroy(); child.stderr?.destroy(); child.unref();
    process.exitCode = 1;
    log += '\nFAIL: owned process did not close within cleanup deadline\n';
  }
  saveReport('startup-diagnostics.json', startup);
  console.log(JSON.stringify({ startup }));
  writeFileSync(path.join(directory, 'native-process.log'), log);
  console.log(JSON.stringify({ nativeProcessLog: log, runtime }));
}
