import { openPresentation, closePresentation, type Presentation } from './presentation';
import {initializePresentationPdf} from './presentation-pdf';
import {initializePresentationEditor,openPresentationEditor,closePresentationEditor,canLeavePresentation,pausePresentation,newPresentationModel,type PresentationModel} from './presentation-editor';
import './style.css';
import { invoke, isTauri } from '@tauri-apps/api/core';
import * as pdfjs from 'pdfjs-dist';
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url';
import { initializeAuthoring, isAuthoring, canLeaveAuthoring, closeAuthoring, openAuthored, openImported, pauseAuthoring } from './authoring';
import type { JSONContent } from '@tiptap/core';
import { extractPdfText, textSummary } from './pdf-text';
import { initializeSpreadsheet, openWorkbook, closeWorkbook, pauseWorkbook, canLeaveWorkbook, workbookView, type Workbook } from './spreadsheet';
import type { EditableWorkbook } from './workbook-model';

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;
interface Inspection { format: string; backend: string; bytes: number; elapsed_ms: number; text: string; warnings: string[]; text_error?: string }
interface OpenedDocument { session_token?: string; presentation_model?:PresentationModel|null; presentation?: Presentation | null; name: string; path: string; inspection: Inspection; authored?: JSONContent | null; workbook?: Workbook | null; workbook_model?: EditableWorkbook | null }
interface SavedDocument { path: string; report: { replacements: number; preserved_parts: number; changed_parts: string[]; warnings: string[] } }
interface SavedPdfRotation { path: string; page: number; pages: number; rotation: number }
interface DisplayedPdfPage { owner: OpenedDocument; documentPdf: pdfjs.PDFDocumentProxy; token?: string; page: number; pages: number; objectId?: number; generation?: number; rotation: number }
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
let current: OpenedDocument | null = null;
let busy = false;
let workspaceAttempt: object | null = null;
let mode: 'text' | 'page' = 'text';
let pdf: pdfjs.PDFDocumentProxy | null = null;
let pageNumber = 1;
let renderTask: pdfjs.RenderTask | null = null;
let pdfTextSummary: string | null = null;
let pdfLoadingTask: pdfjs.PDFDocumentLoadingTask | null = null;
let pdfLoadAttempt: object | null = null;
let cancelPdfPassword: (() => void) | null = null;
let stopText = false;
let pdfRotationPending = false;
let displayedPdfPage: DisplayedPdfPage | null = null;
let pdfRenderAttempt: object | null = null;

