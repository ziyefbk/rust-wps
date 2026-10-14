import {omitDefaultParagraphPagination} from './word-paragraph-pagination-model';
import type {JSONContent} from '@tiptap/core';
import {omitDefaultEmptyParagraphFont} from './word-empty-paragraph-font-model';
import {omitDefaultParagraphIndent} from './word-paragraph-indent-model';
import {fontFamilyCss, isFontFamilyName} from './word-font-family';
import {isFontSizeHalfPoints} from './word-font-size';
import {isPageGeometry} from './word-page-geometry-model';

export interface ListGeometry {leftTwips: number; rightTwips: number; hangingTwips: number}
export interface ListMarkerFont {fontFamily: string; fontSize: number}
export const LIST_GEOMETRY_METADATA = 'data-word-list-geometry';
export const LIST_MARKER_FONT_METADATA = 'data-word-list-marker-font';
export const listGeometryError = '保留编号布局仅支持显式有效页面中的顶层十进制列表，每项须为一个左对齐纯文字段落，并完整保留编号字体和左右/悬挂缩进；正文宽度须为正。仅支持边界明确的整项/整列表选区删除；部分跨项删除、移动、合并、提升、嵌套或切换仍不支持；项内文字可编辑。';
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const onlyAttrs = (attrs: Record<string, unknown>, keys: readonly string[]) => record(attrs) && Object.keys(attrs).every(key => keys.includes(key));
export function isListGeometry(value: unknown): value is ListGeometry {
  const fields = ['leftTwips', 'rightTwips', 'hangingTwips'];
  return record(value) && Object.keys(value).length === fields.length && fields.every(field => own(value, field)
    && typeof value[field] === 'number' && Number.isInteger(value[field]) && Number(value[field]) >= 0 && Number(value[field]) <= 31680)
    && Number(value.hangingTwips) <= Number(value.leftTwips);
}
export function isListMarkerFont(value: unknown): value is ListMarkerFont {
  return record(value) && Object.keys(value).length === 2 && own(value, 'fontFamily') && own(value, 'fontSize')
    && isFontFamilyName(value.fontFamily) && isFontSizeHalfPoints(value.fontSize);
}
export function parseListMetadata(value: string | null, kind: 'geometry' | 'font'): ListGeometry | ListMarkerFont | null {
  if (value === null) return null;
  if (value.length > 4096) throw new Error(listGeometryError);
  let parsed: unknown; try {parsed = JSON.parse(value);} catch {throw new Error(listGeometryError);}
  if (!(kind === 'geometry' ? isListGeometry(parsed) : isListMarkerFont(parsed))) throw new Error(listGeometryError);
  return parsed as ListGeometry | ListMarkerFont;
}
// External JSON is validated before the schema can drop unknown attributes.
export function validateListGeometryDocument(document: JSONContent): void {
  const page = document.attrs?.pageGeometry;
  const stack: {node: JSONContent; parent?: JSONContent; grandparent?: JSONContent; depth: number}[] = [{node: document, depth: 0}];
  let count = 0;
  while (stack.length) {
    const {node, parent, grandparent, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error('编号布局文档结构超出保存上限。');
    const attrs = node.attrs ?? {}, marker = own(attrs, 'markerFont'), geometry = own(attrs, 'listGeometry');
    if (node.marks?.some(mark => mark.attrs && (own(mark.attrs, 'markerFont') || own(mark.attrs, 'listGeometry')))) throw new Error(listGeometryError);
    if (marker) {
      if (!onlyAttrs(attrs, ['start', 'type', 'markerFont']) || node.marks?.length || node.type !== 'orderedList' || parent !== document || document.type !== 'doc' || !isListMarkerFont(attrs.markerFont)
        || !isPageGeometry(page) || !node.content?.length || attrs.type != null
        || attrs.start !== undefined && (typeof attrs.start !== 'number' || !Number.isInteger(attrs.start) || attrs.start < 1 || attrs.start > 1000000)) throw new Error(listGeometryError);
      for (const item of node.content) if (!onlyAttrs(item.attrs ?? {}, []) || item.marks?.length || item.type !== 'listItem' || item.content?.length !== 1 || item.content[0].type !== 'paragraph'
        || !item.content[0].attrs || !own(item.content[0].attrs, 'listGeometry')) throw new Error(listGeometryError);
    }
    if (geometry) {
      if (!onlyAttrs(attrs, ['textAlign', 'spaceBefore', 'spaceAfter', 'lineSpacing', 'listGeometry']) || node.marks?.length
        || node.type !== 'paragraph' || parent?.type !== 'listItem' || parent.content?.length !== 1 || grandparent?.type !== 'orderedList'
        || !isListMarkerFont(grandparent.attrs?.markerFont) || !isListGeometry(attrs.listGeometry) || !isPageGeometry(page)
        || attrs.textAlign != null && attrs.textAlign !== 'left' || own(attrs, 'emptyParagraphFont')
        || ['indentLeftTwips', 'indentRightTwips', 'firstLineIndentTwips'].some(field => own(attrs, field))) throw new Error(listGeometryError);
      const {leftTwips, rightTwips} = attrs.listGeometry;
      if (leftTwips + rightTwips >= page.widthTwips - page.marginLeftTwips - page.marginRightTwips) throw new Error(listGeometryError);
      for (const child of node.content ?? []) if (!['text', 'hardBreak'].includes(child.type ?? '') || child.content?.length
        || child.type === 'text' && (typeof child.text !== 'string' || /[\t\r]/.test(child.text))) throw new Error(listGeometryError);
    }
    for (const child of node.content ?? []) stack.push({node: child, parent: node, grandparent: parent, depth: depth + 1});
  }
}
// Omit only legitimate schema defaults, never malformed or misplaced evidence.
export function omitDefaultListGeometry(document: JSONContent): JSONContent {
  const result = {...document}, key = result.type === 'orderedList' ? 'markerFont' : result.type === 'paragraph' ? 'listGeometry' : null;
  if (key && result.attrs?.[key] === null) {
    result.attrs = {...result.attrs}; delete result.attrs[key];
    if (!Object.keys(result.attrs).length) delete result.attrs;
  }
  if (result.content) result.content = result.content.map(omitDefaultListGeometry);
  return result;
}
export function listMarkerFontStyle(font: ListMarkerFont, start: number): string {
  if (!isListMarkerFont(font) || !Number.isInteger(start) || start < 1 || start > 1000000) throw new Error(listGeometryError);
  return `--word-list-marker-family: ${fontFamilyCss(font.fontFamily)}; --word-list-marker-size: ${font.fontSize / 2}pt; counter-reset: word-preserved-decimal ${start - 1}`;
}
export function listGeometryStyle(geometry: ListGeometry): string {
  if (!isListGeometry(geometry)) throw new Error(listGeometryError);
  return `--word-list-left: ${geometry.leftTwips / 20}pt; --word-list-right: ${geometry.rightTwips / 20}pt; --word-list-hanging: ${geometry.hangingTwips / 20}pt`;
}

// Use only after schema construction: raw incoming JSON must reach the strict
// validator unchanged, while legitimate PM null defaults are omitted here.
export function listGeometryStateSnapshot(document: JSONContent): JSONContent {
  return omitDefaultParagraphPagination(omitDefaultEmptyParagraphFont(omitDefaultParagraphIndent(omitDefaultListGeometry(document))));
}
