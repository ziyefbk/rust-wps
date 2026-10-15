// Real registered callbacks and the full ProseMirror plugin chain, without DOM.
// These are not physical keyboard, browser layout or native IME tests.
import {test, expect} from '@playwright/test';
import {Editor, createChainableState, getExtensionField} from '@tiptap/core';
import {AllSelection, NodeSelection, TextSelection} from '@tiptap/pm/state';
import {CellSelection} from '@tiptap/pm/tables';
import {undoDepth} from '@tiptap/pm/history';
import {interactionExtensions, schema, snapshot, paragraph, heading, doc, blocks} from './word-block-type-harness.mjs';
const a = {textAlign: 'center', spaceBefore: 0, spaceAfter: 120, lineSpacing: 241, indentLeftTwips: 360, indentRightTwips: 0, firstLineIndentTwips: -180};
const b = {textAlign: 'right', spaceBefore: 240, spaceAfter: 0, lineSpacing: 360, indentLeftTwips: 0, indentRightTwips: 100, firstLineIndentTwips: 120};
const editors = [];
function editorFor(document, options = {}) {
  const errors = [], editor = new Editor({element: null, extensions: interactionExtensions({onError: message => errors.push(message), ...options}), content: snapshot(document)});
  // Install the actual entire manager chain, including trailingNode and history.
  for (const plugin of editor.extensionManager.plugins) editor.registerPlugin(plugin);
  editors.push(editor); return {editor, errors};
}
function field(editor, name, method) {
  const extension = editor.extensionManager.extensions.find(value => value.name === name);
  return getExtensionField(extension, method, {name, options: extension.options, storage: extension.storage, editor, type: editor.schema.nodes[name]})();
}
function shortcut(editor, level) {return field(editor, level ? 'heading' : 'paragraph', 'addKeyboardShortcuts')[`Mod-Alt-${level}`]();}
function key(editor, key, modifiers = {}) {
  const event = {key, keyCode: key === 'Backspace' ? 8 : key === 'Enter' ? 13 : key.charCodeAt(0), ctrlKey: false, altKey: false, metaKey: false, shiftKey: false, ...modifiers};
  return editor.state.plugins.some(plugin => plugin.props.handleKeyDown?.(editor.view, event));
}
function input(editor, text, fallback = false) {
  const {from, to} = editor.state.selection;
  const handled = editor.state.plugins.some(plugin => plugin.props.handleTextInput?.(editor.view, from, to, text));
  if (!handled && fallback) editor.view.dispatch(editor.state.tr.insertText(text, from, to));
  return handled;
}
const select = (editor, from, to = from) => editor.commands.setTextSelection({from, to});
const attrs = node => {const {level: _, ...rest} = node.attrs; return rest;};
function preserved(before, after, level, count = 2) {
  for (let i = 0; i < count; i++) {
    expect(after.child(i).type.name).toBe(level ? 'heading' : 'paragraph');
    if (level) expect(after.child(i).attrs.level).toBe(level);
    expect(attrs(after.child(i))).toEqual(attrs(before.child(i)));
    expect(after.child(i).content.eq(before.child(i).content)).toBe(true);
  }
}
test.afterEach(() => {for (const editor of editors.splice(0)) editor.destroy();});

for (const level of [0, 1, 2, 3]) test(`full registered Ctrl-Alt-${level} keymap preserves mixed per-block formatting`, () => {
  const {editor, errors} = editorFor(doc([heading('A', {...a, level: 1}), paragraph('B', b), paragraph('outside')]));
  select(editor, 1, 5); const before = editor.state.doc, selection = editor.state.selection.toJSON();
  expect(key(editor, String(level), {ctrlKey: true, altKey: true})).toBe(true);
  preserved(before, editor.state.doc, level); expect(editor.state.doc.lastChild.eq(before.lastChild)).toBe(true);
  expect(editor.state.selection.toJSON()).toEqual(selection); expect(errors).toEqual([]); expect(undoDepth(editor.state)).toBe(1);
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(before)).toBe(true);
  expect(editor.commands.redo()).toBe(true); preserved(before, editor.state.doc, level);
});

for (const level of [1, 2, 3]) test(`same-level ${level} shortcut toggles two differently formatted headings to body`, () => {
  const {editor} = editorFor(doc([heading('A', {...a, level}), heading('B', {...b, level}), paragraph('outside')]));
  select(editor, 1, 5); const before = editor.state.doc; expect(shortcut(editor, level)).toBe(true); preserved(before, editor.state.doc, 0);
  expect(shortcut(editor, level)).toBe(true); expect(editor.state.doc.eq(before)).toBe(true); expect(undoDepth(editor.state)).toBe(2);
});

