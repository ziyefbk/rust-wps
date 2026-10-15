// Actual complete registered plugin chain; no browser or physical key delivery.
import {test, expect} from '@playwright/test';
import {Editor, getSchema, getExtensionField} from '@tiptap/core';
import {AllSelection, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {CellSelection} from '@tiptap/pm/tables';
import {closeHistory, undoDepth} from '@tiptap/pm/history';
import {interactionExtensions, snapshot} from './word-block-type-harness.mjs';
import {loadTsModule} from './load-ts.mjs';
const {EmptyParagraphFont} = await loadTsModule('./ui/word-empty-paragraph-font.ts');
const a = {textAlign: 'center', spaceBefore: 0, spaceAfter: 120, lineSpacing: 241, indentLeftTwips: 360, indentRightTwips: 0, firstLineIndentTwips: -180};
const font = {family: 'Liberation Serif', halfPoints: 49, bold: false, italic: false};
const schema = getSchema([...interactionExtensions(), EmptyParagraphFont]);
const p = (text = '', attrs = {}) => schema.node('paragraph', attrs, text ? schema.text(text) : null);
const h = (content = '', attrs = {}) => schema.node('heading', {level: 2, ...a, ...attrs}, typeof content === 'string' ? content ? schema.text(content) : null : content);
const doc = (...content) => schema.node('doc', {styleProfile: 'standard-v2'}, content);
const editors = [];
function editorFor(document, options = {}, upstream = false) {
  const errors = [], extensions = interactionExtensions({onError: error => errors.push(error), ...options});
  // A control removes only our new shortcuts, retaining identical app schema,
  // normalizers and the remaining registered keymaps, including core/list/table.
  if (upstream) extensions[0] = extensions[0].extend({addExtensions() {
    return this.parent().map(node => node.name !== 'heading' ? node : node.extend({addKeyboardShortcuts() {
      return Object.fromEntries(Object.entries(this.parent()).filter(([key]) => !['Backspace', 'Mod-Backspace', 'Shift-Backspace', 'Ctrl-h', 'Alt-Backspace'].includes(key)));
    }}));
  }});
  const editor = new Editor({element: null, extensions: [...extensions, EmptyParagraphFont.configure({onError: error => errors.push(error)})], content: snapshot(document)});
  for (const plugin of editor.extensionManager.plugins) editor.registerPlugin(plugin);
  // Structural commands query a view boundary; this deterministic text-offset
  // oracle supports model controls only, never claims rendered line geometry.
  const view = editor.view;
  // Headless Editor bypasses createView; install its real dispatch middleware.
  view.dispatch = editor.extensionManager.dispatchTransaction(view.dispatch.bind(view));
  view.endOfTextblock = direction => direction === 'backward' || direction === 'left' ? editor.state.selection.$from.parentOffset === 0
    : editor.state.selection.$to.parentOffset === editor.state.selection.$to.parent.content.size;
  Object.defineProperty(editor, 'view', {get: () => view});
  editor.view.dispatch(editor.state.tr.setMeta('addToHistory', false));
  editors.push(editor); return {editor, errors};
}
function key(editor, modifiers = {}, name = 'Backspace') {
  const event = {key: name, keyCode: name === 'Backspace' ? 8 : name.charCodeAt(0), ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...modifiers};
  return editor.state.plugins.some(plugin => plugin.props.handleKeyDown?.(editor.view, event));
}
const select = (editor, from, to = from) => editor.commands.setTextSelection({from, to});
const input = (editor, text) => editor.state.plugins.some(plugin => plugin.props.handleTextInput?.(editor.view, editor.state.selection.from, editor.state.selection.to, text));
function ownBackspace(editor) {
  const node = editor.extensionManager.extensions.find(value => value.name === 'heading');
  return getExtensionField(node, 'addKeyboardShortcuts', {name: node.name, options: node.options, storage: node.storage, editor, type: editor.schema.nodes.heading})().Backspace();
}
test.afterEach(() => {for (const editor of editors.splice(0)) editor.destroy();});
for (const level of [1, 2, 3]) for (const [alias, modifiers] of [['Backspace', {}], ['Ctrl-Backspace', {ctrlKey: true}], ['Shift-Backspace', {shiftKey: true}]]) {
  test(`${alias} preserves empty H${level} shell attributes and exact atomic history`, () => {
    const {editor, errors} = editorFor(doc(h('', {level, emptyParagraphFont: font}), p('outside')));
    const marks = [editor.schema.marks.italic.create(), editor.schema.marks.fontSize.create({halfPoints: 23})];
    editor.view.dispatch(editor.state.tr.setStoredMarks(marks));
    const before = editor.state.doc, selection = editor.state.selection.toJSON();
    expect(key(editor, modifiers)).toBe(true); const after = editor.state.doc;
    expect(after.firstChild.type.name).toBe('paragraph');
    const {level: _, ...attrs} = before.firstChild.attrs;
    expect(after.firstChild.attrs).toEqual(attrs); expect(after.firstChild.content.size).toBe(0);
    expect(after.lastChild.eq(before.lastChild)).toBe(true); expect(editor.state.selection.toJSON()).toEqual(selection);
    expect(editor.state.storedMarks).toEqual(marks); expect(undoDepth(editor.state)).toBe(1); expect(errors).toEqual([]);
    expect(snapshot(editor.schema.nodeFromJSON(snapshot(after)))).toEqual(snapshot(after));
    expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(before)).toBe(true);
    expect(editor.commands.redo()).toBe(true); expect(editor.state.doc.eq(after)).toBe(true);
    expect(key(editor, modifiers)).toBe(false); expect(editor.state.doc.eq(after)).toBe(true); expect(undoDepth(editor.state)).toBe(1);
  });
}

