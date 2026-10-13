import {omitDefaultTableVertical, validateTableVerticalDocument} from './word-table-vertical-layout';
import {omitDefaultTableCellMargins, validateTableCellMarginsDocument} from './word-table-cell-margins';
import {omitDefaultParagraphIndent, validateParagraphIndentDocument} from './word-paragraph-indent-model';
import {Extension, type JSONContent} from '@tiptap/core';
import type {Node} from '@tiptap/pm/model';
import {EditorState, Plugin, type Transaction} from '@tiptap/pm/state';
import {Decoration, DecorationSet} from '@tiptap/pm/view';
import {parseParagraphSpacePoints} from './word-paragraph-spacing';
import {inspectWordImageMedia, supportedImagePixelDimensions} from './word-image-media';

import {pageGeometryFields, pagePresets, isPageGeometry, type PageGeometry, type PagePreset, type PageOrientation} from './word-page-geometry-model';
export * from './word-page-geometry-model';

export const pageGeometryError = '页面只支持 A4、Letter、A5 横向或纵向（原始尺寸容差 2 twip）；四边页边距须为 0–216 pt、0.05 pt 递增，正文宽高至少 72 pt。';
const fitError = '图片、固定列宽或定位表格超出页面正文区域；定位自动表格按 450 pt 计算，可先调整图片、设置合适的固定列宽或清除表格位置。修改未应用。';
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);
export function parsePageMarginPoints(value: string): number | null {
  const parsed = parseParagraphSpacePoints(value);
  return parsed !== null && parsed <= 4320 ? parsed : null;
}
// This is an explicit preset action, not automatic canonicalization of source
// twips. Unchanged controls and margin-only edits preserve imported dimensions.
export function presetPageGeometry(preset: PagePreset, orientation: PageOrientation, previous?: PageGeometry): PageGeometry {
  if (!own(pagePresets, preset) || !['portrait', 'landscape'].includes(orientation)) throw new Error(pageGeometryError);
  const [width, height] = pagePresets[preset];
  return {...(previous ?? {marginTopTwips: 1440, marginRightTwips: 1440, marginBottomTwips: 1440, marginLeftTwips: 1440}),
    widthTwips: orientation === 'portrait' ? width : height, heightTwips: orientation === 'portrait' ? height : width};
}
export function omitDefaultPageGeometry(document: JSONContent): JSONContent {
  if (document.type !== 'doc' || document.attrs?.pageGeometry !== null) return document;
  const attrs = {...document.attrs}; delete attrs.pageGeometry;
  const result: JSONContent = {...document, attrs}; if (!Object.keys(attrs).length) delete result.attrs;
  return result;
}
export function documentPageGeometry(document: JSONContent): PageGeometry | null {
  if (document.type !== 'doc') throw new Error('文档根节点必须为 doc。');
  if (!document.attrs || !own(document.attrs, 'pageGeometry')) return null;
  if (!isPageGeometry(document.attrs.pageGeometry)) throw new Error(pageGeometryError);
  return document.attrs.pageGeometry;
}
export function pageImageDimensions(attrs: Record<string, unknown>): {width: number; height: number} {
  const media = inspectWordImageMedia(attrs.src);
  if (media.displayError) throw new Error(media.displayError);
  if (attrs.displayWidthEmu != null || attrs.displayHeightEmu != null) {
    if (!Number.isInteger(attrs.displayWidthEmu) || !Number.isInteger(attrs.displayHeightEmu)
      || Number(attrs.displayWidthEmu) < 1 || Number(attrs.displayHeightEmu) < 1) throw new Error(fitError);
    return {width: Number(attrs.displayWidthEmu), height: Number(attrs.displayHeightEmu)};
  }
  if (media.width === null || media.height === null || !supportedImagePixelDimensions(media.width, media.height)) throw new Error(fitError);
  const scale = Math.min(1, 600 / media.width, 800 / media.height);
  return {width: Math.max(1, Math.round(media.width * scale * 9525)), height: Math.max(1, Math.round(media.height * scale * 9525))};
}
export function unpositionedAutoTable(attrs: Record<string, unknown>): boolean {
  return attrs.columnWidthsTwips == null && attrs.tableAlign == null && attrs.tableIndentTwips == null;
}
// Automatic, unpositioned tables derive their authored base width from the
// explicit page. This is reflow, never a mutation to fixed column metadata.
export function pageTableWidthTwips(attrs: Record<string, unknown>, page: PageGeometry | null): number {
  const columns: unknown = attrs.columnWidthsTwips;
  if (columns != null) {
    if (!Array.isArray(columns) || !columns.length || columns.some(value => !Number.isInteger(value) || value < 1)) throw new Error(fitError);
    return columns.reduce((sum: number, value: number) => sum + value, 0);
  }
  return page && unpositionedAutoTable(attrs)
    ? Math.min(9000, page.widthTwips - page.marginLeftTwips - page.marginRightTwips) : 9000;
}
export function pageAutoTableDOM(attrs: Record<string, unknown>, page: PageGeometry | null): Record<string, string> {
  if (!page || !unpositionedAutoTable(attrs)) return {};
  return {'data-page-auto-table': '', style: `--word-table-width: ${pageTableWidthTwips(attrs, page) / 20}pt`};
}
export function validatePageGeometryDocument(document: JSONContent): void {
  const geometry = documentPageGeometry(document), stack = [{node: document, depth: 0}];
  let nodes = 0;
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++nodes > 10000) throw new Error('页面修改后文档结构超出保存上限。');
    if (node !== document && node.attrs && own(node.attrs, 'pageGeometry')) throw new Error(pageGeometryError);
    if (geometry) {
      const width = geometry.widthTwips - geometry.marginLeftTwips - geometry.marginRightTwips;
      const height = geometry.heightTwips - geometry.marginTopTwips - geometry.marginBottomTwips;
      if (node.type === 'table') {
        if (depth !== 1) throw new Error('显式页面设置仅支持正文顶层表格；嵌套表格或列表内表格的容器宽度无法用页面正文宽度替代。');
        const total = pageTableWidthTwips(node.attrs ?? {}, geometry);
        const indent: unknown = node.attrs?.tableIndentTwips ?? 0;
        // Conservative right-edge test; legacy explicit-zero indentation keeps
        // the intentional 108-twip left-border protrusion.
        if (!Number.isInteger(indent) || Number(indent) < 0 || total + Number(indent) > width) throw new Error(fitError);
      }
      if (node.type === 'image') {
        const image = pageImageDimensions(node.attrs ?? {});
        if (image.width > width * 635 || image.height > height * 635) throw new Error(fitError);
      }
    }
    for (const child of node.content ?? []) stack.push({node: child, depth: depth + 1});
  }
}
export function pageGeometrySheetStyle(page: PageGeometry | null): Record<string, string> {
  if (!page) return {};
  if (!isPageGeometry(page)) throw new Error(pageGeometryError);
  return {width: `${page.widthTwips / 20}pt`, minWidth: `${page.widthTwips / 20}pt`, maxWidth: 'none',
    minHeight: `${page.heightTwips / 20}pt`, padding: `${page.marginTopTwips / 20}pt ${page.marginRightTwips / 20}pt ${page.marginBottomTwips / 20}pt ${page.marginLeftTwips / 20}pt`, border: '0'};
}
export function pageGeometryPrintCss(page: PageGeometry | null): string {
  if (!page) return '';
  if (!isPageGeometry(page)) throw new Error(pageGeometryError);
  return `@page word-explicit-page { size: ${page.widthTwips / 20}pt ${page.heightTwips / 20}pt; margin: ${page.marginTopTwips / 20}pt ${page.marginRightTwips / 20}pt ${page.marginBottomTwips / 20}pt ${page.marginLeftTwips / 20}pt; }\n@media print { #print-document .tiptap[data-page-geometry] { page: word-explicit-page; } }`;
}
const encoder = new TextEncoder();
function validateProspectivePage(document: Node) {
  const stack = [{node: document, depth: 0}]; let count = 0, text = 0;
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error('页面修改后文档结构超出保存上限。');
    if (node.isText) text += encoder.encode(node.text!).length;
    if (text > 2 * 1024 * 1024) throw new Error('正文超过 2 MiB 保存上限，未修改页面。');
    node.forEach(child => stack.push({node: child, depth: depth + 1}));
  }
  const snapshot = omitDefaultPageGeometry(document.toJSON());
  validatePageGeometryDocument(snapshot);
  validateTableCellMarginsDocument(omitDefaultTableCellMargins(snapshot));
  validateTableVerticalDocument(omitDefaultTableVertical(snapshot));
  validateParagraphIndentDocument(omitDefaultParagraphIndent(snapshot));
  if (encoder.encode(JSON.stringify({version: 1, document: snapshot})).length > 32 * 1024 * 1024) throw new Error('页面修改后草稿超过 32 MiB 保存上限。');
}
export interface PageGeometryToken {doc: Node}
export const capturePageGeometry = (state: EditorState): PageGeometryToken => ({doc: state.doc});
export const matchesPageGeometry = (state: EditorState, token: PageGeometryToken): boolean => state.doc === token.doc;
export function createPageGeometryTransaction(state: EditorState, page: PageGeometry | null, token?: PageGeometryToken): Transaction | null {
  if (page !== null && !isPageGeometry(page)) throw new Error(pageGeometryError);
  if (token && !matchesPageGeometry(state, token)) return null;
  const previous: unknown = state.doc.attrs.pageGeometry;
  if (page === null && previous == null || page && isPageGeometry(previous) && pageGeometryFields.every(field => page[field] === previous[field])) return null;
  // A Tiptap chain/can state can expose a shared mutable transaction. Build
  // and validate in isolation so a rejected candidate cannot leak steps.
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr
    .setDocAttribute('pageGeometry', page ? {...page} : null);
  validateProspectivePage(transaction.doc);
  return transaction;
}
export function pageNaturalImageDOM(attrs: Record<string, unknown>): Record<string, string> {
  if (attrs.displayWidthEmu != null || attrs.displayHeightEmu != null) return {};
  const image = pageImageDimensions(attrs);
  return {'data-page-natural-image': '', style: `--word-image-width: ${image.width / 9525}px; --word-image-height: ${image.height / 9525}px`};
}
export function pageGeometryGuard(onRejected: (message: string) => void = () => {}): Plugin {
  return new Plugin({
    filterTransaction: transaction => {
      if (!transaction.docChanged) return true;
      try {validateProspectivePage(transaction.doc); return true;}
      catch (error) {onRejected(String(error)); return false;}
    },
    props: {decorations: state => {
      if (state.doc.attrs.pageGeometry == null) return null;
      const page = state.doc.attrs.pageGeometry as PageGeometry;
      const decorations: Decoration[] = [];
      state.doc.descendants((node, pos) => {
        if (node.type.name === 'table' && unpositionedAutoTable(node.attrs)) decorations.push(Decoration.node(pos, pos + node.nodeSize, pageAutoTableDOM(node.attrs, page)));
        if (node.type.name === 'image' && node.attrs.displayWidthEmu == null && node.attrs.displayHeightEmu == null) {
          decorations.push(Decoration.node(pos, pos + node.nodeSize, pageNaturalImageDOM(node.attrs)));
        }
      });
      return DecorationSet.create(state.doc, decorations);
    }},
  });
}
export const WordPageGeometry = Extension.create<{onPageError: (message: string) => void}>({
  name: 'wordPageGeometry',
  addOptions: () => ({onPageError: () => {}}),
  addGlobalAttributes: () => [{types: ['doc'], attributes: {pageGeometry: {default: null, rendered: false, parseHTML: () => null}}}],
  addProseMirrorPlugins() {return [pageGeometryGuard(this.options.onPageError)];},
});