for (const backward of [false, true]) test(`toggle uses only selected blocks at ${backward ? 'backward' : 'forward'} next-text boundary`, () => {
  const {editor} = editorFor(doc([heading('abc', {...a, level: 2}), paragraph('untouched', b)]));
  const end = editor.state.doc.firstChild.nodeSize + 1; select(editor, backward ? end : 1, backward ? 1 : end);
  const before = editor.state.doc, selection = editor.state.selection.toJSON(); shortcut(editor, 2);
  expect(editor.state.doc.firstChild.type.name).toBe('paragraph'); expect(editor.state.doc.lastChild.eq(before.lastChild)).toBe(true);
  expect(editor.state.selection.toJSON()).toEqual(selection);
});

test('mixed heading levels become requested heading rather than toggling mixed selection', () => {
  const {editor} = editorFor(doc([heading('A', a), heading('B', {...b, level: 3}), paragraph('outside')]));
  select(editor, 1, 5); const before = editor.state.doc; shortcut(editor, 2); preserved(before, editor.state.doc, 2);
});

test('full-plugin shortcuts preserve AllSelection NodeSelection and CellSelection', () => {
  const table = schema.node('table', null, [schema.node('tableRow', null, [schema.node('tableCell', null, heading('A', a)), schema.node('tableCell', null, heading('B', b))])]);
  for (const mode of ['all', 'node', 'cell']) {
    const {editor} = editorFor(doc([table, paragraph('outside')])); const document = editor.state.doc;
    let selection;
    if (mode === 'all') selection = new AllSelection(document);
    if (mode === 'node') selection = NodeSelection.create(document, 3);
    if (mode === 'cell') selection = CellSelection.create(document, 2, 7);
    editor.view.dispatch(editor.state.tr.setSelection(selection)); const original = editor.state.selection.toJSON();
    shortcut(editor, 0); expect(editor.state.selection.toJSON()).toEqual(original);
    expect(blocks(editor.state.doc).slice(0, mode === 'node' ? 1 : 2).every(({node}) => node.type.name === 'paragraph')).toBe(true);
  }
});

for (const empty of [false, true]) test(`shortcut preserves ${empty ? 'explicit empty' : 'italic'} stored marks and following input`, () => {
  const {editor} = editorFor(doc([schema.node('paragraph', a, schema.text('bold', [schema.marks.bold.create()])), paragraph('outside')]));
  select(editor, 3); const marks = empty ? [] : [editor.schema.marks.italic.create()]; editor.view.dispatch(editor.state.tr.setStoredMarks(marks));
  const before = editor.state.doc; shortcut(editor, 2); expect(editor.state.storedMarks).toEqual(marks); expect(editor.state.doc.firstChild.content.eq(before.firstChild.content)).toBe(true);
  editor.view.dispatch(editor.state.tr.insertText('X')); expect(editor.state.doc.firstChild.child(1).marks).toEqual(marks);
});

test('shortcut history separates previous typing conversion and later typing', () => {
  const {editor} = editorFor(doc([paragraph('A', a), paragraph('outside')])); select(editor, 2); editor.view.dispatch(editor.state.tr.insertText('before'));
  const before = editor.state.doc; shortcut(editor, 2); const converted = editor.state.doc; editor.view.dispatch(editor.state.tr.insertText('after'));
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(converted)).toBe(true);
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(before)).toBe(true);
});

test('body no-op does not dirty history and stale selection sync precedes conversion', () => {
  let editor, syncs = 0;
  ({editor} = editorFor(doc([paragraph('first', a), paragraph('second', b), paragraph('outside')]), {syncSelection: () => {syncs++; select(editor, 8);}}));
  shortcut(editor, 0); expect(undoDepth(editor.state)).toBe(0); shortcut(editor, 3);
  expect(syncs).toBe(2); expect(editor.state.doc.firstChild.type.name).toBe('paragraph'); expect(editor.state.doc.child(1).type.name).toBe('heading');
});

