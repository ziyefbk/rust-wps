import type {JSONContent} from '@tiptap/core';
import {simpleCellMarginsTable} from './word-table-cell-margins';
import {isTextColorRgb, parseTextColor} from './word-text-color';

export const cellFillAttribute = 'cellFillRgb';
export const cellTextColorModeAttribute = 'cellTextColorMode';
export const cellFillDataAttribute = 'data-cell-fill-rgb';
export const cellTextColorModeDataAttribute = 'data-cell-text-color-mode';
export const tableCellColorFields = [cellFillAttribute, cellTextColorModeAttribute] as const;
export type CellTextColorMode = 'auto';
export interface TableCellColorsPatch {cellFillRgb?: string | null; cellTextColorMode?: CellTextColorMode | null}
export const tableCellColorsError = '单元格底色须为六位十六进制 RGB，例如 #C03030；文字颜色模式只支持自动或默认。仅支持正文顶层无合并、无嵌套的矩形表格。';
export const cellAutoColorPreviewWarning = '自动文字色在编辑器及打印中仅按底色亮度近似为黑或白，不代表 Word / LibreOffice 的自动对比；显式文字颜色优先，链接保留原有链接色。';
export const isCellFillRgb = isTextColorRgb;
export const parseCellFill = parseTextColor;
export const isCellTextColorMode = (value: unknown): value is CellTextColorMode => value === 'auto';
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);

// Both attributes are independent optional declarations. Absence retains the
// profile/legacy defaults; null exists only inside the editor schema.
export function validateTableCellColorsDocument(document: JSONContent): void {
  const stack: {node: JSONContent; parent?: JSONContent; table?: JSONContent; topLevel: boolean; depth: number}[] = [{node: document, depth: 0, topLevel: false}];
  const checked = new Set<JSONContent>();
  while (stack.length) {
    const {node, parent, table, topLevel, depth} = stack.pop()!;
    for (const field of tableCellColorFields) {
      if (!node.attrs || !own(node.attrs, field)) continue;
      const value = node.attrs[field];
      if (!table || !topLevel || !['tableCell', 'tableHeader'].includes(node.type ?? '') || parent?.type !== 'tableRow'
        || !(field === cellFillAttribute ? isCellFillRgb(value) : isCellTextColorMode(value))) throw new Error(tableCellColorsError);
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
export function cellTextColorModeDOM(attrs: Record<string, unknown>): Record<string, string> {
  return isCellTextColorMode(attrs[cellTextColorModeAttribute])
    ? {[cellTextColorModeDataAttribute]: 'auto', style: `--word-cell-auto-preview: #${cellAutoPreviewRgb(attrs[cellFillAttribute])}`} : {};
}
export function tableCellColorsAttributes() {
  return [{types: ['tableCell', 'tableHeader'], attributes: {
    [cellFillAttribute]: {default: null, parseHTML: (element: HTMLElement) => {
      const raw = element.getAttribute(cellFillDataAttribute); return raw === null ? null : isCellFillRgb(raw) ? raw : 'invalid-cell-fill';
    }, renderHTML: cellFillDOM},
    [cellTextColorModeAttribute]: {default: null, parseHTML: (element: HTMLElement) => {
      const raw = element.getAttribute(cellTextColorModeDataAttribute); return raw === null ? null : isCellTextColorMode(raw) ? raw : 'invalid-cell-text-color-mode';
    }, renderHTML: cellTextColorModeDOM},
  }}];
}
// Owned metadata must be exact and on the correct element before parsing can
// erase misplaced evidence. Foreign CSS backgrounds never become model fills.
export function validTableCellColorsHTML(root: ParentNode): boolean {
  return Array.from(root.querySelectorAll(`[${cellFillDataAttribute}]`)).every(element => ['td', 'th'].includes(element.tagName.toLowerCase())
    && isCellFillRgb(element.getAttribute(cellFillDataAttribute)))
    && Array.from(root.querySelectorAll(`[${cellTextColorModeDataAttribute}]`)).every(element => ['td', 'th'].includes(element.tagName.toLowerCase())
      && isCellTextColorMode(element.getAttribute(cellTextColorModeDataAttribute)));
}
