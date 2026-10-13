import type {JSONContent} from '@tiptap/core';
import {parseParagraphSpacePoints} from './word-paragraph-spacing';

export interface TableCellMargins {left: number; right: number}
export const tableCellMarginsAttribute = 'tableCellMarginsTwips';
export const tableCellMarginsDataAttribute = 'data-table-cell-margins-twips';
export const tableCellMarginsError = '单元格左右边距须完整设置为 0–1584 pt（0.05 pt 递增），且每列宽度必须大于左右边距之和；仅支持正文顶层无合并、无嵌套的矩形表格。';
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);
export function isTableCellMargins(value: unknown): value is TableCellMargins {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const margins = value as TableCellMargins;
  return Object.keys(margins).length === 2 && ['left', 'right'].every(side => own(margins, side))
    && [margins.left, margins.right].every(side => typeof side === 'number' && Number.isInteger(side) && side >= 0 && side <= 31680);
}
export const parseTableCellMarginPoints = parseParagraphSpacePoints;
export function tableCellMargins(attrs: Record<string, unknown>): TableCellMargins {
  const value = attrs[tableCellMarginsAttribute];
  if (value == null) return {left: 108, right: 108};
  if (!isTableCellMargins(value)) throw new Error(tableCellMarginsError);
  return value;
}
export function parseTableCellMarginsMetadata(value: string | null): TableCellMargins | null {
  if (value === null || !/^\d{1,5},\d{1,5}$/.test(value)) return null;
  const [left, right] = value.split(',').map(Number), margins = {left, right};
  return isTableCellMargins(margins) ? margins : null;
}
export function simpleCellMarginsTable(table: JSONContent): boolean {
  const rows = table.content ?? [], count = rows[0]?.content?.length ?? 0;
  if (!rows.length || rows.length > 500 || count < 1 || count > 50) return false;
  if (!rows.every(row => row.type === 'tableRow' && row.content?.length === count && row.content.every(cell =>
    ['tableCell', 'tableHeader'].includes(cell.type ?? '') && (cell.attrs?.colspan ?? 1) === 1 && (cell.attrs?.rowspan ?? 1) === 1 && cell.attrs?.colwidth == null))) return false;
  const stack = [...rows];
  while (stack.length) {const node = stack.pop()!; if (node.type === 'table') return false; stack.push(...node.content ?? []);}
  return true;
}
// Match authored automatic grid widths, including historical truncation and
// the explicit-page unpositioned remainder distribution. Margins alone do not
// opt an automatic table out of page normalization.
export function cellMarginsTableColumnWidths(table: JSONContent, page: unknown): number[] {
  const attrs = table.attrs ?? {}, count = table.content?.[0]?.content?.length ?? 0;
  if (!count) throw new Error(tableCellMarginsError);
  if (attrs.columnWidthsTwips != null) {
    const widths: unknown = attrs.columnWidthsTwips;
    if (!Array.isArray(widths) || widths.length !== count || widths.some(width => typeof width !== 'number' || !Number.isInteger(width) || width < 1 || width > 31680)
      || widths.reduce((sum, width) => sum + width, 0) > 31680) throw new Error(tableCellMarginsError);
    return widths;
  }
  const normalized = page != null && attrs.tableAlign == null && attrs.tableIndentTwips == null;
  const geometry = page as Record<string, number> | null;
  if (normalized && (!geometry || ![geometry.widthTwips, geometry.marginLeftTwips, geometry.marginRightTwips].every(Number.isInteger))) throw new Error(tableCellMarginsError);
  const total = normalized ? Math.min(9000, geometry!.widthTwips - geometry!.marginLeftTwips - geometry!.marginRightTwips) : 9000;
  return Array.from({length: count}, (_, index) => Math.floor(total / count) + Number(normalized && index < total % count));
}
export function validateTableCellMarginsDocument(document: JSONContent): void {
  const stack = [{node: document, depth: 0}];
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (node.attrs && own(node.attrs, tableCellMarginsAttribute)) {
      const margins = node.attrs[tableCellMarginsAttribute];
      if (node.type !== 'table' || depth !== 1 || !isTableCellMargins(margins) || !simpleCellMarginsTable(node)
        || cellMarginsTableColumnWidths(node, document.attrs?.pageGeometry).some(width => width <= margins.left + margins.right)) throw new Error(tableCellMarginsError);
    }
    for (const child of node.content ?? []) stack.push({node: child, depth: depth + 1});
  }
}
export function omitDefaultTableCellMargins(document: JSONContent): JSONContent {
  const result = {...document};
  if (result.type === 'table' && result.attrs?.[tableCellMarginsAttribute] === null) {
    result.attrs = {...result.attrs}; delete result.attrs[tableCellMarginsAttribute];
    if (!Object.keys(result.attrs).length) delete result.attrs;
  }
  if (result.content) result.content = result.content.map(omitDefaultTableCellMargins);
  return result;
}
export function tableCellMarginsDOM(attrs: Record<string, unknown>, automatic: boolean): Record<string, string> {
  const margins = attrs[tableCellMarginsAttribute];
  if (!isTableCellMargins(margins)) return {};
  return {[tableCellMarginsDataAttribute]: `${margins.left},${margins.right}`,
    style: `${automatic ? 'table-layout: auto; width: 600px; min-width: 0; ' : ''}--word-cell-margin-left: ${margins.left / 20}pt; --word-cell-margin-right: ${margins.right / 20}pt`};
}
// Inspect owned clipboard declarations before schema parsing can discard an
// attribute on a cell/paragraph. Ordinary foreign padding is not promoted.
export function validTableCellMarginsHTML(root: ParentNode): boolean {
  return Array.from(root.querySelectorAll(`[${tableCellMarginsDataAttribute}]`)).every(element =>
    element.tagName.toLowerCase() === 'table' && parseTableCellMarginsMetadata(element.getAttribute(tableCellMarginsDataAttribute)) !== null);
}
