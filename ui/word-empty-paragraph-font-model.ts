import type {JSONContent} from '@tiptap/core';
import {fontFamilyCss, isFontFamilyName} from './word-font-family';
import {isFontSizeHalfPoints} from './word-font-size';

export interface EmptyParagraphFont {family: string; halfPoints: number; bold: boolean; italic: boolean}
export const EMPTY_PARAGRAPH_FONT_METADATA = 'data-empty-paragraph-font';
export const emptyParagraphFontError = '导入空段字体仅限无任何内容的正文或标题，须完整提供有效字体、半点字号、粗体和斜体。';
export const emptyParagraphTypes = new Set(['paragraph', 'heading']);
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);
const fields = ['family', 'halfPoints', 'bold', 'italic'];
export function isEmptyParagraphFont(value: unknown): value is EmptyParagraphFont {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const record = value as Record<string, unknown>;
  return Object.keys(record).length === fields.length && fields.every(key => own(record, key))
    && isFontFamilyName(record.family) && isFontSizeHalfPoints(record.halfPoints)
    && typeof record.bold === 'boolean' && typeof record.italic === 'boolean';
}
// Raw external JSON must be checked before schema defaults can erase evidence.
export function validateEmptyParagraphFontDocument(document: JSONContent): void {
  const stack = [{node: document, depth: 0}]; let count = 0;
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error('空段字体文档结构超出保存上限。');
    if (node.attrs && own(node.attrs, 'emptyParagraphFont') && (!emptyParagraphTypes.has(node.type ?? '')
      || !isEmptyParagraphFont(node.attrs.emptyParagraphFont)
      || node.content !== undefined && (!Array.isArray(node.content) || node.content.length !== 0))) throw new Error(emptyParagraphFontError);
    if (node.marks?.some(mark => mark.attrs && own(mark.attrs, 'emptyParagraphFont'))) throw new Error(emptyParagraphFontError);
    for (const child of node.content ?? []) stack.push({node: child, depth: depth + 1});
  }
}
// Internal schema null is omitted only on the two legitimate node types.
export function omitDefaultEmptyParagraphFont(document: JSONContent): JSONContent {
  const result = {...document};
  if (emptyParagraphTypes.has(result.type ?? '') && result.attrs?.emptyParagraphFont === null) {
    result.attrs = {...result.attrs}; delete result.attrs.emptyParagraphFont;
    if (!Object.keys(result.attrs).length) delete result.attrs;
  }
  if (result.content) result.content = result.content.map(omitDefaultEmptyParagraphFont);
  return result;
}
export function parseEmptyParagraphFontMetadata(value: string | null): EmptyParagraphFont | null {
  if (value === null) return null;
  if (value.length > 4096) throw new Error(emptyParagraphFontError);
  let parsed: unknown;
  try {parsed = JSON.parse(value);} catch {throw new Error(emptyParagraphFontError);}
  if (!isEmptyParagraphFont(parsed)) throw new Error(emptyParagraphFontError);
  return parsed;
}
export function emptyParagraphFontStyle(font: EmptyParagraphFont): string {
  if (!isEmptyParagraphFont(font)) throw new Error(emptyParagraphFontError);
  return `font-family: ${fontFamilyCss(font.family)}; font-size: ${font.halfPoints / 2}pt; font-weight: ${font.bold ? 700 : 400}; font-style: ${font.italic ? 'italic' : 'normal'}`;
}
