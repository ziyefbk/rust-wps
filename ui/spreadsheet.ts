export interface Cell { address: string; row: number; column: number; value: { type: string; value?: string | number | boolean }; formula: string | null; formula_cache: string | null; formula_unavailable?: boolean; number_format: string | null }
interface Sheet { name: string; visibility: string; kind: string; cells: Cell[]; merged_ranges: string[] }
export interface Workbook { sheets: Sheet[]; date1904: boolean; recalculated: boolean; warnings: string[] }
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const ROW = 26, COL = 120, ROWS = 1048576, COLS = 16384;
import { invoke, isTauri } from '@tauri-apps/api/core';
import { WorkbookEditor, parseCellInput, type EditableWorkbook, type Content } from './workbook-model';
import { encodeTsv, decodeTsv, decodeTypedClipboard, CELL_MIME } from './workbook-clipboard';
import { translateFormula } from './formula-references';
import {searchWorkbook,type WorkbookSearch} from './workbook-search';
import { formatNumber, type NumericDisplay } from './number-display';
import { validateCalculation, calculationKey, calculationText, type CalculationCell } from './workbook-calculation';
let calculating = false, calculationGeneration = 0, calculationStatus = '未重算';
let calculated = new Map<string, CalculationCell>();
let calculationTimer: ReturnType<typeof setTimeout> | null = null;
function cancelScheduledCalculation() { if(calculationTimer)clearTimeout(calculationTimer);calculationTimer=null; }
function scheduleCalculation() {
  cancelScheduledCalculation();
  if(!editor||!isTauri()||!el<HTMLInputElement>('sheet-auto-calculate').checked)return;
  if(!editor.snapshot().sheets.some(sheet=>sheet.cells.some(cell=>cell.content.type==='formula')))return;
  calculationTimer=setTimeout(()=>{
    calculationTimer=null;
    if(locked()){scheduleCalculation();return;}
    if(!calculating)void calculateWorkbook();
  },350);
}
function resultFor(cell?: Cell) { return cell?.formula != null ? calculated.get(calculationKey(Number(el<HTMLSelectElement>('sheet-picker').value), cell.row, cell.column)) : undefined; }
async function calculateWorkbook() {
  if (!editor || locked() || calculating || !isTauri()) return;
  cancelScheduledCalculation();
  const owner = editor, revision = owner.revision, generation = calculationGeneration, model = owner.snapshot();
  calculating = true; calculated.clear(); calculationStatus = '计算中'; updateEditing(); render();
  try {
    const response = await invoke<unknown>('calculate_workbook', { model });
    if (editor !== owner || owner.revision !== revision || calculationGeneration !== generation) {
      if (editor === owner) info('已丢弃过期计算结果，请重新计算。'); return;
    }
    const report = validateCalculation(response, model);
    calculated = new Map(report.cells.map(cell => [calculationKey(cell.sheet, cell.row, cell.column), cell]));
    calculationStatus = report.complete ? '本次计算完成' : '本次部分计算';
    render();
    const failures = report.cells.filter(cell => cell.failure);
    info(`${calculationStatus}：${report.cells.length - failures.length} 个结果，${failures.length} 个错误或未完成项；未写入 XLSX 缓存。`);
  } catch (error) {
    if (editor === owner && calculationGeneration === generation) { calculationStatus = '计算失败'; info(`计算失败，已保留工作簿：${String(error)}`); }
  } finally { calculating = false; updateEditing(); if(editor===owner&&calculationGeneration!==generation)scheduleCalculation(); }
}
let editor: WorkbookEditor | null = null, paused = false, saving = false, leaving = false;
let draftTimer: ReturnType<typeof setTimeout> | null = null;
let draftQueue: Promise<void> = Promise.resolve();
let draftFailed = false;
let search:WorkbookSearch={matches:[],truncated:false},searchIndex=-1;
function updateSearch(clear=false){
  const input=el<HTMLInputElement>('sheet-search');if(clear)input.value='';
  search=workbook?searchWorkbook(workbook,input.value):{matches:[],truncated:false};searchIndex=-1;
  el('sheet-search-state').textContent=input.value?`${search.matches.length} 个匹配字段${search.truncated?'（达到查找上限，结果不完整）':''}；搜索原值/公式，包含隐藏表`:'';
  for(const id of ['sheet-search-prev','sheet-search-next'])el<HTMLButtonElement>(id).disabled=!search.matches.length;
}
function moveSearch(delta:number){
  if(!workbook||locked()||!search.matches.length)return;
  searchIndex=searchIndex<0?(delta>0?0:search.matches.length-1):(searchIndex+delta+search.matches.length)%search.matches.length;
  const match=search.matches[searchIndex],sheet=workbook.sheets[match.sheet];
  el<HTMLSelectElement>('sheet-picker').value=String(match.sheet);changeSheet(match.sheet);select(match.row,match.column);
  el('sheet-search-state').textContent=`${searchIndex+1}/${search.matches.length} · ${sheet.name}${sheet.visibility!=='Visible'?'（隐藏）':''} · ${columnName(match.column)}${match.row+1} · ${match.field==='formula'?'公式':'原值'}${search.truncated?' · 结果不完整':''}`;
}
function info(text: string) { el('sheet-cell-info').textContent = text; }
function locked() { return paused || saving || leaving; }
function updateEditing() {
  el('spreadsheet').inert = locked();
  el<HTMLButtonElement>('sheet-undo').disabled = !editor?.canUndo;
  el<HTMLButtonElement>('sheet-redo').disabled = !editor?.canRedo;
  el<HTMLButtonElement>('sheet-save').disabled = !isTauri() || !editor;
  el<HTMLButtonElement>('sheet-calculate').disabled = !isTauri() || !editor || calculating;
  el<HTMLInputElement>('sheet-auto-calculate').disabled = !isTauri() || !editor;
  el('sheet-calculation-state').textContent = calculationStatus;
  el<HTMLButtonElement>('sheet-delete').disabled = !editor || editor.sheetCount <= 1;
  const index = Number(el<HTMLSelectElement>('sheet-picker').value);
  el('sheet-hide').textContent = editor?.sheetHidden(index) ? '显示工作表' : '隐藏工作表';
  el<HTMLButtonElement>('sheet-left').disabled = !editor || index <= 0;
  el<HTMLButtonElement>('sheet-right').disabled = !editor || index >= editor.sheetCount - 1;
  el('sheet-edit-state').textContent = `${editor?.dirty ? '有未导出的修改' : '编辑模型已载入'} · ${calculationStatus}`;
}
function writeDraft() {
  if (!editor || !isTauri()) return Promise.resolve();
  const model = editor.snapshot();
  const next = draftQueue.catch(() => {}).then(() => invoke<void>('save_workbook_draft', { model }))
    .then(() => { draftFailed = false; }, error => { draftFailed = true; throw error; });
  draftQueue = next; return next;
}
function queueDraft() {
  if (draftTimer) clearTimeout(draftTimer);
  draftTimer = setTimeout(() => { draftTimer = null; void writeDraft().catch(error => info(`工作簿草稿保存失败：${String(error)}`)); }, 700);
}
function refreshEdited(preferredSheet?: number) {
  if (!editor || !workbook) return;
  if (calculationStatus !== '未重算') calculationStatus = '已过期，需重算';
  calculated.clear(); calculationGeneration++;
  const model = editor.snapshot();
  const picker = el<HTMLSelectElement>('sheet-picker');
  const index = Math.min(preferredSheet ?? Number(picker.value), model.sheets.length - 1);
  workbook = workbookView(model); picker.replaceChildren();
  workbook.sheets.forEach((sheet, i) => { const option = document.createElement('option'); option.value = String(i); option.textContent = `${sheet.name}${sheet.visibility !== 'Visible' ? '（隐藏）' : ''}`; picker.append(option); });
  picker.value = String(index);
  const sheet = workbook.sheets[index];
  cells = new Map(sheet.cells.map(cell => [key(cell.row, cell.column), cell]));
  el('sheet-status').textContent = `${sheet.name} · ${sheet.cells.length.toLocaleString()} 个实际单元格 · ${sheet.kind} · ${model.date1904 ? '1904' : '1900'} 日期系统`;
  select(row, column, false); updateEditing(); queueDraft(); scheduleCalculation(); updateSearch();
}
export async function canLeaveWorkbook() {
  if (!editor) return true;
  if (calculating) { info('公式计算尚未完成，请稍后再离开'); return false; }
  if (locked()) { info('当前工作簿操作尚未完成，请稍后再离开'); return false; }
  if (editor.dirty && !window.confirm('工作簿尚未导出 XLSX。保存本地草稿后离开？')) return false;
  if (editor.dirty && !isTauri()) { info('浏览器预览无法保存工作簿草稿'); return false; }
  leaving = true; updateEditing();
  try {
    const pendingDraft = draftTimer !== null;
    if (draftTimer) { clearTimeout(draftTimer); draftTimer = null; }
    if (isTauri() && (editor.dirty || pendingDraft || draftFailed)) await writeDraft(); else await draftQueue;
    return true;
  } catch (error) { info(`草稿保存失败，已保留工作簿：${String(error)}`); return false; }
  finally { leaving = false; updateEditing(); }
}
async function saveWorkbook() {
  if (!editor || locked() || !isTauri()) return;
  const owner = editor, revision = editor.revision, model = editor.snapshot();
  saving = true; updateEditing();
  try {
    const saved = await invoke<{ path: string } | null>('save_new_workbook', { model });
    if (saved && owner === editor) { owner.markSaved(revision); info(`已导出 XLSX：${saved.path}。未写入本次计算缓存。`); }
  } catch (error) { info(`XLSX 导出失败：${String(error)}`); }
  finally { saving = false; updateEditing(); }
}
let workbook: Workbook | null = null;
let cells = new Map<string, Cell>();
let row = 0, column = 0, anchorRow = 0, anchorColumn = 0, frame = 0;
function selection() { return { top: Math.min(row, anchorRow), bottom: Math.max(row, anchorRow), left: Math.min(column, anchorColumn), right: Math.max(column, anchorColumn) }; }
const key = (r: number, c: number) => `${r}:${c}`;
function columnName(c: number) { let name = ''; for (let n = c + 1; n > 0; n = Math.floor((n - 1) / 26)) name = String.fromCharCode(65 + (n - 1) % 26) + name; return name; }
function rawValue(cell?: Cell): string { if (!cell || cell.value.type === 'empty') return ''; return String(cell.value.value ?? ''); }
function presentation(cell?: Cell): NumericDisplay {
  const result = resultFor(cell);
  if (result) {
    if (result.value?.type === 'number' && typeof result.value.value === 'number' && el<HTMLInputElement>('sheet-formatted').checked) return formatNumber(result.value.value, cell?.number_format, workbook?.date1904);
    return { text: calculationText(result), ...(result.failure ? { fallback: `本次计算：${calculationText(result)}` } : {}) };
  }
  if (!cell || !el<HTMLInputElement>('sheet-formatted').checked || cell.value.type !== 'number' || typeof cell.value.value !== 'number') return { text: rawValue(cell) };
  return formatNumber(cell.value.value, cell.number_format, workbook?.date1904);
}
function display(cell?: Cell): string { return presentation(cell).text; }

