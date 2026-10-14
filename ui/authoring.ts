import {PreservedListGeometry, omitDefaultListGeometry, preflightListGeometryHTML, validateListGeometryDocument, withPreservedListRendering} from './word-list-geometry';
import {EmptyParagraphFont, preflightEmptyParagraphFontHTML, validateEmptyParagraphFontDocument} from './word-empty-paragraph-font';
import {WordEmptyParagraphFontControls} from './word-empty-paragraph-font-controls';
import {WordTableCellColorControls} from './word-table-cell-color-controls';
import {WordTableVerticalControls} from './word-table-vertical-controls';
import {createWordBlockTypeTransaction} from './word-block-type';
import {withWordBlockTypeInteractions} from './word-block-type-interactions';
import {WordTableCellMarginsControls} from './word-table-cell-margins-controls';
import {ParagraphIndent, omitDefaultParagraphIndent, validateParagraphIndentDocument} from './word-paragraph-indent';
import {WordParagraphIndentControls} from './word-paragraph-indent-controls';
import {WordPageGeometry, documentPageGeometry, validatePageGeometryDocument} from './word-page-geometry';
import {WordPageControls} from './word-page-controls';
import {searchWord,type WordMatch} from './word-search';
import {createWordReplacement} from './word-replace';
import {FontSize, createFontSizeTransaction, parseFontSizePoints, selectedFontSize} from './word-font-size';
import {TextColor, createTextColorTransaction, parseTextColor, selectedTextColor} from './word-text-color';
import {FontFamily, createFontFamilyTransaction, isFontFamilyName, selectedFontFamily} from './word-font-family';
import {STANDARD_WORD_STYLE_PROFILE, documentStyleProfile, editorStyleProfile, snapshotDocument as snapshotStyledDocument, withStandardWordStyle} from './word-style-profile';
import {ParagraphSpacing, createParagraphSpacingTransaction, formatLineSpacingLines, omitDefaultParagraphSpacing, paragraphSpacingFields, parseLineSpacingLines, parseParagraphSpacePoints, selectedParagraphSpacing, validateParagraphSpacingDocument, type ParagraphSpacingField, type ParagraphSpacingPatch} from './word-paragraph-spacing';
import { Editor, Extension, Mark as TiptapMark, Node as TiptapNode, type JSONContent } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import {WordTable, createTableColumnWidthTransaction, createTablePositionTransaction, createTableStructureTransaction, omitDefaultTableWidths, parseTableColumnWidthPoints, selectedTableColumn, validateTableWidthsDocument, type TableStructureAction} from './word-table-widths';
import {isTableAlignment, parseTableIndentPoints, selectedTablePosition, type TablePositionPatch} from './word-table-position';
import { TableKit } from '@tiptap/extension-table';
import {WordImage, omitDefaultImageDimensions, validateImageDimensionsDocument} from './word-image-dimensions';
import {WordImageControls} from './word-image-controls';
import {WordImageParagraphControls} from './word-image-paragraph-controls';
import {AllSelection,TextSelection} from '@tiptap/pm/state';
import {closeHistory} from '@tiptap/pm/history';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { preparePrintSnapshot } from './printing';

const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const supportedLink = (href: string) => href.length <= 2048 && /^(https?:\/\/.+|mailto:.+)$/i.test(href) && !/[\s\x00-\x1f\x7f-\x9f]/.test(href);
const alignments = ['left', 'center', 'right', 'justify'];
function syncDomTextSelection() {
  if (!editor || !(editor.state.selection instanceof TextSelection || editor.state.selection instanceof AllSelection)) return;
  const view=editor.view, selection=view.dom.ownerDocument.getSelection();
  if (!selection?.anchorNode || !selection.focusNode || !view.dom.contains(selection.anchorNode) || !view.dom.contains(selection.focusNode)) return;
  const anchor=view.posAtDOM(selection.anchorNode,selection.anchorOffset),head=view.posAtDOM(selection.focusNode,selection.focusOffset);
  if (!editor.state.doc.resolve(anchor).parent.inlineContent || !editor.state.doc.resolve(head).parent.inlineContent) return;
  if (anchor!==editor.state.selection.anchor || head!==editor.state.selection.head) editor.commands.setTextSelection({from:anchor,to:head});
}
const Superscript=TiptapMark.create({name:'superscript',excludes:'subscript',parseHTML:()=>[{tag:'sup'}],renderHTML:()=>['sup',0]});
const Subscript=TiptapMark.create({name:'subscript',excludes:'superscript',parseHTML:()=>[{tag:'sub'}],renderHTML:()=>['sub',0]});
const WordStarterKit = StarterKit.extend({
  addExtensions() {
    const extensions = this.parent?.() ?? [];
    // Keep code's existing exclusions while allowing explicit font, size and color.
    const excluded = [...extensions, Superscript, Subscript]
      .filter(extension => extension.type === 'mark' && !['fontSize', 'textColor', 'fontFamily'].includes(extension.name))
      .map(extension => extension.name).join(' ');
    return extensions.map(extension => {
      if (extension instanceof TiptapNode && ['paragraph', 'heading'].includes(extension.name)) return withPreservedListRendering(withWordBlockTypeInteractions(extension, {
        locked: () => paused || leaving || saving || printing,
        syncSelection: syncDomTextSelection,
        onError: message => showNotice(message, true),
      }));
      if (extension instanceof TiptapNode && extension.name === 'orderedList') return withPreservedListRendering(extension);
      if (extension.name === 'code' && extension instanceof TiptapMark) return extension.extend({excludes: excluded});
      if (extension.name === 'doc' && extension instanceof TiptapNode) return extension.extend({
        addAttributes: () => ({styleProfile: {default: null, rendered: false, parseHTML: () => null}}),
      });
      return extension;
    });
  },
});
const PageBreak = TiptapNode.create({
  name: 'pageBreak', group: 'block', atom: true, selectable: true,
  parseHTML: () => [{ tag: 'div[data-page-break]' }],
  renderHTML: () => ['div', { 'data-page-break': '', class: 'page-break', 'aria-label': '手动分页符' }],
});
const ParagraphAlignment = Extension.create({
  name: 'paragraphAlignment',
  addGlobalAttributes() {
    return [{ types: ['paragraph', 'heading'], attributes: { textAlign: {
      default: null,
      parseHTML: element => alignments.includes(element.style.textAlign) ? element.style.textAlign : null,
      renderHTML: attrs => alignments.includes(attrs.textAlign) ? { style: `text-align: ${attrs.textAlign}` } : {},
    } } }];
  },
});
let editor: Editor | null = null;
let dirty = false;
let version = 0;
let draftTimer: ReturnType<typeof setTimeout> | null = null;
let draftQueue = Promise.resolve();
let saving = false;
let leaving = false;
let paused = false;
let printing = false;
let fontSizeInputChanged = false;
let fontSizeInputEditing = false;
let textColorInputChanged = false;
let textColorInputEditing = false;
let fontFamilyInputChanged = false;
let fontFamilyInputEditing = false;
let paragraphSpacingInputEditing = false;
let tableWidthInputEditing = false;
let pageControls: WordPageControls | null = null;
let tableCellColorControls: WordTableCellColorControls | null = null;
let tableVerticalControls: WordTableVerticalControls | null = null;
let tableCellMarginsControls: WordTableCellMarginsControls | null = null;
let paragraphIndentControls: WordParagraphIndentControls | null = null;
let imageControls: WordImageControls | null = null;
let emptyParagraphFontControls: WordEmptyParagraphFontControls | null = null;
let imageParagraphControls: WordImageParagraphControls | null = null;
let tableIndentInputEditing = false;
let tableIndentInputChanged = false;
const paragraphSpacingInputChanged = new Set<ParagraphSpacingField>();
const paragraphSpacingIds: Record<ParagraphSpacingField, string> = {
  spaceBefore: 'edit-space-before', spaceAfter: 'edit-space-after', lineSpacing: 'edit-line-spacing',
};
function snapshotDocument(owner: Pick<Editor, 'getJSON'>): JSONContent {
  return omitDefaultListGeometry(omitDefaultParagraphIndent(omitDefaultImageDimensions(omitDefaultTableWidths(omitDefaultParagraphSpacing(snapshotStyledDocument(owner))))));
}
let onChangeMode: (() => Promise<void>) | null = null;
let canLeaveOther: () => Promise<boolean> = async () => true;
let showNotice: (text: string, error?: boolean) => void = () => {};
export const isAuthoring = () => !!editor;
export function pauseAuthoring(value: boolean) { paused = value; editor?.setEditable(!paused && !leaving, false); updateToolbar(); }
export async function openAuthored(content: JSONContent, name: string, path: string) {
  await create(content, false); dirty = false;
  el('title').textContent = name; el('file-name').textContent = name; el('subtitle').textContent = path;
  el('draft-state').textContent = '文档已打开';
  showNotice('已打开可编辑文档。修改后另存 DOCX，原文件会保留。');
}
export async function openImported(content: JSONContent, name: string) {
  await create(withStandardWordStyle(content), false);
  el('title').textContent = `${name} · 编辑副本`; el('file-name').textContent = `${name} · 副本`;
  el('subtitle').textContent = '原文件保留；请核对内容并另存 DOCX';
  el('draft-state').textContent = '已导入，尚未导出 DOCX';
  if (editor) queueDraft(snapshotDocument(editor));
  showNotice('已导入编辑副本。部分版式已规范化，请核对后另存 DOCX。');
}
export async function canLeaveAuthoring(): Promise<boolean> {
  if (!(await canLeaveOther())) return false;
  if (printing) { showNotice('正在准备打印，请稍后再离开。'); return false; }
  if (leaving) return false;
  if (paused) { showNotice('当前操作尚未完成，请稍后再离开。'); return false; }
  if (saving) { showNotice('正在导出 DOCX，请等待导出完成后再离开。'); return false; }
  if (dirty && !window.confirm('当前文档尚未导出 DOCX。保存本地草稿后离开？')) return false;
  leaving = true;
  const owner = editor;
  owner?.setEditable(false, false); updateToolbar();
  try {
    if (draftTimer) { clearTimeout(draftTimer); draftTimer = null; }
    if (owner && dirty && isTauri()) await writeDraft(snapshotDocument(owner));
    else if (owner && dirty) { showNotice('浏览器预览无法保存本地草稿，请复制内容后再离开。', true); return false; }
    else await draftQueue;
    return true;
  } catch (error) {
    showNotice(`草稿保存失败，已保留当前文档：${String(error)}。可以重试或先另存 DOCX。`, true);
    return false;
  } finally { leaving = false; if (owner === editor) owner?.setEditable(!paused, false); updateToolbar(); }
}

