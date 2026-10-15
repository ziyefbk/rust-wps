import type {JSONContent} from '@tiptap/core';
import {countRowPaginationHTMLAttributes} from './word-table-row-pagination-model';
import {simpleCellMarginsTable} from './word-table-cell-margins';
import {isTextColorRgb, parseTextColor} from './word-text-color';

export const cellFillModeAttribute = 'cellFillMode';
export const cellFillModeDataAttribute = 'data-cell-fill-mode';
export type CellFillMode = 'none';
export const isCellFillMode = (value: unknown): value is CellFillMode => value === 'none';
export const cellFillAttribute = 'cellFillRgb';
export const cellTextColorModeAttribute = 'cellTextColorMode';
export const cellFillDataAttribute = 'data-cell-fill-rgb';
export const cellTextColorModeDataAttribute = 'data-cell-text-color-mode';
export const tableCellColorFields = [cellFillAttribute, cellFillModeAttribute, cellTextColorModeAttribute] as const;
export type CellTextColorMode = 'auto';
export interface TableCellColorsPatch {cellFillRgb?: string | null; cellFillMode?: CellFillMode | null; cellTextColorMode?: CellTextColorMode | null}
export const tableCellColorsError = '单元格底色须为六位十六进制 RGB，例如 #C03030，或独立的无底色；两者不能同时设置；文字颜色模式只支持自动或默认。仅支持正文顶层无合并、无嵌套的矩形表格。';
export const cellAutoColorPreviewWarning = '自动文字色在编辑器及打印中仅按底色亮度近似为黑或白，不代表 Word / LibreOffice 的自动对比；显式文字颜色优先，链接保留原有链接色。';
export const isCellFillRgb = isTextColorRgb;
export const parseCellFill = parseTextColor;
export const isCellTextColorMode = (value: unknown): value is CellTextColorMode => value === 'auto';
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);

// Fill is an exclusive RGB/none declaration; text mode is independent. Absence retains the
// profile/legacy defaults; null exists only inside the editor schema.
export function validateTableCellColorsDocument(document: JSONContent): void {
  const stack: {node: JSONContent; parent?: JSONContent; table?: JSONContent; topLevel: boolean; depth: number}[] = [{node: document, depth: 0, topLevel: false}];
  const checked = new Set<JSONContent>();
  while (stack.length) {
    const {node, parent, table, topLevel, depth} = stack.pop()!;
    if (node.marks?.some(mark => mark.attrs && own(mark.attrs, cellFillModeAttribute))) throw new Error(tableCellColorsError);
    if (node.attrs && own(node.attrs, cellFillAttribute) && own(node.attrs, cellFillModeAttribute)) throw new Error(tableCellColorsError);
    for (const field of tableCellColorFields) {
      if (!node.attrs || !own(node.attrs, field)) continue;
      const value = node.attrs[field];
      if (!table || !topLevel || !['tableCell', 'tableHeader'].includes(node.type ?? '') || parent?.type !== 'tableRow'
        || !(field === cellFillAttribute ? isCellFillRgb(value) : field === cellFillModeAttribute ? isCellFillMode(value) : isCellTextColorMode(value))) throw new Error(tableCellColorsError);
      if (!checked.has(table)) {if (!simpleCellMarginsTable(table)) throw new Error(tableCellColorsError); checked.add(table);}
    }
    for (const child of node.content ?? []) stack.push({node: child, parent: node, depth: depth + 1,
      table: node.type === 'table' ? node : table, topLevel: node.type === 'table' ? depth === 1 : topLevel});
  }
}

