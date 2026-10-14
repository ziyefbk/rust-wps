import type {JSONContent} from '@tiptap/core';

export const paragraphPaginationFields = ['keepNext', 'keepLines', 'pageBreakBefore'] as const;
export type ParagraphPaginationField = typeof paragraphPaginationFields[number];
export type ParagraphPaginationPatch = Partial<Record<ParagraphPaginationField, boolean | null>>;
export const paragraphPaginationTypes = new Set(['paragraph', 'heading']);
export const paragraphPaginationError = '段落分页仅支持顶层正文和标题的默认、是或否；图片、列表和表格内不支持。合并仅支持其他格式相同、有效分页设置一致且后段不含段前分页的相邻同类段落；部分跨段删除和多段开放粘贴仍不支持。未应用修改。';
export const paragraphPaginationData = {keepNext: 'data-word-keep-next', keepLines: 'data-word-keep-lines', pageBreakBefore: 'data-word-page-break-before'} as const;
const own = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);
export const hasParagraphPagination = (node: JSONContent): boolean => paragraphPaginationFields.some(field => node.attrs && own(node.attrs, field));
export function validateParagraphPaginationDocument(document: JSONContent): void {
  const stack: {node: JSONContent; parent?: JSONContent; depth: number}[] = [{node: document, depth: 0}];
  let count = 0;
  while (stack.length) {
    const {node, parent, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error('段落分页文档结构超出保存上限。');
    if (node.marks?.some(mark => paragraphPaginationFields.some(field => mark.attrs && own(mark.attrs, field)))) throw new Error(paragraphPaginationError);
    if (hasParagraphPagination(node)) {
      if (document.type !== 'doc' || parent !== document || !paragraphPaginationTypes.has(node.type ?? '')
        || paragraphPaginationFields.some(field => node.attrs && own(node.attrs, field) && typeof node.attrs[field] !== 'boolean')
        || node.content?.some(child => !['text', 'hardBreak'].includes(child.type ?? ''))) throw new Error(paragraphPaginationError);
    }
    for (const child of node.content ?? []) stack.push({node: child, parent: node, depth: depth + 1});
  }
}
// Raw external JSON is validated unchanged before schema construction. Only
// schema null defaults are removed here; false is an explicit authored value.
export function omitDefaultParagraphPagination(document: JSONContent): JSONContent {
  const result = {...document};
  if (paragraphPaginationTypes.has(result.type ?? '') && result.attrs) {
    result.attrs = {...result.attrs};
    for (const field of paragraphPaginationFields) if (result.attrs[field] === null) delete result.attrs[field];
    if (!Object.keys(result.attrs).length) delete result.attrs;
  }
  if (result.content) result.content = result.content.map(omitDefaultParagraphPagination);
  return result;
}
