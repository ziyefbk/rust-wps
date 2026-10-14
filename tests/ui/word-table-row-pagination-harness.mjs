import {Editor, getSchema, Node as TiptapNode} from '@tiptap/core';
import {EditorState, TextSelection} from '@tiptap/pm/state';
import {history} from '@tiptap/pm/history';
import {tableEditing} from '@tiptap/pm/tables';
import {extensions, interactionExtensions, snapshot as baseSnapshot} from './word-block-type-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
export const rows = await loadTsModule('./ui/word-table-row-pagination.ts');
export const tables = await loadTsModule('./ui/word-table-widths.ts');
export const pagination = await loadTsModule('./ui/word-paragraph-pagination.ts');
const list = await loadTsModule('./ui/word-list-geometry.ts');
const empty = await loadTsModule('./ui/word-empty-paragraph-font.ts');
const pageBreak = await loadTsModule('./ui/word-page-break.ts');
const family = await loadTsModule('./ui/word-font-family-metadata.ts');
export const schema = getSchema([...extensions, pagination.ParagraphPagination, list.PreservedListGeometry, empty.EmptyParagraphFont, family.WordFontFamilyMetadata, pageBreak.PageBreakParagraph]);
export const p = (text = '', attrs = {}) => schema.node('paragraph', attrs, text ? schema.text(text) : null);
export const table = (flags = [null, false, true], attrs = {}) => schema.node('table', attrs, flags.map((flag, row) => schema.node('tableRow', {rowCantSplit: flag}, [0, 1].map(col => schema.node(row ? 'tableCell' : 'tableHeader', null, p(`${row}:${col}`))))));
export const doc = (nodes = [table()], attrs = {}) => schema.node('doc', attrs, nodes);
export const positions = (document, names) => {const result = []; document.descendants((node, pos) => {if (names.includes(node.type.name)) result.push(pos);}); return result;};
export const cells = document => positions(document, ['tableCell', 'tableHeader']);
export const flags = document => positions(document, ['tableRow']).map(pos => document.nodeAt(pos).attrs.rowCantSplit);
export const snapshot = document => pagination.omitDefaultParagraphPagination(list.omitDefaultListGeometry(empty.omitDefaultEmptyParagraphFont(baseSnapshot(document))));
export const stateAt = (document = doc(), index = 0, errors = []) => EditorState.create({doc: document,
  selection: TextSelection.create(document, cells(document)[index] + 2),
  plugins: [history(), rows.tableRowPaginationGuard(error => errors.push(error)), tableEditing(), tables.tableWidthGuard(error => errors.push(error))]});
export function fullEditor(document, locked = () => false) {
  const errors = [], configured = interactionExtensions({onError: message => errors.push(message), locked}).filter(extension => extension.name !== 'pageBreak').map(extension => extension.name === 'starterKit' ? extension.extend({addExtensions() {
    return (this.parent?.() ?? []).map(node => node instanceof TiptapNode ? list.withPreservedListRendering(node) : node);
  }}) : extension.name === 'table' ? tables.WordTable.configure({onWidthError: message => errors.push(message), locked}) : extension);
  const editor = new Editor({element: null, extensions: [...configured,
    rows.TableRowPagination.configure({onError: message => errors.push(message), locked}),
    pagination.ParagraphPagination.configure({onError: message => errors.push(message), locked}),
    list.PreservedListGeometry.configure({onError: message => errors.push(message), locked}),
    empty.EmptyParagraphFont.configure({onError: message => errors.push(message)}), family.WordFontFamilyMetadata,
    pageBreak.PageBreak, pageBreak.PageBreakParagraph], content: snapshot(document)});
  for (const plugin of editor.extensionManager.plugins) editor.registerPlugin(plugin);
  const base = editor.view, dispatch = editor.extensionManager.dispatchTransaction(base.dispatch.bind(base));
  const view = new Proxy(base, {get(target, property) {return property === 'dispatch' ? dispatch : property === 'state' ? editor.state : Reflect.get(target, property);}});
  Object.defineProperty(editor, 'view', {get: () => view});
  // Settle the actual application TrailingNode initialization without making
  // that pre-existing normalization a user action in the test history.
  view.dispatch(editor.state.tr.setMeta('addToHistory', false));
  return {editor, errors};
}
export function key(editor, key, modifiers = {}) {
  const event = {key, keyCode: ({Enter: 13, Backspace: 8, Delete: 46, Tab: 9})[key] ?? key.charCodeAt(0), ctrlKey: false, altKey: false, metaKey: false, shiftKey: false, ...modifiers};
  return editor.state.plugins.some(plugin => plugin.props.handleKeyDown?.(editor.view, event));
}
export const paste = (editor, slice, event = {}) => editor.state.plugins.some(plugin => plugin.props.handlePaste?.(editor.view, event, slice));