// A bounded preview choice only. Never save this derived RGB as a text mark or
// substitute it for the durable "auto" model/DOCX declaration.
export function cellAutoPreviewRgb(fill: unknown): '000000' | 'FFFFFF' {
  if (!isCellFillRgb(fill)) return '000000';
  const channels = [0, 2, 4].map(at => parseInt(fill.slice(at, at + 2), 16) / 255)
    .map(value => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4);
  const luminance = .2126 * channels[0] + .7152 * channels[1] + .0722 * channels[2];
  return luminance > .179 ? '000000' : 'FFFFFF';
}
export function cellFillDOM(attrs: Record<string, unknown>): Record<string, string> {
  const fill = attrs[cellFillAttribute];
  return isCellFillRgb(fill) ? {[cellFillDataAttribute]: fill, style: `background-color: #${fill}`} : {};
}
export function cellFillModeDOM(attrs: Record<string, unknown>): Record<string, string> {
  return isCellFillMode(attrs[cellFillModeAttribute]) ? {[cellFillModeDataAttribute]: 'none', style: 'background-color: transparent'} : {};
}
export function cellTextColorModeDOM(attrs: Record<string, unknown>): Record<string, string> {
  return isCellTextColorMode(attrs[cellTextColorModeAttribute])
    ? {[cellTextColorModeDataAttribute]: 'auto', style: `--word-cell-auto-preview: #${cellAutoPreviewRgb(attrs[cellFillAttribute])}`} : {};
}
export function tableCellColorsAttributes() {
  return [{types: ['tableCell', 'tableHeader'], attributes: {
    [cellFillAttribute]: {default: null, parseHTML: (element: HTMLElement) => {
      const raw = element.getAttribute(cellFillDataAttribute); return raw === null ? null : isCellFillRgb(raw) ? raw : 'invalid-cell-fill';
    }, renderHTML: cellFillDOM},
    [cellFillModeAttribute]: {default: null, parseHTML: (element: HTMLElement) => {
      const raw = element.getAttribute(cellFillModeDataAttribute); return raw === null ? null : isCellFillMode(raw) ? raw : 'invalid-cell-fill-mode';
    }, renderHTML: cellFillModeDOM},
    [cellTextColorModeAttribute]: {default: null, parseHTML: (element: HTMLElement) => {
      const raw = element.getAttribute(cellTextColorModeDataAttribute); return raw === null ? null : isCellTextColorMode(raw) ? raw : 'invalid-cell-text-color-mode';
    }, renderHTML: cellTextColorModeDOM},
  }}];
}
// Owned metadata must be exact and on the correct element before parsing can
// erase misplaced evidence. Foreign CSS backgrounds never become model fills.
export function validTableCellColorsHTML(root: ParentNode): boolean {
  return Array.from(root.querySelectorAll(`[${cellFillDataAttribute}]`)).every(element => ['td', 'th'].includes(element.tagName.toLowerCase())
    && isCellFillRgb(element.getAttribute(cellFillDataAttribute)) && !element.hasAttribute(cellFillModeDataAttribute))
    && Array.from(root.querySelectorAll(`[${cellFillModeDataAttribute}]`)).every(element => ['td', 'th'].includes(element.tagName.toLowerCase())
      && isCellFillMode(element.getAttribute(cellFillModeDataAttribute)) && !element.hasAttribute(cellFillDataAttribute) && simpleNoFillHTMLCell(element))
    && Array.from(root.querySelectorAll(`[${cellTextColorModeDataAttribute}]`)).every(element => ['td', 'th'].includes(element.tagName.toLowerCase())
      && isCellTextColorMode(element.getAttribute(cellTextColorModeDataAttribute)));
}

// No-fill ownership cannot be repaired from an orphan/merged/nested fragment.
// The existing RGB/auto HTML scope is deliberately unchanged.
function simpleNoFillHTMLCell(cell: Element): boolean {
  const row = cell.parentElement, parent = row?.parentElement;
  const table = parent?.tagName === 'TABLE' ? parent : ['TBODY', 'THEAD', 'TFOOT'].includes(parent?.tagName ?? '') ? parent?.parentElement : null;
  if (row?.tagName !== 'TR' || table?.tagName !== 'TABLE'
    || table.parentElement?.closest('table,li,blockquote') || table.querySelectorAll('table').length) return false;
  const rows = Array.from(table.querySelectorAll('tr')), width = rows[0]?.children.length ?? 0;
  return rows.length > 0 && rows.length <= 500 && width > 0 && width <= 50 && rows.every(item => {
    const owner = item.parentElement;
    return (owner === table || ['TBODY', 'THEAD', 'TFOOT'].includes(owner?.tagName ?? '') && owner?.parentElement === table)
      && item.children.length === width && Array.from(item.children).every(child => ['TD', 'TH'].includes(child.tagName)
        && [null, '1'].includes(child.getAttribute('colspan')) && [null, '1'].includes(child.getAttribute('rowspan')));
  });
}
export function isValidTableCellColorsHTML(html: string): boolean {
  if (!/data-cell-fill-mode/i.test(html)) return true;
  const rawCount = countRowPaginationHTMLAttributes(html, cellFillModeDataAttribute);
  if (html.length > 32 * 1024 * 1024) return rawCount === 0;
  const parsed = new DOMParser().parseFromString(html, 'text/html');
  const parsedCount = parsed.querySelectorAll(`[${cellFillModeDataAttribute}]`).length;
  if (rawCount === 0 && parsedCount === 0) return true;
  if (rawCount !== parsedCount || !validTableCellColorsHTML(parsed)) return false;
  let count = 0;
  const stack = [{element: parsed.body, depth: 0}];
  while (stack.length) {
    const {element, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) return false;
    for (const child of Array.from(element.children)) stack.push({element: child as HTMLElement, depth: depth + 1});
  }
  return true;
}
export function preflightTableCellColorsHTML(html: string): string {
  return isValidTableCellColorsHTML(html) ? html
    : `<table><tbody><tr><td ${cellFillModeDataAttribute}="invalid"><p></p></td></tr></tbody></table>`;
}
