// Remaining user-reachable upstream paths, not desired application behavior.
// Invoke the registered key callback / input-rule handler without claiming DOM
// keyboard dispatch, browser selection, or native input-method validation.
import {test, expect} from '@playwright/test';
import {Editor, createChainableState, getExtensionField} from '@tiptap/core';
import {history} from '@tiptap/pm/history';
import {extensions, snapshot, paragraph, heading, doc, blocks} from './word-block-type-harness.mjs';
const attrs = [{textAlign: 'center', spaceBefore: 0, spaceAfter: 120, lineSpacing: 241, indentLeftTwips: 360, indentRightTwips: 0, firstLineIndentTwips: -180},
  {textAlign: 'right', spaceBefore: 240, spaceAfter: 0, lineSpacing: 360, indentLeftTwips: 0, indentRightTwips: 100, firstLineIndentTwips: 120}];
const editors = [];
function editorFor(document) {const editor = new Editor({element: null, extensions, content: snapshot(document)}); editor.registerPlugin(history()); editors.push(editor); return editor;}
function extensionField(editor, name, field) {
  const extension = editor.extensionManager.extensions.find(value => value.name === name);
  return getExtensionField(extension, field, {name: extension.name, options: extension.options, storage: extension.storage,
    editor, type: editor.schema.nodes[name]})();
}
test.afterEach(() => {for (const editor of editors.splice(0)) editor.destroy();});

for (const level of [1, 2, 3]) test(`remaining upstream Mod-Alt-${level} callback loses per-block attrs on a multi-paragraph range`, () => {
  const editor = editorFor(doc([paragraph('A', attrs[0]), paragraph('B', attrs[1])]));
  const before = editor.state.doc; editor.commands.setTextSelection({from: 1, to: 5});
  const shortcuts = extensionField(editor, 'heading', 'addKeyboardShortcuts');
  expect(shortcuts[`Mod-Alt-${level}`]()).toBe(true);
  for (const {node} of blocks(editor.state.doc)) {
    expect(node.type.name).toBe('heading'); expect(node.attrs.level).toBe(level);
    for (const key of Object.keys(attrs[0])) expect(node.attrs[key]).toBeNull();
  }
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(before)).toBe(true);
});

for (const key of ['Mod-Alt-0', 'Mod-Alt-2']) test(`remaining upstream ${key} callback loses per-block attrs when returning multiple headings to body`, () => {
  const editor = editorFor(doc([heading('A', attrs[0]), heading('B', attrs[1])]));
  const before = editor.state.doc; editor.commands.setTextSelection({from: 1, to: 5});
  const shortcuts = extensionField(editor, key.endsWith('0') ? 'paragraph' : 'heading', 'addKeyboardShortcuts');
  expect(shortcuts[key]()).toBe(true);
  for (const {node} of blocks(editor.state.doc)) {
    expect(node.type.name).toBe('paragraph'); for (const field of Object.keys(attrs[0])) expect(node.attrs[field]).toBeNull();
  }
  expect(editor.commands.undo()).toBe(true); expect(editor.state.doc.eq(before)).toBe(true);
});

for (const level of [1, 2, 3]) test(`remaining upstream ${'#'.repeat(level)} space input rule loses even single-paragraph attrs`, () => {
  const prefix = '#'.repeat(level), editor = editorFor(doc([paragraph(prefix, attrs[0]), paragraph('neighbor', attrs[1])]));
  editor.commands.setTextSelection(1 + prefix.length);
  const rules = extensionField(editor, 'heading', 'addInputRules'), rule = rules.find(value => value.find.test(`${prefix} `));
  expect(rule).toBeDefined();
  const before = editor.state.doc, transaction = editor.state.tr;
  const state = createChainableState({state: editor.state, transaction});
  // Actual handler range excludes the pending space, as in Tiptap applyRule.
  rule.handler({state, range: {from: 1, to: 1 + prefix.length}, match: rule.find.exec(`${prefix} `)});
  expect(transaction.doc.firstChild.type.name).toBe('heading'); expect(transaction.doc.firstChild.attrs.level).toBe(level);
  for (const key of Object.keys(attrs[0])) expect(transaction.doc.firstChild.attrs[key]).toBeNull();
  expect(transaction.doc.firstChild.textContent).toBe(''); expect(transaction.doc.lastChild.eq(before.lastChild)).toBe(true);
  expect(editor.state.doc).toBe(before);
});
