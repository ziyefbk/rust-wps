import {omitDefaultPageGeometry, validatePageGeometryDocument} from './word-page-geometry';
import {mergeAttributes, type JSONContent} from '@tiptap/core';
import {inspectWordImageMedia, supportedImagePixelDimensions, validateImageDisplayMedia} from './word-image-media';
import {imageParagraphDOM, imageParagraphFontStyle, imageParagraphHtmlImage, imageParagraphError, isImageParagraph, parseImageParagraphMetadata, validateImageParagraphDocument, IMAGE_PARAGRAPH_METADATA, type ImageParagraph} from './word-image-paragraph';
import Image, {type ImageOptions} from '@tiptap/extension-image';
import type {Node} from '@tiptap/pm/model';
import {EditorState, NodeSelection, Plugin, Selection, type Transaction} from '@tiptap/pm/state';

export const EMU_PER_PIXEL = 9525;
export const EMU_PER_POINT = 12700;
export const MAX_IMAGE_WIDTH_EMU = 5715000;
export const MAX_IMAGE_HEIGHT_EMU = 7620000;
export const imageDimensionFields = ['displayWidthEmu', 'displayHeightEmu'] as const;
export interface ImageDimensions {displayWidthEmu: number; displayHeightEmu: number}
export type ImageDimensionField = typeof imageDimensionFields[number];
export const imageDimensionsError = '图片显示尺寸须成对设置为正整数 EMU，宽最多 450 pt、高最多 600 pt；仅支持正文顶层图片。';
const metadataNames = ['data-display-width-emu', 'data-display-height-emu'] as const;
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);

export function isImageDimensions(value: unknown): value is ImageDimensions {
  if (!value || typeof value !== 'object') return false;
  const {displayWidthEmu: width, displayHeightEmu: height} = value as ImageDimensions;
  return Number.isInteger(width) && width >= 1 && width <= MAX_IMAGE_WIDTH_EMU
    && Number.isInteger(height) && height >= 1 && height <= MAX_IMAGE_HEIGHT_EMU;
}

// Point decimals are parsed exactly, never silently rounded. A fraction is
// shown only when /12700 cannot terminate, so any imported EMU remains editable.
export function parseImageDimensionPoints(value: string, field: ImageDimensionField): number | null {
  const input = value.trim();
  if (input.length > 64) return null;
  const fraction = /^(\d{1,7})\s*\/\s*12700$/.exec(input);
  let emu: bigint;
  if (fraction) emu = BigInt(fraction[1]);
  else {
    if (!/^(?:\d+(?:\.\d+)?|\.\d+)$/.test(input)) return null;
    const [whole, decimals = ''] = input.split('.');
    const numerator = BigInt(`${whole || '0'}${decimals}`) * BigInt(EMU_PER_POINT);
    const denominator = 10n ** BigInt(decimals.length);
    if (numerator % denominator !== 0n) return null;
    emu = numerator / denominator;
  }
  const maximum = field === 'displayWidthEmu' ? MAX_IMAGE_WIDTH_EMU : MAX_IMAGE_HEIGHT_EMU;
  return emu >= 1n && emu <= BigInt(maximum) ? Number(emu) : null;
}
export function formatImageDimensionPoints(emu: number): string {
  if (!Number.isInteger(emu) || emu < 1 || emu > MAX_IMAGE_HEIGHT_EMU) throw new Error(imageDimensionsError);
  return emu % 127 === 0 ? String(emu / EMU_PER_POINT) : `${emu}/12700`;
}

export function parseImageDimensionsMetadata(width: string | null, height: string | null): ImageDimensions | null {
  if (width === null && height === null) return null;
  if (width === null || height === null || !/^\d{1,7}$/.test(width) || !/^\d{1,7}$/.test(height)) throw new Error(imageDimensionsError);
  const result = {displayWidthEmu: Number(width), displayHeightEmu: Number(height)};
  if (!isImageDimensions(result)) throw new Error(imageDimensionsError);
  return result;
}