test('locked readonly and composing callbacks and full keymaps never convert', () => {
  for (const mode of ['locked', 'readonly', 'composing']) {
    const {editor} = editorFor(doc([paragraph('A', a), paragraph('B', b)]), {locked: () => mode === 'locked'});
    if (mode === 'readonly') editor.setEditable(false);
    if (mode === 'composing') {const view = editor.view; view.composing = true; Object.defineProperty(editor, 'view', {get: () => view});}
    const before = editor.state.doc; expect(shortcut(editor, 2)).toBe(false);
    expect(key(editor, '2', {ctrlKey: true, altKey: true})).toBe(false); expect(editor.state.doc.eq(before)).toBe(true); expect(undoDepth(editor.state)).toBe(0);
  }
});

test('list-first and invalid candidates consume key without partial change or fallback', () => {
  const list = schema.node('bulletList', null, schema.node('listItem', null, paragraph('list')));
  for (const document of [doc([paragraph('first', a), list, paragraph('outside')]), doc([paragraph('first', a), paragraph('invalid', {spaceAfter: -1}), paragraph('outside')])]) {
    const {editor, errors} = editorFor(document); editor.view.dispatch(editor.state.tr.setSelection(new AllSelection(editor.state.doc)));
    const before = editor.state.doc, selection = editor.state.selection.toJSON(); expect(key(editor, '2', {ctrlKey: true, altKey: true})).toBe(true);
    expect(editor.state.doc.eq(before)).toBe(true); expect(editor.state.selection.toJSON()).toEqual(selection); expect(undoDepth(editor.state)).toBe(0); expect(errors).toHaveLength(1);
  }
});

for (const level of [1, 2, 3]) test(`actual ${'#'.repeat(level)} space plugin preserves all seven fields and undoInputRule`, () => {
  const prefix = '#'.repeat(level), {editor, errors} = editorFor(doc([paragraph(prefix, a), paragraph('neighbor', b)]));
  select(editor, 1 + level); const before = editor.state.doc; expect(input(editor, ' ')).toBe(true);
  expect(editor.state.doc.firstChild.type.name).toBe('heading'); expect(editor.state.doc.firstChild.attrs).toEqual({...before.firstChild.attrs, level});
  expect(editor.state.doc.firstChild.textContent).toBe(''); expect(editor.state.doc.lastChild.eq(before.lastChild)).toBe(true); expect(editor.state.selection.from).toBe(1); expect(errors).toEqual([]);
  expect(editor.commands.undoInputRule()).toBe(true); expect(editor.state.doc.firstChild.attrs).toEqual(before.firstChild.attrs); expect(editor.state.doc.firstChild.textContent).toBe(`${prefix} `);
  expect(editor.state.doc.lastChild.eq(before.lastChild)).toBe(true); expect(editor.commands.undoInputRule()).toBe(false);
});

for (const reordered of [false, true]) test(`single final paragraph input rule undoes appended trailing node ${reordered ? 'with reversed append order' : 'in app order'}`, () => {
  const {editor} = editorFor(doc([paragraph('##', a)]));
  if (reordered) {const plugins = [...editor.state.plugins]; const index = plugins.findIndex(plugin => plugin.key.startsWith('wordBlockTypeInputHistory')); const [plugin] = plugins.splice(index, 1); plugins.unshift(plugin); editor.view.updateState(editor.state.reconfigure({plugins}));}
  select(editor, 3); expect(input(editor, ' ')).toBe(true); expect(editor.state.doc.childCount).toBe(2); expect(editor.state.doc.lastChild.type.name).toBe('paragraph');
  expect(key(editor, 'Backspace')).toBe(true); expect(editor.state.doc.childCount).toBe(1); expect(editor.state.doc.firstChild.textContent).toBe('## '); expect(attrs(editor.state.doc.firstChild)).toEqual({...a, minimumLineHeightTwips: null});
});

test('input undo retains pre-existing trailing empty paragraph and exact suffix marks', () => {
  const content = [schema.text('##'), schema.text('中文😀', [schema.marks.bold.create(), schema.marks.link.create({href: 'https://example.com'})]), schema.node('hardBreak'), schema.text('tail')];
  const {editor} = editorFor(doc([schema.node('paragraph', a, content), paragraph('', b)])); select(editor, 3); const before = editor.state.doc;
  expect(input(editor, ' ')).toBe(true); const result = editor.state.doc;
  expect(result.firstChild.content.eq(before.firstChild.content.cut(2))).toBe(true); expect(result.lastChild.eq(before.lastChild)).toBe(true);
  expect(editor.commands.undoInputRule()).toBe(true); expect(editor.state.doc.childCount).toBe(2); expect(editor.state.doc.lastChild.eq(before.lastChild)).toBe(true);
  expect(editor.state.doc.firstChild.content.cut(3).eq(before.firstChild.content.cut(2))).toBe(true);
});

