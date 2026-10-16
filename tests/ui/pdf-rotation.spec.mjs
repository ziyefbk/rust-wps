// Execute production main.ts and the production native close callback with
// deterministic DOM/IPC doubles. This is not browser or native IPC acceptance.
import { test, expect } from '@playwright/test';
import ts from 'typescript';
import { readFileSync } from 'node:fs';
const source = readFileSync('ui/main.ts', 'utf8');
const ast = ts.createSourceFile('main.ts', source, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TS);
const names = ['pdfTextEvidence', 'pdfSemanticHint', 'notice', 'controls', 'operation', 'rotateCurrentPdf', 'blockPdfRotationLeave', 'canLeaveWorkspace', 'protectPdfRotationUnload', 'currentDisplayedPdfPage', 'renderPage', 'clearPdf'];
const actual = names.map(name => {
  const node = ast.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
  if (!node) throw new Error(`Missing production function ${name}`);
  return node.getText(ast);
}).join('\n');
const transpile = code => ts.transpileModule(code, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText;
const authorSource = readFileSync('ui/authoring.ts', 'utf8');
const authorAst = ts.createSourceFile('authoring.ts', authorSource, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TS);
const leaveNode = authorAst.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === 'canLeaveAuthoring');
let closeCallback;
function visit(node) {
  if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === 'onCloseRequested') closeCallback = node.arguments[0];
  ts.forEachChild(node, visit);
}
visit(authorAst);
if (!closeCallback) throw new Error('Missing production native close callback');
const clickBinding = ast.statements.find(node => node.getText(ast).startsWith("el('rotate-pdf-page').addEventListener"));
if (!clickBinding) throw new Error('Missing production rotation click binding');
const initAuthoring = ast.statements.find(node => ts.isExpressionStatement(node) && ts.isCallExpression(node.expression) && node.expression.expression.getText(ast) === 'initializeAuthoring').expression;
const beforeLeaveBinding = initAuthoring.arguments[2].getText(ast), finalCloseBinding = initAuthoring.arguments[3].getText(ast);
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const tick = () => new Promise(resolve => setImmediate(resolve));
const owner = (token = '9007199254740993') => ({ session_token: token, path: '/source.pdf', name: 'source.pdf', inspection: { format: 'pdf', text: 'source text' } });
function page(extra = {}) { return { pageNumber: 2, ref: { num: 37, gen: 4 }, rotate: 270, cleaned: 0, getViewport: () => ({width: 200, height: 300}), render: () => ({promise: Promise.resolve(), cancel() {}}), cleanup() { this.cleaned++; }, ...extra }; }
function proxy(selected = page(), extra = {}) { return { numPages: 3, async getPage(number) { this.requestedPage = number; return selected; }, ...extra }; }
function harness({ current = owner(), pdf = proxy(), number = 2, desktop = true, mode = 'page', invoke = async () => ({ path: '/rotated.pdf', page: 2, pages: 3, rotation: 0 }), leavePresentation = async () => true, leaveWorkbook = async () => true } = {}) {
  const elements = new Map(), calls = [], pauses = [], notices = [], events = [];
  const el = id => {
    if (!elements.has(id)) {
      const listeners = new Map(), classes = new Set();
      const element = { id, hidden: false, disabled: false, value: id === 'zoom' ? '1' : '', style: {}, getContext: () => ({}), setAttribute() {}, textContent: '', nodes: [], dataset: {},
        classList: { toggle(name, on) { if (on) classes.add(name); else classes.delete(name); }, contains: name => classes.has(name) },
        querySelectorAll: () => [], addEventListener(name, fn) { listeners.set(name, fn); },
        dispatch: name => listeners.get(name)?.(), replaceChildren(...nodes) { this.nodes = nodes; },
      };
      if (id === 'notice') Object.defineProperty(element, 'textContent', { get: () => notices.at(-1) ?? '', set: text => notices.push(text) });
      elements.set(id, element);
    }
    return elements.get(id);
  };
  const deps = { current, pdf, pageNumber: number, mode, el, isTauri: () => desktop, canLeavePresentation: leavePresentation, canLeaveWorkbook: leaveWorkbook,
    invoke: async (...args) => { calls.push(args); return invoke(...args); }, pauses, events,
  };
  const build = new Function('deps', `
    let {current,pdf,pageNumber,mode,el,isTauri,canLeavePresentation,canLeaveWorkbook,invoke,pauses,events}=deps;
    let pdfTextAttempt=null,stopText=false; let busy=false,pdfRotationPending=false,displayedPdfPage=null,pdfRenderAttempt=null,renderTask=null,pdfLoadingTask=null,pdfLoadAttempt=null,cancelPdfPassword=null;
    let paused=false,printing=false,leaving=false,saving=false,dirty=false,editor=null,draftTimer=null,draftQueue=Promise.resolve(),closingWindow=false;
    const pauseAuthoring=value=>{paused=value;pauses.push(['authoring',value]);},pauseWorkbook=value=>pauses.push(['workbook',value]),pausePresentation=value=>pauses.push(['presentation',value]);
    const updateToolbar=()=>{},writeDraft=async()=>{},snapshotDocument=()=>({});
    const window={confirm:()=>true,devicePixelRatio:1};const document={createElement:tag=>el('created-'+tag+'-'+Math.random())};
    const getCurrentWindow=()=>({destroy:async()=>events.push('destroy')});
    ${transpile(actual)}
    const canLeaveOther=${beforeLeaveBinding},blockWindowClose=${finalCloseBinding},showNotice=notice;
    const exports={};
    ${transpile(leaveNode.getText(authorAst))}
    let afterLeave=null;
    const originalCanLeaveAuthoring=canLeaveAuthoring;
    canLeaveAuthoring=async()=>{const result=await originalCanLeaveAuthoring();afterLeave?.();return result;};
    const requestNativeClose=${transpile(`const callback=${closeCallback.getText(authorAst)};`).replace(/^const callback=/, '').replace(/^const callback = /, '').trim().replace(/;$/, '')};
    ${transpile(clickBinding.getText(ast))}
    controls();
    return {${names.join(',')}, canLeaveAuthoring, requestNativeClose:()=>requestNativeClose({preventDefault:()=>events.push('prevent-close')}),
      state:()=>({current,pdf,pageNumber,busy,pdfRotationPending,paused,mode,displayedPdfPage}),set:(next,nextPdf=null)=>{current=next;pdf=nextPdf;},setPage:value=>{pageNumber=value;},afterLeave:hook=>{afterLeave=hook;},
      setToken:value=>{current.session_token=value;},setMode:value=>{mode=value;controls();}};
  `);
  return { ...build(deps), el, calls, notices, events, pauses };
}
// A usable display identity is established only by the actual successful
// renderPage function. Deferred/failing metadata tests explicitly provide the
// page shown before replacing the subsequent metadata lookup transport.
async function shownHarness(args = {}) {
  const app = harness(args), documentPdf = app.state().pdf, previousGetPage = documentPdf?.getPage;
  if (args.renderedPage && documentPdf) documentPdf.getPage = async () => args.renderedPage;
  await app.renderPage();
  if (documentPdf) documentPdf.getPage = previousGetPage;
  app.controls();
  return app;
}
const cancelled = () => null;