// Run BEFORE nodeFromJSON: schema defaults would erase the difference between
// an omitted pair and explicit null input. Geometry on any other node rejects.
export function validateImageDimensionsDocument(document: JSONContent): void {
  validateImageParagraphDocument(document);
  const stack = [{node: document, depth: 0}];
  let mediaBytes = 0, imageCount = 0;
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    const attrs = node.attrs;
    if (node.type === 'image') {
      if (attrs?.width != null || attrs?.height != null) throw new Error('不支持图片像素 width/height；请使用成对的显示尺寸 EMU。');
      mediaBytes += inspectWordImageMedia(attrs?.src).bytes;
      if (++imageCount > 100 || mediaBytes > 20 * 1024 * 1024) throw new Error('图片合计超过 20 MiB 或 100 张，未修改正文。');
    }
    if (attrs && imageDimensionFields.some(field => own(attrs, field))
      && (node.type !== 'image' || depth !== 1 || !imageDimensionFields.every(field => own(attrs, field)) || !isImageDimensions(attrs))) {
      throw new Error(imageDimensionsError);
    }
    if (node.type === 'image' && attrs && (isImageDimensions(attrs) || isImageParagraph(attrs.imageParagraph))) validateImageDisplayMedia(node.attrs!.src);
    for (const child of node.content ?? []) stack.push({node: child, depth: depth + 1});
  }
}

// Only the complete editor default is omitted. Keep malformed partial values
// visible to validation instead of accidentally turning them into legacy data.
export function omitDefaultImageDimensions(document: JSONContent): JSONContent {
  const result = {...document};
  if (result.type === 'image' && result.attrs?.imageParagraph === null) {
    result.attrs = {...result.attrs}; delete result.attrs.imageParagraph;
    if (!Object.keys(result.attrs).length) delete result.attrs;
  }
  if (result.type === 'image' && result.attrs?.displayWidthEmu === null && result.attrs?.displayHeightEmu === null) {
    result.attrs = {...result.attrs};
    for (const field of imageDimensionFields) delete result.attrs[field];
    if (!Object.keys(result.attrs).length) delete result.attrs;
  }
  if (result.content) result.content = result.content.map(omitDefaultImageDimensions);
  return result;
}

export function imageDimensionsDOM(attrs: Record<string, unknown>): Record<string, string> {
  if (!isImageDimensions(attrs)) return {};
  return {
    [metadataNames[0]]: String(attrs.displayWidthEmu), [metadataNames[1]]: String(attrs.displayHeightEmu),
    // Override legacy responsive/print limits AND object-fit:contain. Explicit
    // dimensions must represent the same (possibly stretched) image as DOCX.
    style: `width: ${attrs.displayWidthEmu / EMU_PER_PIXEL}px; height: ${attrs.displayHeightEmu / EMU_PER_PIXEL}px; max-width: none; max-height: none; object-fit: fill`,
  };
}

export interface SelectedWordImage {node: Node; pos: number; supported: boolean; dimensions: ImageDimensions | null}
export function selectedWordImage(state: EditorState): SelectedWordImage | null {
  if (!(state.selection instanceof NodeSelection) || state.selection.node.type.name !== 'image') return null;
  const {node, from: pos, $from} = state.selection;
  return {node, pos, supported: $from.depth === 0,
    dimensions: isImageDimensions(node.attrs) ? {displayWidthEmu: node.attrs.displayWidthEmu, displayHeightEmu: node.attrs.displayHeightEmu} : null};
}
export interface ImageSelectionToken {doc: Node; selection: Selection; image: SelectedWordImage}
export function captureImageSelection(state: EditorState): ImageSelectionToken | null {
  const image = selectedWordImage(state);
  return image?.supported ? {doc: state.doc, selection: state.selection, image} : null;
}
export function matchesImageSelection(state: EditorState, token: ImageSelectionToken): boolean {
  return state.doc === token.doc && state.selection.eq(token.selection) && state.doc.nodeAt(token.image.pos) === token.image.node;
}

