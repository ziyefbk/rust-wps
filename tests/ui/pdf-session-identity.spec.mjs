// Actual main.ts functions with deterministic transport/DOM doubles, no browser.
import { test, expect } from '@playwright/test';
import ts from 'typescript';
import { readFileSync } from 'node:fs';
const source = readFileSync('ui/main.ts', 'utf8');
const ast = ts.createSourceFile('main.ts', source, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TS);
const names = ['clearPdf', 'renderPage', 'showPdf', 'recoverPdfText', 'renderText', 'setMode', 'operation', 'openDocument', 'prepareEditorWorkspace'];
const functions = names.map(name => ast.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name).getText(ast)).join('\n');
const js = ts.transpileModule(functions, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const tick = () => new Promise(resolve => setImmediate(resolve));
const owner = (token = '1', path = 'a.pdf') => ({ session_token: token, name: path, path, inspection: { format: 'pdf', text: 'retained Rust text', bytes: 10, warnings: [] } });
function proxy(extra = {}) { return { numPages: 1, destroyed: 0, async destroy() { this.destroyed++; }, async getPage() { return { getViewport: () => ({ width: 200, height: 300 }), render: () => ({ promise: Promise.resolve(), cancel() {} }), cleanup() {} }; }, ...extra }; }
function loading() { const pending = deferred(); return { ...pending, destroyed: 0, async destroy() { this.destroyed++; pending.reject(new Error('Loading aborted')); } }; }
function harness({ queuedClose = false, current = owner(), pdf = null, invoke = async () => Uint8Array.of(1, 2).buffer, task = loading(), extract = async () => ({ text: 'recovered', complete: true }) } = {}) {
  const elements = new Map(), notices = [], calls = [], loads = [], modes = [], closeEvents = [];
  const el = id => {
    if (!elements.has(id)) {
      const listeners = new Map();
      elements.set(id, { value: id === 'zoom' ? '1' : '', textContent: '', hidden: false, open: false, nodes: [], style: {}, classList: { toggle() {} },
        replaceChildren(...nodes) { this.nodes = nodes; this.textContent = ''; }, append(node) { this.nodes.push(node); }, setAttribute() {}, getContext: () => ({}), focus() {},
        addEventListener(type, fn) { if (!listeners.has(type)) listeners.set(type, new Set()); listeners.get(type).add(fn); },
        removeEventListener(type, fn) { listeners.get(type)?.delete(fn); },
        showModal() { this.open = true; }, close(value) { this.open = false; this.returnValue = value; const dispatch = () => { for (const fn of [...listeners.get('close') ?? []]) fn(); }; if (queuedClose) closeEvents.push(dispatch); else dispatch(); },
      });
    }
    return elements.get(id);
  };
  const document = { createElement: name => ({ ...el(`new-${name}-${Math.random()}`), tag: name }), createTextNode: textContent => ({ textContent }), querySelector: () => el('zoom-wrap') };
  const deps = { current, pdf, el, document, window: { devicePixelRatio: 1 }, invoke: async (...args) => { calls.push(args); return invoke(...args); },
    pdfjs: { PasswordResponses: { INCORRECT_PASSWORD: 2 }, getDocument: args => { loads.push(args); return typeof task === 'function' ? task() : task; } },
    extractPdfText: extract, textSummary: report => report.complete ? 'complete' : 'partial', notice: (...args) => notices.push(args),
  };
  const build = new Function('deps', `
    let {current,pdf,el,document,window,invoke,pdfjs,extractPdfText,textSummary,notice}=deps;
    let workspaceAttempt=null; let busy=false, mode='text', pageNumber=1, renderTask=null, pdfTextSummary=null, stopText=false;
    let pdfLoadingTask=null,pdfLoadAttempt=null,cancelPdfPassword=null;
    const controls=()=>{},pauseAuthoring=()=>{},pauseWorkbook=()=>{},pausePresentation=()=>{};
    const canLeaveAuthoring=async()=>true,closeAuthoring=()=>{},closeWorkbook=()=>{},closePresentation=()=>{},closePresentationEditor=()=>{};
    const openPresentationEditor=()=>{},openPresentation=()=>{},openWorkbook=()=>{},openAuthored=async()=>{};
    ${js}
    return {${names.join(',')}, state:()=>({current,pdf,busy,mode,pdfTextSummary}), set:(value,documentPdf=null)=>{current=value;pdf=documentPdf;}, setToken:value=>{current.session_token=value;}};
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
