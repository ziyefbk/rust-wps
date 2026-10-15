import {Editor, getSchema} from '@tiptap/core';
import {EditorState, TextSelection} from '@tiptap/pm/state';
import {history} from '@tiptap/pm/history';
import {extensions, interactionExtensions, snapshot as baseSnapshot} from './word-block-type-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
export const spacing = await loadTsModule('./ui/word-paragraph-spacing.ts');
export const minimum = await loadTsModule('./ui/word-minimum-line-height.ts');
export const pagination = await loadTsModule('./ui/word-paragraph-pagination.ts');
export const empty = await loadTsModule('./ui/word-empty-paragraph-font.ts');
export const block = await loadTsModule('./ui/word-block-type.ts');
export const controls = await loadTsModule('./ui/word-paragraph-spacing-controls.ts');
export const schema = getSchema([...extensions, minimum.MinimumLineHeight, pagination.ParagraphPagination, empty.EmptyParagraphFont]);
export const p = (text = '', attrs = {}) => schema.node('paragraph', attrs, text ? schema.text(text) : null);
export const h = (text = '', attrs = {}) => schema.node('heading', {level: 2, ...attrs}, text ? schema.text(text) : null);
export const doc = (nodes, attrs = {}) => schema.node('doc', attrs, nodes);
export const snapshot = document => pagination.omitDefaultParagraphPagination(empty.omitDefaultEmptyParagraphFont(baseSnapshot(document)));
export const stateFor = (document, selection = TextSelection.atStart(document), errors = []) => EditorState.create({doc: document, selection,
  plugins: [history(), minimum.minimumLineHeightGuard(message => errors.push(message)), pagination.paragraphPaginationGuard(message => errors.push(message))]});
export function fullEditor(document, locked = () => false) {
  const errors = [], editor = new Editor({element: null, extensions: [...interactionExtensions({onError: message => errors.push(message), locked}),
    minimum.MinimumLineHeight.configure({onError: message => errors.push(message), locked}),
    pagination.ParagraphPagination.configure({onError: message => errors.push(message), locked}),
    empty.EmptyParagraphFont.configure({onError: message => errors.push(message)})], content: snapshot(document)});
  for (const plugin of editor.extensionManager.plugins) editor.registerPlugin(plugin);
  const base = editor.view, dispatch = editor.extensionManager.dispatchTransaction(base.dispatch.bind(base));
  const view = new Proxy(base, {get(target, property) {return property === 'dispatch' ? dispatch : property === 'state' ? editor.state : Reflect.get(target, property);}});
  Object.defineProperty(editor, 'view', {get: () => view}); return {editor, errors};
}
export function key(editor, name, modifiers = {}) {
  const event = {key: name, keyCode: ({Enter: 13, Backspace: 8, Delete: 46})[name] ?? name.charCodeAt(0), ctrlKey: false, altKey: false, metaKey: false, shiftKey: false, ...modifiers};
  return editor.state.plugins.some(plugin => plugin.props.handleKeyDown?.(editor.view, event));
}