test('PDF rotation captures exact token and loaded page identity; source stays open unchanged', async () => {
  const selected = page(), documentPdf = proxy(selected), current = owner(), finish = deferred();
  const before = structuredClone(current), app = await shownHarness({ current, pdf: documentPdf, invoke: () => finish.promise });
  app.el('title').textContent = current.name; app.el('subtitle').textContent = current.path; app.el('pdf-pages').nodes = ['original canvas'];
  const running = app.el('rotate-pdf-page').dispatch('click'); await tick();
  expect(app.calls).toEqual([['rotate_current_pdf', { sessionToken: '9007199254740993', selection: { page: 2, pages: 3, object_id: 37, generation: 4, rotation: 270 } }]]);
  selected.ref.num = 55; selected.ref.gen = 0; selected.rotate = 90;
  expect(app.calls[0][1].selection).toEqual({ page: 2, pages: 3, object_id: 37, generation: 4, rotation: 270 });
  finish.resolve({ path: '/copy.pdf', page: 2, pages: 3, rotation: 0 }); await running;
  expect(current).toEqual(before); expect(app.state().pdf).toBe(documentPdf); expect(selected.cleaned).toBe(2);
  expect(app.el('title').textContent).toBe('source.pdf'); expect(app.el('subtitle').textContent).toBe('/source.pdf'); expect(app.el('pdf-pages').nodes).toEqual(['original canvas']);
  expect(app.notices.at(-1)).toContain('/copy.pdf'); expect(app.notices.at(-1)).toContain('当前仍显示未修改的原文件');
  expect(app.state().pdfRotationPending).toBe(false); expect(app.state().busy).toBe(false);
});

