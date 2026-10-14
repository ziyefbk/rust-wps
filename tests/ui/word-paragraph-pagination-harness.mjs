import {Editor, getSchema, Node as TiptapNode} from '@tiptap/core';
import {EditorState, TextSelection} from '@tiptap/pm/state';
import {history} from '@tiptap/pm/history';
import {extensions, interactionExtensions, snapshot as baseSnapshot} from './word-block-type-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
export const pagination = await loadTsModule('./ui/word-paragraph-pagination.ts');
export const list = await loadTsModule('./ui/word-list-geometry.ts');
export const empty = await loadTsModule('./ui/word-empty-paragraph-font.ts');
export const page = await loadTsModule('./ui/word-page-geometry.ts');
export const indent = await loadTsModule('./ui/word-paragraph-indent.ts');
export const schema = getSchema([...extensions, pagination.ParagraphPagination, list.PreservedListGeometry, empty.EmptyParagraphFont]);
export const p = (text = '', attrs = {}) => schema.node('paragraph', attrs, text ? schema.text(text) : null);
export const h = (text = '', attrs = {}) => schema.node('heading', {level: 2, ...attrs}, text ? schema.text(text) : null);
export const doc = (nodes, attrs = {}) => schema.node('doc', attrs, nodes);
export const snapshot = document => pagination.omitDefaultParagraphPagination(list.omitDefaultListGeometry(empty.omitDefaultEmptyParagraphFont(baseSnapshot(document))));
export const stateFor = (document, selection = TextSelection.atStart(document), errors = []) => EditorState.create({doc: document, selection,
  plugins: [history(), pagination.paragraphPaginationGuard(message => errors.push(message)), list.listGeometryGuard(message => errors.push(message)), page.pageGeometryGuard(), indent.paragraphIndentGuard()]});
export function fullEditor(document, locked = () => false) {
  const errors = [], configured = interactionExtensions({onError: message => errors.push(message), locked}).map(extension => extension.name === 'starterKit' ? extension.extend({addExtensions() {
    return (this.parent?.() ?? []).map(node => node instanceof TiptapNode ? list.withPreservedListRendering(node) : node);
  }}) : extension);
  const editor = new Editor({element: null, extensions: [...configured,
    pagination.ParagraphPagination.configure({onError: message => errors.push(message), locked}),
    list.PreservedListGeometry.configure({onError: message => errors.push(message), locked}),
    empty.EmptyParagraphFont.configure({onError: message => errors.push(message)})], content: snapshot(document)});
  for (const plugin of editor.extensionManager.plugins) editor.registerPlugin(plugin);
  const base = editor.view, dispatch = editor.extensionManager.dispatchTransaction(base.dispatch.bind(base));
  const view = new Proxy(base, {get(target, property) {return property === 'dispatch' ? dispatch : property === 'state' ? editor.state : Reflect.get(target, property);}});
  Object.defineProperty(editor, 'view', {get: () => view}); return {editor, errors};
}
export function key(editor, key, modifiers = {}) {
  const event = {key, keyCode: ({Enter: 13, Backspace: 8, Delete: 46})[key] ?? key.charCodeAt(0), ctrlKey: false, altKey: false, metaKey: false, shiftKey: false, ...modifiers};
  return editor.state.plugins.some(plugin => plugin.props.handleKeyDown?.(editor.view, event));
}