let wordMatches:WordMatch[]=[];
let wordMatchIndex=-1,wordSearchTruncated=false;
function refreshWordSearch(){
 const result=editor?searchWord(editor.state.doc,el<HTMLInputElement>('edit-find').value):{matches:[],truncated:false};
 wordMatches=result.matches;wordSearchTruncated=result.truncated;wordMatchIndex=-1;updateWordSearchState();
}
function updateWordSearchState(){
 const query=el<HTMLInputElement>('edit-find').value;
 el('edit-find-state').textContent=!query?'':`${wordMatchIndex<0?'共':wordMatchIndex+1+'/'}${wordMatches.length} 处${wordSearchTruncated?'（扫描不完整）':''}`;
}
function jumpWordMatch(direction:number){
 if(!editor||leaving||paused||printing||!wordMatches.length)return;
 wordMatchIndex=wordMatchIndex<0?(direction>0?0:wordMatches.length-1):(wordMatchIndex+direction+wordMatches.length)%wordMatches.length;
 const match=wordMatches[wordMatchIndex];editor.commands.setTextSelection(match);editor.view.focus();editor.commands.scrollIntoView();updateWordSearchState();updateToolbar();
}
function replaceWordMatch(all=false){
 if(!editor||leaving||paused||printing)return;
 const query=el<HTMLInputElement>('edit-find').value;
 const result=all?searchWord(editor.state.doc,query):{matches:wordMatches[wordMatchIndex]?[wordMatches[wordMatchIndex]]:[],truncated:false};
 if(result.truncated){showNotice('正文扫描不完整，无法全部替换；请缩小查找范围或逐处替换。',true);return;}
 if(!result.matches.length)return;
 const replacement=el<HTMLInputElement>('edit-replacement').value;
 const {state,view}=editor;
 try{
  const transaction=createWordReplacement(state,result.matches,query,replacement);if(!transaction)return;
  transaction.setSelection(TextSelection.create(transaction.doc,result.matches[0].from+replacement.length));
  view.dispatch(closeHistory(transaction).scrollIntoView());view.dispatch(closeHistory(editor.state.tr));view.focus();
  if(all)showNotice(`已替换 ${result.matches.length} 处，可一次撤销。`);
 }catch(error){showNotice(String(error),true);}
}
function applyFontSize(reset = false) {
  if (!editor || leaving || paused || printing || saving) return;
  const input = el<HTMLInputElement>('edit-font-size');
  // Selection synchronization can update the toolbar: read the pending input first.
  const value = input.value.trim(), changed = fontSizeInputChanged;
  const untouchedMixed = input.dataset.fontSizeState === 'mixed' && !changed;
  const halfPoints = reset || !value ? null : parseFontSizePoints(value);
  if (!reset && (input.validity.badInput || (value !== '' && halfPoints === null))) {
    showNotice('字号须为 1–1638 pt，且以 0.5 pt 递增。', true); return;
  }
  syncDomTextSelection();
  try {
    if (!(untouchedMixed && !value && !reset)) {
      const transaction = createFontSizeTransaction(editor.state, halfPoints);
      if (transaction) {
        editor.view.dispatch(closeHistory(transaction));
        // Keep the next typing action separate while preserving cursor stored marks.
        editor.view.dispatch(closeHistory(editor.state.tr));
      }
    }
  } catch (error) { showNotice(String(error), true); return; }
  fontSizeInputEditing = false;
  editor.view.focus();
  updateToolbar();
}
function applyTextColor(reset = false) {
  if (!editor || leaving || paused || printing || saving) return;
  const input = el<HTMLInputElement>('edit-text-color');
  // Read before DOM-selection synchronization can refresh the toolbar.
  const value = input.value.trim(), changed = textColorInputChanged;
  const untouchedMixed = input.dataset.textColorState === 'mixed' && !changed;
  const rgb = reset || !value ? null : parseTextColor(value);
  if (!reset && value !== '' && rgb === null) {
    showNotice('文字颜色须为六位十六进制 RGB，例如 #C03030。', true); return;
  }
  syncDomTextSelection();
  try {
    if (!(untouchedMixed && !value && !reset)) {
      const transaction = createTextColorTransaction(editor.state, rgb);
      if (transaction) {
        editor.view.dispatch(closeHistory(transaction));
        editor.view.dispatch(closeHistory(editor.state.tr));
      }
    }
  } catch (error) { showNotice(String(error), true); return; }
  textColorInputEditing = false;
  editor.view.focus();
  updateToolbar();
}
function applyFontFamily(reset = false) {
  if (!editor || leaving || paused || printing || saving) return;
  const input = el<HTMLInputElement>('edit-font-family');
  // Read before selection synchronization can refresh the toolbar.
  const value = input.value, changed = fontFamilyInputChanged;
  const untouchedMixed = input.dataset.fontFamilyState === 'mixed' && !changed;
  if (!reset && value !== '' && !isFontFamilyName(value)) {
    showNotice('字体名须为 1–31 个 UTF-16 字符，不含首尾空白、控制字符或无效 Unicode。', true); return;
  }
  syncDomTextSelection();
  try {
    if (!(untouchedMixed && !value && !reset)) {
      const transaction = createFontFamilyTransaction(editor.state, reset || !value ? null : value);
      if (transaction) {
        editor.view.dispatch(closeHistory(transaction));
        editor.view.dispatch(closeHistory(editor.state.tr));
      }
    }
  } catch (error) { showNotice(String(error), true); return; }
  fontFamilyInputEditing = false;
  editor.view.focus();
  updateToolbar();
}
function applyParagraphSpacing(reset = false) {
  if (!editor || leaving || paused || printing || saving) return;
  const patch: ParagraphSpacingPatch = {};
  // Read the whole group before selection synchronization updates the toolbar.
  for (const field of paragraphSpacingFields) {
    const input = el<HTMLInputElement>(paragraphSpacingIds[field]), value = input.value.trim();
    if (reset) { patch[field] = null; continue; }
    if (!value && input.dataset.spacingState === 'mixed' && !paragraphSpacingInputChanged.has(field)) continue;
    const parsed = !value ? null : field === 'lineSpacing' ? parseLineSpacingLines(value) : parseParagraphSpacePoints(value);
    if (input.validity.badInput || value !== '' && parsed === null) {
      showNotice('段前/段后须为 0–1584 pt（0.05 pt 递增），行距须为 0.5–10 行；非整小数可输入 241/240 这样的精确行数。', true);
      return;
    }
    patch[field] = parsed;
  }
  syncDomTextSelection();
  try {
    const transaction = createParagraphSpacingTransaction(editor.state, patch);
    if (transaction) {
      editor.view.dispatch(closeHistory(transaction));
      editor.view.dispatch(closeHistory(editor.state.tr));
    }
  } catch (error) { showNotice(String(error), true); return; }
  paragraphSpacingInputEditing = false;
  paragraphSpacingInputChanged.clear();
  editor.view.focus();
  updateToolbar();
}
function applyTableColumnWidth(reset = false) {
  if (!editor || leaving || paused || printing || saving) return;
  const input = el<HTMLInputElement>('edit-column-width'), value = input.value.trim();
  const width = reset ? null : parseTableColumnWidthPoints(value);
  if (!reset && (input.validity.badInput || width === null)) {
    showNotice('列宽须为 0.05–1584 pt（0.05 pt 递增）；使用“自动列宽”恢复整表默认布局。', true); return;
  }
  syncDomTextSelection();
  const wasAuto = !selectedTableColumn(editor.state)?.widths;
  try {
    const transaction = createTableColumnWidthTransaction(editor.state, width);
    if (transaction) {
      editor.view.dispatch(closeHistory(transaction));
      editor.view.dispatch(closeHistory(editor.state.tr));
      if (wasAuto && !reset) showNotice('已固定整表列宽：先按 450 pt 总宽平均分配各列，再应用当前列宽；可逐列调整或一次撤销。');
    }
  } catch (error) { showNotice(String(error), true); return; }
  tableWidthInputEditing = false;
  editor.view.focus(); updateToolbar();
}
function changeTableStructure(action: TableStructureAction) {
  if (!editor || leaving || paused || printing || saving) return;
  syncDomTextSelection();
  try {
    const transaction = createTableStructureTransaction(editor.state, action);
    if (transaction) {
      editor.view.dispatch(closeHistory(transaction));
      editor.view.dispatch(closeHistory(editor.state.tr));
    }
  } catch (error) { showNotice(String(error), true); return; }
  editor.view.focus(); updateToolbar();
}
function applyTablePosition(patch?: TablePositionPatch) {
  if (!editor || leaving || paused || printing || saving) return;
  if (!patch) {
    const input = el<HTMLInputElement>('edit-table-indent'), value = input.value.trim();
    if (!value && input.dataset.tableIndentState === 'mixed' && !tableIndentInputChanged) return;
    const indent = value ? parseTableIndentPoints(value) : null;
    if (input.validity.badInput || value !== '' && indent === null) {
      showNotice('左缩进须为 0–1584 pt（0.05 pt 递增）；清空后应用恢复默认，0 表示显式零缩进。', true); return;
    }
    patch = {tableIndentTwips: indent};
  }
  syncDomTextSelection();
  try {
    const transaction = createTablePositionTransaction(editor.state, patch);
    if (transaction) {
      editor.view.dispatch(closeHistory(transaction));
      editor.view.dispatch(closeHistory(editor.state.tr));
    }
  } catch (error) {showNotice(String(error), true); updateToolbar(); return;}
  tableIndentInputEditing = false;
  tableIndentInputChanged = false;
  editor.view.focus(); updateToolbar();
}
function updateToolbar() {
  el('authoring-sheet').dataset.styleProfile = editor ? editorStyleProfile(editor) ?? 'legacy-v1' : 'legacy-v1';
  const locked = leaving || paused || printing;
  el<HTMLInputElement>('edit-find').disabled=locked;
  el<HTMLInputElement>('edit-replacement').disabled=locked;
  for (const button of document.querySelectorAll<HTMLButtonElement>('#editor-toolbar button')) button.disabled = locked;
  // Image-specific selection/saving locks must follow the toolbar defaults.
  pageControls?.update();
  emptyParagraphFontControls?.update();
  paragraphIndentControls?.update();
  tableCellMarginsControls?.update();
  tableCellColorControls?.update();
  tableVerticalControls?.update();
  imageControls?.update();
  imageParagraphControls?.update();
  for (const mark of ['bold', 'italic', 'underline', 'strike', 'superscript', 'subscript', 'bulletList', 'orderedList']) {
    el(`edit-${mark}`).classList.toggle('active', !!editor?.isActive(mark));
    el(`edit-${mark}`).setAttribute('aria-pressed', String(!!editor?.isActive(mark)));
  }
  for(const id of ['edit-find-prev','edit-find-next'])el<HTMLButtonElement>(id).disabled=locked||wordMatches.length===0;
  el<HTMLButtonElement>('edit-replace-current').disabled=locked||wordMatchIndex<0||!wordMatches[wordMatchIndex];
  el<HTMLButtonElement>('edit-replace-all').disabled=locked||!wordMatches.length||wordSearchTruncated;
  el('edit-replace-all').title=wordSearchTruncated?'扫描不完整，不能全部替换':'完整扫描后一次替换所有匹配，可一次撤销；使用各匹配首字符的格式和链接';
  el<HTMLButtonElement>('edit-undo').disabled = locked || !editor?.can().undo();
  el<HTMLButtonElement>('edit-redo').disabled = locked || !editor?.can().redo();
  for (const action of ['row', 'column', 'delete-row', 'delete-column', 'delete-table']) {
    el<HTMLButtonElement>(`edit-${action}`).disabled = locked || saving || !editor?.isActive('table');
  }
  el<HTMLButtonElement>('edit-page-break').disabled = locked || !!editor?.isActive('listItem') || !!editor?.isActive('table');
  el<HTMLButtonElement>('edit-save').disabled = locked || saving || !isTauri();
  el<HTMLButtonElement>('edit-export-pdf').disabled = locked || saving || !isTauri();
  el<HTMLSelectElement>('edit-style').disabled = locked;
  el<HTMLSelectElement>('edit-align').disabled = locked;
  el<HTMLSelectElement>('edit-align').value = String(editor?.getAttributes(editor?.isActive('heading') ? 'heading' : 'paragraph').textAlign ?? 'left');
  el<HTMLSelectElement>('edit-style').value = editor?.isActive('heading') ? String(editor.getAttributes('heading').level) : '0';
  const column = editor ? selectedTableColumn(editor.state) : null;
  const columnWidth = el<HTMLInputElement>('edit-column-width');
  columnWidth.disabled = locked || saving || !column;
  el<HTMLButtonElement>('edit-column-width-apply').disabled = locked || saving || !column;
  el<HTMLButtonElement>('edit-column-width-reset').disabled = locked || saving || !column?.widths;
  if (!tableWidthInputEditing && document.activeElement !== columnWidth) {
    columnWidth.value = column?.widths ? String(column.widths[column.column] / 20) : '';
    columnWidth.placeholder = column ? '自动' : '先选表格';
    columnWidth.dataset.columnWidthState = column?.widths ? 'fixed' : 'auto';
  }
  el('edit-column-width-state').textContent = column ? `第 ${column.column + 1} 列${column.widths ? ` · 总宽 ${column.widths.reduce((sum, width) => sum + width, 0) / 20} pt` : ''}` : '';
  const position = editor ? selectedTablePosition(editor.state) : null;
  const positionLocked = locked || saving || !position?.supported;
  const tableAlign = el<HTMLSelectElement>('edit-table-align'), tableIndent = el<HTMLInputElement>('edit-table-indent');
  tableAlign.disabled = positionLocked;
  tableAlign.value = position?.tableAlign.mixed ? 'mixed' : position?.tableAlign.value ?? '';
  tableAlign.dataset.tableAlignState = position?.tableAlign.mixed ? 'mixed' : position?.tableAlign.value == null ? 'default' : 'explicit';
  tableIndent.disabled = positionLocked;
  el<HTMLButtonElement>('edit-table-indent-apply').disabled = positionLocked;
  el<HTMLButtonElement>('edit-table-position-reset').disabled = positionLocked || !position?.hasExplicit;
  if (!tableIndentInputEditing && document.activeElement !== tableIndent) {
    tableIndent.value = position?.tableIndentTwips.value == null ? '' : String(position.tableIndentTwips.value / 20);
    tableIndent.placeholder = !position?.count ? '先选表格' : position.tableIndentTwips.mixed ? '混合' : '默认';
    tableIndent.dataset.tableIndentState = position?.tableIndentTwips.mixed ? 'mixed' : position?.tableIndentTwips.value == null ? 'default' : 'explicit';
    tableIndentInputChanged = false;
  }
  el('edit-table-position-state').textContent = !position?.count ? '' : !position.supported ? '仅限无合并、无嵌套的矩形表格'
    : position.count > 1 ? `${position.count} 张表格` : '当前表格';
  const selectedSpacing = editor ? selectedParagraphSpacing(editor.state) : null;
  for (const field of paragraphSpacingFields) {
    const input = el<HTMLInputElement>(paragraphSpacingIds[field]);
    input.disabled = locked || saving;
    if (!paragraphSpacingInputEditing && document.activeElement !== input) {
      const selected = selectedSpacing?.[field] ?? {value: null, mixed: false};
      input.value = selected.value === null ? '' : field === 'lineSpacing' ? formatLineSpacingLines(selected.value) : String(selected.value / 20);
      input.placeholder = selected.mixed ? '混合' : '默认';
      input.dataset.spacingState = selected.mixed ? 'mixed' : selected.value === null ? 'default' : 'explicit';
      paragraphSpacingInputChanged.delete(field);
    }
  }
  el<HTMLButtonElement>('edit-paragraph-spacing-apply').disabled = locked || saving;
  el<HTMLButtonElement>('edit-paragraph-spacing-reset').disabled = locked || saving;
  const fontFamily = el<HTMLInputElement>('edit-font-family');
  fontFamily.disabled = locked || saving;
  el<HTMLButtonElement>('edit-font-family-apply').disabled = locked || saving;
  el<HTMLButtonElement>('edit-font-family-reset').disabled = locked || saving;
  if (!fontFamilyInputEditing && document.activeElement !== fontFamily) {
    const selected = editor ? selectedFontFamily(editor.state) : {name: null, mixed: false};
    fontFamily.value = selected.name ?? '';
    fontFamily.placeholder = selected.mixed ? '混合' : '默认';
    fontFamily.dataset.fontFamilyState = selected.mixed ? 'mixed' : selected.name === null ? 'default' : 'explicit';
    fontFamilyInputChanged = false;
  }
  const fontSize = el<HTMLInputElement>('edit-font-size');
  fontSize.disabled = locked || saving;
  el<HTMLButtonElement>('edit-font-size-apply').disabled = locked || saving;
  el<HTMLButtonElement>('edit-font-size-reset').disabled = locked || saving;
  if (!fontSizeInputEditing && document.activeElement !== fontSize) {
    const selected = editor ? selectedFontSize(editor.state) : {halfPoints: null, mixed: false};
    fontSize.value = selected.halfPoints === null ? '' : String(selected.halfPoints / 2);
    fontSize.placeholder = selected.mixed ? '混合' : '默认';
    fontSize.dataset.fontSizeState = selected.mixed ? 'mixed' : selected.halfPoints === null ? 'default' : 'explicit';
    fontSizeInputChanged = false;
  }
  const textColor = el<HTMLInputElement>('edit-text-color');
  const textColorPicker = el<HTMLInputElement>('edit-text-color-picker');
  textColor.disabled = locked || saving;
  textColorPicker.disabled = locked || saving;
  el<HTMLButtonElement>('edit-text-color-apply').disabled = locked || saving;
  el<HTMLButtonElement>('edit-text-color-reset').disabled = locked || saving;
  el<HTMLButtonElement>('edit-text-color-black').disabled = locked || saving;
  const selectedColor = editor ? selectedTextColor(editor.state) : {rgb: null, mixed: false};
  if (document.activeElement !== textColorPicker) {
    textColorPicker.value = selectedColor.rgb === null ? '#000000' : `#${selectedColor.rgb}`;
    textColorPicker.title = selectedColor.mixed
      ? '当前选区颜色混合；色盘以黑色开始。可点黑色色块直接设为黑色'
      : selectedColor.rgb === null
        ? '当前使用默认颜色；色盘以黑色开始。可点黑色色块直接设为黑色'
        : `当前文字颜色 #${selectedColor.rgb}；确认选择后应用`;
  }
  if (!textColorInputEditing && document.activeElement !== textColor) {
    textColor.value = selectedColor.rgb === null ? '' : `#${selectedColor.rgb}`;
    textColor.placeholder = selectedColor.mixed ? '混合' : '默认';
    textColor.dataset.textColorState = selectedColor.mixed ? 'mixed' : selectedColor.rgb === null ? 'default' : 'explicit';
    textColorInputChanged = false;
  }
}