test('PDF metadata lookup and native save disable conflicting controls and repeated clicks', async () => {
  const selected = page(), metadata = deferred(), save = deferred(), documentPdf = proxy(selected, { getPage: () => metadata.promise });
  const app = await shownHarness({ pdf: documentPdf, renderedPage: selected, invoke: () => save.promise });
  const first = app.rotateCurrentPdf();
  for (const id of ['rotate-pdf-page', 'open', 'open-empty', 'new-document', 'new-workbook', 'new-presentation', 'restore-draft', 'restore-workbook', 'restore-presentation', 'text-view', 'page-view', 'extract-pdf-text', 'zoom', 'prev-page', 'next-page', 'search']) expect(app.el(id).disabled, id).toBe(true);
  await app.rotateCurrentPdf(); expect(app.calls).toHaveLength(0);
  metadata.resolve(selected); await tick(); await app.el('rotate-pdf-page').dispatch('click'); expect(app.calls).toHaveLength(1);
  save.resolve(null); await first;
  for (const id of ['rotate-pdf-page', 'open', 'new-document', 'text-view', 'extract-pdf-text', 'zoom', 'prev-page', 'next-page']) expect(app.el(id).disabled, id).toBe(false);
  expect(app.pauses).toEqual([['authoring',true],['workbook',true],['presentation',true],['authoring',false],['workbook',false],['presentation',false]]);
});

test('PDF rotation native close and beforeunload are blocked from metadata capture through save', async () => {
  const selected = page(), metadata = deferred(), save = deferred(), app = await shownHarness({ pdf: proxy(selected, { getPage: () => metadata.promise }), renderedPage: selected, invoke: () => save.promise });
  const running = app.rotateCurrentPdf();
  await app.requestNativeClose(); expect(app.events).toEqual(['prevent-close']); expect(app.notices.at(-1)).toContain('等待完成或取消保存');
  let prevented = false; const event = { preventDefault() { prevented = true; } };
  app.protectPdfRotationUnload(event); expect(prevented).toBe(true); expect(event.returnValue).toBe('');
  metadata.resolve(selected); await tick(); await app.requestNativeClose(); expect(app.events).not.toContain('destroy');
  save.resolve(null); await running; await app.requestNativeClose(); expect(app.events.at(-1)).toBe('destroy');
  prevented = false; app.protectPdfRotationUnload(event); expect(prevented).toBe(false);
});

test('PDF rotation workspace leave rechecks after another editor asynchronous leave', async () => {
  const leaving = deferred(), save = deferred(), app = await shownHarness({ leavePresentation: () => leaving.promise, invoke: () => save.promise });
  const close = app.requestNativeClose(); const running = app.rotateCurrentPdf(); leaving.resolve(true); await close;
  expect(app.events).not.toContain('destroy'); save.resolve(null); await running;
});

test('PDF save starting after an earlier native close passed asynchronous leave is caught before destroy', async () => {
  const save = deferred(), app = await shownHarness({ invoke: () => save.promise }); let running;
  app.afterLeave(() => { running = app.rotateCurrentPdf(); });
  await app.requestNativeClose(); expect(app.events).toEqual(['prevent-close']); expect(app.state().pdfRotationPending).toBe(true);
  expect(app.notices.at(-1)).toContain('等待完成或取消保存');
  app.afterLeave(null); save.resolve(null); await running; await app.requestNativeClose(); expect(app.events.at(-1)).toBe('destroy');
});