// Ratios need not be exactly representable at an integer EMU. Round the computed
// companion dimension to nearest EMU, without changing the user's exact input.
export function imageDimensionsWithRatio(value: number, field: ImageDimensionField, ratio: {width: number; height: number}): ImageDimensions | null {
  if (![value, ratio.width, ratio.height].every(item => Number.isSafeInteger(item) && item > 0)) return null;
  const numerator = BigInt(field === 'displayWidthEmu' ? ratio.height : ratio.width);
  const denominator = BigInt(field === 'displayWidthEmu' ? ratio.width : ratio.height);
  const other = Number((BigInt(value) * numerator * 2n + denominator) / (denominator * 2n));
  const result = field === 'displayWidthEmu' ? {displayWidthEmu: value, displayHeightEmu: other} : {displayWidthEmu: other, displayHeightEmu: value};
  return isImageDimensions(result) ? result : null;
}
export function naturalImageDimensions(width: number, height: number): ImageDimensions | null {
  if (!supportedImagePixelDimensions(width, height)) return null;
  const scale = Math.min(1, 600 / width, 800 / height);
  const result = {displayWidthEmu: Math.max(1, Math.round(width * EMU_PER_PIXEL * scale)), displayHeightEmu: Math.max(1, Math.round(height * EMU_PER_PIXEL * scale))};
  return isImageDimensions(result) ? result : null;
}

const encoder = new TextEncoder();
function validateProspectiveImageDocument(document: Node): void {
  let nodes = 0, bytes = 0;
  const stack = [{node: document, depth: 0}];
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++nodes > 10000 || depth > 32) throw new Error('图片修改后文档结构超出保存上限，未修改正文。');
    if (node.isText) {
      bytes += encoder.encode(node.text!).length;
      if (bytes > 2 * 1024 * 1024) throw new Error('正文超过 2 MiB 保存上限，未修改图片。');
    }
    for (let index = node.childCount - 1; index >= 0; index--) stack.push({node: node.child(index), depth: depth + 1});
  }
  const snapshot = omitDefaultImageDimensions(document.toJSON());
  validateImageDimensionsDocument(snapshot);
  validatePageGeometryDocument(omitDefaultPageGeometry(snapshot));
  if (encoder.encode(JSON.stringify({version: 1, document: snapshot})).length > 32 * 1024 * 1024) {
    throw new Error('图片修改后草稿超过 32 MiB 保存上限，未修改正文。');
  }
}
export function createImageDimensionsTransaction(state: EditorState, dimensions: ImageDimensions | null, token?: ImageSelectionToken): Transaction | null {
  if (dimensions !== null && (!isImageDimensions(dimensions) || Object.keys(dimensions).some(field => !imageDimensionFields.includes(field as ImageDimensionField)))) throw new Error(imageDimensionsError);
  if (token && !matchesImageSelection(state, token)) return null;
  const image = selectedWordImage(state);
  if (!image) return null;
  if (!image.supported) throw new Error(imageDimensionsError);
  const attrs = {...image.node.attrs, displayWidthEmu: dimensions?.displayWidthEmu ?? null, displayHeightEmu: dimensions?.displayHeightEmu ?? null};
  if (imageDimensionFields.every(field => attrs[field] === image.node.attrs[field])) return null;
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr
    .setNodeMarkup(image.pos, undefined, attrs, image.node.marks);
  validateProspectiveImageDocument(transaction.doc);
  transaction.setSelection(Selection.fromJSON(transaction.doc, state.selection.toJSON()));
  if (state.storedMarks) transaction.setStoredMarks(state.storedMarks);
  return transaction;
}
export function imageDimensionsGuard(onRejected: (message: string) => void = () => {}): Plugin {
  return new Plugin({filterTransaction: transaction => {
    if (!transaction.docChanged) return true;
    try {validateProspectiveImageDocument(transaction.doc); return true;}
    catch (error) {onRejected(String(error)); return false;}
  }});
}