test('input conversion uses stored formatting with and without an appended trailing paragraph', () => {
  for (const clear of [false, true]) for (const final of [false, true]) {
    const {editor} = editorFor(doc([schema.node('paragraph', a, schema.text('##', [schema.marks.bold.create()])), ...(final ? [] : [paragraph('outside')])])); select(editor, 3);
    const marks = clear ? [] : [editor.schema.marks.italic.create()]; editor.view.dispatch(editor.state.tr.setStoredMarks(marks));
    expect(input(editor, ' ')).toBe(true); expect(editor.state.storedMarks).toEqual(marks); editor.view.dispatch(editor.state.tr.insertText('X'));
    expect(editor.state.doc.firstChild.firstChild.marks).toEqual(marks); expect(editor.commands.undoInputRule()).toBe(false);
  }
});

test('input conversion history is separate from trigger and later typing', () => {
  const {editor} = editorFor(doc([paragraph('#', a)])); select(editor, 2); expect(input(editor, ' ')).toBe(true); const converted = editor.state.doc;
  editor.view.dispatch(editor.state.tr.insertText('later')); expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(converted)).toBe(true);
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.childCount).toBe(1); expect(editor.state.doc.firstChild.textContent).toBe('# '); expect(editor.state.doc.firstChild.attrs).toEqual({...a, minimumLineHeightTwips: null});
  expect(editor.commands.redo()).toBe(true); expect(editor.state.doc.eq(converted)).toBe(true);
});

test('same-level heading input still removes marker and validates whole document', () => {
  for (const invalid of [false, true]) {
    const {editor, errors} = editorFor(doc([heading('##', a), paragraph('outside', invalid ? {spaceAfter: -1} : {})])); select(editor, 3); const before = editor.state.doc;
    expect(input(editor, ' ')).toBe(!invalid);
    if (invalid) {expect(editor.state.doc.eq(before)).toBe(true); expect(errors).toHaveLength(1);}
    else {expect(editor.state.doc.firstChild.type.name).toBe('heading'); expect(editor.state.doc.firstChild.textContent).toBe(''); expect(editor.state.doc.firstChild.attrs).toEqual(before.firstChild.attrs); expect(editor.commands.undoInputRule()).toBe(true);}
  }
});

test('rejected list-first input leaves hashes and normal typed space without lifting', () => {
  const list = schema.node('bulletList', null, schema.node('listItem', null, paragraph('##', {spaceAfter: 0})));
  const {editor, errors} = editorFor(doc([list, paragraph('outside')])); select(editor, 5);
  expect(input(editor, ' ', true)).toBe(false); expect(editor.state.doc.firstChild.type.name).toBe('bulletList'); expect(blocks(editor.state.doc)[0].node.textContent).toBe('## ');
  expect(blocks(editor.state.doc)[0].node.attrs.spaceAfter).toBe(0); expect(errors).toHaveLength(1); expect(editor.commands.undoInputRule()).toBe(false);
});

test('list trailing paragraph input converts safely and undoInputRule keeps the list wrapper', () => {
  const list = schema.node('bulletList', null, schema.node('listItem', null, [paragraph('first'), paragraph('##', {spaceAfter: 120})]));
  const {editor} = editorFor(doc([list, paragraph('outside')])); const pos = blocks(editor.state.doc)[1].pos; select(editor, pos + 3);
  expect(input(editor, ' ')).toBe(true); expect(editor.state.doc.firstChild.type.name).toBe('bulletList'); expect(blocks(editor.state.doc)[1].node.type.name).toBe('heading');
  expect(editor.commands.undoInputRule()).toBe(true); expect(blocks(editor.state.doc)[1].node.textContent).toBe('## ');
});

test('direct registered rule rejects invalid candidate without polluting shared transaction', () => {
  const {editor, errors} = editorFor(doc([paragraph('##', {...a, lineSpacing: -1}), paragraph('outside')])); select(editor, 3);
  const transaction = editor.state.tr, state = createChainableState({state: editor.state, transaction});
  const rule = field(editor, 'heading', 'addInputRules')[1]; expect(rule.handler({state, range: {from: 1, to: 3}, match: rule.find.exec('## ')})).toBeNull();
  expect(transaction.steps).toEqual([]); expect(transaction.selection.eq(editor.state.selection)).toBe(true); expect(errors).toHaveLength(1); expect(undoDepth(editor.state)).toBe(0);
});