test('empty heading default and explicit-zero attributes remain distinct', () => {
  for (const attrs of [{textAlign: null, spaceBefore: null, spaceAfter: null, lineSpacing: null, indentLeftTwips: null, indentRightTwips: null, firstLineIndentTwips: null},
    {textAlign: 'left', spaceBefore: 0, spaceAfter: 0, lineSpacing: 240, indentLeftTwips: 0, indentRightTwips: 0, firstLineIndentTwips: 0}]) {
    const {editor} = editorFor(doc(h('', attrs), p('outside'))), before = editor.state.doc.firstChild;
    expect(key(editor)).toBe(true); const {level: _, ...expected} = before.attrs;
    expect(editor.state.doc.firstChild.attrs).toEqual(expected);
  }
});

test('active input rule has priority, expired window uses shell-only conversion', () => {
  for (const expire of [false, true]) {
    const {editor} = editorFor(doc(p('##', a), p('outside'))); select(editor, 3);
    expect(input(editor, ' ')).toBe(true); const heading = editor.state.doc;
    if (expire) {select(editor, 4); select(editor, 1);}
    expect(key(editor)).toBe(true); expect(editor.state.doc.firstChild.type.name).toBe('paragraph');
    expect(editor.state.doc.firstChild.textContent).toBe(expire ? '' : '## ');
    expect(editor.state.doc.firstChild.attrs).toEqual({...a, minimumLineHeightTwips: null, emptyParagraphFont: null});
    expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(heading)).toBe(true);
  }
});

test('Backspace history isolates earlier deletion and later first typing clears only the baseline', () => {
  const {editor} = editorFor(doc(h('x'), p('outside'))); select(editor, 2);
  editor.view.dispatch(closeHistory(editor.state.tr.delete(1, 2))); const empty = editor.state.doc;
  editor.view.dispatch(editor.state.tr.setNodeMarkup(0, undefined, {...empty.firstChild.attrs, emptyParagraphFont: font}));
  const before = editor.state.doc; expect(key(editor)).toBe(true); const converted = editor.state.doc;
  expect(input(editor, '输入')).toBe(true); expect(editor.state.doc.firstChild.attrs.emptyParagraphFont).toBeNull();
  expect(editor.state.doc.firstChild.attrs.spaceAfter).toBe(120); expect(editor.state.doc.firstChild.firstChild.marks).toEqual([]);
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(converted)).toBe(true);
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(before)).toBe(true);
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.firstChild.textContent).toBe('x');
});

test('unavailable matching conversion is consumed without destructive core fallback', () => {
  for (const mode of ['locked', 'readonly', 'composing']) {
    const {editor} = editorFor(doc(h('', {emptyParagraphFont: font}), p('outside')), {locked: () => mode === 'locked'});
    if (mode === 'readonly') editor.setEditable(false);
    if (mode === 'composing') editor.view.composing = true;
    const before = editor.state.doc;
    expect(key(editor)).toBe(true); expect(editor.state.doc.eq(before)).toBe(true); expect(undoDepth(editor.state)).toBe(0);
  }
});

test('invalid whole candidate is consumed atomically instead of clearing existing formatting', () => {
  const {editor, errors} = editorFor(doc(h(), p('invalid', {spaceAfter: -1})));
  const before = editor.state.doc, selection = editor.state.selection.toJSON();
  expect(key(editor)).toBe(true); expect(editor.state.doc.eq(before)).toBe(true); expect(editor.state.selection.toJSON()).toEqual(selection);
  expect(errors).toHaveLength(1); expect(undoDepth(editor.state)).toBe(0);
});

test('pending DOM-selection synchronization chooses the current caret before normalization', () => {
  let editor, calls = 0;
  ({editor} = editorFor(doc(h('', {emptyParagraphFont: font}), p('outside')), {syncSelection: () => {calls++; select(editor, 1);}}));
  select(editor, 4); expect(key(editor)).toBe(true); expect(calls).toBe(1); expect(editor.state.doc.firstChild.type.name).toBe('paragraph');
  expect(editor.state.doc.firstChild.attrs.emptyParagraphFont).toEqual(font);
});

