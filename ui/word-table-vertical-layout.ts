import type {JSONContent} from '@tiptap/core';
import {simpleCellMarginsTable} from './word-table-cell-margins';
import {isParagraphSpace, parseParagraphSpacePoints} from './word-paragraph-spacing';

export const rowMinHeightAttribute = 'rowMinHeightTwips';
export const cellVerticalAlignAttribute = 'cellVerticalAlign';
export const rowMinHeightDataAttribute = 'data-row-min-height-twips';
export const cellVerticalAlignDataAttribute = 'data-cell-vertical-align';
export type CellVerticalAlignment = 'top' | 'center' | 'bottom';
export interface TableVerticalPatch {rowMinHeightTwips?: number | null; cellVerticalAlign?: CellVerticalAlignment | null}
export const tableVerticalError = '行最小高度须为 0–1584 pt（0.05 pt 递增）且不超过页面正文高度；单元格垂直对齐只支持顶部、居中、底部。仅支持正文顶层无合并、无嵌套的矩形表格。';
export const isRowMinHeight = isParagraphSpace;
export const parseRowMinHeightPoints = parseParagraphSpacePoints;
export const isCellVerticalAlignment = (value: unknown): value is CellVerticalAlignment => value === 'top' || value === 'center' || value === 'bottom';
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);
export function parseRowMinHeightMetadata(value: string | null): number | null {
  return value !== null && /^\d{1,5}$/.test(value) && isRowMinHeight(Number(value)) ? Number(value) : null;
}
export function tableBodyHeightTwips(page: unknown): number {
  if (page == null) return 13958;
  const geometry = page as Record<string, number>;
  if (![geometry.heightTwips, geometry.marginTopTwips, geometry.marginBottomTwips].every(Number.isInteger)) throw new Error(tableVerticalError);
  return geometry.heightTwips - geometry.marginTopTwips - geometry.marginBottomTwips;
}
export const simpleVerticalTable = simpleCellMarginsTable;
// Raw model values are checked before schema defaults are introduced. Null is
// editor-only; explicit 0 and explicit top remain durable declarations.
export function validateTableVerticalDocument(document: JSONContent): void {
  const stack: {node: JSONContent; parent?: JSONContent; table?: JSONContent; topLevel: boolean; depth: number}[] = [{node: document, depth: 0, topLevel: false}];
  const checked = new Set<JSONContent>();
  while (stack.length) {
    const {node, parent, table, topLevel, depth} = stack.pop()!;
    for (const field of [rowMinHeightAttribute, cellVerticalAlignAttribute]) {
      if (!node.attrs || !own(node.attrs, field)) continue;
      const row = field === rowMinHeightAttribute, value = node.attrs[field];
      if (!table || !topLevel || (row ? node.type !== 'tableRow' || parent !== table || !isRowMinHeight(value)
        || value > tableBodyHeightTwips(document.attrs?.pageGeometry) : !['tableCell', 'tableHeader'].includes(node.type ?? '')
        || parent?.type !== 'tableRow' || !isCellVerticalAlignment(value))) throw new Error(tableVerticalError);
      if (!checked.has(table)) {if (!simpleVerticalTable(table)) throw new Error(tableVerticalError); checked.add(table);}
    }
    for (const child of node.content ?? []) stack.push({node: child, parent: node, depth: depth + 1,
      table: node.type === 'table' ? node : table, topLevel: node.type === 'table' ? depth === 1 : topLevel});
  }
}
export function omitDefaultTableVertical(document: JSONContent): JSONContent {
  const result = {...document};
  if (result.attrs) {
    result.attrs = {...result.attrs};
    if (result.type === 'tableRow' && result.attrs[rowMinHeightAttribute] === null) delete result.attrs[rowMinHeightAttribute];
    if (['tableCell', 'tableHeader'].includes(result.type ?? '') && result.attrs[cellVerticalAlignAttribute] === null) delete result.attrs[cellVerticalAlignAttribute];
    if (!Object.keys(result.attrs).length) delete result.attrs;
  }
  if (result.content) result.content = result.content.map(omitDefaultTableVertical);
  return result;
}
export function rowMinHeightDOM(attrs: Record<string, unknown>): Record<string, string> {
  const value = attrs[rowMinHeightAttribute];
  // In the CSS table formatting model row height is a minimum: content grows
  // naturally. Never use fixed block height, max-height, or overflow clipping.
  return isRowMinHeight(value) ? {[rowMinHeightDataAttribute]: String(value), style: `height: ${value / 20}pt`} : {};
}
export function cellVerticalAlignDOM(attrs: Record<string, unknown>): Record<string, string> {
  const value = attrs[cellVerticalAlignAttribute];
  return isCellVerticalAlignment(value) ? {[cellVerticalAlignDataAttribute]: value, style: `vertical-align: ${value === 'center' ? 'middle' : value}`} : {};
}
export function tableVerticalAttributes() {
  return [{types: ['tableRow'], attributes: {[rowMinHeightAttribute]: {default: null,
    parseHTML: (element: HTMLElement) => {const raw = element.getAttribute(rowMinHeightDataAttribute); return raw === null ? null : parseRowMinHeightMetadata(raw) ?? 'invalid-row-min-height';},
    renderHTML: rowMinHeightDOM}}}, {types: ['tableCell', 'tableHeader'], attributes: {[cellVerticalAlignAttribute]: {default: null,
    parseHTML: (element: HTMLElement) => {const raw = element.getAttribute(cellVerticalAlignDataAttribute); return raw === null ? null : isCellVerticalAlignment(raw) ? raw : 'invalid-cell-vertical-align';},
    renderHTML: cellVerticalAlignDOM}}}];
}
// Reject owned declarations before schema parsing can erase wrong-node attrs.
// Foreign CSS/exact heights and valign are never promoted into this subset.
export function validTableVerticalHTML(root: ParentNode): boolean {
  return Array.from(root.querySelectorAll(`[${rowMinHeightDataAttribute}]`)).every(element => element.tagName.toLowerCase() === 'tr'
    && parseRowMinHeightMetadata(element.getAttribute(rowMinHeightDataAttribute)) !== null)
    && Array.from(root.querySelectorAll(`[${cellVerticalAlignDataAttribute}]`)).every(element => ['td', 'th'].includes(element.tagName.toLowerCase())
      && isCellVerticalAlignment(element.getAttribute(cellVerticalAlignDataAttribute)));
}