for (const trigger of ['\t', 'Enter']) test(`registered ${trigger === 'Enter' ? 'Enter' : 'tab text'} heading rule preserves prior whitespace behavior`, () => {
  // Existing indentation filter rejects literal tabs before rule conversion.
  const attributes = trigger === 'Enter' ? a : {textAlign: 'center', spaceBefore: 0, spaceAfter: 120, lineSpacing: 241};
  const {editor} = editorFor(doc([paragraph('##', attributes), paragraph('outside')])); select(editor, 3); const before = editor.state.doc;
  expect(trigger === 'Enter' ? key(editor, 'Enter') : input(editor, trigger)).toBe(true); expect(editor.state.doc.firstChild.attrs).toEqual({...before.firstChild.attrs, level: 2});
  expect(editor.commands.undoInputRule()).toBe(true); expect(editor.state.doc.firstChild.textContent).toBe(`##${trigger === 'Enter' ? '\n' : '\t'}`);
});

test('existing indented-paragraph tab protection rejects the trigger without deleting hashes', () => {
  const {editor} = editorFor(doc([paragraph('##', a), paragraph('outside')])); select(editor, 3); const before = editor.state.doc;
  expect(input(editor, '\t')).toBe(true); expect(editor.state.doc.eq(before)).toBe(true); expect(undoDepth(editor.state)).toBe(0); expect(editor.commands.undoInputRule()).toBe(false);
});

test('compositionend processes completed marker after composing input is skipped', async () => {
  const {editor} = editorFor(doc([paragraph('##', a), paragraph('outside')])); select(editor, 3);
  const view = editor.view; Object.defineProperty(editor, 'view', {get: () => view}); view.composing = true;
  expect(input(editor, ' ')).toBe(false); editor.view.dispatch(editor.state.tr.insertText(' ')); view.composing = false;
  for (const plugin of editor.state.plugins) plugin.props.handleDOMEvents?.compositionend?.(view, {});
  await new Promise(resolve => setTimeout(resolve, 20)); expect(editor.state.doc.firstChild.attrs).toEqual({...a, minimumLineHeightTwips: null, level: 2});
  expect(editor.commands.undoInputRule()).toBe(true); expect(editor.state.doc.firstChild.textContent).toBe('## ');
});

test('code marks composing and read-only states block input-rule conversion', () => {
  for (const mode of ['code', 'locked', 'readonly']) {
    const marked = schema.node('paragraph', a, schema.text('##', mode === 'code' ? [schema.marks.code.create()] : []));
    const {editor, errors} = editorFor(doc([marked, paragraph('outside')]), {locked: () => mode === 'locked'}); select(editor, 3); if (mode === 'readonly') editor.setEditable(false);
    const before = editor.state.doc; expect(input(editor, ' ')).toBe(false); expect(editor.state.doc.eq(before)).toBe(true); expect(errors).toEqual([]);
  }
});

test('plain paste transaction does not become a heading or create input-rule metadata', () => {
  const {editor} = editorFor(doc([paragraph('', a), paragraph('outside')])); select(editor, 1);
  editor.view.dispatch(editor.state.tr.insertText('## ').setMeta('paste', true).setMeta('uiEvent', 'paste'));
  expect(editor.state.doc.firstChild.type.name).toBe('paragraph'); expect(editor.state.doc.firstChild.attrs).toEqual({...a, minimumLineHeightTwips: null}); expect(editor.commands.undoInputRule()).toBe(false);
});

test('non-prefix and unsupported hash counts remain ordinary text', () => {
  for (const prefix of ['####', 'x#', ' ##', '# #']) {
    const {editor, errors} = editorFor(doc([paragraph(prefix, a), paragraph('outside')])); select(editor, prefix.length + 1); expect(input(editor, ' ', true)).toBe(false);
    expect(editor.state.doc.firstChild.type.name).toBe('paragraph'); expect(editor.state.doc.firstChild.textContent).toBe(`${prefix} `); expect(errors).toEqual([]);
  }
});

test('selection change clears undoInputRule window without removing any content', () => {
  const {editor} = editorFor(doc([paragraph('##', a), paragraph('outside')])); select(editor, 3); expect(input(editor, ' ')).toBe(true); const before = editor.state.doc;
  select(editor, 4); expect(editor.commands.undoInputRule()).toBe(false); expect(editor.state.doc.eq(before)).toBe(true);
});
