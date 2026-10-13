import {Extension} from '@tiptap/core';
import type {Node} from '@tiptap/pm/model';
import {EditorState, Plugin, Selection, type Transaction} from '@tiptap/pm/state';
import {Decoration, DecorationSet} from '@tiptap/pm/view';
import {omitDefaultPageGeometry, pageTableWidthTwips, isPageGeometry, validatePageGeometryDocument} from './word-page-geometry';
import {parseParagraphSpacePoints, parseParagraphSpaceCss} from './word-paragraph-spacing';
import {omitDefaultParagraphIndent, paragraphIndentError, paragraphIndentFields, paragraphIndentTypes,
  simpleIndentTable, tableHasParagraphIndent, validParagraphIndent, validateParagraphIndentDocument, type ParagraphIndentField, type ParagraphIndentPatch} from './word-paragraph-indent-model';
export * from './word-paragraph-indent-model';

export function parseParagraphIndentPoints(field: ParagraphIndentField, value: string): number | null {
  const text = value.trim(), negative = text.startsWith('-');
  const absolute = parseParagraphSpacePoints(negative ? text.slice(1) : text);
  if (absolute === null || negative && field !== 'firstLineIndentTwips') return null;
  return negative && absolute ? -absolute : absolute;
}
export function parseParagraphIndentCss(field: ParagraphIndentField, value: string): number | null {
  const text = value.trim(), negative = text.startsWith('-');
  const absolute = parseParagraphSpaceCss(negative ? text.slice(1) : text);
  return absolute === null || negative && field !== 'firstLineIndentTwips' ? null : negative && absolute ? -absolute : absolute;
}
const dataNames = {indentLeftTwips: 'data-indent-left-twips', indentRightTwips: 'data-indent-right-twips', firstLineIndentTwips: 'data-first-line-indent-twips'};
const properties = {indentLeftTwips: 'margin-left', indentRightTwips: 'margin-right', firstLineIndentTwips: 'text-indent'};
const styleProperties = {indentLeftTwips: 'marginLeft', indentRightTwips: 'marginRight', firstLineIndentTwips: 'textIndent'} as const;
function selectedParagraphs(state: EditorState): Map<number, Node> {
  const result = new Map<number, Node>();
  if (state.selection.empty) {
    const {$from} = state.selection;
    for (let depth = $from.depth; depth > 0; depth--) if (paragraphIndentTypes.has($from.node(depth).type.name)) {
      result.set($from.before(depth), $from.node(depth)); break;
    }
  } else for (const {$from, $to} of state.selection.ranges) state.doc.nodesBetween($from.pos, $to.pos, (node, pos) => {
    if (!paragraphIndentTypes.has(node.type.name)) return;
    if (pos + 1 < $to.pos && pos + node.nodeSize > $from.pos) result.set(pos, node);
    return false;
  });
  return result;
}
function supportedParagraph(state: EditorState, pos: number, tables: Map<Node, boolean>): boolean {
  const $pos = state.doc.resolve(pos);
  if ($pos.depth === 0) return true;
  if ($pos.depth !== 3 || $pos.node(1).type.name !== 'table' || $pos.node(2).type.name !== 'tableRow'
    || !['tableCell', 'tableHeader'].includes($pos.node(3).type.name)) return false;
  const table = $pos.node(1);
  if (!tables.has(table)) tables.set(table, simpleIndentTable(table.toJSON()));
  return tables.get(table)!;
}
export function selectedParagraphIndent(state: EditorState) {
  const selected = selectedParagraphs(state), nodes = [...selected.values()], tables = new Map<Node, boolean>();
  const fields = Object.fromEntries(paragraphIndentFields.map(field => {
    const values = nodes.map(node => validParagraphIndent(field, node.attrs[field]) ? node.attrs[field] as number : null);
    const first = values[0] ?? null, mixed = values.some(value => value !== first);
    return [field, {value: mixed ? null : first, mixed, explicit: values.some(value => value !== null)}];
  })) as Record<ParagraphIndentField, {value: number | null; mixed: boolean; explicit: boolean}>;
  return {count: nodes.length, supported: nodes.length > 0 && [...selected.keys()].every(pos => supportedParagraph(state, pos, tables)), fields};
}
const encoder = new TextEncoder();
function validateProspectiveIndent(document: Node): void {
  let count = 0, bytes = 0;
  const stack = [{node: document, depth: 0}];
  while (stack.length) {
    const {node, depth} = stack.pop()!;
    if (++count > 10000 || depth > 32) throw new Error('段落缩进修改后文档结构超出保存上限。');
    if (node.isText) bytes += encoder.encode(node.text!).length;
    if (bytes > 2 * 1024 * 1024) throw new Error('正文超过 2 MiB 保存上限，未应用段落缩进。');
    node.forEach(child => stack.push({node: child, depth: depth + 1}));
  }
  const snapshot = omitDefaultParagraphIndent(omitDefaultPageGeometry(document.toJSON()));
  validateParagraphIndentDocument(snapshot);
  validatePageGeometryDocument(snapshot);
  if (encoder.encode(JSON.stringify({version: 1, document: snapshot})).length > 32 * 1024 * 1024) throw new Error('段落缩进修改后草稿超过 32 MiB 保存上限。');
}
export interface ParagraphIndentToken {doc: Node; selection: Selection}
export const captureParagraphIndent = (state: EditorState): ParagraphIndentToken => ({doc: state.doc, selection: state.selection});
export const matchesParagraphIndent = (state: EditorState, token: ParagraphIndentToken): boolean => state.doc === token.doc && state.selection.eq(token.selection);
export function createParagraphIndentTransaction(state: EditorState, patch: ParagraphIndentPatch, token?: ParagraphIndentToken): Transaction | null {
  const fields = Object.keys(patch) as ParagraphIndentField[];
  for (const field of fields) if (!paragraphIndentFields.includes(field) || patch[field] !== null && !validParagraphIndent(field, patch[field])) throw new Error(paragraphIndentError);
  if (token && !matchesParagraphIndent(state, token)) return null;
  const selected = selectedParagraphs(state), tables = new Map<Node, boolean>();
  if (!fields.length || !selected.size) return null;
  if ([...selected.keys()].some(pos => !supportedParagraph(state, pos, tables))) throw new Error(paragraphIndentError);
  const transaction = EditorState.create({doc: state.doc, selection: state.selection, storedMarks: state.storedMarks}).tr;
  for (const [pos, node] of selected) if (fields.some(field => (node.attrs[field] ?? null) !== patch[field])) {
    transaction.setNodeMarkup(pos, undefined, {...node.attrs, ...patch}, node.marks);
  }
  if (!transaction.docChanged) return null;
  validateProspectiveIndent(transaction.doc);
  transaction.setSelection(Selection.fromJSON(transaction.doc, state.selection.toJSON()));
  if (state.storedMarks) transaction.setStoredMarks(state.storedMarks);
  return transaction;
}
export function paragraphIndentGuard(onRejected: (message: string) => void = () => {}): Plugin {
  return new Plugin({filterTransaction: transaction => {
    if (!transaction.docChanged) return true;
    try {validateProspectiveIndent(transaction.doc); return true;} catch (error) {onRejected(String(error)); return false;}
  }, props: {decorations: state => {
    const decorations: Decoration[] = [], page = state.doc.attrs.pageGeometry;
    state.doc.forEach((node, pos) => {
      if (node.type.name !== 'table' || !tableHasParagraphIndent(omitDefaultParagraphIndent(node.toJSON()))) return;
      decorations.push(Decoration.node(pos, pos + node.nodeSize, {'data-paragraph-indent-table': '',
        style: `--word-indent-table-width: ${pageTableWidthTwips(node.attrs, isPageGeometry(page) ? page : null) / 20}pt`}));
    });
    return DecorationSet.create(state.doc, decorations);
  }}});
}
export const ParagraphIndent = Extension.create<{onIndentError: (message: string) => void}>({
  name: 'paragraphIndent',
  addOptions: () => ({onIndentError: () => {}}),
  addGlobalAttributes() {return [{types: [...paragraphIndentTypes], attributes: Object.fromEntries(paragraphIndentFields.map(field => [field, {
    default: null,
    parseHTML: (element: HTMLElement) => {
      const exact = element.getAttribute(dataNames[field]);
      // Preserve evidence of malformed/unsupported active HTML indentation as
      // a rejected sentinel, rather than silently dropping it on paste.
      if (exact !== null) return /^-?\d{1,5}$/.test(exact) && validParagraphIndent(field, Number(exact)) ? Number(exact) : 'invalid-paragraph-indent';
      const css = element.style[styleProperties[field]] ?? '';
      return css.trim() ? parseParagraphIndentCss(field, css) ?? 'invalid-paragraph-indent' : null;
    },
    renderHTML: (attrs: Record<string, unknown>) => validParagraphIndent(field, attrs[field])
      ? {[dataNames[field]]: String(attrs[field]), style: `${properties[field]}: ${Number(attrs[field]) / 20}pt`} : {},
  }]))}];},
  addProseMirrorPlugins() {return [paragraphIndentGuard(this.options.onIndentError)];},
});