function writeDraft(doc: JSONContent): Promise<void> {
  const generation = version;
  const owner = editor;
  // Serialize writes so an older autosave can never overwrite a newer one.
  const write = draftQueue.then(async () => {
    await invoke('save_draft', { doc });
    if (editor === owner && generation === version) el('draft-state').textContent = '草稿已保存在本机';
  });
  draftQueue = write.catch(() => {});
  return write;
}
function queueDraft(doc: JSONContent) {
  if (!isTauri()) return;
  const owner = editor, generation = version;
  void writeDraft(doc).catch(error => {
    if (editor === owner && generation === version) el('draft-state').textContent = `草稿保存失败：${String(error)}`;
  });
}
export function closeAuthoring() {
  pageControls?.reset();
  emptyParagraphFontControls?.reset();
  paragraphIndentControls?.reset();
  tableCellMarginsControls?.reset();
  tableCellColorControls?.reset();
  tableVerticalControls?.reset();
  imageControls?.reset();
  imageParagraphControls?.reset();
  tableWidthInputEditing = false;
  tableIndentInputEditing = false;
  tableIndentInputChanged = false;
  paragraphSpacingInputEditing = false;
  paragraphSpacingInputChanged.clear();
  fontFamilyInputEditing = false;
  fontSizeInputEditing = false;
  textColorInputEditing = false;
  el('print-document').replaceChildren();
  if (draftTimer) { clearTimeout(draftTimer); draftTimer = null; if (editor && dirty) queueDraft(snapshotDocument(editor)); }
  editor?.destroy(); editor = null; dirty = false;
  el<HTMLInputElement>('edit-find').value='';refreshWordSearch();
  el<HTMLInputElement>('edit-replacement').value='';
  el('authoring-sheet').hidden = true; el('editor-toolbar').hidden = true;
}

