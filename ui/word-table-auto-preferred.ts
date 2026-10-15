import {countRowPaginationHTMLAttributes} from './word-table-row-pagination-model';
import type {Slice} from '@tiptap/pm/model';
import {TextSelection, type EditorState} from '@tiptap/pm/state';
import type {JSONContent} from '@tiptap/core';
import {isPageGeometry} from './word-page-geometry-model';

export const autoPreferredAttribute = 'autoPreferredColumnWidthsTwips';
export const autoPreferredDataAttribute = 'data-auto-preferred-column-widths-twips';
export const autoPreferredError = '自动首选列宽只支持显式页面内的顶层矩形表格；首选总宽须适合正文区域。修改位置、单元格边距或段落缩进前，请先重置列宽或转换为固定列宽。首选值不保证实际自动布局宽度。';
export function isAutoPreferredWidths(value: unknown): value is number[] {
  return Array.isArray(value) && value.length >= 1 && value.length <= 50
    && Array.from(value).every(width => typeof width === 'number' && Number.isInteger(width) && width > 0 && width <= 31680)
    && value.reduce((sum, width) => sum + width, 0) <= 31680;
}
export function parseAutoPreferredMetadata(raw: string | null): number[] | null {
  if (raw === null || raw.length > 300 || !/^\d+(?:,\d+)*$/.test(raw)) return null;
  const widths = raw.split(',').map(Number);
  return isAutoPreferredWidths(widths) ? widths : null;
}
// Pure nominal-budget validation. Never expose these hints as physical frames.
// Editor defaults may be ignored only when validating a live schema document.
export function validateAutoPreferredDocument(document: JSONContent, editorDefaults = false): void {
  const page = document.attrs?.pageGeometry;
  const visit = (node: JSONContent, depth: number, preferredAncestor: boolean) => {
    if (node.marks?.some(mark => mark.attrs && Object.prototype.hasOwnProperty.call(mark.attrs, autoPreferredAttribute))) throw new Error(autoPreferredError);
    const attrs = node.attrs ?? {};
    const has = Object.prototype.hasOwnProperty.call(attrs, autoPreferredAttribute)
      && !(editorDefaults && attrs[autoPreferredAttribute] === null);
    if (preferredAncestor && (node.type === 'table' || ['indentLeftTwips', 'indentRightTwips', 'firstLineIndentTwips'].some(field =>
      Object.prototype.hasOwnProperty.call(attrs, field) && !(editorDefaults && attrs[field] === null)))) throw new Error(autoPreferredError);
    if (has) {
      const widths = attrs[autoPreferredAttribute];
      if (node.type !== 'table' || depth !== 1 || !isAutoPreferredWidths(widths) || attrs.columnWidthsTwips != null
        || attrs.tableCellMarginsTwips != null || ![undefined, null, 'left'].includes(attrs.tableAlign)
        || ![undefined, null, 0].includes(attrs.tableIndentTwips) || !isPageGeometry(page)
        || widths.reduce((sum, width) => sum + width, 0) > page.widthTwips - page.marginLeftTwips - page.marginRightTwips
        || !node.content?.length || node.content.length > 500) throw new Error(autoPreferredError);
      for (const row of node.content) if (row.type !== 'tableRow' || row.content?.length !== widths.length
        || row.content.some(cell => !['tableCell', 'tableHeader'].includes(cell.type ?? '')
          || (cell.attrs?.colspan ?? 1) !== 1 || (cell.attrs?.rowspan ?? 1) !== 1 || cell.attrs?.colwidth != null)) throw new Error(autoPreferredError);
    }
    for (const child of node.content ?? []) visit(child, depth + 1, preferredAncestor || has);
  };
  visit(document, 0, false);
}
export function validAutoPreferredHTML(root: Document | HTMLElement): boolean {
  return Array.from(root.querySelectorAll(`[${autoPreferredDataAttribute}]`)).every(element =>
    element.tagName.toLowerCase() === 'table' && parseAutoPreferredMetadata(element.getAttribute(autoPreferredDataAttribute)) !== null
      && !element.hasAttribute('data-column-widths-twips'));
}

export function isValidAutoPreferredHTML(html: string): boolean {
  if (!/data-auto-preferred-column-widths-twips/i.test(html)) return true;
  const raw = countRowPaginationHTMLAttributes(html, autoPreferredDataAttribute);
  if (html.length > 32 * 1024 * 1024) return raw === 0;
  const parsed = new DOMParser().parseFromString(html, 'text/html');
  return raw === parsed.querySelectorAll(`[${autoPreferredDataAttribute}]`).length && validAutoPreferredHTML(parsed);
}
export function preflightAutoPreferredHTML(html: string): string {
  return isValidAutoPreferredHTML(html) ? html
    : `<table ${autoPreferredDataAttribute}="invalid"><tbody><tr><td><p></p></td></tr></tbody></table>`;
}
// Partial table shells must not silently transfer or discard width ownership.
// Ordinary copied inline text can retain its context shells without owning them.
export function validateAutoPreferredPaste(state: EditorState, slice: Slice): void {
  let owned = false; slice.content.descendants(node => {if (node.attrs[autoPreferredAttribute] != null) owned = true;});
  if (!owned) return;
  const {selection} = state;
  if (slice.openStart === 4 && slice.openEnd === 4 && slice.content.childCount === 1
    && selection instanceof TextSelection && selection.$from.sameParent(selection.$to) && selection.$from.parent.isTextblock) {
    let node = slice.content.firstChild!;
    for (const type of ['table', 'tableRow']) {if (node.type.name !== type || node.childCount !== 1) throw new Error(autoPreferredError); node = node.firstChild!;}
    if (!['tableCell', 'tableHeader'].includes(node.type.name) || node.childCount !== 1) throw new Error(autoPreferredError);
    node = node.firstChild!;
    if (node.isTextblock && Array.from({length: node.childCount}, (_, i) => node.child(i)).every(child => child.isInline)) return;
  }
  const outside = selection.$from.depth === 0 && selection.$to.depth === 0;
  const empty = selection.empty && selection.$from.depth === 1 && selection.$from.parent.type.name === 'paragraph' && !selection.$from.parent.content.size;
  if (slice.openStart || slice.openEnd || !outside && !empty
    || Array.from({length: slice.content.childCount}, (_, i) => slice.content.child(i)).some(node => node.type.name !== 'table')) throw new Error(autoPreferredError);
}
