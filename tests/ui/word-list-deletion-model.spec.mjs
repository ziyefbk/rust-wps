import {test, expect} from '@playwright/test';
import {Editor, getSchema, Node as TiptapNode} from '@tiptap/core';
import {AllSelection, EditorState, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {history, closeHistory, undo, redo, undoDepth} from '@tiptap/pm/history';
import {deleteSelection} from '@tiptap/pm/commands';
import {extensions, interactionExtensions, snapshot as baseSnapshot} from './word-block-type-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
const list = await loadTsModule('./ui/word-list-geometry.ts');
const page = await loadTsModule('./ui/word-page-geometry.ts');
const indent = await loadTsModule('./ui/word-paragraph-indent.ts');
const empty = await loadTsModule('./ui/word-empty-paragraph-font.ts');
const schema = getSchema([...extensions, list.PreservedListGeometry, empty.EmptyParagraphFont]);
const geometry = page.presetPageGeometry('Letter', 'portrait');
const p = (text = '', attrs = {}) => schema.node('paragraph', attrs, text ? schema.text(text) : null);
const items = ['First', 'Middle', 'Last'].map((text, i) => schema.node('listItem', null,
  schema.node('paragraph', {listGeometry: {leftTwips: 720 + i * 240, rightTwips: i * 80, hangingTwips: 360},
    spaceBefore: i * 20, spaceAfter: i * 40, lineSpacing: 240 + i * 12}, schema.text(text, [schema.mark(i === 1 ? 'italic' : 'bold')]))));
const ordered = (content = items, start = 7) => schema.node('orderedList', {start, markerFont: {fontFamily: 'Calibri', fontSize: 22}}, content);
const doc = (content = [ordered(), p('Outside', {spaceAfter: 0, indentLeftTwips: 120})]) => schema.node('doc', {styleProfile: 'standard-v2', pageGeometry: geometry}, content);
const itemPosition = (document, index, listPosition = 0) => listPosition + 1 + Array.from({length: index}, (_, i) => document.nodeAt(listPosition).child(i).nodeSize).reduce((a, b) => a + b, 0);
const textPosition = (document, index) => itemPosition(document, index) + 2;
const snapshot = document => list.omitDefaultListGeometry(empty.omitDefaultEmptyParagraphFont(baseSnapshot(document)));
const stateFor = (document, selection, errors = []) => EditorState.create({doc: document, selection,
  plugins: [history(), list.listGeometryGuard(message => errors.push(message)), page.pageGeometryGuard(), indent.paragraphIndentGuard()]});
function checkHistory(state, before, selected) {
  const edited = state.doc;
  expect(schema.nodeFromJSON(snapshot(edited)).eq(edited)).toBe(true);
  expect(state.doc.attrs).toEqual(before.attrs); expect(undoDepth(state)).toBe(1);
  expect(undo(state, tr => {state = state.apply(tr);})).toBe(true);
  expect(state.doc.eq(before)).toBe(true); expect(state.selection.eq(selected)).toBe(true);
  expect(redo(state, tr => {state = state.apply(tr);})).toBe(true); expect(state.doc.eq(edited)).toBe(true);
}
function applyCandidate(document, selection) {
  const errors = [], state = stateFor(document, selection, errors), transaction = list.createPreservedListDeleteTransaction(state);
  expect(transaction).not.toBeNull(); const result = state.applyTransaction(closeHistory(transaction));
  expect(result.transactions).toHaveLength(1); expect(errors).toEqual([]); checkHistory(result.state, document, selection);
  return result.state;
}
for (const index of [0, 1, 2]) test(`node-selected item ${index} deletion keeps start and every survivor exact with draft undo redo`, () => {
  for (const start of [1, 7, 999999]) {
    const initial = doc([ordered(items, start), p('Outside')]);
    const state = applyCandidate(initial, NodeSelection.create(initial, itemPosition(initial, index)));
    expect(state.doc.firstChild.attrs).toEqual(initial.firstChild.attrs);
    const survivors = items.filter((_, i) => i !== index);
    expect(state.doc.firstChild.childCount).toBe(2);
    survivors.forEach((item, i) => expect(state.doc.firstChild.child(i).eq(item)).toBe(true));
    expect(state.doc.lastChild.eq(initial.lastChild)).toBe(true);
  }
});
for (const kind of ['list', 'sole-item', 'document']) test(`${kind} deletion of only list produces neutral paragraph and retains exact document page attrs`, () => {
  const initial = doc([ordered(kind === 'sole-item' ? [items[0]] : items)]);
  const selection = kind === 'document' ? new AllSelection(initial) : NodeSelection.create(initial, kind === 'list' ? 0 : 1);
  const state = applyCandidate(initial, selection);
  expect(state.doc.childCount).toBe(1); expect(state.doc.firstChild.eq(p())).toBe(true);
  expect(state.selection.from).toBe(1); expect(state.storedMarks).toBeNull();
});
test('AllSelection replacement keeps document attributes and does not inherit list or neighboring paragraph styling', () => {
  const initial = doc([p('Before', {spaceBefore: 400}), ordered(), p('After')]);
  const state = applyCandidate(initial, new AllSelection(initial)); expect(state.doc.firstChild.eq(p())).toBe(true);
});
test('node-selected whole list removes only that block and keeps adjacent lists separate', () => {
  const initial = doc([p('Before'), ordered(), ordered([items[2]], 99), p('After')]);
  const state = applyCandidate(initial, NodeSelection.create(initial, initial.firstChild.nodeSize));
  expect(state.doc.childCount).toBe(3);
  [initial.child(0), initial.child(2), initial.child(3)].forEach((node, i) => expect(state.doc.child(i).eq(node)).toBe(true));
});
for (const [label, first, last] of [['first', 0, 1], ['middle', 1, 2], ['prefix', 0, 2], ['last', 2, 3], ['suffix', 1, 3], ['whole-list', 0, 3]]) {
  test(`text-selected ${label} complete boundary deletes only crossed items in either direction`, () => {
    for (const reverse of [false, true]) {
      const initial = doc(), from = textPosition(initial, first), to = last === 3 ? initial.firstChild.nodeSize + 1 : textPosition(initial, last);
      const state = applyCandidate(initial, TextSelection.create(initial, reverse ? to : from, reverse ? from : to));
      const survivors = items.filter((_, i) => i < first || i >= last);
      if (!survivors.length) {expect(state.doc.childCount).toBe(1); expect(state.doc.firstChild.eq(initial.lastChild)).toBe(true);}
      else {expect(state.doc.firstChild.attrs).toEqual(initial.firstChild.attrs); survivors.forEach((item, i) => expect(state.doc.firstChild.child(i).eq(item)).toBe(true));}
      expect(state.doc.lastChild.eq(initial.lastChild)).toBe(true);
    }
  });
}
test('candidate construction never mutates a chain shared transaction and cannot authorize later appended topology changes', () => {
  const initial = doc(), selection = NodeSelection.create(initial, 1), state = stateFor(initial, selection);
  const shared = state.tr, chainable = Object.create(state); Object.defineProperty(chainable, 'tr', {get: () => shared});
  const candidate = list.createPreservedListDeleteTransaction(chainable); expect(shared.steps).toEqual([]); expect(shared.doc).toBe(initial);
  candidate.replaceWith(0, candidate.doc.firstChild.nodeSize, p('erased'));
  expect(state.applyTransaction(candidate).transactions).toEqual([]); expect(undoDepth(state)).toBe(0);
  const forged = state.tr.delete(1, 1 + initial.firstChild.firstChild.nodeSize).setMeta('safeDeletions', true);
  expect(state.applyTransaction(forged).transactions).toEqual([]);
});
test('raw valid node-boundary deletion remains refused without app-owned selection candidate proof', () => {
  const initial = doc(), state = stateFor(initial, NodeSelection.create(initial, 1)); let candidate;
  expect(deleteSelection(state, tr => {candidate = tr;})).toBe(true);
  expect(state.applyTransaction(candidate).transactions).toEqual([]);
  expect(state.applyTransaction(state.tr.delete(0, initial.firstChild.nodeSize)).transactions).toEqual([]);
  expect(undoDepth(state)).toBe(0);
});
test('partial cross-item endpoints and complete-text ranges without terminal boundary remain atomic refusals', () => {
  const initial = doc();
  for (const [from, to] of [[4, textPosition(initial, 1)], [3, textPosition(initial, 1) + 2], [3, textPosition(initial, 2) + items[2].firstChild.content.size], [textPosition(initial, 2), initial.firstChild.nodeSize + 2]]) {
    const state = stateFor(initial, TextSelection.create(initial, from, to));
    expect(() => list.createPreservedListDeleteTransaction(state)).toThrow('编号');
    expect(state.applyTransaction(state.tr.deleteSelection()).transactions).toEqual([]); expect(undoDepth(state)).toBe(0);
  }
});
test('cross-list text selection does not infer deletion of mixed content', () => {
  const initial = doc([ordered(), p('gap'), ordered([items[2]], 12)]), state = stateFor(initial, TextSelection.create(initial, 3, initial.child(0).nodeSize + initial.child(1).nodeSize + 3));
  expect(() => list.createPreservedListDeleteTransaction(state)).toThrow('编号');
  expect(state.applyTransaction(state.tr.deleteSelection()).transactions).toEqual([]);
});
test('one-item entire text selection still clears text and retains item number and metadata', () => {
  const initial = doc(), selection = TextSelection.create(initial, 3, 3 + items[0].firstChild.content.size), state = stateFor(initial, selection);
  expect(list.createPreservedListDeleteTransaction(state)).toBeNull();
  const result = state.applyTransaction(closeHistory(state.tr.deleteSelection())); expect(result.transactions).toHaveLength(1);
  expect(result.state.doc.firstChild.childCount).toBe(3); expect(result.state.doc.firstChild.firstChild.textContent).toBe('');
  expect(result.state.doc.firstChild.firstChild.firstChild.attrs).toEqual(items[0].firstChild.attrs); checkHistory(result.state, initial, selection);
});
test('collapsed cursors and unpreserved selections retain the ordinary path', () => {
  const initial = doc();
  for (const position of [3, textPosition(initial, 1), textPosition(initial, 2)]) expect(list.createPreservedListDeleteTransaction(stateFor(initial, TextSelection.create(initial, position)))).toBeNull();
  const plain = doc([p('plain')]); expect(list.createPreservedListDeleteTransaction(stateFor(plain, new AllSelection(plain)))).toBeNull();
  const outside = stateFor(initial, TextSelection.create(initial, initial.firstChild.nodeSize + 1, initial.firstChild.nodeSize + 3));
  expect(list.createPreservedListDeleteTransaction(outside)).toBeNull(); expect(outside.applyTransaction(outside.tr.deleteSelection()).transactions).toHaveLength(1);
});
const editors = [];
function fullEditor(document, locked = () => false) {
  const errors = [], configured = interactionExtensions({onError: message => errors.push(message)}).map(extension => extension.name === 'starterKit' ? extension.extend({addExtensions() {return (this.parent?.() ?? []).map(node => node instanceof TiptapNode ? list.withPreservedListRendering(node) : node);}}) : extension);
  const editor = new Editor({element: null, extensions: [...configured, list.PreservedListGeometry.configure({onError: message => errors.push(message), locked}), empty.EmptyParagraphFont.configure({onError: message => errors.push(message)})], content: snapshot(document)});
  for (const plugin of editor.extensionManager.plugins) editor.registerPlugin(plugin);
  const base = editor.view, dispatch = editor.extensionManager.dispatchTransaction(base.dispatch.bind(base));
  const view = new Proxy(base, {get(target, property) {return property === 'dispatch' ? dispatch : property === 'state' ? editor.state : Reflect.get(target, property);}});
  Object.defineProperty(editor, 'view', {get: () => view}); editors.push(editor); return {editor, errors};
}
function key(editor, name) {const event = {key: name, keyCode: name === 'Backspace' ? 8 : 46, ctrlKey: false, altKey: false, metaKey: false, shiftKey: false}; return editor.state.plugins.some(plugin => plugin.props.handleKeyDown?.(editor.view, event));}
test.afterEach(() => {for (const editor of editors.splice(0)) editor.destroy();});
for (const name of ['Backspace', 'Delete']) test(`registered ${name} deletes a boundary selection and keeps exact full-chain undo redo`, () => {
  const {editor, errors} = fullEditor(doc()); const initial = editor.state.doc;
  editor.commands.setTextSelection({from: textPosition(initial, 1), to: textPosition(initial, 2)});
  expect(key(editor, name)).toBe(true); expect(editor.state.doc.firstChild.childCount).toBe(2);
  expect(editor.state.doc.firstChild.child(1).eq(initial.firstChild.child(2))).toBe(true); const edited = editor.state.doc;
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(initial)).toBe(true);
  expect(editor.commands.redo()).toBe(true); expect(editor.state.doc.eq(edited)).toBe(true); expect(errors).toEqual([]);
});
test('registered deletion respects readonly and app lock without mutating history', () => {
  for (const name of ['Backspace', 'Delete']) for (const mode of ['readonly', 'locked']) {
    const {editor, errors} = fullEditor(doc(), () => mode === 'locked'); const initial = editor.state.doc;
    editor.commands.setNodeSelection(1); if (mode === 'readonly') editor.setEditable(false, false);
    expect(key(editor, name)).toBe(true); expect(editor.state.doc.eq(initial)).toBe(true); expect(undoDepth(editor.state)).toBe(0); expect(errors).toEqual([]);
  }
});
test('AllSelection with full extension chain does not add inherited formatting or extra trailing blocks', () => {
  const {editor, errors} = fullEditor(doc([ordered()])); editor.view.dispatch(editor.state.tr.setSelection(new AllSelection(editor.state.doc)));
  const initial = editor.state.doc, beforeDepth = undoDepth(editor.state); expect(key(editor, 'Delete')).toBe(true); expect(editor.state.doc.childCount).toBe(1); expect(editor.state.doc.firstChild.eq(editor.schema.nodes.paragraph.createAndFill())).toBe(true);
  expect(editor.state.doc.attrs).toEqual(initial.attrs); expect(undoDepth(editor.state)).toBe(beforeDepth + 1); const edited = editor.state.doc;
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(initial)).toBe(true); expect(editor.commands.redo()).toBe(true); expect(editor.state.doc.eq(edited)).toBe(true); expect(errors).toEqual([]);
});
test('identical immutable list instances at distinct positions never authorize cross-list deletion', () => {
  for (const gap of [[], [p('Gap')]]) {
    const shared = ordered(), initial = doc([shared, ...gap, shared]);
    const second = shared.nodeSize + gap.reduce((sum, node) => sum + node.nodeSize, 0);
    for (const fromIndex of [0, 1]) for (const toIndex of [0, 2]) {
      const state = stateFor(initial, TextSelection.create(initial, textPosition(initial, fromIndex), itemPosition(initial, toIndex, second) + 2));
      expect(() => list.createPreservedListDeleteTransaction(state)).toThrow('编号');
      expect(state.applyTransaction(state.tr.deleteSelection()).transactions).toEqual([]);
    }
  }
});
test('registered deletion is a separate undo event between immediate prior and following typing', () => {
  const {editor, errors} = fullEditor(doc()); const original = editor.state.doc;
  editor.commands.setTextSelection(3); editor.view.dispatch(editor.state.tr.insertText('EDIT')); const before = editor.state.doc;
  editor.commands.setNodeSelection(1); expect(key(editor, 'Delete')).toBe(true); const deleted = editor.state.doc;
  editor.view.dispatch(editor.state.tr.insertText('AFTER')); const after = editor.state.doc; expect(undoDepth(editor.state)).toBe(3);
  for (const expected of [deleted, before, original]) {expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(expected)).toBe(true);}
  for (const expected of [before, deleted, after]) {expect(editor.commands.redo()).toBe(true); expect(editor.state.doc.eq(expected)).toBe(true);}
  expect(errors).toEqual([]);
});
test('empty sole item deletion removes the empty list shell and clears stored typing marks', () => {
  const blank = schema.node('listItem', null, schema.node('paragraph', items[0].firstChild.attrs));
  const initial = doc([ordered([blank])]), selection = NodeSelection.create(initial, 1);
  let state = stateFor(initial, selection); state = state.apply(state.tr.setStoredMarks([schema.mark('bold')]));
  const result = state.applyTransaction(list.createPreservedListDeleteTransaction(state));
  expect(result.transactions).toHaveLength(1); expect(result.state.doc.firstChild.eq(p())).toBe(true); expect(result.state.storedMarks).toBeNull(); checkHistory(result.state, initial, selection);
});