async function create(content?: JSONContent, ask = true) {
  const initial = content ?? {type: 'doc', attrs: {styleProfile: STANDARD_WORD_STYLE_PROFILE}, content: [{type: 'paragraph'}]};
  documentStyleProfile(initial);
  validateListGeometryDocument(initial);
  validateEmptyParagraphFontDocument(initial);
  validateParagraphSpacingDocument(initial);
  validateTableWidthsDocument(initial);
  validateImageDimensionsDocument(initial);
  validatePageGeometryDocument(initial);
  validateParagraphIndentDocument(initial);
  if (ask && !(await canLeaveAuthoring())) return;
  // Flush the previous document before a new draft enters the same slot.
  closeAuthoring();
  await onChangeMode?.();
  el('authoring-sheet').hidden = false; el('editor-toolbar').hidden = false;
  el('title').textContent = '新文档'; el('subtitle').textContent = '编辑后另存为 DOCX';
  el('file-name').textContent = '新文档'; el('format').hidden = false; el('format').textContent = 'DOCX';
  editor = new Editor({
    editable: !paused,
    element: el('editor-content'),
    extensions: [PreservedListGeometry.configure({onError: message => showNotice(message, true), locked: () => paused || leaving || saving || printing}),EmptyParagraphFont.configure({onError: message => showNotice(message, true)}),FontFamily,FontSize,TextColor,Superscript,Subscript,WordStarterKit.configure({ heading: { levels: [1, 2, 3] }, link: { openOnClick: false, autolink: false, linkOnPaste: false, isAllowedUri: url => supportedLink(url) }, blockquote: false, horizontalRule: false, codeBlock: false }),
      WordPageGeometry.configure({onPageError: message => showNotice(message, true)}), ParagraphAlignment, ParagraphSpacing, ParagraphIndent.configure({onIndentError: message => showNotice(message, true)}), PageBreak, TableKit.configure({ table: false }), WordTable.configure({onWidthError: message => showNotice(message, true)}), WordImage.configure({onDimensionError: message => showNotice(message, true)})],
    content: initial,
    editorProps: {
      attributes: state => ({ 'aria-label': '文档编辑区', spellcheck: 'false', 'data-style-profile': state.doc.attrs.styleProfile ?? 'legacy-v1', ...(state.doc.attrs.pageGeometry ? {'data-page-geometry': JSON.stringify(state.doc.attrs.pageGeometry)} : {}) }),
      handleKeyDown: (_view,event) => {
        // WebView may deliver the next editing key before selectionchange.
        if (!event.isComposing && event.keyCode !== 229 && ['Enter','Backspace','Delete'].includes(event.key)) syncDomTextSelection();
        return false;
      },
      // Do not load remote images pasted from web/Office HTML. Preserve text.
      transformPastedHTML: html => {
        // Audit raw empty-font metadata before remote-image removal can erase it.
        const parsed = new DOMParser().parseFromString(preflightEmptyParagraphFontHTML(preflightListGeometryHTML(html)), 'text/html');
        parsed.querySelectorAll('img').forEach(image => {
          if (!/^data:image\/(png|jpeg);base64,/.test(image.getAttribute('src') ?? '')) image.remove();
        });
        return parsed.body.innerHTML;
      },
    },
    onUpdate: ({editor: owner}) => {
      if (owner !== editor) return;
      refreshWordSearch();
      dirty = true; version++; el('draft-state').textContent = '有未导出的修改';
      el('stats').textContent = `${owner.getText().length.toLocaleString()} 字符`;
      if (draftTimer) clearTimeout(draftTimer);
      draftTimer = setTimeout(() => { draftTimer = null; if (owner === editor) queueDraft(snapshotDocument(owner)); }, 700);
      updateToolbar();
    },
    onFocus: () => { tableCellColorControls?.cancelEditing(); tableVerticalControls?.cancelEditing(); tableCellMarginsControls?.cancelEditing(); paragraphIndentControls?.cancelEditing(); tableIndentInputEditing = false; tableWidthInputEditing = false; paragraphSpacingInputEditing = false; fontFamilyInputEditing = false; fontSizeInputEditing = false; textColorInputEditing = false; updateToolbar(); },
    onSelectionUpdate: updateToolbar,
  });
  dirty = !!content; version++;
  el('draft-state').textContent = content ? '已恢复本地草稿，尚未导出 DOCX' : '新文档';
  el('stats').textContent = `${editor.getText().length.toLocaleString()} 字符`;
  updateToolbar(); editor.commands.focus();
  showNotice('支持标题、列表、链接、图片、分页和简单表格。DOC/DOCX 可导入编辑副本，修改后另存 DOCX，原文件保留。');
}