function notice(message: string, error = false) {
  const box = el('notice'); box.textContent = message; box.hidden = !message; box.classList.toggle('error', error);
}
function controls() {
  const desktop = isTauri();
  el('presentation').querySelectorAll<HTMLInputElement | HTMLButtonElement | HTMLSelectElement>('input,button,select,textarea').forEach(node => node.disabled = busy || !desktop || node.dataset.unavailable==='true');
  el<HTMLButtonElement>('new-presentation').disabled=busy;
  el<HTMLButtonElement>('restore-presentation').disabled=busy||!desktop;
  for (const id of ['open', 'open-empty']) el<HTMLButtonElement>(id).disabled = busy || !desktop;
  el<HTMLButtonElement>('new-document').disabled = busy;
  el<HTMLButtonElement>('new-workbook').disabled = busy;
  el<HTMLButtonElement>('restore-workbook').disabled = busy || !desktop;
  el<HTMLButtonElement>('restore-draft').disabled = busy || !desktop;
  el<HTMLButtonElement>('text-view').disabled = busy || !current;
  el<HTMLButtonElement>('page-view').disabled = busy;
  el<HTMLButtonElement>('extract-pdf-text').disabled = busy || !pdf;
  el('rotate-pdf-page').hidden = current?.inspection.format !== 'pdf' || mode !== 'page';
  el('pdf-rotation-hint').hidden = current?.inspection.format !== 'pdf' || mode !== 'page';
  el<HTMLButtonElement>('rotate-pdf-page').disabled = busy || pdfRotationPending || !desktop || !currentDisplayedPdfPage();
  el<HTMLButtonElement>('replace-toggle').disabled = busy || current?.inspection.format !== 'docx';
  el<HTMLButtonElement>('import-document').disabled = busy || !desktop || !['doc', 'docx', 'xlsx','pptx'].includes(current?.inspection.format ?? '') || (current?.inspection.format === 'xlsx' && !!current.workbook_model) || (current?.inspection.format==='pptx' && !!current.presentation_model);
  el('sheet-import').hidden = current?.inspection.format !== 'xlsx' || !!current.workbook_model;
  el<HTMLButtonElement>('sheet-import').disabled = busy;
  el<HTMLButtonElement>('save-replacement').disabled = busy;
  el<HTMLInputElement>('search').disabled = busy || !current || mode === 'page' || !current.inspection.text;
  el<HTMLSelectElement>('zoom').disabled = busy;
  el<HTMLButtonElement>('prev-page').disabled = busy || !pdf || pageNumber <= 1;
  el<HTMLButtonElement>('next-page').disabled = busy || !pdf || pageNumber >= pdf.numPages;
}
async function operation(label: string, action: () => Promise<void>, owns: () => boolean = () => true) {
  if (busy) return;
  busy = true; pauseAuthoring(true); pauseWorkbook(true); pausePresentation(true); controls(); el('status').textContent = label;
  try { await action(); }
  catch (error) {
    if (!owns()) return;
    const message = String(error).includes('Hidden text cannot be preserved in an editable copy')
      ? '文档含隐藏文字声明，暂不能创建编辑副本，以免丢失内容。原文件保持不变；只读正文可能不显示隐藏文字。未使用或已覆盖的隐藏样式也会保守拦截。'
      : String(error).includes('Unsupported paragraph spacing cannot be preserved in an editable copy')
        ? '文档包含暂不支持的段落间距、行距或网格声明，无法创建编辑副本。同样式单侧上下文间距的两个相邻间距均为正时，标准与已验证消费者结果不同，同样会保守拦截；无法证明的活动样式也不猜测。原文件保持不变。'
        : String(error).includes('Unsupported table geometry cannot be preserved in an editable copy')
          ? '文档包含暂不支持的表格宽度或布局声明，无法创建编辑副本。固定列宽仅支持一致的矩形网格；百分比、冲突宽度、合并及嵌套固定表格等会保守拦截，原文件保持不变。'
          : String(error).includes('Unsupported page geometry cannot be preserved in an editable copy')
            ? '文档包含暂不支持或无法证明的页面设置，无法创建编辑副本。仅支持单节、A4/Letter/A5 横纵向和有界四边页边距；多节、分栏、镜像或装订线等会保守拒绝，原文件保持不变。'
            : String(error).includes('Table width and indentation exceed the explicit page content width')
              ? '表格宽度加缩进超出源页面正文区域，无法创建编辑副本。定位自动表格按 450 pt 计算；请在原文件中明确设置合适的固定列宽或清除表格位置后再导入，原文件保持不变。'
              : String(error).includes('Image frame exceeds the explicit page content area')
                ? '图片显示框超出源页面正文区域，无法创建编辑副本。请在原文件中明确调整图片尺寸后再导入；不会自动缩放或裁剪，原文件保持不变。'
                : String(error);
    notice(`操作未完成：${message}`, true);
  }
  finally { busy = false; pauseAuthoring(false); pauseWorkbook(false); pausePresentation(false); if (owns()) el('status').textContent = '就绪'; controls(); }
}
initializePresentationPdf(operation,notice);
function renderText() {
  if (!current) return;
  const text = current.inspection.text;
  const query = el<HTMLInputElement>('search').value;
  const target = el('text-content'); target.replaceChildren();
  if (!text && current.inspection.format === 'pdf') {
    target.textContent = pdfTextSummary ?? (current.inspection.text_error ? 'PDF 文字提取未成功。可尝试查看页面，解锁后可提取正文。' : '未提取到正文。扫描件需文字识别，仍可尝试查看 PDF 页面。');
    el('match-count').textContent = ''; return;
  }
  // Use text nodes throughout: document text is never interpreted as HTML.
  let offset = 0, count = 0;
  if (query) {
    let at = text.indexOf(query);
    while (at !== -1 && count < 2000) {
      target.append(document.createTextNode(text.slice(offset, at)));
      const mark = document.createElement('mark'); mark.textContent = query; target.append(mark);
      offset = at + query.length; count++; at = text.indexOf(query, offset);
    }
  }
  target.append(document.createTextNode(text.slice(offset)));
  el('match-count').textContent = query ? (count >= 2000 ? '2000+' : `${count} 处`) : '';
}
function setMode(next: 'text' | 'page') {
  mode = next;
  el('document').hidden = !current || next !== 'text';
  el('pdf-pages').hidden = !current || next !== 'page';
  el('page-nav').hidden = next !== 'page';
  document.querySelector<HTMLElement>('.zoom')!.hidden = next !== 'page';
  el('text-view').classList.toggle('active', next === 'text');
  el('page-view').classList.toggle('active', next === 'page');
  controls();
}
async function clearPdf() {
  const old = pdf, loading = pdfLoadingTask;
  pdf = null; pdfLoadingTask = null; pdfLoadAttempt = null; displayedPdfPage = null; pdfRenderAttempt = null;
  cancelPdfPassword?.(); cancelPdfPassword = null;
  renderTask?.cancel(); renderTask = null;
  if (loading) { try { await loading.destroy(); } catch { /* obsolete load */ } }
  if (old) { try { await old.destroy(); } catch { /* obsolete document */ } }
}
async function renderPage() {
  const owner = current, documentPdf = pdf, number = pageNumber;
  if (!documentPdf || !owner) return;
  const token = owner.session_token, pages = documentPdf.numPages;
  const attempt = {}; pdfRenderAttempt = attempt;
  const owns = () => current === owner && pdf === documentPdf && owner.session_token === token && pageNumber === number && pdfRenderAttempt === attempt;
  // A failed page switch may leave the old canvas visible. Never let it select
  // a page for saving until the requested page has finished rendering.
  displayedPdfPage = null; controls();
  if (renderTask) { renderTask.cancel(); try { await renderTask.promise; } catch { /* cancelled */ } }
  if (!owns()) return;
  const page = await documentPdf.getPage(number);
  if (!owns()) { page.cleanup(); return; }
  const displayed: DisplayedPdfPage = { owner, documentPdf, token, page: page.pageNumber, pages, objectId: page.ref?.num, generation: page.ref?.gen, rotation: page.rotate };
  const scale = Number(el<HTMLSelectElement>('zoom').value);
  const viewport = page.getViewport({ scale });
  // Bound the backing canvas for large pages while keeping the aspect ratio.
  const density = Math.min(window.devicePixelRatio || 1, 2, 4096 / Math.max(viewport.width, viewport.height));
  const canvas = document.createElement('canvas'); canvas.className = 'pdf-page';
  canvas.width = Math.ceil(viewport.width * density); canvas.height = Math.ceil(viewport.height * density);
  canvas.style.width = `${viewport.width}px`;
  canvas.setAttribute('aria-label', `PDF 第 ${number} 页`);
  const context = canvas.getContext('2d'); if (!context) throw new Error('无法创建页面画布');
  const host = el('pdf-pages'); host.replaceChildren(canvas);
  el('page-position').textContent = `${number} / ${documentPdf.numPages}`;
  const task = page.render({ canvasContext: context, canvas, viewport, transform: [density, 0, 0, density, 0, 0] });
  renderTask = task;
  try { await task.promise; if (owns() && displayed.page === number) displayedPdfPage = displayed; }
  finally { if (renderTask === task) renderTask = null; page.cleanup(); }
}
function currentDisplayedPdfPage() {
  const displayed = displayedPdfPage;
  return displayed && displayed.owner === current && displayed.documentPdf === pdf &&
    displayed.token === current?.session_token && displayed.page === pageNumber && displayed.pages === pdf?.numPages ? displayed : null;
}
async function showPdf() {
  const owner = current, token = owner?.session_token;
  if (!owner || owner.inspection.format !== 'pdf') return;
  if (typeof token !== 'string' || !/^[1-9][0-9]{0,19}$/.test(token)) throw new Error('PDF 会话身份无效，请重新打开文档。');
  const owns = () => current === owner && owner.session_token === token;
  if (!pdf) {
    if (pdfLoadingTask) {
      cancelPdfPassword?.();
      void pdfLoadingTask.destroy().catch(() => { /* obsolete load */ });
      pdfLoadingTask = null;
    }
    const attempt = {}; pdfLoadAttempt = attempt;
    const active = () => owns() && pdfLoadAttempt === attempt;
    let data: ArrayBuffer;
    try { data = await invoke<ArrayBuffer>('read_current_pdf', { sessionToken: token }); }
    catch (error) { if (!active()) return; setMode('text'); throw error; }
    if (!active()) return;
    const task = pdfjs.getDocument({ data: new Uint8Array(data), isEvalSupported: false, useSystemFonts: true,
      cMapUrl: '/pdf/cmaps/', cMapPacked: true, standardFontDataUrl: '/pdf/standard_fonts/', wasmUrl: '/pdf/wasm/', iccUrl: '/pdf/iccs/' });
    pdfLoadingTask = task;
    let cancelled = false;
    const passwordCleanup: { run: (() => void) | null } = { run: null };
    task.onPassword = (updatePassword: (password: string) => void, reason: number) => {
      if (!active()) { void task.destroy(); return; }
      passwordCleanup.run?.();
      const dialog = el<HTMLDialogElement>('pdf-password');
      const input = el<HTMLInputElement>('pdf-password-input'); input.value = ''; dialog.returnValue = '';
      el('pdf-password-hint').textContent = reason === pdfjs.PasswordResponses.INCORRECT_PASSWORD ? '密码不正确，请重试。密码仅用于本次在本机打开。' : '密码仅用于本次在本机打开文件。';
      const close = () => {
        // close() queues an event: an older close can arrive after this dialog
        // was reopened for a newer prompt. Ignore it without removing this listener.
        if (dialog.open) return;
        const password = input.value, answer = dialog.returnValue;
        passwordCleanup.run?.();
        if (!active()) { void task.destroy(); return; }
        if (answer === 'unlock') updatePassword(password);
        else { cancelled = true; void task.destroy(); }
      };
      const cleanup = () => {
        dialog.removeEventListener('close', close); input.value = '';
        if (dialog.open) dialog.close('cancel');
        if (cancelPdfPassword === cleanup) cancelPdfPassword = null;
        passwordCleanup.run = null;
      };
      passwordCleanup.run = cleanup; cancelPdfPassword = cleanup;
      dialog.addEventListener('close', close);
      dialog.showModal(); input.focus();
    };
    try {
      const loaded = await task.promise;
      if (!active()) { await loaded.destroy(); return; }
      pdf = loaded; pageNumber = 1;
    } catch (error) {
      try { await task.destroy(); } catch { /* failed task cleanup */ }
      if (!active()) return;
      setMode('text'); throw new Error(cancelled ? '已取消解锁 PDF；可点击“PDF 页面”重试。' : `PDF 页面无法打开：${String(error)}`);
    } finally {
      passwordCleanup.run?.();
      if (pdfLoadingTask === task) pdfLoadingTask = null;
    }
  }
  if (!owns()) return;
  setMode('page');
  try { await renderPage(); } catch (error) { if (!owns()) return; setMode('text'); throw error; }
  if (!owns()) return;
  if (!pdfTextSummary && (!owner.inspection.text || owner.inspection.text_error)) await recoverPdfText();
  if (owns()) notice(pdfTextSummary ?? '按原文件显示 PDF 页面。搜索请切换到正文；扫描件的文字识别尚未支持。');
}
async function recoverPdfText() {
  const owner = current, documentPdf = pdf;
  if (!documentPdf || !owner) return;
  const owns = () => current === owner && pdf === documentPdf;
  stopText = false; el('stop-pdf-text').hidden = false;
  try {
    const report = await extractPdfText(documentPdf, () => stopText || !owns(), page => { if (owns()) el('status').textContent = `正在提取正文 ${page}/${documentPdf.numPages} 页…`; });
    if (!owns()) return;
    // Preserve an earlier extraction if a retry fails before producing text.
    if (report.text || report.complete) owner.inspection.text = report.text;
    if (report.complete) delete owner.inspection.text_error;
    pdfTextSummary = textSummary(report);
    renderText(); el('stats').textContent = `${owner.inspection.text.length.toLocaleString()} 字符 / ${documentPdf.numPages} 页`;
    notice(pdfTextSummary);
  } finally { if (owns()) el('stop-pdf-text').hidden = true; }
}
async function rotateCurrentPdf() {
  if (busy || pdfRotationPending) return;
  // These values belong to the clicked page, not to a later save-dialog result.
  const owner = current, documentPdf = pdf, token = owner?.session_token;
  const number = pageNumber, pages = documentPdf?.numPages;
  const displayed = displayedPdfPage;
  const owns = () => current === owner && pdf === documentPdf && owner?.session_token === token && pageNumber === number && Object.is(documentPdf?.numPages, pages) && displayedPdfPage === displayed;
  pdfRotationPending = true;
  try {
    await operation('正在校验并另存旋转后的 PDF…', async () => {
      if (!isTauri() || !owner || owner.inspection.format !== 'pdf' || !documentPdf || mode !== 'page') {
        throw new Error('请先在桌面程序中打开 PDF 页面，再旋转当前页。');
      }
      if (typeof token !== 'string' || !/^[1-9][0-9]{0,19}$/.test(token) || BigInt(token) > 18446744073709551615n) {
        throw new Error('PDF 会话身份无效，请重新打开文档。');
      }
      if (!Number.isSafeInteger(pages) || pages! < 1 || pages! > 0xffffffff || !Number.isSafeInteger(number) || number < 1 || number > pages!) {
        throw new Error('PDF 页码身份无效，请重新打开文档。');
      }
      if (!displayed || currentDisplayedPdfPage() !== displayed) throw new Error('当前 PDF 页面尚未成功显示，不能确认要旋转的页面。请重新显示页面后再试。');
      const page = await documentPdf.getPage(number);
      let selection: { page: number; pages: number; object_id: number; generation: number; rotation: number };
      try {
        if (!owns()) return;
        const objectId = page.ref?.num, generation = page.ref?.gen, rotation = page.rotate;
        if (page.pageNumber !== number || !Number.isSafeInteger(objectId) || objectId! < 1 || objectId! > 0xffffffff ||
            !Number.isSafeInteger(generation) || generation! < 0 || generation! > 65535 || ![0, 90, 180, 270].includes(rotation)) {
          throw new Error('PDF 页面对象或旋转身份无法核对，请重新打开文档；此文件可能不支持旋转另存。');
        }
        if (objectId !== displayed.objectId || generation !== displayed.generation || rotation !== displayed.rotation) {
          throw new Error('PDF 页面身份与已显示页面不一致，请重新打开文档后再旋转另存。');
        }
        selection = { page: number, pages: pages!, object_id: objectId!, generation: generation!, rotation };
      } finally { page.cleanup(); }
      if (!owns()) return;
      const saved = await invoke<SavedPdfRotation | null>('rotate_current_pdf', { sessionToken: token, selection });
      if (!owns()) return;
      if (!saved) { notice('已取消旋转另存，当前仍显示原文件。'); return; }
      if (typeof saved.path !== 'string' || !saved.path || saved.page !== selection.page || saved.pages !== selection.pages || saved.rotation !== (selection.rotation + 90) % 360) {
        throw new Error('旋转另存的返回结果无法核对，请检查另存位置。当前仍显示原文件。');
      }
      notice(`已将第 ${saved.page} / ${saved.pages} 页顺时针旋转 90° 并另存：${saved.path}。当前仍显示未修改的原文件；可打开副本核对，但副本暂不能再次旋转另存。`);
    }, owns);
  } finally { pdfRotationPending = false; controls(); }
}
function blockPdfRotationLeave() {
  if (!pdfRotationPending) return false;
  notice('正在旋转另存 PDF，请等待完成或取消保存后再离开。');
  return true;
}
async function canLeaveWorkspace() {
  if (blockPdfRotationLeave()) return false;
  if (!(await canLeavePresentation()) || !(await canLeaveWorkbook())) return false;
  return !blockPdfRotationLeave();
}
function protectPdfRotationUnload(event: BeforeUnloadEvent) {
  if (!pdfRotationPending) return;
  event.preventDefault(); event.returnValue = '';
}
async function openDocument() {
  if (!(await canLeaveAuthoring())) return;
  await operation('正在打开文档…', async () => {
    const selection = {}; workspaceAttempt = selection;
    const opened = await invoke<OpenedDocument | null>('open_document');
    if (!opened || workspaceAttempt !== selection) return;
    closeAuthoring(); closeWorkbook(); closePresentation(); closePresentationEditor(); el('reading-toolbar').hidden = false;
    current = opened;
    await clearPdf();
    if (current !== opened || workspaceAttempt !== selection) return;
    pageNumber = 1; renderTask = null; pdfTextSummary = null;
    el('empty').hidden = true; el('replace-panel').hidden = true;
    el('title').textContent = opened.name; el('file-name').textContent = opened.name;
    el('subtitle').textContent = opened.path;
    el('format').hidden = false; el('format').textContent = opened.inspection.format.toUpperCase();
    el('page-view').hidden = opened.inspection.format !== 'pdf';
    el('extract-pdf-text').hidden = opened.inspection.format !== 'pdf';
    el<HTMLInputElement>('search').value = ''; el<HTMLInputElement>('find').value = ''; el<HTMLInputElement>('replacement').value = '';
    el('stats').textContent = `${opened.inspection.text.length.toLocaleString()} 字符 / ${(opened.inspection.bytes / 1024).toFixed(1)} KB`;
    renderText(); setMode('text');
    if (opened.presentation) {
      el('document').hidden = true; el('reading-toolbar').hidden = true;
      if(opened.presentation_model){openPresentationEditor(opened.presentation_model);notice('已载入经整包校验的自建演示模型，可修改文字、样式、页和文本框后另存 PPTX。');return;}
      openPresentation(opened.presentation);
      el('stats').textContent = `${opened.presentation.slides.length} 张幻灯片`;
      notice(opened.inspection.format==='ppt'?'旧版 PPT 逐页文本阅读；版式、图片、动画尚未还原，暂不支持编辑或 PDF 导出。':'PPTX 只读近似预览：支持顶层文字样式与内嵌 PNG/JPEG。原始版式、图片裁剪、组合和主题尚未完整还原；支持文字替换另存，暂未提供自由编辑。');
      return;
    }
    if (opened.workbook) {
      el('document').hidden = true; el('reading-toolbar').hidden = true;
      openWorkbook(opened.workbook, opened.workbook_model);
      el('stats').textContent = `${opened.workbook.sheets.length} 个工作表 / ${(opened.inspection.bytes / 1024).toFixed(1)} KB`;
      notice(opened.workbook_model ? '已载入经校验的编辑模型。可修改、撤销和另存 XLSX；点击“计算公式”获得有限范围的计算结果。' : opened.inspection.format === 'xls' ? 'XLS 只读实验预览：公式仅显示源缓存，表达式尚未恢复，不能编辑或重算。样式、对象和工作簿行为未完整呈现。' : '只读表格：网格显示原始值或源缓存，公式未重算。日期、样式、合并单元格和图表尚未完整呈现。');
      return;
    }
    if (opened.authored) { await openAuthored(opened.authored, opened.name, opened.path); return; }
    if (opened.inspection.format === 'pdf') await showPdf();
    else notice('当前为正文预览，未显示原始分页和完整样式。DOC / DOCX 可导入编辑副本并另存 DOCX。');
  });
}
el('open').addEventListener('click', openDocument);
el('open-empty').addEventListener('click', openDocument);
el('presentation').addEventListener('click',event => {
  if ((event.target as HTMLElement).id !== 'pptx-replace-save') return;
  void operation('正在替换并另存演示文稿…',async()=>{
    const owner = current;
    if (!owner?.presentation) return;
    const find=el<HTMLInputElement>('pptx-find').value;
    if (!find) { notice('请输入要查找的文字。',true);return; }
    const selected=el<HTMLSelectElement>('slide-picker').value;
    const saved=await invoke<(SavedDocument & {presentation:Presentation;session_token:string}) | null>('replace_presentation_current',{find,replacement:el<HTMLInputElement>('pptx-replacement').value});
    if (current !== owner) return;
    if (!saved) { notice('已取消另存，当前演示文稿保持不变。');return; }
    current.session_token=saved.session_token;
    current.path=saved.path;current.name=saved.path.split(/[\\/]/).pop() || saved.path;current.presentation=saved.presentation;
    openPresentation(saved.presentation);
    const picker=el<HTMLSelectElement>('slide-picker');if (picker.querySelector(`option[value="${selected}"]`)) { picker.value=selected;picker.dispatchEvent(new Event('change')); }
    el('title').textContent=current.name;el('file-name').textContent=current.name;el('subtitle').textContent=saved.path;
    notice(`已替换 ${saved.report.replacements} 处并另存：${saved.path}。当前显示已保存副本，请核对版面。`);
  });
});
el('sheet-import').addEventListener('click', () => el('import-document').click());
el('import-document').addEventListener('click', () => operation('正在准备编辑副本…', async () => {
  const source = current;
  if(source?.inspection.format==='pptx'){
    el('import-description').textContent='原文件会保留。以下转换会改变版式和部分格式；接受后创建未另存的基础演示编辑副本，请逐页核对并另存 PPTX。';
    const report=await invoke<{model:PresentationModel|null;risks:string[];blockers:string[]}>('import_presentation_current');
    const risks=el('import-risks');risks.replaceChildren();
    for(const risk of [...report.blockers.map(reason=>`不能转换：${reason}`),...report.risks]){const item=document.createElement('li');item.textContent=risk;risks.append(item);}
    const dialog=el<HTMLDialogElement>('import-review');dialog.returnValue='';
    const accept=dialog.querySelector<HTMLButtonElement>('button[value="import"]')!;accept.disabled=!report.model||report.blockers.length>0;
    const answer=new Promise<string>(resolve=>dialog.addEventListener('close',()=>resolve(dialog.returnValue),{once:true}));dialog.showModal();
    const decision=await answer;accept.disabled=false;
    if(decision==='import'&&report.model&&!report.blockers.length&&current===source){
      closePresentation();openPresentationEditor(report.model,true);source.presentation_model=report.model;
      el('title').textContent=`${source.name} · 编辑副本`;notice('已建立 PPTX 基础编辑副本；请检查转换后的文字、图片及版式，再另存 PPTX。原文件保持不变。');
    }
    return;
  }
  el('import-description').textContent = `原文件会保留。以下内容可能改变；导入后请核对并另存 ${source?.inspection.format === 'xlsx' ? 'XLSX' : 'DOCX'}。`;
  if (source?.inspection.format === 'xlsx') {
    const report = await invoke<{ model: EditableWorkbook | null; risks: string[]; blockers: string[] }>('import_workbook_current');
    const risks = el('import-risks'); risks.replaceChildren();
    for (const risk of [...report.blockers.map(reason => `不能转换：${reason}`), ...report.risks]) {
      const item = document.createElement('li'); item.textContent = risk; risks.append(item);
    }
    const dialog = el<HTMLDialogElement>('import-review'); dialog.returnValue = '';
    const accept = dialog.querySelector<HTMLButtonElement>('button[value="import"]')!;
    accept.disabled = !report.model || report.blockers.length > 0;
    const answer = new Promise<string>(resolve => dialog.addEventListener('close', () => resolve(dialog.returnValue), { once: true }));
    dialog.showModal();
    const decision = await answer; accept.disabled = false;
    if (decision === 'import' && report.model && !report.blockers.length) {
      closeWorkbook(); openWorkbook(workbookView(report.model), report.model, true);
      source.workbook_model = report.model;
      el('title').textContent = `${source.name} · 编辑副本`;
      notice('已建立 XLSX 编辑副本；兼容性变化见导入审计。原文件保持不变，请另存 XLSX。');
    }
    return;
  }
  if (!source || !['doc', 'docx'].includes(source.inspection.format)) return;
  const report = await invoke<{ document: JSONContent; compatibility: { risks: string[] } }>('import_current');
  const risks = el('import-risks'); risks.replaceChildren();
  const labels: Record<string, string> = {
    'Legacy DOC conversion is experimental; unsupported binary features may not be detected': '旧版 DOC 转换仍属实验功能，部分二进制特性可能未被识别，请与原文件逐项核对',
    'Legacy DOC page and section breaks may become line or paragraph boundaries; explicit pagination is not preserved': '旧版 DOC 的分页符和分节符可能变为换行或空段落；转换副本不保留原始分页，请重新设置分页',
    'Legacy DOC body manual page breaks are preserved; section layout is not preserved': '已保留受支持的正文手动分页；分节版式未保留，完整分页仍需核对',
    'Legacy whitespace and paragraph boundaries may be normalized': '旧版文档的空白和段落边界可能改变',
    'Page layout, fonts and pagination require visual verification': '页面布局、字体和分页需要人工核对',
    'Section and page geometry': '分节、纸张尺寸和页边距不保留',
    'Unsupported page geometry cannot be preserved in an editable copy': '页面设置超出受支持的单节纸张和页边距范围，暂不能建立编辑副本，避免改变原页面几何',
    'Source page geometry is incomplete; editable copy uses default A4 margins': '源文件缺少完整纸张或页边距证据；编辑副本使用默认 A4 和四边 72 pt，不宣称保留源页面',
    'Paper code and header/footer distance metadata normalized; only page size and body margins preserved': '已保留纸张尺寸和正文四边页边距；纸张代码、页眉页脚距离等附属元数据会规范化，仍需核对分页',
    'Typography, spacing or alignment outside the basic editor model': '可解析的字体名称、字号与 RGB 颜色会保留；DOCX 中受支持的段前/段后间距和倍数行距会保留；自动色和复杂样式可能改变',
    'Font names are preserved; unavailable fonts may be substituted locally': '保留可解析的字体名称；未安装的字体由本机替代，请核对显示效果',
    'Parser warning: DOCX_FONT_THEME_UNRESOLVED': '部分主题字体无法确定，相关文字使用编辑器默认字体，请核对原文',
  'Theme and automatic text colors normalized to fixed RGB or editor defaults': '主题色近似转为固定 RGB；自动色或无法解析的主题色使用编辑器默认颜色，不保留主题联动',
  'Custom theme color mapping is unsupported': '文档使用了自定义主题颜色映射，暂时只能只读查看',
  'Unsupported theme color scheme content': '文字引用的主题包含尚不支持的颜色来源或变换，暂时只能只读查看',
  'Default document style normalized in editable copy': '编辑副本使用统一的正文和标题默认样式；保留可解析的字体名称、字号与颜色；DOCX 有效段落间距单独保留，请核对字体和换页',
    'Contextual same-style spacing resolved to fixed paragraph values; named-style and reorder dynamics are not retained': '同样式上下文间距已按源段落顺序转换为固定段前/段后值；命名样式及重排后的动态间距不保留。之后请用段前/段后控件调整，恢复默认也不会恢复源动态样式',
    'List marker appearance normalized to editor defaults': '列表编号外观改为编辑器默认样式',
    'Unpositioned automatic table widths normalized to the explicit page content area; source widths are not preserved': '无定位的自动表格按当前正文宽度重排；不保留原始表格列宽，固定宽度/定位表格及图片不会自动缩放',
    'Autofit table widths normalized; fixed column widths were not inferred': '自动调整表格的宽度使用默认布局；不会仅凭网格推断固定列宽，请核对原文',
    'Table alignment normalized to editor defaults': '表格整体对齐改为编辑器默认位置，请核对页面中的摆放',
    'Inherited table indentation normalized; automatic table geometry not preserved': '继承的表格缩进已规范化；自动表格的原始几何布局不保留，请核对位置、宽度和换行',
    'Horizontal cell margins preserved; source compatibility-dependent table position normalized': '水平单元格边距数值已保留；源兼容模式下的表格位置仍会规范化，请核对边框与文字起点',
    'Some table cell fills are unproved and remain normalized': '部分单元格底色未能证明为受支持的直接实色，仍按既有方式规范化；请核对图案、主题或活动样式底纹',
    'Some filled-cell automatic text colors are unproved; profile or explicit text colors remain in use': '部分填色格的自动文字色未能单独证明，仍使用编辑器默认或可解析的显式文字色；保留底色不表示自动前景也已恢复',
    'Direct table cell theme fills resolved to fixed RGB; dynamic theme linkage is not preserved': '已证明的直接单元格主题底色转为固定 RGB；不保留主题变更时的动态联动',
    'Filled-cell automatic text color is preserved locally; consumers choose contrast and editor preview is an approximation': '已证明的填色格自动文字色会局部保存为自动，不写死为白色；显式文字 RGB 优先，编辑器 / 打印仅作黑白近似预览，DOCX 由阅读软件决定对比',
    'Table cell color identity could not be bound to converted content; original color normalization retained': '单元格颜色无法与转换后的完整内容一一核对，已取消这次新增颜色保留并沿用原有规范化；源文件不变',
    'Table borders and cell formatting normalized': '受支持的固定列宽、边距及单项提示确认的颜色会保留；其余表格边框和单元格格式规范化，请核对换行和分页',
    'Table widths, borders and cell formatting normalized': '表格宽度、边框和单元格格式改为默认样式',
    'Unsupported image geometry cannot be preserved in an editable copy': '图片包含尚不支持的几何布局，暂时只能只读查看，避免改变原始显示效果',
    'Image-only paragraph spacing and alignment preserved; other page and paragraph layout may be normalized': '已保留单图段落的间距和对齐；其他段落版式可能规范化，受支持的页面尺寸和页边距另行保留，请核对换行和分页',
    'Image paragraph baseline font is unspecified; editor defaults may change its line box': '图片段落的基线字体未能确定，编辑器默认字体可能改变行框和后续正文位置，请核对原文',
    'Image paragraph placement and formatting normalized; original layout not preserved': '受支持的图片显示尺寸会保留；图片所在段落的位置和格式会规范化，整体原始版式未完整恢复，请核对换行和分页',
    'Image position and display dimensions normalized': '图片位置和显示尺寸会改变',
    'Page and column breaks converted to paragraph boundaries': '分页和分栏改为段落分隔',
    'Column breaks converted to paragraph boundaries': '分栏符改为段落分隔；手动分页符保留',
    'Underline pattern converted to single underline': '特殊下划线改为单下划线',
    'Hyperlinks': '常见外部链接保留目标；链接外观、提示文字及打开方式会规范化',
    'Unsupported or unresolved hyperlink target': '存在书签、未支持或无法解析的链接目标，暂不能转为编辑副本',
  };
  for (const risk of report.compatibility.risks) { const item = document.createElement('li'); item.textContent = labels[risk] ?? risk; risks.append(item); }
  const dialog = el<HTMLDialogElement>('import-review'); dialog.returnValue = '';
  const answer = new Promise<string>(resolve => dialog.addEventListener('close', () => resolve(dialog.returnValue), { once: true }));
  dialog.showModal();
  if (await answer === 'import') await openImported(report.document, source.name);
}));
el('text-view').addEventListener('click', () => { setMode('text'); notice(pdfTextSummary ?? (current?.inspection.text_error ? '文字提取失败，正文搜索不可用；请查看 PDF 页面。' : '正文提取预览；原始版面请查看 PDF 页面或使用参考办公软件核对。')); });
el('extract-pdf-text').addEventListener('click', () => operation('正在提取正文…', recoverPdfText));
el('rotate-pdf-page').addEventListener('click', rotateCurrentPdf);
el('stop-pdf-text').addEventListener('click', () => { stopText = true; });
el('pdf-password-cancel').addEventListener('click', () => el<HTMLDialogElement>('pdf-password').close('cancel'));
el('page-view').addEventListener('click', () => operation('正在显示页面…', showPdf));
el('replace-toggle').addEventListener('click', () => { const panel = el('replace-panel'); panel.hidden = !panel.hidden; if (!panel.hidden) el('find').focus(); });
el('search').addEventListener('input', renderText);
el('save-replacement').addEventListener('click', () => operation('正在替换并另存…', async () => {
  const find = el<HTMLInputElement>('find').value;
  if (!find) { notice('请输入要查找的文字。', true); el('find').focus(); return; }
  const saved = await invoke<SavedDocument | null>('replace_current', { find, replacement: el<HTMLInputElement>('replacement').value });
  if (saved) notice(`已替换 ${saved.report.replacements} 处并另存至 ${saved.path}。当前仍显示原文件；打开另存文件可继续阅读。`);
}));
el('zoom').addEventListener('change', () => operation('正在缩放…', renderPage));
el('prev-page').addEventListener('click', () => operation('正在显示上一页…', async () => { pageNumber--; await renderPage(); }));
el('next-page').addEventListener('click', () => operation('正在显示下一页…', async () => { pageNumber++; await renderPage(); }));
document.addEventListener('keydown', event => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'o') { event.preventDefault(); if (isTauri()) void openDocument(); }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f' && current && mode === 'text' && !isAuthoring()) { event.preventDefault(); el('search').focus(); }
});
if (!isTauri()) notice('此页面是桌面界面预览。请运行 npm run desktop，使用本地文件打开与保存功能。');
async function prepareEditorWorkspace() {
  const selection = {}; workspaceAttempt = selection;
  closePresentationEditor();
  closePresentation();
  closeWorkbook();
  current = null;
  await clearPdf();
  if (workspaceAttempt !== selection) throw new Error('文档会话已更新，请重试。');
  el('empty').hidden = true; el('document').hidden = true; el('pdf-pages').hidden = true;
  el('replace-panel').hidden = true; el('reading-toolbar').hidden = true; controls();
}
initializeAuthoring(prepareEditorWorkspace, notice, canLeaveWorkspace, blockPdfRotationLeave);
window.addEventListener('beforeunload', protectPdfRotationUnload);
initializePresentationEditor(notice,operation);
el('new-presentation').addEventListener('click',async()=>{
  if(!(await canLeaveAuthoring()))return;
  await operation('正在新建演示…',async()=>{
    closeAuthoring();await prepareEditorWorkspace();openPresentationEditor(newPresentationModel(),true);
    el('title').textContent='新演示文稿';el('file-name').textContent='新演示文稿';el('subtitle').textContent='文本框编辑；修改后另存 PPTX';el('format').hidden=false;el('format').textContent='PPTX';
    notice('新建演示：文字修改即时预览，演示草稿保存在本机；完成后另存 PPTX。');
  });
});
initializeSpreadsheet();
el('restore-presentation').addEventListener('click',async()=>{
  if(!(await canLeaveAuthoring()))return;
  await operation('正在恢复演示草稿…',async()=>{
    const model=await invoke<PresentationModel|null>('load_presentation_draft');if(!model){notice('尚无演示草稿');return;}
    closeAuthoring();await prepareEditorWorkspace();openPresentationEditor(model,true);
    el('title').textContent='恢复的演示文稿';el('file-name').textContent='恢复的演示文稿';el('subtitle').textContent='演示草稿；完成后另存 PPTX';el('format').hidden=false;el('format').textContent='PPTX';notice('已恢复演示草稿，完成后另存 PPTX。');
  });
});
async function startWorkbook(restore: boolean) {
  if (!(await canLeaveAuthoring())) return;
  await operation(restore ? '正在恢复工作簿…' : '正在新建工作簿…', async () => {
    const model: EditableWorkbook | null = restore ? await invoke('load_workbook_draft') : { version: 1, sheets: [{ name: '工作表1', hidden: false, cells: [] }] };
    if (!model) { notice('尚无工作簿草稿'); return; }
    closeAuthoring(); await prepareEditorWorkspace();
    openWorkbook(workbookView(model), model, true);
    el('title').textContent = restore ? '恢复的工作簿' : '新工作簿'; el('file-name').textContent = el('title').textContent;
    el('subtitle').textContent = '编辑后另存 XLSX；支持有限公式计算'; el('format').hidden = false; el('format').textContent = 'XLSX';
    el('stats').textContent = `${model.sheets.length} 个工作表`;
    notice('输入后按 Enter 向下录入、Tab 向右录入，也可点击“应用单元格”；点击“计算公式”或开启自动重算更新结果。工作簿草稿独立保存。');
  });
}
el('new-workbook').addEventListener('click', () => void startWorkbook(false));
el('restore-workbook').addEventListener('click', () => void startWorkbook(true));
controls();
