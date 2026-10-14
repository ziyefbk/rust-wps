import type {JSONContent} from '@tiptap/core';
import {isFontFamilyName} from './word-font-family';
import {isFontSizeHalfPoints} from './word-font-size';
import {isLineSpacing, isParagraphSpace} from './word-paragraph-spacing';

export interface BreakParagraph {
  font: {family: string; halfPoints: number};
  spaceBefore: number;
  spaceAfter: number;
  lineSpacing: number;
}
export const BREAK_PARAGRAPH_METADATA = 'data-break-paragraph';
export const SPLIT_PAGE_BREAK_METADATA = 'data-split-page-break-and-paragraph-mark';
export const pageBreakParagraphError = '分页段属性仅限正文顶层分页符，须完整提供有效字体、半点字号、段前、段后和倍数行距；文档分页兼容属性仅支持 true 或省略。';
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);
const exact = (value: unknown, fields: string[]): value is Record<string, unknown> => !!value && typeof value === 'object'
  && !Array.isArray(value) && Object.keys(value).length === fields.length && fields.every(key => own(value, key));

export function isBreakParagraph(value: unknown): value is BreakParagraph {
  return exact(value, ['font', 'spaceBefore', 'spaceAfter', 'lineSpacing'])
    && exact(value.font, ['family', 'halfPoints']) && isFontFamilyName(value.font.family) && isFontSizeHalfPoints(value.font.halfPoints)
    && isParagraphSpace(value.spaceBefore) && isParagraphSpace(value.spaceAfter) && isLineSpacing(value.lineSpacing);
}

// Compatibility and paragraph formatting are independent authored state. A bare
// break in a true document uses that document's normal defaults; pasting a
// formatted atom never imports or changes the destination document's flag.
export function validatePageBreakDocument(document: JSONContent): void {
  const stack = [{node: document, depth: 0}]; let count = 0;
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error('分页段文档结构超出保存上限。');
    const attrs = node.attrs;
    if (attrs && own(attrs, 'splitPageBreakAndParagraphMark')
      && (node !== document || node.type !== 'doc' || attrs.splitPageBreakAndParagraphMark !== true)) throw new Error(pageBreakParagraphError);
    if (attrs && own(attrs, 'breakParagraph')
      && (node.type !== 'pageBreak' || depth !== 1 || !isBreakParagraph(attrs.breakParagraph))) throw new Error(pageBreakParagraphError);
    if (node.type === 'pageBreak' && (depth !== 1 || node.text !== undefined
      || node.content !== undefined && (!Array.isArray(node.content) || node.content.length !== 0)
      || node.marks !== undefined && (!Array.isArray(node.marks) || node.marks.length !== 0)
      || attrs && Object.keys(attrs).some(key => key !== 'breakParagraph'))) throw new Error(pageBreakParagraphError);
    if (node.marks?.some(mark => mark.attrs && ['breakParagraph', 'splitPageBreakAndParagraphMark'].some(key => own(mark.attrs!, key)))) throw new Error(pageBreakParagraphError);
    for (const child of node.content ?? []) stack.push({node: child, depth: depth + 1});
  }
}

// Only schema defaults are stripped. In particular, malformed external nulls
// must be validated before schema loading, and no false flag is canonicalized.
export function omitDefaultPageBreak(document: JSONContent): JSONContent {
  const result = {...document};
  const field = result.type === 'doc' ? 'splitPageBreakAndParagraphMark' : result.type === 'pageBreak' ? 'breakParagraph' : null;
  if (field && result.attrs?.[field] === null) {
    result.attrs = {...result.attrs}; delete result.attrs[field];
    if (!Object.keys(result.attrs).length) delete result.attrs;
  }
  if (result.content) result.content = result.content.map(omitDefaultPageBreak);
  return result;
}

export function parseBreakParagraphMetadata(value: string | null): BreakParagraph | null {
  if (value === null) return null;
  if (value.length > 4096) throw new Error(pageBreakParagraphError);
  let parsed: unknown;
  try {parsed = JSON.parse(value);} catch {throw new Error(pageBreakParagraphError);}
  if (!isBreakParagraph(parsed)) throw new Error(pageBreakParagraphError);
  return parsed;
}

export function pageBreakDocumentDOM(attrs: Record<string, unknown>): Record<string, string> {
  return attrs.splitPageBreakAndParagraphMark === true ? {[SPLIT_PAGE_BREAK_METADATA]: 'true'} : {};
}

export function pageBreakPreviewWarning(document: JSONContent): string {
  const breaks = document.content?.filter(node => node.type === 'pageBreak') ?? [];
  return breaks.some(node => node.attrs?.breakParagraph != null) || breaks.length > 0 && document.attrs?.splitPageBreakAndParagraphMark === true
    ? 'DOCX 导出会保留分页段格式；编辑预览、打印和 PDF 暂不复现其段落行框或换页后的段落标记行，请用导出的 DOCX 核对。' : '';
}