export function createImageParagraphTransaction(state: EditorState, paragraph: ImageParagraph | null, token?: ImageSelectionToken): Transaction | null {
  if (paragraph !== null && !isImageParagraph(paragraph)) throw new Error(imageParagraphError);
  if (token && !matchesImageSelection(state, token)) return null;
  const image = selectedWordImage(state);
  if (!image) return null;
  if (!image.supported) throw new Error(imageParagraphError);
  // Compare canonical field values rather than caller JSON key order.
  const attrs = {...image.node.attrs, imageParagraph: paragraph};
  const next = image.node.type.create(attrs, image.node.content, image.node.marks);
  if (next.eq(image.node)) return null;
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr
    .setNodeMarkup(image.pos, undefined, attrs, image.node.marks);
  validateProspectiveImageDocument(transaction.doc);
  transaction.setSelection(Selection.fromJSON(transaction.doc, state.selection.toJSON()));
  if (state.storedMarks) transaction.setStoredMarks(state.storedMarks);
  return transaction;
}

export const WordImage = Image.extend<ImageOptions & {onDimensionError: (message: string) => void}>({
  addOptions() {return {...this.parent!(), onDimensionError: () => {}};},
  parseHTML() {
    return [{tag: `[${IMAGE_PARAGRAPH_METADATA}]`, priority: 100, getAttrs: (element: HTMLElement) => {
      const image = imageParagraphHtmlImage(element);
      return image ? Object.fromEntries(['src', 'alt', 'title'].map(field => [field, image.getAttribute(field)])) : {};
    }}, ...(this.parent?.() ?? [])];
  },
  addAttributes() {
    const parent: Record<string, Record<string, unknown>> = this.parent?.() ?? {};
    return {...parent,
      width: {...parent.width, parseHTML: () => null}, height: {...parent.height, parseHTML: () => null},
      imageParagraph: {default: null, rendered: false, parseHTML: (element: HTMLElement) => {
        try {
          const value = element.getAttribute(IMAGE_PARAGRAPH_METADATA);
          if (value !== null && !imageParagraphHtmlImage(element)) throw new Error(imageParagraphError);
          return parseImageParagraphMetadata(value);
        } catch {return 'invalid-image-paragraph';}
      }},
      ...Object.fromEntries(imageDimensionFields.map(field => [field, {
        default: null, rendered: false,
        parseHTML: (element: HTMLElement) => {
          try {
            const image = element.tagName?.toLowerCase() === 'div' ? imageParagraphHtmlImage(element) : element;
            return parseImageDimensionsMetadata(image?.getAttribute(metadataNames[0]) ?? null, image?.getAttribute(metadataNames[1]) ?? null)?.[field] ?? null;
          }
          // Carry invalid owned metadata to the transaction guard. Do not silently
          // paste a differently sized image by discarding a malformed pair.
          catch {return 'invalid-image-dimensions';}
        },
      }]))};
  },
  renderHTML({node, HTMLAttributes}) {
    const paragraph: unknown = node.attrs.imageParagraph;
    if (isImageParagraph(paragraph)) {
      const image = this.parent!({node, HTMLAttributes: mergeAttributes(HTMLAttributes, imageDimensionsDOM(node.attrs),
        {style: 'display: inline; vertical-align: baseline; margin: 0'})});
      return ['div', imageParagraphDOM(paragraph), ['span', {style: paragraph.runFont ? imageParagraphFontStyle(paragraph.runFont) : ''}, image]];
    }
    return this.parent!({node, HTMLAttributes: mergeAttributes(HTMLAttributes, imageDimensionsDOM(node.attrs))});
  },
  addProseMirrorPlugins() {return [...(this.parent?.() ?? []), imageDimensionsGuard(this.options.onDimensionError)];},
}).configure({allowBase64: true, resize: false});