export function initializeAuthoring(onMode: () => Promise<void>, notice: typeof showNotice, beforeLeave: () => Promise<boolean> = async () => true, blockWindowClose: () => boolean = () => false) {
  emptyParagraphFontControls = new WordEmptyParagraphFontControls({editor: () => editor, locked: () => leaving || paused || printing || saving, notice: (message, error) => showNotice(message, error), syncSelection: syncDomTextSelection});
  tableCellColorControls = new WordTableCellColorControls({editor: () => editor, locked: () => leaving || paused || printing || saving, notice: (message, error) => showNotice(message, error), syncSelection: syncDomTextSelection});
  tableVerticalControls = new WordTableVerticalControls({editor: () => editor, locked: () => leaving || paused || printing || saving, notice: (message, error) => showNotice(message, error), syncSelection: syncDomTextSelection});
  tableCellMarginsControls = new WordTableCellMarginsControls({editor: () => editor, locked: () => leaving || paused || printing || saving, notice: (message, error) => showNotice(message, error), syncSelection: syncDomTextSelection});
  paragraphIndentControls = new WordParagraphIndentControls({editor: () => editor, locked: () => leaving || paused || printing || saving, notice: (message, error) => showNotice(message, error), syncSelection: syncDomTextSelection});
  imageParagraphControls = new WordImageParagraphControls({editor: () => editor, locked: () => leaving || paused || printing || saving, notice: (message, error) => showNotice(message, error)});
  pageControls = new WordPageControls({editor: () => editor, locked: () => leaving || paused || printing || saving, notice: (message, error) => showNotice(message, error)});
  imageControls = new WordImageControls({editor: () => editor, locked: () => leaving || paused || printing || saving, notice: (message, error) => showNotice(message, error)});
  canLeaveOther = beforeLeave;
  onChangeMode = onMode; showNotice = notice;
  el('new-document').addEventListener('click', () => void create());
  el('restore-draft').addEventListener('click', async () => {
    if (!(await canLeaveAuthoring())) return;
    pauseAuthoring(true);
    try { await draftQueue; const doc = await invoke<JSONContent | null>('load_draft'); if (doc) await create(doc, false); else showNotice('没有可恢复的本地草稿。'); }
    catch (error) { showNotice(`草稿恢复失败：${String(error)}`, true); }
    finally { pauseAuthoring(false); }
  });
  el<HTMLButtonElement>('restore-draft').disabled = !isTauri();
  el('edit-link').addEventListener('click', () => {
    if (!editor) return;
    const href = window.prompt('输入 HTTP、HTTPS 或 mailto 链接；留空移除链接', String(editor.getAttributes('link').href ?? ''));
    if (href === null) return;
    if (!href) { editor.chain().focus().extendMarkRange('link').unsetLink().run(); return; }
    if (!supportedLink(href)) { showNotice('链接须使用 HTTP、HTTPS 或 mailto，且不含空白字符。', true); return; }
    editor.chain().focus().extendMarkRange('link').setLink({ href }).run();
  });
  el('edit-find').addEventListener('input',()=>{refreshWordSearch();updateToolbar();});
  el('paragraph-spacing-controls').addEventListener('focusout', event => {
    if (event.relatedTarget instanceof HTMLElement && el('paragraph-spacing-controls').contains(event.relatedTarget)) return;
    paragraphSpacingInputEditing = false;
    updateToolbar();
  });
  for (const field of paragraphSpacingFields) {
    const input = el<HTMLInputElement>(paragraphSpacingIds[field]);
    input.addEventListener('pointerdown', syncDomTextSelection);
    input.addEventListener('focus', () => { paragraphSpacingInputEditing = true; });
    input.addEventListener('input', () => { paragraphSpacingInputChanged.add(field); });
    input.addEventListener('keydown', event => {
      if (event.isComposing || event.keyCode === 229) return;
      if (event.key === 'Enter') { event.preventDefault(); applyParagraphSpacing(); }
      if (event.key === 'Escape') {
        event.preventDefault(); paragraphSpacingInputEditing = false; paragraphSpacingInputChanged.clear(); editor?.view.focus(); updateToolbar();
      }
    });
  }
  el('edit-paragraph-spacing-apply').addEventListener('click', () => applyParagraphSpacing());
  el('edit-paragraph-spacing-reset').addEventListener('click', () => applyParagraphSpacing(true));
  el('edit-font-family').addEventListener('pointerdown', syncDomTextSelection);
  el('edit-font-family').addEventListener('focus', () => { fontFamilyInputEditing = true; });
  el('edit-font-family').addEventListener('blur', event => {
    const next = event.relatedTarget;
    if (next === el('edit-font-family-apply') || next === el('edit-font-family-reset')) return;
    fontFamilyInputEditing = false;
    updateToolbar();
  });
  el('edit-font-family').addEventListener('input', () => { fontFamilyInputChanged = true; });
  el('edit-font-family').addEventListener('keydown', event => {
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === 'Enter') { event.preventDefault(); applyFontFamily(); }
    if (event.key === 'Escape') { event.preventDefault(); fontFamilyInputEditing = false; editor?.view.focus(); updateToolbar(); }
  });
  el('edit-font-family-apply').addEventListener('click', () => applyFontFamily());
  el('edit-font-family-reset').addEventListener('click', () => applyFontFamily(true));
  el('edit-font-size').addEventListener('pointerdown', syncDomTextSelection);
  el('edit-font-size').addEventListener('focus', () => { fontSizeInputEditing = true; });
  el('edit-font-size').addEventListener('blur', event => {
    const next = event.relatedTarget;
    // Preserve pending input across the focus transfer that precedes an apply click.
    if (next === el('edit-font-size-apply') || next === el('edit-font-size-reset')) return;
    fontSizeInputEditing = false;
    updateToolbar();
  });
  el('edit-font-size').addEventListener('input', () => { fontSizeInputChanged = true; });
  el('edit-font-size').addEventListener('keydown', event => {
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === 'Enter') { event.preventDefault(); applyFontSize(); }
    if (event.key === 'Escape') { event.preventDefault(); fontSizeInputEditing = false; editor?.view.focus(); updateToolbar(); }
  });
  el('edit-font-size-apply').addEventListener('click', () => applyFontSize());
  el('edit-font-size-reset').addEventListener('click', () => applyFontSize(true));
  el('edit-text-color').addEventListener('pointerdown', syncDomTextSelection);
  el('edit-text-color').addEventListener('focus', () => { textColorInputEditing = true; });
  el('edit-text-color').addEventListener('blur', event => {
    const next = event.relatedTarget;
    if (next === el('edit-text-color-apply') || next === el('edit-text-color-reset')) return;
    textColorInputEditing = false;
    updateToolbar();
  });
  el('edit-text-color').addEventListener('input', () => { textColorInputChanged = true; });
  el('edit-text-color').addEventListener('keydown', event => {
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === 'Enter') { event.preventDefault(); applyTextColor(); }
    if (event.key === 'Escape') { event.preventDefault(); textColorInputEditing = false; editor?.view.focus(); updateToolbar(); }
  });
  el('edit-text-color-apply').addEventListener('click', () => applyTextColor());
  el('edit-text-color-reset').addEventListener('click', () => applyTextColor(true));
  el('edit-text-color-picker').addEventListener('pointerdown', () => {
    if (!editor || leaving || paused || printing || saving) return;
    syncDomTextSelection();
  });
  el('edit-text-color-picker').addEventListener('change', () => {
    if (!editor || leaving || paused || printing || saving) return;
    const rgb = parseTextColor(el<HTMLInputElement>('edit-text-color-picker').value);
    if (rgb === null) return;
    el<HTMLInputElement>('edit-text-color').value = `#${rgb}`;
    textColorInputChanged = true;
    textColorInputEditing = true;
    applyTextColor();
  });
  el('edit-text-color-black').addEventListener('pointerdown', () => {
    if (!editor || leaving || paused || printing || saving) return;
    syncDomTextSelection();
  });
  el('edit-text-color-black').addEventListener('click', () => {
    if (!editor || leaving || paused || printing || saving) return;
    el<HTMLInputElement>('edit-text-color').value = '#000000';
    textColorInputChanged = true;
    textColorInputEditing = true;
    applyTextColor();
  });
  el('edit-find-next').addEventListener('click',()=>jumpWordMatch(1));
  el('edit-find-prev').addEventListener('click',()=>jumpWordMatch(-1));
  el('edit-replace-current').addEventListener('click',()=>replaceWordMatch());
  el('edit-replace-all').addEventListener('click',()=>replaceWordMatch(true));
  el('edit-find').addEventListener('keydown',event=>{
    if(event.isComposing||event.keyCode===229)return;
    if(event.key==='Enter'){event.preventDefault();jumpWordMatch(event.shiftKey?-1:1);}
    if(event.key==='Escape'){event.preventDefault();el<HTMLInputElement>('edit-find').value='';refreshWordSearch();updateToolbar();editor?.view.focus();}
  });
  el('edit-clear-marks').addEventListener('click', () => {
    if (!editor || paused || leaving || saving || printing) return;
    syncDomTextSelection();
    const chain=editor.chain();
    for(const mark of ['bold','italic','underline','strike','code','superscript','subscript','fontSize','textColor','fontFamily'])chain.unsetMark(mark);
    chain.run();editor.view.focus();updateToolbar();
  });
  for(const mark of ['strike','superscript','subscript'])el(`edit-${mark}`).addEventListener('click', () => {
    if (!editor || paused || leaving || saving || printing) return;
    syncDomTextSelection();
    editor.chain().toggleMark(mark).run();
    editor.view.focus();
    updateToolbar();
  });
  for (const action of ['bold', 'italic', 'underline', 'bulletList', 'orderedList', 'undo', 'redo']) {
    el(`edit-${action}`).addEventListener('click', () => {
      if (!editor || paused || leaving || saving || printing) return;
      syncDomTextSelection();
      const chain = editor.chain();
      if (action === 'bold') chain.toggleBold().run();
      if (action === 'italic') chain.toggleItalic().run();
      if (action === 'underline') chain.toggleUnderline().run();
      if (action === 'bulletList') chain.toggleBulletList().run();
      if (action === 'orderedList') chain.toggleOrderedList().run();
      if (action === 'undo') chain.undo().run();
      if (action === 'redo') chain.redo().run();
      editor.view.focus();
      updateToolbar();
    });
  }
  el('edit-style').addEventListener('change', () => {
    if (!editor || paused || leaving || saving || printing) return;
    // Read the requested value before selection synchronization refreshes controls.
    const value = Number(el<HTMLSelectElement>('edit-style').value);
    syncDomTextSelection();
    try {
      const transaction = createWordBlockTypeTransaction(editor.state, value);
      if (transaction) {
        editor.view.dispatch(closeHistory(transaction));
        editor.view.dispatch(closeHistory(editor.state.tr));
      }
    } catch (error) { showNotice(String(error), true); }
    editor.view.focus();
    updateToolbar();
  });
  el('edit-align').addEventListener('change', () => {
    const textAlign = el<HTMLSelectElement>('edit-align').value;
    if (!alignments.includes(textAlign)) return;
    if (!editor) return;
    // Native selectionchange may still be queued when a toolbar change arrives.
    syncDomTextSelection();
    editor.chain().updateAttributes('paragraph', { textAlign }).updateAttributes('heading', { textAlign }).run();
    // Vanilla DOM toolbar: restore the current selection now, without a deferred focus frame.
    editor.view.focus();
    updateToolbar();
  });
  el('edit-page-break').addEventListener('click', () => {
    if (!editor || editor.isActive('listItem') || editor.isActive('table')) return;
    editor.chain().focus().insertContent({ type: 'pageBreak' }).createParagraphNear().run();
    // Synchronize the DOM selection before the next input; Tiptap focus is deferred to a frame.
    editor.view.focus();
  });
  el('edit-table').addEventListener('click', () => {
    if (!editor || leaving || paused || printing || saving) return;
    syncDomTextSelection();
    const current = selectedTableColumn(editor.state);
    if (current && (current.widths || current.table.attrs.tableAlign != null || current.table.attrs.tableIndentTwips != null)) { showNotice('已设置固定列宽或位置的表格中不支持嵌套表格，请将光标移到表格外再插入。', true); return; }
    editor.chain().insertTable({rows: 3, cols: 3, withHeaderRow: true}).run(); editor.view.focus();
  });
  for (const [id, action] of Object.entries({row: 'addRowAfter', column: 'addColumnAfter',
    'delete-row': 'deleteRow', 'delete-column': 'deleteColumn', 'delete-table': 'deleteTable'})) {
    el(`edit-${id}`).addEventListener('click', () => changeTableStructure(action as TableStructureAction));
  }
  el('table-width-controls').addEventListener('focusout', event => {
    if (event.relatedTarget instanceof HTMLElement && el('table-width-controls').contains(event.relatedTarget)) return;
    tableWidthInputEditing = false; updateToolbar();
  });
  el('edit-column-width').addEventListener('pointerdown', syncDomTextSelection);
  el('edit-column-width').addEventListener('focus', () => { tableWidthInputEditing = true; });
  el('edit-column-width').addEventListener('keydown', event => {
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === 'Enter') { event.preventDefault(); applyTableColumnWidth(); }
    if (event.key === 'Escape') { event.preventDefault(); tableWidthInputEditing = false; editor?.view.focus(); updateToolbar(); }
  });
  el('edit-column-width-apply').addEventListener('click', () => applyTableColumnWidth());
  el('edit-column-width-reset').addEventListener('click', () => applyTableColumnWidth(true));
  el('table-position-controls').addEventListener('focusout', event => {
    if (event.relatedTarget instanceof HTMLElement && el('table-position-controls').contains(event.relatedTarget)) return;
    tableIndentInputEditing = false; updateToolbar();
  });
  el('edit-table-align').addEventListener('pointerdown', syncDomTextSelection);
  el('edit-table-align').addEventListener('change', () => {
    const align = el<HTMLSelectElement>('edit-table-align').value;
    if (align === '' || isTableAlignment(align)) applyTablePosition({tableAlign: align || null});
  });
  el('edit-table-indent').addEventListener('pointerdown', syncDomTextSelection);
  el('edit-table-indent').addEventListener('focus', () => {tableIndentInputEditing = true;});
  el('edit-table-indent').addEventListener('input', () => {tableIndentInputChanged = true;});
  el('edit-table-indent').addEventListener('keydown', event => {
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === 'Enter') {event.preventDefault(); applyTablePosition();}
    if (event.key === 'Escape') {event.preventDefault(); tableIndentInputEditing = false; tableIndentInputChanged = false; editor?.view.focus(); updateToolbar();}
  });
  el('edit-table-indent-apply').addEventListener('click', () => applyTablePosition());
  el('edit-table-position-reset').addEventListener('click', () => applyTablePosition({tableAlign: null, tableIndentTwips: null}));
  el('edit-image').addEventListener('click', () => {
    if (!editor || leaving || paused || printing || saving) return;
    el<HTMLInputElement>('image-file').click();
  });
  el('image-file').addEventListener('change', async () => {
    const input = el<HTMLInputElement>('image-file'); const file = input.files?.[0]; input.value = '';
    if (!file || !editor || leaving || paused || printing || saving) return;
    if (!['image/png', 'image/jpeg'].includes(file.type) || file.size > 8 * 1024 * 1024) { showNotice('请选择小于 8 MiB 的 PNG 或 JPEG 图片。', true); return; }
    const owner = editor, before = owner.state.doc, selection = owner.state.selection;
    const reader = new FileReader(); reader.onload = () => {
      if (owner !== editor || leaving || paused || printing || saving || owner.state.doc !== before || !owner.state.selection.eq(selection)) return;
      owner.chain().focus().setImage({src: String(reader.result), alt: file.name}).run();
    };
    reader.onerror = () => showNotice('图片读取失败。', true); reader.readAsDataURL(file);
  });
  el('edit-save').addEventListener('click', () => void save());
  const print = async () => {
    if (!editor || paused || leaving || saving || printing) return;
    printing = true; editor.setEditable(false, false); updateToolbar();
    try {
      await preparePrintSnapshot(el('editor-content').querySelector<HTMLElement>('.tiptap')!, documentPageGeometry(snapshotDocument(editor!)));
      showNotice(`${editor!.state.doc.attrs.pageGeometry ? '已准备所选页面的打印副本' : '已准备 A4 打印副本'}。请在系统打印窗口选择打印机或 PDF 目标，并核对预览、纸张、缩放和页眉页脚设置。`);
      window.print();
    } catch (error) { showNotice(`打印准备失败：${String(error)}`, true); }
    finally { printing = false; editor?.setEditable(!paused && !leaving, false); updateToolbar(); }
  };
  el('edit-print').addEventListener('click', () => void print());
  el('edit-export-pdf').addEventListener('click', async () => {
    if (!editor || paused || leaving || saving || printing || !isTauri()) return;
    const wordStyleProfile = editorStyleProfile(editor);
    const wordPageGeometry = documentPageGeometry(snapshotDocument(editor));
    printing = true; editor.setEditable(false, false); updateToolbar();
    try {
      await preparePrintSnapshot(el('editor-content').querySelector<HTMLElement>('.tiptap')!, documentPageGeometry(snapshotDocument(editor!)));
      const saved = wordStyleProfile || wordPageGeometry
        ? await invoke<{ path: string; bytes: number } | null>('export_current_pdf', {...(wordStyleProfile ? {wordStyleProfile} : {}), ...(wordPageGeometry ? {wordPageGeometry} : {})})
        : await invoke<{ path: string; bytes: number } | null>('export_current_pdf');
      showNotice(saved ? `已另存 PDF：${saved.path}。DOCX 中的修改仍需另存。` : '已取消 PDF 导出，文档保留。');
    } catch (error) { showNotice(`PDF 导出失败：${String(error)}`, true); }
    finally { el('print-document').replaceChildren(); printing = false; editor?.setEditable(!paused && !leaving, false); updateToolbar(); }
  });
  window.addEventListener('afterprint', () => el('print-document').replaceChildren());
  document.addEventListener('keydown', event => {
    if(editor&&!paused&&!leaving&&!printing&&(event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='f'){event.preventDefault();el<HTMLInputElement>('edit-find').focus();el<HTMLInputElement>('edit-find').select();}
    if (editor && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') { event.preventDefault(); void save(); }
    if (editor && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'p') { event.preventDefault(); void print(); }
  });
  window.addEventListener('beforeunload', event => { if (dirty) { event.preventDefault(); event.returnValue = ''; } });
  if (isTauri()) {
    try {
      let closingWindow = false;
      void getCurrentWindow().onCloseRequested(async event => {
        event.preventDefault();
        if (closingWindow) return;
        closingWindow = true;
        try {
          if (!(await canLeaveAuthoring())) return;
          // A workspace save can start while an earlier close awaits leave checks.
          // Recheck synchronously immediately before issuing the native destroy.
          if (blockWindowClose()) return;
          const hadDirty = dirty; dirty = false;
          try { await getCurrentWindow().destroy(); }
          catch (error) { dirty = hadDirty; showNotice(`窗口未关闭：${String(error)}`, true); }
        } finally { closingWindow = false; }
      }).catch(error => showNotice(`退出保护未启用：${String(error)}。请先另存 DOCX。`, true));
    } catch (error) { showNotice(`退出保护未启用：${String(error)}。请先另存 DOCX。`, true); }
  }
}

async function save() {
  if (!editor || saving || printing || leaving || paused || !isTauri()) return;
  const owner = editor, generation = version, doc = snapshotDocument(editor);
  saving = true; updateToolbar();
  try {
    const saved = await invoke<{ path: string } | null>('save_new_document', { doc });
    if (saved && editor === owner) {
      if (version === generation) dirty = false;
      showNotice(`已导出 DOCX：${saved.path}${dirty ? '。导出期间新增的修改尚未导出。' : ''}`);
      el('draft-state').textContent = dirty ? '有未导出的修改' : 'DOCX 已导出';
    }
  } catch (error) { showNotice(`DOCX 导出失败：${String(error)}`, true); }
  finally { saving = false; updateToolbar(); }
}