for (const [label, invoke, expected] of [
  ['cancel', cancelled, '已取消旋转另存'],
  ['source mismatch', async () => { throw new Error('PDF 源文件已改变，请重新打开文档'); }, '重新打开'],
  ['unsupported source', async () => { throw new Error('不支持加密、表单或已有修订'); }, '不支持加密'],
  ['write failure', async () => { throw new Error('另存失败：磁盘空间不足'); }, '磁盘空间不足'],
]) test(`PDF rotation ${label} preserves source and recovers controls`, async () => {
  const current = owner(), documentPdf = proxy(), app = await shownHarness({ current, pdf: documentPdf, invoke });
  await app.rotateCurrentPdf(); expect(app.notices.at(-1)).toContain(expected); expect(app.state().current).toBe(current); expect(app.state().pdf).toBe(documentPdf);
  expect(app.state().busy).toBe(false); expect(app.state().pdfRotationPending).toBe(false); expect(app.state().paused).toBe(false); expect(app.el('rotate-pdf-page').disabled).toBe(false); expect(app.el('status').textContent).toBe('就绪');
});

test('PDF rotation missing/numeric/noncanonical/out-of-range tokens never reach IPC', async () => {
  for (const token of [undefined, null, 1, 9007199254740992, '', '01', '-1', '1e3', '18446744073709551616']) {
    const current = owner(); current.session_token = token; const app = await shownHarness({ current }); await app.rotateCurrentPdf();
    expect(app.calls, String(token)).toHaveLength(0); expect(app.notices.at(-1)).toContain('会话身份无效'); expect(app.state().busy).toBe(false);
  }
  const app = await shownHarness({ current: owner('18446744073709551615') }); await app.rotateCurrentPdf(); expect(app.calls[0][1].sessionToken).toBe('18446744073709551615');
});

test('PDF rotation invalid page counts/current pages never fetch a page or invoke save', async () => {
  for (const [number, pages] of [[0,3],[-1,3],[1.5,3],[4,3],[1,0],[1,3.5],[1,NaN],[1,Infinity],[1,Number.MAX_SAFE_INTEGER+1]]) {
    let fetched = false; const app = harness({ number, pdf: proxy(page(), { numPages: pages, getPage: async () => { fetched = true; return page(); } }) });
    await app.rotateCurrentPdf(); expect(fetched).toBe(false); expect(app.calls).toHaveLength(0); expect(app.notices.at(-1)).toContain('页码身份无效');
  }
});

test('PDF rotation missing or invalid actual page identity is refused, including repaired PDF.js page refs', async () => {
  for (const extra of [{ref:null},{ref:undefined},{ref:{num:0,gen:0}},{ref:{num:-1,gen:0}},{ref:{num:1.5,gen:0}},{ref:{num:0x100000000,gen:0}},{ref:{num:1,gen:-1}},{ref:{num:1,gen:65536}},{ref:{num:1,gen:0.5}},{rotate:45},{rotate:-90},{rotate:360},{rotate:'90'}]) {
    const selected = page(extra), app = await shownHarness({ pdf: proxy(selected) }); await app.rotateCurrentPdf();
    expect(app.calls, JSON.stringify(extra)).toHaveLength(0); expect(app.notices.at(-1)).toContain('页面对象或旋转身份'); expect(selected.cleaned).toBe(2);
  }
});

test('PDF metadata rejection recovers controls and does not open save dialog', async () => {
  const app = await shownHarness({ pdf: proxy(page(), { getPage: async () => { throw new Error('页面解析失败'); } }), renderedPage: page() }); await app.rotateCurrentPdf();
  expect(app.calls).toHaveLength(0); expect(app.notices.at(-1)).toContain('页面解析失败'); expect(app.state().busy).toBe(false); expect(app.el('open').disabled).toBe(false);
});

