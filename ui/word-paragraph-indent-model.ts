import type {JSONContent} from '@tiptap/core';
import {cellMarginsTableColumnWidths, tableCellMargins} from './word-table-cell-margins';

export const paragraphIndentFields = ['indentLeftTwips', 'indentRightTwips', 'firstLineIndentTwips'] as const;
export type ParagraphIndentField = typeof paragraphIndentFields[number];
export type ParagraphIndentPatch = Partial<Record<ParagraphIndentField, number | null>>;
export const paragraphIndentError = '段落缩进仅支持正文、标题和简单顶层表格的单元格段落；左右缩进为 0–1584 pt，首行为 −1584–1584 pt（负数为悬挂），须按 0.05 pt 递增。首行和后续行均须保留正文宽度，悬挂不能越过容器左边。';
export const paragraphIndentTabsError = '段落缩进暂不支持含制表符的段落；制表位尚不能可靠保留，修改未应用。';
export const paragraphIndentTypes = new Set(['paragraph', 'heading']);
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);
export function validParagraphIndent(field: ParagraphIndentField, value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= (field === 'firstLineIndentTwips' ? -31680 : 0) && value <= 31680;
}
export function hasParagraphIndent(node: JSONContent): boolean {
  return paragraphIndentFields.some(field => node.attrs && own(node.attrs, field));
}
export function tableHasParagraphIndent(table: JSONContent): boolean {
  return (table.content ?? []).some(row => (row.content ?? []).some(cell => (cell.content ?? []).some(hasParagraphIndent)));
}
export function simpleIndentTable(table: JSONContent): boolean {
  const rows = table.content ?? [], columns = rows[0]?.content?.length ?? 0;
  if (!rows.length || rows.length > 500 || !columns || columns > 50) return false;
  if (!rows.every(row => row.type === 'tableRow' && row.content?.length === columns && row.content.every(cell =>
    ['tableCell', 'tableHeader'].includes(cell.type ?? '') && (cell.attrs?.colspan ?? 1) === 1 && (cell.attrs?.rowspan ?? 1) === 1 && cell.attrs?.colwidth == null))) return false;
  const stack = rows.flatMap(row => row.content!.flatMap(cell => cell.content ?? []));
  while (stack.length) {const node = stack.pop()!; if (node.type === 'table') return false; stack.push(...node.content ?? []);}
  return true;
}
// Match the authored DOCX grid exactly. Old no-page/positioned automatic grids
// truncate each column; only explicit-page unpositioned grids share remainder.
export function indentTableColumnWidths(table: JSONContent, page: unknown): number[] {
  if (!simpleIndentTable(table)) throw new Error(paragraphIndentError);
  try {return cellMarginsTableColumnWidths(table, page);} catch {throw new Error(paragraphIndentError);}
}
export function indentBodyWidth(page: unknown): number {
  if (page == null) return 9026;
  if (typeof page !== 'object' || Array.isArray(page)) throw new Error(paragraphIndentError);
  const {widthTwips, marginLeftTwips, marginRightTwips} = page as Record<string, number>;
  if (![widthTwips, marginLeftTwips, marginRightTwips].every(Number.isInteger)) throw new Error(paragraphIndentError);
  return widthTwips - marginLeftTwips - marginRightTwips;
}
export function validateParagraphIndentDocument(document: JSONContent): void {
  const page = document.attrs?.pageGeometry, bodyWidth = indentBodyWidth(page);
  const stack: {node: JSONContent; width: number | null; tableWidths?: number[]; cellMarginTotal?: number}[] = [{node: document, width: null}];
  let count = 0;
  while (stack.length) {
    const {node, width, tableWidths, cellMarginTotal} = stack.pop()!;
    if (++count > 10000) throw new Error('段落缩进修改后文档结构超出保存上限。');
    if (hasParagraphIndent(node)) {
      if (!paragraphIndentTypes.has(node.type ?? '') || width === null) throw new Error(paragraphIndentError);
      for (const field of paragraphIndentFields) if (node.attrs && own(node.attrs, field) && !validParagraphIndent(field, node.attrs[field])) throw new Error(paragraphIndentError);
      // Tab stops are not modeled; CSS and DOCX use different default stops.
      // Only new explicit indentation opts into this restriction. Ordinary
      // newlines and hardBreak nodes remain supported, as before.
      const inline = [...node.content ?? []];
      while (inline.length) {
        const child = inline.pop()!;
        if (child.type === 'text' && child.text?.includes('\t')) throw new Error(paragraphIndentTabsError);
        inline.push(...child.content ?? []);
      }
      const left = node.attrs?.indentLeftTwips ?? 0, right = node.attrs?.indentRightTwips ?? 0, first = node.attrs?.firstLineIndentTwips ?? 0;
      if (left + Math.min(first, 0) < 0 || left + right + Math.max(first, 0) >= width) throw new Error(paragraphIndentError);
    }
    for (const [index, child] of (node.content ?? []).entries()) {
      if (node === document) {
        const widths = child.type === 'table' && simpleIndentTable(child) ? indentTableColumnWidths(child, page) : undefined;
        stack.push({node: child, width: paragraphIndentTypes.has(child.type ?? '') ? bodyWidth : null, tableWidths: widths, cellMarginTotal: widths ? tableCellMargins(child.attrs ?? {}).left + tableCellMargins(child.attrs ?? {}).right : undefined});
      } else if (node.type === 'table' && tableWidths && child.type === 'tableRow') stack.push({node: child, width: null, tableWidths, cellMarginTotal});
      else if (node.type === 'tableRow' && tableWidths && ['tableCell', 'tableHeader'].includes(child.type ?? '')) stack.push({node: child, width: tableWidths[index] - (cellMarginTotal ?? 216)});
      else stack.push({node: child, width: ['tableCell', 'tableHeader'].includes(node.type ?? '') && paragraphIndentTypes.has(child.type ?? '') ? width : null});
    }
  }
}
// Strip only schema defaults; explicit zero is durable and raw input nulls are
// rejected by validateParagraphIndentDocument before creating the editor.
export function omitDefaultParagraphIndent(document: JSONContent): JSONContent {
  const result = {...document};
  if (paragraphIndentTypes.has(result.type ?? '') && result.attrs) {
    result.attrs = {...result.attrs};
    for (const field of paragraphIndentFields) if (result.attrs[field] === null) delete result.attrs[field];
    if (!Object.keys(result.attrs).length) delete result.attrs;
  }
  if (result.content) result.content = result.content.map(omitDefaultParagraphIndent);
  return result;
}
