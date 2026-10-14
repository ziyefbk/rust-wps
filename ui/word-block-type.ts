import {omitDefaultParagraphPagination, validateParagraphPaginationDocument} from './word-paragraph-pagination-model';
import {omitDefaultEmptyParagraphFont, validateEmptyParagraphFontDocument} from './word-empty-paragraph-font-model';
import type {Node} from '@tiptap/pm/model';
import {EditorState, type Transaction} from '@tiptap/pm/state';
import {Mapping} from '@tiptap/pm/transform';
import {omitDefaultParagraphSpacing, validateParagraphSpacingDocument} from './word-paragraph-spacing';
import {omitDefaultParagraphIndent, validateParagraphIndentDocument} from './word-paragraph-indent-model';
import {omitDefaultPageGeometry, validatePageGeometryDocument} from './word-page-geometry';

const paragraphTypes = new Set(['paragraph', 'heading']);
const encoder = new TextEncoder();
const unsupported = '当前选区不能完整转换为正文或标题；列表首段须保留为正文，未修改任何段落。';

function selectedTextblocks(state: EditorState): Map<number, Node> {
  const result = new Map<number, Node>();
  if (state.selection.empty) {
    const {$from} = state.selection;
    for (let depth = $from.depth; depth > 0; depth--) {
      const node = $from.node(depth);
      if (node.isTextblock) {result.set($from.before(depth), node); break;}
    }
  } else for (const {$from, $to} of state.selection.ranges) {
    state.doc.nodesBetween($from.pos, $to.pos, (node, pos) => {
      if (!node.isTextblock) return;
      // As with spacing/indentation, ending at the next text start excludes it.
      if (pos + 1 < $to.pos && pos + node.nodeSize > $from.pos) result.set(pos, node);
      return false;
    });
  }
  return result;
}

// Toggle exactly the blocks the conversion will touch, including disjoint cell
// selections and ranges ending at the next paragraph's text start.
export function wordBlockTypeIsActive(state: EditorState, level: number): boolean {
  const nodes = [...selectedTextblocks(state).values()];
  return nodes.length > 0 && nodes.every(node => node.type.name === 'heading' && node.attrs.level === level);
}

export function validateWordBlockTypeDocument(document: Node): void {
  let count = 0, textBytes = 0;
  const stack = [{node: document, depth: 0}];
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error('段落类型修改后文档结构超出保存上限，未修改正文。');
    if (paragraphTypes.has(node.type.name) && node.attrs.textAlign != null
      && !['left', 'center', 'right', 'justify'].includes(node.attrs.textAlign)) throw new Error('不支持的段落对齐，未修改段落类型。');
    if (node.type.name === 'heading' && (!Number.isInteger(node.attrs.level) || node.attrs.level < 1 || node.attrs.level > 3)) {
      throw new Error('标题级别须为 1–3，未修改段落类型。');
    }
    if (node.isText) textBytes += encoder.encode(node.text!).length;
    if (textBytes > 2 * 1024 * 1024) throw new Error('正文超过 2 MiB 保存上限，未修改段落类型。');
    node.forEach(child => stack.push({node: child, depth: depth + 1}));
  }
  document.check();
  const snapshot = omitDefaultParagraphIndent(omitDefaultParagraphSpacing(omitDefaultPageGeometry(document.toJSON())));
  validateEmptyParagraphFontDocument(omitDefaultEmptyParagraphFont(snapshot));
  validateParagraphPaginationDocument(omitDefaultParagraphPagination(snapshot));
  validateParagraphSpacingDocument(snapshot);
  validateParagraphIndentDocument(snapshot);
  validatePageGeometryDocument(snapshot);
  if (encoder.encode(JSON.stringify({version: 2, document: snapshot})).length > 32 * 1024 * 1024) {
    throw new Error('段落类型修改后草稿超过 32 MiB 保存上限，未修改正文。');
  }
}

// App-owned conversion: no clearNodes fallback, list lifting or shared attrs.
// A fresh transaction keeps failed candidates out of Tiptap's shared chain tr.
export function createWordBlockTypeTransaction(state: EditorState, level: number): Transaction | null {
  validateEmptyParagraphFontDocument(omitDefaultEmptyParagraphFont(state.doc.toJSON()));
  if (!Number.isInteger(level) || level < 0 || level > 3) throw new Error('正文或标题级别须为 0–3。');
  const type = state.schema.nodes[level === 0 ? 'paragraph' : 'heading'];
  if (!type?.isTextblock) throw new Error(unsupported);
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  for (const [pos, node] of selectedTextblocks(state)) {
    if (!paragraphTypes.has(node.type.name) || !type.validContent(node.content)) throw new Error(unsupported);
    const attrs: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(node.attrs)) {
      if (key === 'level') continue;
      if (Object.prototype.hasOwnProperty.call(type.spec.attrs ?? {}, key)) attrs[key] = value;
      else if (value !== null && value !== undefined) throw new Error('目标段落不支持已有属性，未修改任何段落。');
    }
    if (level !== 0) attrs.level = level;
    const replacement = type.create(attrs, node.content, node.marks);
    if (node.eq(replacement)) continue;
    const $pos = transaction.doc.resolve(pos), index = $pos.index();
    if (!$pos.parent.canReplaceWith(index, index + 1, type, node.marks)) throw new Error(unsupported);
    transaction.setNodeMarkup(pos, type, attrs, node.marks);
  }
  if (!transaction.docChanged) return null;
  validateWordBlockTypeDocument(transaction.doc);
  // Types change in place without changing node sizes. Identity mapping keeps
  // backward text, node, all and table-cell selections at their exact boundaries.
  transaction.setSelection(state.selection.map(transaction.doc, new Mapping()));
  if (state.storedMarks !== null) transaction.setStoredMarks(state.storedMarks);
  return transaction;
}