test('PDF rotation stale metadata is cleaned and never invokes save for a newer session or changed page count', async () => {
  for (const change of ['owner', 'token', 'count']) {
    const selected = page(), metadata = deferred(), app = await shownHarness({ pdf: proxy(selected, { getPage: () => metadata.promise }), renderedPage: selected }); const running = app.rotateCurrentPdf();
    if (change === 'token') app.setToken('2');
    else if (change === 'count') app.state().pdf.numPages = 4;
    else app.set(owner('2'), proxy());
    app.el('status').textContent = 'new status'; app.el('notice').textContent = 'new notice';
    metadata.resolve(selected); await running;
    expect(selected.cleaned).toBe(2); expect(app.calls).toHaveLength(0); expect(app.el('notice').textContent).toBe('new notice'); expect(app.el('status').textContent).toBe('new status');
  }
});

for (const completion of ['success', 'cancel', 'error']) test(`PDF stale rotation ${completion} cannot replace a newer same-path document or its status`, async () => {
  const save = deferred(), app = await shownHarness({ invoke: () => save.promise }); const running = app.rotateCurrentPdf(); await tick();
  const next = owner('2'), nextPdf = proxy(); app.set(next, nextPdf); app.el('status').textContent = 'new status'; app.el('notice').textContent = 'new notice';
  if (completion === 'error') save.reject(new Error('obsolete error')); else save.resolve(completion === 'cancel' ? null : { path:'/old-copy.pdf',page:2,pages:3,rotation:0 });
  await running; expect(app.state().current).toBe(next); expect(app.state().pdf).toBe(nextPdf); expect(app.el('status').textContent).toBe('new status'); expect(app.el('notice').textContent).toBe('new notice'); expect(app.state().busy).toBe(false);
});

test('PDF stale metadata errors and replaced PDF.js object do not report against current document', async () => {
  const metadata = deferred(), app = await shownHarness({ pdf: proxy(page(), { getPage: () => metadata.promise }), renderedPage: page() }); const running = app.rotateCurrentPdf();
  app.set(app.state().current, proxy()); app.el('notice').textContent='current notice'; app.el('status').textContent='current status'; metadata.reject(new Error('stale metadata error')); await running;
  expect(app.el('notice').textContent).toBe('current notice'); expect(app.el('status').textContent).toBe('current status');
});

test('PDF page navigation during metadata lookup drops the old selection before the save dialog', async () => {
  const metadata=deferred(),selected=page(),app=await shownHarness({pdf:proxy(selected,{getPage:()=>metadata.promise}),renderedPage:selected});const running=app.rotateCurrentPdf();
  app.setPage(3);app.el('notice').textContent='new page';app.el('status').textContent='new status';metadata.resolve(selected);await running;
  expect(app.calls).toHaveLength(0);expect(selected.cleaned).toBe(2);expect(app.el('notice').textContent).toBe('new page');expect(app.el('status').textContent).toBe('new status');
});

test('PDF page or count change during save keeps captured request and suppresses old result status', async () => {
  for (const change of ['page', 'count']) {
    const save = deferred(), app = await shownHarness({ invoke: () => save.promise });
    const running = app.rotateCurrentPdf(); await tick();
    if (change === 'page') app.setPage(3); else app.state().pdf.numPages = 99;
    app.el('notice').textContent = 'new page'; app.el('status').textContent = 'new status';
    expect(app.calls[0][1].selection).toEqual({ page: 2, pages: 3, object_id: 37, generation: 4, rotation: 270 });
    save.resolve({ path: '/old-page-copy.pdf', page: 2, pages: 3, rotation: 0 }); await running;
    expect(app.el('notice').textContent).toBe('new page'); expect(app.el('status').textContent).toBe('new status');
    expect(app.el('next-page').disabled).toBe(change === 'page'); expect(app.el('rotate-pdf-page').disabled).toBe(true);
  }
});

test('PDF rotation requires loaded native PDF page mode; unrelated documents hide controls and limitation', async () => {
  for (const args of [{pdf:null},{desktop:false},{mode:'text'},{current:null},{current:{...owner(),inspection:{format:'docx'}}}]) {
    const app = await shownHarness(args); await app.rotateCurrentPdf(); expect(app.calls).toHaveLength(0);
    if (args.mode === 'text' || args.current === null || args.current?.inspection.format === 'docx') {
      expect(app.el('rotate-pdf-page').hidden).toBe(true); expect(app.el('pdf-rotation-hint').hidden).toBe(true);
    }
  }
});