function render() {
  frame = 0;
  if (!workbook) return;
  const viewport = el('sheet-viewport'), layer = el('sheet-layer');
  const firstRow = Math.max(0, Math.floor(viewport.scrollTop / ROW));
  const firstCol = Math.max(0, Math.floor(viewport.scrollLeft / COL));
  const lastRow = Math.min(ROWS, firstRow + Math.min(60, Math.ceil(viewport.clientHeight / ROW) + 2));
  const lastCol = Math.min(COLS, firstCol + Math.min(30, Math.ceil(viewport.clientWidth / COL) + 2));
  const fragment = document.createDocumentFragment();
  function box(text: string, x: number, y: number, className: string, role: string) {
    const node = document.createElement('div'); node.className = className; node.textContent = text; node.setAttribute('role', role);
    Object.assign(node.style, { left: `${x}px`, top: `${y}px`, width: `${COL}px`, height: `${ROW}px` });
    fragment.append(node); return node;
  }
  for (let r = firstRow; r < lastRow; r++) {
    for (let c = firstCol; c < lastCol; c++) {
      const cell = cells.get(key(r, c)), address = `${columnName(c)}${r + 1}`;
      const node = box(display(cell), (c + 1) * COL, (r + 1) * ROW, 'sheet-cell', 'gridcell');
      const range = selection();
      node.dataset.address = address; node.setAttribute('aria-label', `${address} ${display(cell)}`); node.setAttribute('aria-selected', String(r >= range.top && r <= range.bottom && c >= range.left && c <= range.right));
      const presented = presentation(cell);
      node.title = `${address}: ${presented.text} · 原值 ${rawValue(cell)}${presented.fallback ? ` · ${presented.fallback}` : ''}`;
      if (presented.fallback) { node.dataset.formatFallback = 'true'; node.setAttribute('aria-label', `${address} ${presented.text} · ${presented.fallback}`); }
      if (cell?.formula_unavailable) {
        node.title += ' · XLS 公式缓存，表达式未恢复';
        node.setAttribute('aria-label', `${node.getAttribute('aria-label')} · XLS 公式缓存，表达式未恢复`);
      }
      node.addEventListener('click', event => { select(r, c, false, event.shiftKey); viewport.focus(); });
      node.addEventListener('dblclick',()=>{if(editor&&!locked()){select(r,c,false);const input=el<HTMLInputElement>('sheet-formula');input.focus();input.select();}});
    }
    box(String(r + 1), viewport.scrollLeft, (r + 1) * ROW, 'sheet-header', 'rowheader');
  }
  for (let c = firstCol; c < lastCol; c++) box(columnName(c), (c + 1) * COL, viewport.scrollTop, 'sheet-header', 'columnheader');
  box('', viewport.scrollLeft, viewport.scrollTop, 'sheet-corner', 'presentation');
  layer.replaceChildren(fragment);
}
function schedule() { if (!frame) frame = requestAnimationFrame(render); }
function select(r: number, c: number, reveal = true, extend = false) {
  row = Math.max(0, Math.min(ROWS - 1, r)); column = Math.max(0, Math.min(COLS - 1, c));
  if (!extend) { anchorRow = row; anchorColumn = column; }
  const cell = cells.get(key(row, column));
  el<HTMLInputElement>('sheet-format-code').value = cell?.number_format ?? '';
  const code = cell?.number_format ?? '';
  const presets = el<HTMLSelectElement>('sheet-format-preset');
  presets.value = [...presets.options].some(option => option.value === code) ? code : 'custom';
  el<HTMLInputElement>('sheet-address').value = `${columnName(column)}${row + 1}`;
  el<HTMLInputElement>('sheet-formula').value = cell?.formula != null ? `=${cell.formula}` : rawValue(cell);
  if (editor) {
    const current = editor.getCell(Number(el<HTMLSelectElement>('sheet-picker').value), row, column);
    el<HTMLSelectElement>('sheet-input-type').value = current?.content.type ?? 'text';
  }
  el('sheet-cell-info').textContent = cell ? `${cell.value.type}${cell.formula_unavailable ? ` · XLS 公式表达式未恢复，仅源缓存 ${cell.formula_cache ?? '未知'}（未重算）` : cell.formula != null ? ` · 源缓存 ${cell.formula_cache ?? '未知'}（未重算）` : ''}${cell.number_format ? ` · ${cell.number_format}` : ''}` : '空单元格';
  const range = selection();
  const presented = presentation(cell);
  const result = resultFor(cell);
  if (result) el('sheet-cell-info').textContent += ` · 本次计算 ${calculationText(result)}（未写入缓存）`;
  if (presented.fallback) el('sheet-cell-info').textContent += ` · ${presented.fallback}`;
  if (range.top !== range.bottom || range.left !== range.right) el('sheet-cell-info').textContent = `选区 ${columnName(range.left)}${range.top + 1}:${columnName(range.right)}${range.bottom + 1} · Shift 扩选 · Ctrl+C 复制内容`;
  const viewport = el('sheet-viewport');
  if (reveal) {
    if (row * ROW < viewport.scrollTop || (row + 2) * ROW > viewport.scrollTop + viewport.clientHeight) viewport.scrollTop = row * ROW;
    if (column * COL < viewport.scrollLeft || (column + 2) * COL > viewport.scrollLeft + viewport.clientWidth) viewport.scrollLeft = column * COL;
  }
  render();
}
function changeSheet(index: number) {
  if (!workbook) return;
  const sheet = workbook.sheets[index];
  cells = new Map(sheet.cells.map(cell => [key(cell.row, cell.column), cell]));
  el('sheet-status').textContent = `${sheet.name} · ${sheet.cells.length.toLocaleString()} 个实际单元格 · ${sheet.kind} · ${sheet.visibility} · ${workbook.date1904 ? '1904' : '1900'} 日期系统${sheet.merged_ranges.length ? ' · 合并范围暂按原始单元格展示' : ''}`;
  el('sheet-viewport').scrollTop = 0; el('sheet-viewport').scrollLeft = 0;
  select(0, 0, false);
  updateEditing();
}
export function closeWorkbook() {
  cancelScheduledCalculation();
  calculated.clear(); calculationGeneration++; calculationStatus = '未重算';
  if (draftTimer) clearTimeout(draftTimer); draftTimer = null; editor = null;
  el('sheet-edit-toolbar').hidden = true; el<HTMLInputElement>('sheet-formula').readOnly = true;
  el('sheet-management').hidden = true;
  el('sheet-format-toolbar').hidden = true;
  workbook = null; updateSearch(true); cells.clear(); if (frame) cancelAnimationFrame(frame); frame = 0;
  el('spreadsheet').hidden = true; el('workspace').classList.remove('spreadsheet-mode'); el('sheet-layer').replaceChildren();
}
export function pauseWorkbook(value: boolean) { paused = value; updateEditing(); }
export function workbookView(model: EditableWorkbook): Workbook {
  return { date1904: model.date1904 ?? false, recalculated: false, warnings: [], sheets: model.sheets.map(sheet => ({ name: sheet.name,
    visibility: sheet.hidden ? 'Hidden' : 'Visible', kind: 'Worksheet', merged_ranges: [], cells: sheet.cells.map(cell => ({
      address: `${columnName(cell.column)}${cell.row + 1}`, row: cell.row, column: cell.column,
      value: cell.content.type === 'formula' ? { type: 'empty' } : cell.content,
      formula: cell.content.type === 'formula' ? cell.content.value : null,
      formula_cache: cell.content.type === 'formula' ? 'absent' : null, number_format: cell.number_format ?? null,
    })) })) };
}
export function openWorkbook(value: Workbook, model?: EditableWorkbook | null, unsaved = false) {
  cancelScheduledCalculation();
  calculated.clear(); calculationGeneration++; calculationStatus = '未重算';
  editor = model ? new WorkbookEditor(model) : null;
  if (unsaved) editor?.markUnsaved();
  el('spreadsheet').setAttribute('aria-label', editor ? '工作簿编辑器' : '只读工作簿');
  el('sheet-edit-toolbar').hidden = !editor; el<HTMLInputElement>('sheet-formula').readOnly = !editor;
  el('sheet-management').hidden = !editor;
  el('sheet-format-toolbar').hidden = !editor;
  workbook = value; el('spreadsheet').hidden = false; el('workspace').classList.add('spreadsheet-mode');
  const picker = el<HTMLSelectElement>('sheet-picker'); picker.replaceChildren();
  value.sheets.forEach((sheet, index) => { const option = document.createElement('option'); option.value = String(index); option.textContent = `${sheet.name}${sheet.visibility !== 'Visible' ? '（隐藏）' : ''}`; picker.append(option); });
  if (value.sheets.length) { const initial = Math.max(0, value.sheets.findIndex(sheet => sheet.visibility === 'Visible')); picker.value = String(initial); changeSheet(initial); }
  else { cells.clear(); el('sheet-status').textContent = '工作簿没有可读取的工作表'; render(); }
  updateEditing();updateSearch(true);
  if (unsaved) queueDraft();
  scheduleCalculation();
}
export function initializeSpreadsheet() {
  el('sheet-search').addEventListener('input',()=>updateSearch());
  el('sheet-search').addEventListener('keydown',event=>{if(event.key==='Enter'&&!event.isComposing&&event.keyCode!==229){event.preventDefault();moveSearch(event.shiftKey?-1:1);}});
  el('sheet-search-prev').addEventListener('click',()=>moveSearch(-1));
  el('sheet-search-next').addEventListener('click',()=>moveSearch(1));
  for (const direction of ['asc', 'desc']) el(`sheet-sort-${direction}`).addEventListener('click', () => {
    if (!editor || locked()) return;
    const range = selection(), sheet = Number(el<HTMLSelectElement>('sheet-picker').value);
    const column = range.left + Number(el<HTMLInputElement>('sheet-sort-key').value) - 1;
    try {
      const results = new Map<number, Content>();
      for (const result of calculated.values()) if (result.sheet === sheet && result.column === column && result.value) results.set(result.row, parseCellInput(result.value.type, result.value.type === 'empty' ? '' : String(result.value.value)));
      if (editor.sortRange(sheet, range, column, direction === 'desc', el<HTMLInputElement>('sheet-sort-header').checked, results)) {
        refreshEdited(); select(range.top, range.left, false); select(range.bottom, range.right, false, true);
        info('已按选区列排序，整行选区内容和格式随行移动，可整体撤销；公式需重新计算。');
      } else info('选区顺序未改变');
    } catch (error) { info(String(error)); }
  });
  el('sheet-calculate').addEventListener('click', () => void calculateWorkbook());
  el('sheet-auto-calculate').addEventListener('change',()=>{scheduleCalculation();info(el<HTMLInputElement>('sheet-auto-calculate').checked?'已开启自动重算：编辑停止后更新支持的公式；未支持项仍明确提示。':'已关闭自动重算，可手动计算公式。');});
  el('sheet-formatted').addEventListener('change', () => {
    if (!workbook) return;
    render();
    const fallback = presentation(cells.get(key(row, column))).fallback;
    info(`${el<HTMLInputElement>('sheet-formatted').checked ? '已开启有限预设格式显示' : '已切换为原值显示'}；公式栏和未应用输入保持不变${fallback ? ` · ${fallback}` : ''}`);
  });
  el('sheet-format-preset').addEventListener('change', () => {
    const value = el<HTMLSelectElement>('sheet-format-preset').value;
    if (value !== 'custom') el<HTMLInputElement>('sheet-format-code').value = value;
  });
  el('sheet-format-apply').addEventListener('click', () => {
    if (!editor || locked()) return;
    const range = selection(), code = el<HTMLInputElement>('sheet-format-code').value;
    try {
      if (editor.setNumberFormat(Number(el<HTMLSelectElement>('sheet-picker').value), range, code || null)) {
        refreshEdited(); select(range.top, range.left, false); select(range.bottom, range.right, false, true);
      }
      info(`选区格式已保存，可整体撤销并导出；${el<HTMLInputElement>('sheet-formatted').checked ? '支持的预设已显示，不支持的代码显示原值并提示' : '网格暂显示原值'}，内容类型和数值未改变。`);
    } catch (error) { info(String(error)); }
  });
  el('sheet-hide').addEventListener('click', () => {
    if (!editor || locked()) return;
    const index = Number(el<HTMLSelectElement>('sheet-picker').value);
    try {
      if (editor.setSheetHidden(index, !editor.sheetHidden(index))) refreshEdited(index);
      info('已更新工作表可见性；隐藏表仍可从工作表列表选择，以便恢复显示。');
    } catch (error) { info(String(error)); }
  });
  for (const [id, delta] of [['sheet-left', -1], ['sheet-right', 1]] as const) el(id).addEventListener('click', () => {
    if (!editor || locked()) return;
    const index = Number(el<HTMLSelectElement>('sheet-picker').value);
    try {
      if (editor.moveSheet(index, index + delta)) refreshEdited(index + delta);
      info('已调整工作表顺序；三维引用按新的表顺序解释，公式未重算。');
    } catch (error) { info(String(error)); }
  });
  el('sheet-add').addEventListener('click', () => {
    if (!editor || locked()) return;
    try {
      const sheets = editor.snapshot().sheets; let n = 1;
      while (sheets.some(sheet => sheet.name.toLowerCase() === `工作表${n}`)) n++;
      editor.addSheet(`工作表${n}`); refreshEdited(sheets.length);
    } catch (error) { info(String(error)); }
  });
  el('sheet-rename').addEventListener('click', () => {
    if (!editor || locked()) return;
    const index = Number(el<HTMLSelectElement>('sheet-picker').value);
    const name = window.prompt('工作表名称', editor.snapshot().sheets[index].name);
    if (name === null) return;
    try { if (editor.renameSheet(index, name)) refreshEdited(index); }
    catch (error) { info(String(error)); }
  });
  el('sheet-delete').addEventListener('click', () => {
    if (!editor || locked()) return;
    const index = Number(el<HTMLSelectElement>('sheet-picker').value);
    if (!window.confirm(`删除工作表“${editor.snapshot().sheets[index].name}”？可撤销，相关公式引用会更新。`)) return;
    try { editor.deleteSheet(index); refreshEdited(index); }
    catch (error) { info(String(error)); }
  });
  el('sheet-viewport').addEventListener('copy', event => {
    if (!workbook || locked() || !event.clipboardData) return;
    const range = selection(), count = (range.bottom - range.top + 1) * (range.right - range.left + 1);
    if (count > 10000) { event.preventDefault(); info('复制最多 10000 个单元格'); return; }
    const rows: string[][] = [];
    for (let r = range.top; r <= range.bottom; r++) { const values: string[] = [];
      for (let c = range.left; c <= range.right; c++) { const cell = cells.get(key(r, c)); values.push(cell?.formula != null ? `=${cell.formula}` : display(cell)); }
      rows.push(values);
    }
    const text = encodeTsv(rows);
    event.preventDefault();
    if (new TextEncoder().encode(text).length > 4 * 1024 * 1024) { info('复制内容超过 4 MiB'); return; }
    event.clipboardData.setData('text/plain', text);
    if (editor) {
      const typedRows: Content[][] = rows.map((values, r) => values.map((_, c) => editor!.getCell(Number(el<HTMLSelectElement>('sheet-picker').value), range.top + r, range.left + c)?.content ?? { type: 'empty' }));
      const typedText = JSON.stringify({ version: 2, origin: { row: range.top, column: range.left }, rows: typedRows });
      if (new TextEncoder().encode(typedText).length > 4 * 1024 * 1024) { event.clipboardData.clearData(); info('复制内容含类型信息后超过 4 MiB'); return; }
      event.clipboardData.setData(CELL_MIME, typedText);
    }
    info(editor ? `已复制 ${count} 个单元格的内容；内部粘贴保留类型并调整 A1 相对引用` : `已复制 ${count} 个单元格的文字内容；粘贴按文本保留`);
  });
  el('sheet-viewport').addEventListener('paste', event => {
    if (!editor || locked() || !event.clipboardData) return;
    event.preventDefault();
    try {
      const typed = event.clipboardData.getData(CELL_MIME);
      if (!typed && !event.clipboardData.types.includes('text/plain')) throw new Error('仅支持表格文字内容');
      const payload = typed ? decodeTypedClipboard(typed) : null;
      const matrix: Content[][] = payload ? payload.rows : decodeTsv(event.clipboardData.getData('text/plain')).map(values => values.map(value => ({ type: 'text', value })));
      const range = selection();
      const inputs = matrix.flatMap((values, r) => values.map((content, c) => ({ row: range.top + r, column: range.left + c, type: content.type,
        input: content.type === 'formula' && payload?.origin ? translateFormula(content.value, range.top - payload.origin.row, range.left - payload.origin.column) : 'value' in content ? String(content.value) : '' })));
      if (editor.setCells(Number(el<HTMLSelectElement>('sheet-picker').value), inputs)) refreshEdited();
      select(range.top, range.left, false);
      select(range.top + matrix.length - 1, range.left + matrix[0].length - 1, true, true);
      info(`已${typed ? '保留类型' : '按文本'}粘贴 ${inputs.length} 个单元格，可整体撤销；${payload?.origin ? '已调整 A1 相对引用' : '缺少源地址，公式按原文保留'}，未重算`);
    } catch (error) { info(`粘贴未完成：${String(error)}`); }
  });
  function applyCurrentCell() {
    if (!editor || locked()) return false;
    try {
      if (editor.setCell(Number(el<HTMLSelectElement>('sheet-picker').value), row, column,
        el<HTMLSelectElement>('sheet-input-type').value as Content['type'], el<HTMLInputElement>('sheet-formula').value)) refreshEdited();
      return true;
    } catch (error) { info(String(error)); return false; }
  }
  el('sheet-apply').addEventListener('click', applyCurrentCell);
  el('sheet-formula').addEventListener('keydown',event=>{
    if(!editor||locked()||event.isComposing||event.keyCode===229||event.ctrlKey||event.metaKey||event.altKey)return;
    if(event.key==='Escape'){
      event.preventDefault();select(row,column,false);el('sheet-viewport').focus();return;
    }
    if(event.key!=='Enter'&&event.key!=='Tab')return;
    event.preventDefault();
    if(!applyCurrentCell())return;
    const delta=event.shiftKey?-1:1;
    select(row+(event.key==='Enter'?delta:0),column+(event.key==='Tab'?delta:0));
    const input=el<HTMLInputElement>('sheet-formula');input.focus();input.select();
  });
  el('sheet-undo').addEventListener('click', () => { if (!locked() && editor?.undo()) refreshEdited(); });
  el('sheet-redo').addEventListener('click', () => { if (!locked() && editor?.redo()) refreshEdited(); });
  el('sheet-save').addEventListener('click', () => void saveWorkbook());
  window.addEventListener('beforeunload', event => { if (editor?.dirty) { event.preventDefault(); event.returnValue = ''; } });
  el('sheet-viewport').addEventListener('scroll', schedule);
  new ResizeObserver(schedule).observe(el('sheet-viewport'));
  el('sheet-picker').addEventListener('change', () => changeSheet(Number(el<HTMLSelectElement>('sheet-picker').value)));
  el('sheet-address').addEventListener('keydown', event => {
    if (event.key !== 'Enter') return;
    event.preventDefault();
    const match = /^([a-z]{1,3})([1-9][0-9]{0,6})$/i.exec(el<HTMLInputElement>('sheet-address').value.trim());
    if (!match) { el('sheet-cell-info').textContent = '请输入 A1 至 XFD1048576 的地址'; return; }
    const col = [...match[1].toUpperCase()].reduce((n, char) => n * 26 + char.charCodeAt(0) - 64, 0) - 1;
    const r = Number(match[2]) - 1;
    if (col >= COLS || r >= ROWS) { el('sheet-cell-info').textContent = '地址超出工作表范围'; return; }
    select(r, col); el('sheet-viewport').focus();
  });
  el('sheet-viewport').addEventListener('keydown', event => {
    if (locked()||event.isComposing||event.keyCode===229) return;
    if(editor&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&(event.key==='F2'||event.key==='Enter')){event.preventDefault();const input=el<HTMLInputElement>('sheet-formula');input.focus();input.select();return;}
    if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='f'){event.preventDefault();event.stopPropagation();const input=el<HTMLInputElement>('sheet-search');input.focus();input.select();return;}
    if (editor && (event.ctrlKey || event.metaKey)) {
      const action = event.key.toLowerCase();
      if (action === 'z' || action === 'y') { event.preventDefault(); if ((action === 'y' || event.shiftKey) ? editor.redo() : editor.undo()) refreshEdited(); return; }
      if (action === 's') { event.preventDefault(); void saveWorkbook(); return; }
    }
    if (editor && (event.key === 'Delete' || event.key === 'Backspace')) {
      event.preventDefault(); const range = selection();
      const count = (range.bottom - range.top + 1) * (range.right - range.left + 1);
      if (count > 10000) { info('清空最多 10000 个单元格'); return; }
      const inputs = [];
      for (let r = range.top; r <= range.bottom; r++) for (let c = range.left; c <= range.right; c++) inputs.push({ row: r, column: c, type: 'empty' as const, input: '' });
      try { if (editor.setCells(Number(el<HTMLSelectElement>('sheet-picker').value), inputs)) refreshEdited(); }
      catch (error) { info(String(error)); }
      return;
    }
    const delta: Record<string, number[]> = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1] };
    if (delta[event.key]) { event.preventDefault(); select(row + delta[event.key][0], column + delta[event.key][1], true, event.shiftKey); }
  });
}
