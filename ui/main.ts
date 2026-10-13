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
interface OpenedDocument { presentation_model?:PresentationModel|null; presentation?: Presentation | null; name: string; path: string; inspection: Inspection; authored?: JSONContent | null; workbook?: Workbook | null; workbook_model?: EditableWorkbook | null }
interface SavedDocument { path: string; report: { replacements: number; preserved_parts: number; changed_parts: string[]; warnings: string[] } }
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
let current: OpenedDocument | null = null;
let busy = false;
let mode: 'text' | 'page' = 'text';
let pdf: pdfjs.PDFDocumentProxy | null = null;
let pageNumber = 1;
let renderTask: pdfjs.RenderTask | null = null;
let pdfTextSummary: string | null = null;
let stopText = false;

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
async function operation(label: string, action: () => Promise<void>) {
  if (busy) return;
  busy = true; pauseAuthoring(true); pauseWorkbook(true); pausePresentation(true); controls(); el('status').textContent = label;
  try { await action(); }
  catch (error) {
    const message = String(error).includes('Hidden text cannot be preserved in an editable copy')
      ? '文档含隐藏文字声明，暂不能创建编辑副本，以免丢失内容。原文件保持不变；只读正文可能不显示隐藏文字。未使用或已覆盖的隐藏样式也会保守拦截。'
      : String(error).includes('Unsupported paragraph spacing cannot be preserved in an editable copy')
        ? '文档包含暂不支持的段落间距、行距或网格声明，无法创建编辑副本。已使用样式链中的不支持声明也会保守拦截；原文件保持不变。'
        : String(error);
    notice(`操作未完成：${message}`, true);
  }
  finally { busy = false; pauseAuthoring(false); pauseWorkbook(false); pausePresentation(false); el('status').textContent = '就绪'; controls(); }
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
async function renderPage() {
  if (!pdf) return;
  if (renderTask) { renderTask.cancel(); try { await renderTask.promise; } catch { /* cancelled */ } }
  const page = await pdf.getPage(pageNumber);
  const scale = Number(el<HTMLSelectElement>('zoom').value);
  const viewport = page.getViewport({ scale });
  // Bound the backing canvas for large pages while keeping the aspect ratio.
  const density = Math.min(window.devicePixelRatio || 1, 2, 4096 / Math.max(viewport.width, viewport.height));
  const canvas = document.createElement('canvas'); canvas.className = 'pdf-page';
  canvas.width = Math.ceil(viewport.width * density); canvas.height = Math.ceil(viewport.height * density);
  canvas.style.width = `${viewport.width}px`;
  canvas.setAttribute('aria-label', `PDF 第 ${pageNumber} 页`);
  const context = canvas.getContext('2d'); if (!context) throw new Error('无法创建页面画布');
  const host = el('pdf-pages'); host.replaceChildren(canvas);
  el('page-position').textContent = `${pageNumber} / ${pdf.numPages}`;
  renderTask = page.render({ canvasContext: context, canvas, viewport, transform: [density, 0, 0, density, 0, 0] });
  await renderTask.promise; renderTask = null; page.cleanup();
}
async function showPdf() {
  if (!pdf) {
    const data = await invoke<ArrayBuffer>('read_current_pdf');
    const task = pdfjs.getDocument({ data: new Uint8Array(data), isEvalSupported: false, useSystemFonts: true,
      cMapUrl: '/pdf/cmaps/', cMapPacked: true, standardFontDataUrl: '/pdf/standard_fonts/', wasmUrl: '/pdf/wasm/', iccUrl: '/pdf/iccs/' });
    let cancelled = false;
    task.onPassword = (updatePassword: (password: string) => void, reason: number) => {
      const dialog = el<HTMLDialogElement>('pdf-password');
      const input = el<HTMLInputElement>('pdf-password-input'); input.value = ''; dialog.returnValue = '';
      el('pdf-password-hint').textContent = reason === pdfjs.PasswordResponses.INCORRECT_PASSWORD ? '密码不正确，请重试。密码仅用于本次在本机打开。' : '密码仅用于本次在本机打开文件。';
      dialog.addEventListener('close', () => {
        const password = input.value; input.value = '';
        if (dialog.returnValue === 'unlock') updatePassword(password);
        else { cancelled = true; void task.destroy(); }
      }, { once: true });
      dialog.showModal(); input.focus();
    };
    try { pdf = await task.promise; }
    catch (error) { await task.destroy(); setMode('text'); throw new Error(cancelled ? '已取消解锁 PDF；可点击“PDF 页面”重试。' : `PDF 页面无法打开：${String(error)}`); }
    pageNumber = 1;
  }
  setMode('page');
  try { await renderPage(); } catch (error) { setMode('text'); throw error; }
  if (!pdfTextSummary && current && (!current.inspection.text || current.inspection.text_error)) await recoverPdfText();
  notice(pdfTextSummary ?? '按原文件显示 PDF 页面。搜索请切换到正文；扫描件的文字识别尚未支持。');
}
async function recoverPdfText() {
  if (!pdf || !current) return;
  stopText = false; el('stop-pdf-text').hidden = false;
  try {
    const report = await extractPdfText(pdf, () => stopText, page => { el('status').textContent = `正在提取正文 ${page}/${pdf!.numPages} 页…`; });
    // Preserve an earlier extraction if a retry fails before producing text.
    if (report.text || report.complete) current.inspection.text = report.text;
    if (report.complete) delete current.inspection.text_error;
    pdfTextSummary = textSummary(report);
    renderText(); el('stats').textContent = `${current.inspection.text.length.toLocaleString()} 字符 / ${pdf.numPages} 页`;
    notice(pdfTextSummary);
  } finally { el('stop-pdf-text').hidden = true; }
}
async function openDocument() {
  if (!(await canLeaveAuthoring())) return;
  await operation('正在打开文档…', async () => {
    const opened = await invoke<OpenedDocument | null>('open_document');
    if (!opened) return;
    closeAuthoring(); closeWorkbook(); closePresentation(); closePresentationEditor(); el('reading-toolbar').hidden = false;
    if (pdf) { await pdf.destroy(); pdf = null; }
    current = opened; pageNumber = 1; renderTask = null; pdfTextSummary = null;
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
    if (!current?.presentation) return;
    const find=el<HTMLInputElement>('pptx-find').value;
    if (!find) { notice('请输入要查找的文字。',true);return; }
    const selected=el<HTMLSelectElement>('slide-picker').value;
    const saved=await invoke<(SavedDocument & {presentation:Presentation}) | null>('replace_presentation_current',{find,replacement:el<HTMLInputElement>('pptx-replacement').value});
    if (!saved) { notice('已取消另存，当前演示文稿保持不变。');return; }
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
    'Typography, spacing or alignment outside the basic editor model': '可解析的字体名称、字号与 RGB 颜色会保留；DOCX 中受支持的段前/段后间距和倍数行距会保留；自动色和复杂样式可能改变',
    'Font names are preserved; unavailable fonts may be substituted locally': '保留可解析的字体名称；未安装的字体由本机替代，请核对显示效果',
    'Parser warning: DOCX_FONT_THEME_UNRESOLVED': '部分主题字体无法确定，相关文字使用编辑器默认字体，请核对原文',
  'Theme and automatic text colors normalized to fixed RGB or editor defaults': '主题色近似转为固定 RGB；自动色或无法解析的主题色使用编辑器默认颜色，不保留主题联动',
  'Custom theme color mapping is unsupported': '文档使用了自定义主题颜色映射，暂时只能只读查看',
  'Unsupported theme color scheme content': '文字引用的主题包含尚不支持的颜色来源或变换，暂时只能只读查看',
  'Default document style normalized in editable copy': '编辑副本使用统一的正文和标题默认样式；保留可解析的字体名称、字号与颜色；DOCX 有效段落间距单独保留，请核对字体和换页',
    'List marker appearance normalized to editor defaults': '列表编号外观改为编辑器默认样式',
    'Table widths, borders and cell formatting normalized': '表格宽度、边框和单元格格式改为默认样式',
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
  closePresentationEditor();
  closePresentation();
  closeWorkbook();
  if (pdf) { await pdf.destroy(); pdf = null; }
  current = null; el('empty').hidden = true; el('document').hidden = true; el('pdf-pages').hidden = true;
  el('replace-panel').hidden = true; el('reading-toolbar').hidden = true; controls();
}
initializeAuthoring(prepareEditorWorkspace, notice, async()=>await canLeavePresentation() && await canLeaveWorkbook());
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