test('PDF rotation refuses mismatched save receipt and does not claim success', async () => {
  for (const receipt of [{path:'/copy.pdf',page:1,pages:3,rotation:0},{path:'/copy.pdf',page:2,pages:4,rotation:0},{path:'/copy.pdf',page:2,pages:3,rotation:90},{path:'',page:2,pages:3,rotation:0}]) {
    const app=await shownHarness({invoke:async()=>receipt}); await app.rotateCurrentPdf(); expect(app.notices.at(-1)).toContain('返回结果无法核对'); expect(app.el('rotate-pdf-page').disabled).toBe(false);
  }
});

test('PDF rotation production bindings expose bounded-save limitation and workspace exit guard', async () => {
  const html=readFileSync('ui/index.html','utf8');
  expect(html).toContain('当前页顺时针旋转 90°，另存副本'); expect(html).toContain('aria-describedby="pdf-rotation-hint"');
  expect(html).toContain('已有增量修订'); expect(html).toContain('另存副本暂不能再次旋转另存');
  expect(source).toContain('initializeAuthoring(prepareEditorWorkspace, notice, canLeaveWorkspace');
  expect(source).toContain("window.addEventListener('beforeunload', protectPdfRotationUnload)");
});

test('PDF failed next-page lookup leaves old canvas visible but cannot rotate the unshown page', async () => {
  const app = await shownHarness(), documentPdf = app.state().pdf;
  const canvas = app.el('pdf-pages').nodes[0];
  expect(app.currentDisplayedPdfPage().page).toBe(2);
  app.setPage(3); documentPdf.getPage = async () => { throw new Error('next-page decode failed'); };
  await app.operation('rendering next page', app.renderPage);
  expect(app.el('pdf-pages').nodes[0]).toBe(canvas);
  expect(app.state().displayedPdfPage).toBeNull(); expect(app.el('rotate-pdf-page').disabled).toBe(true);
  await app.rotateCurrentPdf(); expect(app.calls).toHaveLength(0); expect(app.notices.at(-1)).toContain('页面尚未成功显示');
  documentPdf.getPage = async () => page({ pageNumber: 3, ref: { num: 42, gen: 0 }, rotate: 90 });
  await app.operation('retry page', app.renderPage);
  expect(app.currentDisplayedPdfPage().page).toBe(3); expect(app.el('rotate-pdf-page').disabled).toBe(false);
});

test('PDF in-flight and failed rendering never establish a selectable display identity', async () => {
  const app = await shownHarness(), rendering = deferred();
  app.state().pdf.getPage = async () => page({ render: () => ({ promise: rendering.promise, cancel() {} }) });
  const run = app.renderPage(); await tick();
  expect(app.currentDisplayedPdfPage()).toBeNull(); expect(app.el('rotate-pdf-page').disabled).toBe(true);
  await app.rotateCurrentPdf(); expect(app.calls).toHaveLength(0);
  rendering.reject(new Error('raster failure')); await expect(run).rejects.toThrow('raster failure');
  expect(app.state().displayedPdfPage).toBeNull(); expect(app.el('rotate-pdf-page').disabled).toBe(true);
});

test('PDF metadata must still match the exact reference, generation and rotation that was displayed', async () => {
  for (const change of [{ ref: { num: 38, gen: 4 } }, { ref: { num: 37, gen: 5 } }, { rotate: 90 }]) {
    const app = await shownHarness(); app.state().pdf.getPage = async () => page(change);
    await app.rotateCurrentPdf(); expect(app.calls).toHaveLength(0); expect(app.notices.at(-1)).toContain('与已显示页面不一致');
  }
  const app = await shownHarness(); app.state().pdf.getPage = async () => page({ pageNumber: 1 });
  await app.rotateCurrentPdf(); expect(app.calls).toHaveLength(0); expect(app.notices.at(-1)).toContain('页面对象或旋转身份');
});

