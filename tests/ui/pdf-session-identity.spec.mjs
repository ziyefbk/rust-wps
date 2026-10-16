// Actual main.ts functions with deterministic transport/DOM doubles, no browser.
import { test, expect } from '@playwright/test';
import ts from 'typescript';
import { readFileSync } from 'node:fs';
const searchJs = ts.transpileModule(readFileSync('ui/reading-search.ts', 'utf8'), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { findReadingMatches } = await import(`data:text/javascript;base64,${Buffer.from(searchJs).toString('base64')}`);
const source = readFileSync('ui/main.ts', 'utf8');
const ast = ts.createSourceFile('main.ts', source, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TS);
const names = ['pdfTextEvidence', 'pdfSemanticHint', 'clearPdf', 'renderPage', 'showPdf', 'recoverPdfText', 'renderText', 'setMode', 'operation', 'openDocument', 'prepareEditorWorkspace', 'controls', 'currentDisplayedPdfPage'];
const functions = names.map(name => ast.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name).getText(ast)).join('\n');
const changeBinding = ['pdf-ligature-search', 'pdf-whitespace-search', 'extract-pdf-text', 'stop-pdf-text', 'text-view'].map(id => ast.statements.find(node => node.getText(ast).startsWith(`el('${id}').addEventListener`)).getText(ast)).join('\n');
const js = ts.transpileModule(functions + '\n' + changeBinding, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const tick = () => new Promise(resolve => setImmediate(resolve));
const owner = (token = '1', path = 'a.pdf') => ({ session_token: token, name: path, path, inspection: { format: 'pdf', pdf_actual_text: 'not_detected', text: 'retained Rust text', bytes: 10, warnings: [] } });
function proxy(extra = {}) { return { numPages: 1, destroyed: 0, async destroy() { this.destroyed++; }, async getPage(number) { return { pageNumber: number, ref: { num: number * 2 + 1, gen: 0 }, rotate: 0, getViewport: () => ({ width: 200, height: 300 }), render: () => ({ promise: Promise.resolve(), cancel() {} }), cleanup() {} }; }, ...extra }; }
function loading() { const pending = deferred(); return { ...pending, destroyed: 0, async destroy() { this.destroyed++; pending.reject(new Error('Loading aborted')); } }; }
function harness({ queuedClose = false, current = owner(), pdf = null, invoke = async () => Uint8Array.of(1, 2).buffer, task = loading(), extract = async () => ({ text: 'recovered', complete: true }) } = {}) {
  const elements = new Map(), notices = [], calls = [], loads = [], modes = [], closeEvents = [];
  const el = id => {
    if (!elements.has(id)) {
      const listeners = new Map();
      elements.set(id, { value: id === 'zoom' ? '1' : '', textContent: '', hidden: false, open: false, checked: false, disabled: false, nodes: [], style: {}, classList: { toggle() {} },
        querySelectorAll: () => [], dispatch(type) { return Promise.all([...listeners.get(type) ?? []].map(fn => fn())); }, replaceChildren(...nodes) { this.nodes = nodes; this.textContent = ''; }, append(node) { this.nodes.push(node); }, setAttribute() {}, getContext: () => ({}), focus() {},
        addEventListener(type, fn) { if (!listeners.has(type)) listeners.set(type, new Set()); listeners.get(type).add(fn); },
        removeEventListener(type, fn) { listeners.get(type)?.delete(fn); },
        showModal() { this.open = true; }, close(value) { this.open = false; this.returnValue = value; const dispatch = () => { for (const fn of [...listeners.get('close') ?? []]) fn(); }; if (queuedClose) closeEvents.push(dispatch); else dispatch(); },
      });
    }
    return elements.get(id);
  };
  const document = { createElement: name => ({ ...el(`new-${name}-${Math.random()}`), tag: name }), createTextNode: textContent => ({ textContent }), querySelector: () => el('zoom-wrap') };
  const deps = { current, pdf, findReadingMatches, el, document, window: { devicePixelRatio: 1 }, invoke: async (...args) => { calls.push(args); return invoke(...args); },
    pdfjs: { PasswordResponses: { INCORRECT_PASSWORD: 2 }, getDocument: args => { loads.push(args); return typeof task === 'function' ? task() : task; } },
    extractPdfText: extract, textSummary: report => report.complete ? 'complete' : 'partial', notice: (...args) => notices.push(args),
  };
  const build = new Function('deps', `
    let {current,pdf,findReadingMatches,el,document,window,invoke,pdfjs,extractPdfText,textSummary,notice}=deps;
    let pdfTextAttempt=null; let workspaceAttempt=null; let busy=false, mode='text', pageNumber=1, renderTask=null, pdfTextSummary=null, stopText=false;
    let pdfLoadingTask=null,pdfLoadAttempt=null,cancelPdfPassword=null,displayedPdfPage=null,pdfRenderAttempt=null;
    const isTauri=()=>true; let pdfRotationPending=false; const pauseAuthoring=()=>{},pauseWorkbook=()=>{},pausePresentation=()=>{};
    const canLeaveAuthoring=async()=>true,closeAuthoring=()=>{},closeWorkbook=()=>{},closePresentation=()=>{},closePresentationEditor=()=>{};
    const openPresentationEditor=()=>{},openPresentation=()=>{},openWorkbook=()=>{},openAuthored=async()=>{};
    ${js}
    return {${names.join(',')}, state:()=>({current,pdf,busy,mode,pdfTextSummary}), stop:()=>{stopText=true;}, set:(value,documentPdf=null)=>{current=value;pdf=documentPdf;}, setToken:value=>{current.session_token=value;}};
  `);
  return { ...build(deps), el, calls, loads, notices, modes, flushClose: () => { while (closeEvents.length) closeEvents.shift()(); } };
}

test('PDF delivery echoes exact string session token without JS number conversion', async () => {
  const task = loading(), app = harness({ current: owner('9007199254740993'), task });
  const run = app.showPdf(); await tick(); task.resolve(proxy()); await run;
  expect(app.calls).toEqual([['read_current_pdf', { sessionToken: '9007199254740993' }]]);
  expect([...app.loads[0].data]).toEqual([1, 2]); expect(app.state().mode).toBe('page');
});

test('PDF source mismatch never reaches PDF.js and operation restores controls retaining inspection', async () => {
  const original = owner(), app = harness({ current: original, invoke: async () => { throw new Error('PDF 源文件已改变，请重新打开文档后再预览'); } });
  app.el('title').textContent = original.name;
  await app.operation('opening', app.showPdf);
  expect(app.loads).toHaveLength(0); expect(app.state().current).toBe(original); expect(original.inspection.text).toBe('retained Rust text');
  expect(app.el('title').textContent).toBe('a.pdf'); expect(app.state().busy).toBe(false); expect(app.state().mode).toBe('text'); expect(app.notices.at(-1)[0]).toContain('重新打开');
});

test('PDF missing or numeric session tokens fail before transport', async () => {
  for (const token of [undefined, null, 1, 9007199254740992, '', '01', '1e3', '-1']) {
    const app = harness({ current: owner(token) }); if (token === undefined) delete app.state().current.session_token;
    await expect(app.showPdf()).rejects.toThrow('会话身份无效'); expect(app.calls).toHaveLength(0);
  }
});

test('PDF stale read completion is discarded before loading, including same path reopen', async () => {
  for (const mutateToken of [false, true]) {
    const response = deferred(), app = harness({ invoke: () => response.promise }); const run = app.showPdf(); await tick();
    if (mutateToken) app.setToken('2'); else app.set(owner('2'));
    response.resolve(Uint8Array.of(7).buffer); await run;
    expect(app.loads).toHaveLength(0); expect(app.state().pdf).toBeNull(); expect(app.notices).toHaveLength(0);
  }
});

test('PDF stale successful load is destroyed without replacing newer document or controls', async () => {
  const task = loading(); task.destroy = async function () { this.destroyed++; }; // deliberately late successful transport
  const app = harness({ task }), run = app.showPdf(); await tick();
  const next = owner('2'), nextPdf = proxy(); app.set(next, nextPdf);
  const obsolete = proxy(); task.resolve(obsolete); await run;
  expect(obsolete.destroyed).toBe(1); expect(app.state().current).toBe(next); expect(app.state().pdf).toBe(nextPdf); expect(app.notices).toHaveLength(0);
});

test('PDF stale loading errors and stale read errors cannot change new view', async () => {
  const task = loading(), app = harness({ task }), run = app.showPdf(); await tick(); app.set(owner('2'), proxy()); task.reject(new Error('old failure')); await run;
  expect(app.notices).toHaveLength(0); expect(app.state().pdf).not.toBeNull();
  const pending = deferred(), other = harness({ invoke: () => pending.promise }), read = other.showPdf(); await tick(); other.set(owner('2'), proxy()); pending.reject(new Error('old read')); await read;
  expect(other.notices).toHaveLength(0);
});

test('PDF password retry stays local and cancel permits unchanged-source retry', async () => {
  const first = loading(), second = loading(), tasks = [first, second], app = harness({ task: () => tasks.shift() });
  const run = app.showPdf(); await tick(); const passwords = [];
  first.onPassword(value => passwords.push(value), 1); app.el('pdf-password-input').value = 'wrong-password'; app.el('pdf-password').close('unlock');
  first.onPassword(value => passwords.push(value), 2); expect(app.el('pdf-password-hint').textContent).toContain('密码不正确'); expect(app.el('pdf-password-input').value).toBe('');
  app.el('pdf-password').close('cancel'); await expect(run).rejects.toThrow('已取消解锁'); expect(app.state().pdf).toBeNull();
  const retry = app.showPdf(); await tick(); second.resolve(proxy()); await retry;
  expect(passwords).toEqual(['wrong-password']); expect(JSON.stringify(app.calls)).not.toContain('password'); expect(app.calls).toHaveLength(2); expect(app.calls[0]).toEqual(app.calls[1]);
});

test('PDF password cancel then source change rejects until explicit reopen gets a new token', async () => {
  const first = loading(), third = loading(); let reads = 0;
  const app = harness({ task: () => reads === 1 ? first : third, invoke: async () => { if (++reads === 2) throw new Error('PDF 源文件已改变，请重新打开文档后再预览'); return Uint8Array.of(reads).buffer; } });
  const initial = app.showPdf(); await tick(); first.onPassword(() => {}, 1); app.el('pdf-password').close('cancel'); await expect(initial).rejects.toThrow('已取消');
  await expect(app.showPdf()).rejects.toThrow('重新打开'); expect(app.loads).toHaveLength(1);
  app.set(owner('2')); const reopened = app.showPdf(); await tick(); third.resolve(proxy()); await reopened;
  expect(app.calls.at(-1)[1].sessionToken).toBe('2'); expect(app.loads).toHaveLength(2);
});

test('PDF clearing a pending password removes callbacks and destroys obsolete task', async () => {
  const task = loading(), app = harness({ task }), run = app.showPdf(); await tick(); let updated = false;
  task.onPassword(() => { updated = true; }, 1); app.el('pdf-password-input').value = 'secret'; app.set(owner('2')); await app.clearPdf(); await run;
  expect(app.el('pdf-password').open).toBe(false); expect(app.el('pdf-password-input').value).toBe('');
  app.el('pdf-password').close('unlock'); expect(updated).toBe(false); expect(app.notices).toHaveLength(0);
});

test('PDF stale extraction cannot mutate another document, progress or finally controls', async () => {
  const pending = deferred(); let progress, cancelled;
  const original = owner(), app = harness({ current: original, pdf: proxy(), extract: async (_, stop, update) => { progress = update; cancelled = stop; return pending.promise; } });
  const run = app.recoverPdfText(); await tick(); const next = owner('2'); app.set(next, proxy()); app.el('status').textContent = 'new status'; app.el('stop-pdf-text').hidden = false;
  progress(1); expect(cancelled()).toBe(true); pending.resolve({ text: 'obsolete text', complete: true }); await run;
  expect(next.inspection.text).toBe('retained Rust text'); expect(original.inspection.text).toBe('retained Rust text'); expect(app.el('status').textContent).toBe('new status'); expect(app.el('stop-pdf-text').hidden).toBe(false); expect(app.notices).toHaveLength(0);
});

test('PDF stale page acquisition cleans page without replacing the canvas', async () => {
  const pending = deferred(); let cleaned = 0;
  const app = harness({ pdf: proxy({ getPage: () => pending.promise }) }); const run = app.renderPage(); await tick(); app.set(owner('2'), proxy());
  app.el('pdf-pages').nodes = ['new canvas']; pending.resolve({ cleanup() { cleaned++; } }); await run;
  expect(cleaned).toBe(1); expect(app.el('pdf-pages').nodes).toEqual(['new canvas']);
});

test('PDF cancelled and failed open preserve committed frontend document and loaded PDF', async () => {
  for (const fail of [false, true]) {
    const original = owner(), originalPdf = proxy(), app = harness({ current: original, pdf: originalPdf, invoke: async () => { if (fail) throw new Error('worker failed'); return null; } });
    await app.openDocument(); expect(app.state().current).toBe(original); expect(app.state().pdf).toBe(originalPdf); expect(originalPdf.destroyed).toBe(0); expect(app.state().busy).toBe(false);
  }
});

test('PDF open invalidates old owner before awaiting old document destruction', async () => {
  const destruction = deferred(), extraction = deferred(), previous = owner(), next = owner('2'); next.inspection.format = 'docx';
  const app = harness({ current: previous, pdf: proxy({ destroy: () => destruction.promise }), invoke: async () => next, extract: () => extraction.promise });
  const recovering = app.recoverPdfText(), opening = app.openDocument(); await tick();
  expect(app.state().current).toBe(next); expect(app.state().pdf).toBeNull(); extraction.resolve({ text: 'old extraction', complete: true }); await recovering;
  expect(next.inspection.text).toBe('retained Rust text'); destruction.resolve(); await opening; expect(app.state().current).toBe(next);
});

test('PDF newer same-owner load disposes older password callback before new prompt', async () => {
  const first = loading(), second = loading(), tasks = [first, second], app = harness({ task: () => tasks.shift() });
  const earlier = app.showPdf(); await tick(); first.onPassword(() => {}, 1);
  const newer = app.showPdf(); await tick(); second.onPassword(() => {}, 1); await earlier;
  expect(first.destroyed).toBeGreaterThan(0); expect(app.el('pdf-password').open).toBe(true);
  second.resolve(proxy()); await newer; expect(app.state().mode).toBe('page');
});

test('PDF obsolete editor cleanup cannot hide a newer opened selection', async () => {
  const destruction = deferred(), next = owner('2'); next.inspection.format = 'docx';
  const app = harness({ pdf: proxy({ destroy: () => destruction.promise }), invoke: async () => next });
  const preparing = app.prepareEditorWorkspace(); await tick();
  await app.openDocument(); expect(app.state().current).toBe(next); app.el('document').hidden = false;
  destruction.resolve(); await expect(preparing).rejects.toThrow('会话已更新'); expect(app.el('document').hidden).toBe(false);
});

test('PDF late open response cannot replace a newer editor selection', async () => {
  const pending = deferred(), app = harness({ invoke: () => pending.promise });
  const opening = app.openDocument(); await tick(); await app.prepareEditorWorkspace(); pending.resolve(owner('2')); await opening;
  expect(app.state().current).toBeNull(); expect(app.loads).toHaveLength(0);
});


test('PDF queued old dialog close event cannot consume a newer password prompt listener', async () => {
  const first = loading(), second = loading(), tasks = [first, second], app = harness({ queuedClose: true, task: () => tasks.shift() });
  const earlier = app.showPdf(); await tick(); first.onPassword(() => {}, 1);
  const newer = app.showPdf(); await tick(); let password;
  second.onPassword(value => { password = value; }, 1); await earlier;
  app.flushClose(); expect(app.el('pdf-password').open).toBe(true); expect(second.destroyed).toBe(0);
  app.el('pdf-password-input').value = 'correct local password'; app.el('pdf-password').close('unlock'); app.flushClose();
  expect(password).toBe('correct local password'); expect(app.el('pdf-password-input').value).toBe('');
  second.resolve(proxy()); await newer; expect(app.state().mode).toBe('page');
});

test('PDF ligature toggle re-renders actual original marks and count through its change binding', () => {
  const current = owner(); current.inspection.text = '<img onerror="ﬃ">😀 ffi';
  const app = harness({ current }); app.el('search').value = 'ffi'; app.renderText();
  const marks = () => app.el('text-content').nodes.filter(node => node.tag === 'mark').map(node => node.textContent);
  expect(marks()).toEqual(['ffi']);
  app.el('pdf-ligature-search').checked = true; app.el('pdf-ligature-search').dispatch('change');
  expect(marks()).toEqual(['ﬃ', 'ffi']); expect(app.el('match-count').textContent).toBe('2 处');
  expect(app.el('text-content').nodes.map(node => node.textContent).join('')).toBe(current.inspection.text);
  expect(app.el('text-content').nodes.every(node => !node.tag || node.tag === 'mark')).toBe(true);
  app.el('pdf-ligature-search').checked = false; app.el('pdf-ligature-search').dispatch('change');
  expect(marks()).toEqual(['ffi']); expect(app.el('match-count').textContent).toBe('1 处');
});

test('PDF ligature control is PDF-only and disabled on pages, empty text, and busy operations without reset', async () => {
  const app = harness(); app.controls(); expect(app.el('pdf-ligature-search-control').hidden).toBe(false);
  expect(app.el('pdf-ligature-search').disabled).toBe(false);
  app.el('pdf-ligature-search').checked = true; app.setMode('page');
  expect(app.el('pdf-ligature-search').disabled).toBe(true); expect(app.el('pdf-ligature-search-hint').hidden).toBe(true);
  app.setMode('text'); expect(app.el('pdf-ligature-search').disabled).toBe(false);
  await app.operation('busy', async () => { expect(app.el('pdf-ligature-search').disabled).toBe(true); });
  expect(app.el('pdf-ligature-search').checked).toBe(true);
  app.state().current.inspection.text = ''; app.controls(); expect(app.el('pdf-ligature-search').disabled).toBe(true);
  for (const format of ['doc', 'docx', 'xlsx', 'pptx']) {
    const next = owner(); next.inspection.format = format; next.inspection.text = 'ﬃ ffi'; app.set(next); app.controls();
    expect(app.el('pdf-ligature-search-control').hidden).toBe(true); expect(app.el('pdf-ligature-search').disabled).toBe(true);
    app.el('search').value = 'ffi'; app.renderText();
    expect(app.el('text-content').nodes.filter(node => node.tag === 'mark').map(node => node.textContent)).toEqual(['ffi']);
  }
});

test('PDF ligature preference and original matches survive canceled or failed opens', async () => {
  for (const fail of [false, true]) {
    const current = owner(); current.inspection.text = 'oﬃce';
    const app = harness({ current, invoke: async () => { if (fail) throw new Error('failed'); return null; } });
    app.el('pdf-ligature-search').checked = true; app.el('search').value = 'office'; app.renderText();
    await app.openDocument();
    expect(app.el('pdf-ligature-search').checked).toBe(true); expect(app.el('search').value).toBe('office');
    expect(app.el('text-content').nodes.filter(node => node.tag === 'mark').map(node => node.textContent)).toEqual(['oﬃce']);
  }
});

test('PDF ligature preference resets on a committed same-path reopen and on editor reset', async () => {
  const next = owner('2'), app = harness({ invoke: async command => command === 'open_document' ? next : Uint8Array.of(1).buffer, task: { promise: Promise.resolve(proxy()), async destroy() {} } });
  app.el('pdf-ligature-search').checked = true; app.el('search').value = 'old';
  await app.openDocument(); expect(app.state().current).toBe(next);
  expect(app.el('pdf-ligature-search').checked).toBe(false); expect(app.el('search').value).toBe('');
  app.el('pdf-ligature-search').checked = true; await app.prepareEditorWorkspace();
  expect(app.el('pdf-ligature-search').checked).toBe(false); expect(app.el('pdf-ligature-search-control').hidden).toBe(true);
});

test('PDF ligature preference survives extraction cancellation and searches only current accepted text', async () => {
  const pending = deferred(); let canceled;
  const current = owner(); current.inspection.text = 'oﬃce';
  const app = harness({ current, pdf: proxy(), extract: async (_, stop) => { canceled = stop; return pending.promise; } });
  app.el('pdf-ligature-search').checked = true; app.el('search').value = 'office';
  const run = app.recoverPdfText(); app.stop(); expect(canceled()).toBe(true);
  pending.resolve({ text: '', complete: false }); await run;
  expect(app.el('pdf-ligature-search').checked).toBe(true); expect(app.state().current.inspection.text).toBe('oﬃce');
  expect(app.el('text-content').nodes.filter(node => node.tag === 'mark').map(node => node.textContent)).toEqual(['oﬃce']);
});

test('PDF whitespace change binding composes with ligatures and preserves safe original marks', () => {
  const current = owner(); current.inspection.text = '<img onerror="oﬃce\r\n work">😀';
  const app = harness({ current }); app.el('search').value = 'office work';
  const marks = () => app.el('text-content').nodes.filter(node => node.tag === 'mark').map(node => node.textContent);
  app.renderText(); expect(marks()).toEqual([]);
  app.el('pdf-whitespace-search').checked = true; app.el('pdf-whitespace-search').dispatch('change'); expect(marks()).toEqual([]);
  app.el('pdf-ligature-search').checked = true; app.el('pdf-ligature-search').dispatch('change'); expect(marks()).toEqual(['oﬃce\r\n work']);
  expect(app.el('match-count').textContent).toBe('1 处');
  expect(app.el('text-content').nodes.map(node => node.textContent).join('')).toBe(current.inspection.text);
  expect(app.el('text-content').nodes.every(node => !node.tag || node.tag === 'mark')).toBe(true);
  app.el('pdf-whitespace-search').checked = false; app.el('pdf-whitespace-search').dispatch('change'); expect(marks()).toEqual([]);
});

test('PDF whitespace controls are gated by format, page view, empty text and busy state', async () => {
  const app = harness(); app.controls(); expect(app.el('pdf-whitespace-search-control').hidden).toBe(false); expect(app.el('pdf-whitespace-search').disabled).toBe(false);
  app.el('pdf-whitespace-search').checked = true; app.setMode('page'); expect(app.el('pdf-whitespace-search').disabled).toBe(true); expect(app.el('pdf-whitespace-search-hint').hidden).toBe(true);
  app.setMode('text'); expect(app.el('pdf-whitespace-search').disabled).toBe(false);
  await app.operation('busy', async () => { expect(app.el('pdf-whitespace-search').disabled).toBe(true); }); expect(app.el('pdf-whitespace-search').checked).toBe(true);
  app.state().current.inspection.text = ''; app.controls(); expect(app.el('pdf-whitespace-search').disabled).toBe(true);
  for (const format of ['doc', 'docx', 'xlsx', 'pptx']) {
    const next = owner(); next.inspection.format = format; next.inspection.text = 'a\nb'; app.set(next); app.controls();
    expect(app.el('pdf-whitespace-search-control').hidden).toBe(true); expect(app.el('pdf-whitespace-search').disabled).toBe(true);
    app.el('search').value = 'a b'; app.renderText(); expect(app.el('match-count').textContent).toBe('0 处');
  }
});

test('PDF whitespace preference and matches survive canceled and failed opens', async () => {
  for (const fail of [false, true]) {
    const current = owner(); current.inspection.text = 'a\nb';
    const app = harness({ current, invoke: async () => { if (fail) throw new Error('failed'); return null; } });
    app.el('pdf-whitespace-search').checked = true; app.el('search').value = 'a b'; app.renderText(); await app.openDocument();
    expect(app.el('pdf-whitespace-search').checked).toBe(true);
    expect(app.el('text-content').nodes.filter(node => node.tag === 'mark').map(node => node.textContent)).toEqual(['a\nb']);
  }
});

test('PDF whitespace preference resets on successful same-path open and editor workspace reset', async () => {
  const next = owner('2'), app = harness({ invoke: async command => command === 'open_document' ? next : Uint8Array.of(1).buffer, task: { promise: Promise.resolve(proxy()), async destroy() {} } });
  app.el('pdf-whitespace-search').checked = true; await app.openDocument(); expect(app.el('pdf-whitespace-search').checked).toBe(false);
  app.el('pdf-whitespace-search').checked = true; await app.prepareEditorWorkspace(); expect(app.el('pdf-whitespace-search').checked).toBe(false); expect(app.el('pdf-whitespace-search-control').hidden).toBe(true);
});

test('PDF whitespace preference survives canceled extraction without using discarded text', async () => {
  const pending = deferred(), current = owner(); current.inspection.text = 'a\nb';
  const app = harness({ current, pdf: proxy(), extract: async () => pending.promise });
  app.el('pdf-whitespace-search').checked = true; app.el('search').value = 'a b';
  const run = app.recoverPdfText(); app.stop(); pending.resolve({ text: '', complete: false }); await run;
  expect(app.el('pdf-whitespace-search').checked).toBe(true); expect(app.state().current.inspection.text).toBe('a\nb');
  expect(app.el('text-content').nodes.filter(node => node.tag === 'mark').map(node => node.textContent)).toEqual(['a\nb']);
});

// Per-source ActualText transport evidence is independent of PDF.js's page
// completion. Exercise production entry points and DOM bindings, not a copied
// replacement policy.
test('PDF semantic evidence treats absent and malformed transport values as unknown', () => {
  const app = harness();
  for (const evidence of [undefined, null, '', 'APPLIED', true, 1, { applied: true }]) {
    const inspection = { format: 'pdf', pdf_actual_text: evidence };
    expect(app.pdfTextEvidence(inspection)).toBe('unknown');
    expect(app.pdfSemanticHint(inspection)).toContain('尚未完成核对');
  }
  for (const evidence of ['not_detected', 'applied', 'unsupported', 'unknown']) {
    expect(app.pdfTextEvidence({ format: 'pdf', pdf_actual_text: evidence })).toBe(evidence);
  }
  expect(app.pdfTextEvidence({ format: 'docx', pdf_actual_text: 'applied' })).toBeNull();
  expect(app.pdfSemanticHint({ format: 'pdf', pdf_actual_text: 'not_detected' })).toBeNull();
});

test('PDF open consumes applied backend Chinese text and exact search without PDF.js overwrite', async () => {
  const next = owner('9007199254740994', 'same.pdf');
  next.inspection.text = '会议纪要：文件保存与用户反馈'; next.inspection.pdf_actual_text = 'applied';
  let extracted = 0;
  const app = harness({ invoke: async command => command === 'open_document' ? next : Uint8Array.of(1).buffer,
    task: { promise: Promise.resolve(proxy()), async destroy() {} }, extract: async () => { extracted++; return { text: '⽂件保存与⽤户反馈', complete: true }; } });
  await app.openDocument();
  expect(extracted).toBe(0); expect(app.state().current).toBe(next);
  expect(app.el('extract-pdf-text').disabled).toBe(true);
  expect(app.el('pdf-semantic-hint').textContent).toContain('受支持的 ActualText');
  await app.el('text-view').dispatch('click'); app.el('search').value = '用户反馈'; app.renderText();
  expect(app.el('text-content').nodes.filter(node => node.tag === 'mark').map(node => node.textContent)).toEqual(['用户反馈']);
  for (let repeat = 0; repeat < 3; repeat++) await app.el('extract-pdf-text').dispatch('click');
  expect(extracted).toBe(0); expect(next.inspection.text).toBe('会议纪要：文件保存与用户反馈');
  expect(app.el('text-content').nodes.filter(node => node.tag === 'mark').map(node => node.textContent)).toEqual(['用户反馈']);
});

test('PDF applied intentionally empty text never falls back to font text or a scanning claim', async () => {
  const current = owner(); current.inspection.pdf_actual_text = 'applied'; current.inspection.text = '';
  current.inspection.text_error = 'retained diagnostic'; let extracted = 0;
  const app = harness({ current, pdf: proxy(), extract: async () => { extracted++; return { text: 'hidden underlying glyph text', complete: true }; } });
  await app.showPdf(); await app.recoverPdfText(); app.renderText(); app.controls();
  expect(extracted).toBe(0); expect(current.inspection.text).toBe('');
  expect(current.inspection.text_error).toBe('retained diagnostic');
  expect(app.el('text-content').textContent).toContain('源 ActualText 提取后的正文为空');
  expect(app.el('text-content').textContent).not.toContain('扫描');
  expect(app.el('search').disabled).toBe(true); expect(app.el('extract-pdf-text').disabled).toBe(true);
});

for (const evidence of ['unsupported', 'unknown', undefined]) {
  test(`PDF ${String(evidence)} evidence allows provisional text but keeps warning and error through repeated completion`, async () => {
    const current = owner(); current.inspection.pdf_actual_text = evidence;
    current.inspection.text = ''; current.inspection.text_error = 'Original semantic/parser refusal';
    const app = harness({ current, pdf: proxy(), extract: async () => ({ text: 'provisional ⽂本', complete: true }) });
    for (let repeat = 0; repeat < 2; repeat++) {
      await app.el('extract-pdf-text').dispatch('click');
      expect(current.inspection.text).toBe('provisional ⽂本');
      expect(current.inspection.pdf_actual_text).toBe(evidence);
      expect(current.inspection.text_error).toBe('Original semantic/parser refusal');
      expect(app.state().pdfTextSummary).toContain('临时字体正文');
      expect(app.el('pdf-semantic-hint').hidden).toBe(false);
      expect(app.el('pdf-semantic-hint').textContent).toContain('不能保证搜索或复制');
      expect(app.el('extract-pdf-text').disabled).toBe(false);
    }
    await app.el('text-view').dispatch('click'); expect(app.notices.at(-1)[0]).toContain('临时字体正文');
    app.el('search').value = '⽂本'; app.renderText(); expect(app.el('match-count').textContent).toBe('1 处');
    expect(current.inspection.text_error).toBe('Original semantic/parser refusal');
  });
}

test('PDF not_detected keeps ordinary successful recovery and error clearing', async () => {
  const current = owner(); current.inspection.text = ''; current.inspection.text_error = 'Extraction failed';
  const app = harness({ current, pdf: proxy() });
  await app.el('extract-pdf-text').dispatch('click');
  expect(current.inspection.text).toBe('recovered'); expect(current.inspection.text_error).toBeUndefined();
  expect(current.inspection.pdf_actual_text).toBe('not_detected');
  expect(app.state().pdfTextSummary).toBe('complete'); expect(app.el('pdf-semantic-hint').hidden).toBe(true);
  expect(app.el('extract-pdf-text').disabled).toBe(false);
});

test('PDF unsupported canceled retry keeps prior accepted text and its semantic refusal', async () => {
  const current = owner(); current.inspection.pdf_actual_text = 'unsupported'; current.inspection.text_error = 'Unsupported ActualText';
  const pending = deferred(); let canceled;
  const app = harness({ current, pdf: proxy(), extract: async (_, stop) => { canceled = stop; return pending.promise; } });
  const run = app.el('extract-pdf-text').dispatch('click'); await app.el('stop-pdf-text').dispatch('click');
  expect(canceled()).toBe(true); pending.resolve({ text: '', complete: false, stopped: true }); await run;
  expect(current.inspection.text).toBe('retained Rust text'); expect(current.inspection.text_error).toBe('Unsupported ActualText');
  expect(app.el('stop-pdf-text').hidden).toBe(true); expect(app.el('pdf-semantic-hint').hidden).toBe(false);
  expect(app.notices.at(-1)[0]).toContain('超出支持范围'); expect(app.state().busy).toBe(false);
});

test('PDF extraction rejects late same-owner session changes before text, progress, summary and controls', async () => {
  const pending = deferred(); let canceled, progress;
  const current = owner(); current.inspection.pdf_actual_text = 'unsupported';
  const app = harness({ current, pdf: proxy(), extract: async (_, stop, update) => { canceled = stop; progress = update; return pending.promise; } });
  const run = app.recoverPdfText(); app.setToken('2'); app.el('status').textContent = 'new session';
  app.el('stop-pdf-text').hidden = false; progress(1);
  expect(canceled()).toBe(true); pending.resolve({ text: 'late font text', complete: true }); await run;
  expect(current.inspection.text).toBe('retained Rust text'); expect(app.el('status').textContent).toBe('new session');
  expect(app.state().pdfTextSummary).toBeNull(); expect(app.notices).toHaveLength(0); expect(app.el('stop-pdf-text').hidden).toBe(false);
});

test('PDF newer extraction attempt owns repeated same-source completion and stop controls', async () => {
  const first = deferred(), second = deferred(); const pending = [first, second]; const callbacks = [];
  const app = harness({ pdf: proxy(), extract: async (_, stop, progress) => { callbacks.push({ stop, progress }); return pending.shift().promise; } });
  const earlier = app.recoverPdfText(), newer = app.recoverPdfText();
  expect(callbacks[0].stop()).toBe(true); expect(callbacks[1].stop()).toBe(false);
  app.el('status').textContent = 'new extraction'; callbacks[0].progress(1);
  first.resolve({ text: 'obsolete', complete: true }); await earlier;
  expect(app.state().current.inspection.text).toBe('retained Rust text'); expect(app.el('status').textContent).toBe('new extraction');
  expect(app.el('stop-pdf-text').hidden).toBe(false); expect(app.notices).toHaveLength(0);
  second.resolve({ text: 'latest', complete: true }); await newer;
  expect(app.state().current.inspection.text).toBe('latest'); expect(app.el('stop-pdf-text').hidden).toBe(true);
});

test('PDF applied evidence appearing before an asynchronous provisional result blocks its commit', async () => {
  const pending = deferred(), current = owner(); current.inspection.pdf_actual_text = 'unknown';
  const app = harness({ current, pdf: proxy(), extract: () => pending.promise });
  const run = app.recoverPdfText(); current.inspection.pdf_actual_text = 'applied'; current.inspection.text = '真实语义';
  pending.resolve({ text: 'wrong glyph text', complete: true }); await run;
  expect(current.inspection.text).toBe('真实语义'); expect(app.state().pdfTextSummary).toBeNull();
  expect(app.notices.at(-1)[0]).toContain('受支持的 ActualText');
});

test('PDF semantic warning follows committed source identity and survives canceled or failed opens', async () => {
  for (const fails of [false, true]) {
    const current = owner(); current.inspection.pdf_actual_text = 'unsupported';
    const app = harness({ current, pdf: proxy(), invoke: async () => { if (fails) throw new Error('failed open'); return null; } });
    app.controls(); const message = app.el('pdf-semantic-hint').textContent;
    await app.openDocument(); expect(app.el('pdf-semantic-hint').textContent).toBe(message); expect(app.state().current).toBe(current);
  }
  const next = owner('2'); const app = harness({ invoke: async command => command === 'open_document' ? next : Uint8Array.of(1).buffer,
    task: { promise: Promise.resolve(proxy()), async destroy() {} } });
  app.state().current.inspection.pdf_actual_text = 'unsupported'; app.controls();
  expect(app.el('pdf-semantic-hint').hidden).toBe(false); await app.openDocument();
  expect(app.state().current).toBe(next); expect(app.el('pdf-semantic-hint').hidden).toBe(true);
  expect(app.state().pdfTextSummary).toBeNull();
  await app.prepareEditorWorkspace(); expect(app.el('pdf-semantic-hint').hidden).toBe(true);
});