test('isolating single-block cell converts in place with formatting and table geometry intact', () => {
  const table = schema.node('table', {columnWidthsTwips: [2400, 2400]}, schema.node('tableRow', {rowMinHeightTwips: 360}, [
    schema.node('tableCell', {cellVerticalAlign: 'bottom'}, h('', {emptyParagraphFont: font})), schema.node('tableCell', null, p('neighbor'))]));
  const {editor} = editorFor(doc(p('before'), table, p('outside'))); let pos;
  editor.state.doc.descendants((node, at) => {if (node.type.name === 'heading') pos = at;}); select(editor, pos + 1);
  const before = editor.state.doc; expect(key(editor)).toBe(true);
  const after = editor.state.doc, {level: _, ...attrs} = before.nodeAt(pos).attrs;
  expect(before.child(1).attrs.columnWidthsTwips).toEqual([2400, 2400]);
  expect(before.child(1).firstChild.attrs.rowMinHeightTwips).toBe(360);
  expect(before.child(1).firstChild.firstChild.attrs.cellVerticalAlign).toBe('bottom');
  expect(after.nodeAt(pos).type.name).toBe('paragraph'); expect(after.nodeAt(pos).attrs).toEqual(attrs);
  expect(after.child(1).attrs).toEqual(before.child(1).attrs); expect(after.child(1).firstChild.firstChild.attrs).toEqual(before.child(1).firstChild.firstChild.attrs);
  expect(after.child(1).firstChild.lastChild.eq(before.child(1).firstChild.lastChild)).toBe(true);
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(before)).toBe(true);
});

test('single imported heading retains the automatically appended tail through conversion and undo', () => {
  const {editor} = editorFor(doc(h('', {emptyParagraphFont: font})));
  expect(editor.state.doc.childCount).toBe(2); expect(editor.state.doc.lastChild.type.name).toBe('paragraph');
  expect(editor.state.doc.lastChild.content.size).toBe(0); expect(undoDepth(editor.state)).toBe(0);
  const before = editor.state.doc; expect(key(editor)).toBe(true); const after = editor.state.doc;
  expect(after.childCount).toBe(2); expect(after.lastChild.eq(before.lastChild)).toBe(true);
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(before)).toBe(true);
  expect(editor.commands.redo()).toBe(true); expect(editor.state.doc.eq(after)).toBe(true);
});

const list = () => schema.node('bulletList', null, schema.node('listItem', null, [p('item'), h()]));
const table = () => schema.node('table', null, schema.node('tableRow', null, [schema.node('tableCell', null, [p('cell'), h()]), schema.node('tableCell', null, p('neighbor'))]));
for (const [name, document, selection] of [
  ['nonempty heading join', () => doc(p('before'), h('after'), p('outside')), d => TextSelection.create(d, 9)],
  ['nonfirst empty heading join', () => doc(p('before'), h(), p('outside')), d => TextSelection.create(d, 9)],
  ['hard-break heading', () => doc(h(schema.node('hardBreak')), p('outside')), d => TextSelection.create(d, 1)],
  ['range selection', () => doc(h('AB'), p('outside')), d => TextSelection.create(d, 1, 2)],
  ['node selection', () => doc(h(), p('outside')), d => NodeSelection.create(d, 0)],
  ['all selection', () => doc(h(), p('outside')), d => new AllSelection(d)],
  ['list trailing heading', () => doc(list(), p('outside')), d => {let pos; d.descendants((n, at) => {if (n.type.name === 'heading') pos = at;}); return TextSelection.create(d, pos + 1);}],
  ['multi-block cell trailing heading', () => doc(table(), p('outside')), d => {let pos; d.descendants((n, at) => {if (n.type.name === 'heading') pos = at;}); return TextSelection.create(d, pos + 1);}],
  ['cell selection', () => doc(table(), p('outside')), d => CellSelection.create(d, 2)],
]) test(`outside-scope ${name} retains the upstream full-plugin behavior`, () => {
  const results = [];
  for (const upstream of [false, true]) {
    const {editor, errors} = editorFor(document(), {}, upstream);
    editor.view.dispatch(editor.state.tr.setSelection(selection(editor.state.doc)));
    if (!upstream) {const before = editor.state.doc; expect(ownBackspace(editor)).toBe(false); expect(editor.state.doc.eq(before)).toBe(true);}
    const handled = key(editor);
    results.push({handled, document: snapshot(editor.state.doc), selection: editor.state.selection.toJSON(), history: undoDepth(editor.state), errors});
  }
  expect(results[0]).toEqual(results[1]);
});

test('separate whole-document deletion still resets formatting through clearDocument', () => {
  const results = [];
  for (const upstream of [false, true]) {
    const {editor} = editorFor(doc(h('delete all'))); select(editor, 1, editor.state.doc.content.size - 1);
    const before = editor.state.doc; expect(key(editor)).toBe(true);
    expect(editor.state.doc.firstChild.type.name).toBe('paragraph'); expect(editor.state.doc.firstChild.attrs.spaceAfter).toBeNull();
    results.push(snapshot(editor.state.doc)); expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(before)).toBe(true);
  }
  expect(results[0]).toEqual(results[1]);
});