test('PDF mismatched returned page cannot establish displayed-page provenance', async () => {
  const app = await shownHarness({ pdf: proxy(page({ pageNumber: 1 })) });
  expect(app.currentDisplayedPdfPage()).toBeNull(); expect(app.el('rotate-pdf-page').disabled).toBe(true);
  await app.rotateCurrentPdf(); expect(app.calls).toHaveLength(0); expect(app.notices.at(-1)).toContain('页面尚未成功显示');
});

test('PDF late successful render cannot give a new document the old selectable-page identity', async () => {
  const rendering = deferred(), app = harness({ pdf: proxy(page({ render: () => ({ promise: rendering.promise, cancel() {} }) })) });
  const run = app.renderPage(); await tick(); app.set(owner('2'), proxy()); rendering.resolve(); await run; app.controls();
  expect(app.state().displayedPdfPage).toBeNull(); expect(app.el('rotate-pdf-page').disabled).toBe(true);
  await app.rotateCurrentPdf(); expect(app.calls).toHaveLength(0);
});

test('PDF clearing and replacing the document invalidate displayed identity until another actual render', async () => {
  const app = await shownHarness(); await app.clearPdf(); app.controls();
  expect(app.state().displayedPdfPage).toBeNull(); expect(app.el('rotate-pdf-page').disabled).toBe(true);
  app.set(owner('2'), proxy()); await app.rotateCurrentPdf(); expect(app.calls).toHaveLength(0);
  await app.operation('display new document', app.renderPage); expect(app.el('rotate-pdf-page').disabled).toBe(false);
  await app.rotateCurrentPdf(); expect(app.calls[0][1].sessionToken).toBe('2');
});

test('PDF overlapping same-page lookups cannot establish provenance from an older render attempt', async () => {
  const app = await shownHarness(), firstLookup = deferred(), secondLookup = deferred(), secondRender = deferred();
  const lookups = [firstLookup, secondLookup]; app.state().pdf.getPage = () => lookups.shift().promise;
  const older = app.renderPage(), newer = app.renderPage();
  const obsoletePage = page(); firstLookup.resolve(obsoletePage); await older;
  expect(obsoletePage.cleaned).toBe(1); expect(app.currentDisplayedPdfPage()).toBeNull();
  secondLookup.resolve(page({ render: () => ({ promise: secondRender.promise, cancel() {} }) })); await tick();
  expect(app.currentDisplayedPdfPage()).toBeNull(); expect(app.el('rotate-pdf-page').disabled).toBe(true);
  secondRender.resolve(); await newer; app.controls(); expect(app.el('rotate-pdf-page').disabled).toBe(false);
});

test('PDF late same-page lookup cleanup never clears a newer successful render identity', async () => {
  const app = await shownHarness(), firstLookup = deferred(), secondLookup = deferred(), lookups = [firstLookup, secondLookup];
  app.state().pdf.getPage = () => lookups.shift().promise;
  const older = app.renderPage(), newer = app.renderPage();
  secondLookup.resolve(page()); await newer; const displayed = app.currentDisplayedPdfPage(), canvas = app.el('pdf-pages').nodes[0];
  expect(displayed).not.toBeNull(); firstLookup.resolve(page()); await older;
  expect(app.currentDisplayedPdfPage()).toBe(displayed); expect(app.el('pdf-pages').nodes[0]).toBe(canvas);
});

test('PDF obsolete same-page render completion cannot restore identity while newer render awaits it', async () => {
  const app = await shownHarness(), olderRender = deferred(), nextLookup = deferred();
  let lookups = 0; app.state().pdf.getPage = async () => ++lookups === 1
    ? page({ render: () => ({ promise: olderRender.promise, cancel() {} }) }) : nextLookup.promise;
  const older = app.renderPage(); await tick(); const newer = app.renderPage();
  olderRender.resolve(); await older; await tick();
  expect(app.currentDisplayedPdfPage()).toBeNull(); expect(app.el('rotate-pdf-page').disabled).toBe(true);
  nextLookup.resolve(page()); await newer; expect(app.currentDisplayedPdfPage()).not.toBeNull();
});
