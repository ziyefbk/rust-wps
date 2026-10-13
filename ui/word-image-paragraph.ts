import type {JSONContent} from '@tiptap/core';
import {fontFamilyCss, isFontFamilyName} from './word-font-family';
import {isFontSizeHalfPoints} from './word-font-size';
import {isLineSpacing, isParagraphSpace} from './word-paragraph-spacing';

export interface ImageParagraphFont {family: string; halfPoints: number; bold: boolean; italic: boolean}
export interface ImageParagraph {
  spaceBefore: number; spaceAfter: number; lineSpacing: number; textAlign: 'left' | 'center' | 'right' | 'justify';
  paragraphFont?: ImageParagraphFont; runFont?: ImageParagraphFont;
}
export const imageParagraphError = '图片段落须完整设置段前、段后、倍数行距和对齐；字体须成对设置，且仅支持正文顶层单图段落。';
export const IMAGE_PARAGRAPH_METADATA = 'data-image-paragraph';
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);
const layoutKeys = ['spaceBefore', 'spaceAfter', 'lineSpacing', 'textAlign'];
const fontKeys = ['family', 'halfPoints', 'bold', 'italic'];
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
export function isImageParagraphFont(value: unknown): value is ImageParagraphFont {
  return record(value) && Object.keys(value).length === fontKeys.length && fontKeys.every(key => own(value, key))
    && isFontFamilyName(value.family) && isFontSizeHalfPoints(value.halfPoints)
    && typeof value.bold === 'boolean' && typeof value.italic === 'boolean';
}
export function isImageParagraph(value: unknown): value is ImageParagraph {
  if (!record(value) || !layoutKeys.every(key => own(value, key))
    || Object.keys(value).some(key => ![...layoutKeys, 'paragraphFont', 'runFont'].includes(key))
    || !isParagraphSpace(value.spaceBefore) || !isParagraphSpace(value.spaceAfter) || !isLineSpacing(value.lineSpacing)
    || typeof value.textAlign !== 'string' || !['left', 'center', 'right', 'justify'].includes(value.textAlign)) return false;
  const hasParagraph = own(value, 'paragraphFont'), hasRun = own(value, 'runFont');
  return !hasParagraph && !hasRun || hasParagraph && hasRun && isImageParagraphFont(value.paragraphFont) && isImageParagraphFont(value.runFont);
}
export function defaultImageParagraph(profile: unknown): ImageParagraph {
  // The legacy editor paragraph has a 12px after gap and 1.9 line height. Opting
  // in makes those values explicit; it does not upgrade untouched legacy images.
  return {spaceBefore: 0, spaceAfter: profile === 'standard-v2' ? 120 : 180,
    lineSpacing: profile === 'standard-v2' ? 360 : 456, textAlign: 'left'};
}
export function validateImageParagraphDocument(document: JSONContent): void {
  const stack = [{node: document, depth: 0}];
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (node.attrs && own(node.attrs, 'imageParagraph')
      && (node.type !== 'image' || depth !== 1 || !isImageParagraph(node.attrs.imageParagraph))) throw new Error(imageParagraphError);
    for (const child of node.content ?? []) stack.push({node: child, depth: depth + 1});
  }
}
export function omitDefaultImageParagraph(document: JSONContent): JSONContent {
  const result = {...document};
  if (result.type === 'image' && result.attrs?.imageParagraph === null) {
    result.attrs = {...result.attrs}; delete result.attrs.imageParagraph;
    if (!Object.keys(result.attrs).length) delete result.attrs;
  }
  if (result.content) result.content = result.content.map(omitDefaultImageParagraph);
  return result;
}
export function parseImageParagraphMetadata(value: string | null): ImageParagraph | null {
  if (value === null) return null;
  if (value.length > 4096) throw new Error(imageParagraphError);
  let parsed: unknown;
  try {parsed = JSON.parse(value);} catch {throw new Error(imageParagraphError);}
  if (!isImageParagraph(parsed)) throw new Error(imageParagraphError);
  return parsed;
}
export function imageParagraphFontStyle(font: ImageParagraphFont): string {
  return `font-family: ${fontFamilyCss(font.family)}; font-size: ${font.halfPoints / 2}pt; font-weight: ${font.bold ? 700 : 400}; font-style: ${font.italic ? 'italic' : 'normal'}`;
}
export function imageParagraphDOM(value: ImageParagraph): Record<string, string> {
  if (!isImageParagraph(value)) throw new Error(imageParagraphError);
  return {[IMAGE_PARAGRAPH_METADATA]: JSON.stringify(value),
    style: `display: block; margin: ${value.spaceBefore / 20}pt 0 ${value.spaceAfter / 20}pt; line-height: ${value.lineSpacing / 240}; text-align: ${value.textAlign}`
      + (value.paragraphFont ? `; ${imageParagraphFontStyle(value.paragraphFont)}` : '')};
}

// Clipboard round trips own wrapper markup as a single atom. Reject a wrapper
// with text, multiple images or other children rather than silently dropping it.
export function imageParagraphHtmlImage(element: HTMLElement): HTMLElement | null {
  if (element.tagName.toLowerCase() === 'img') return element;
  if (element.tagName.toLowerCase() !== 'div' || !element.hasAttribute(IMAGE_PARAGRAPH_METADATA)) return null;
  const span = element.firstElementChild;
  if (element.childNodes.length !== 1 || span?.tagName.toLowerCase() !== 'span' || span.childNodes.length !== 1) return null;
  const image = span.firstElementChild;
  return image?.tagName.toLowerCase() === 'img' && !image.hasAttribute(IMAGE_PARAGRAPH_METADATA) ? image as HTMLElement : null;
}
