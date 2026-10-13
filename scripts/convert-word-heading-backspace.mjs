// Full registered keymap/dispatch middleware, without physical browser input.
// input.json output.json [existing|expose-original-empty]
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
import {Editor} from '@tiptap/core';
import {closeHistory, undoDepth} from '@tiptap/pm/history';
import {interactionExtensions, snapshot} from '../tests/ui/word-block-type-harness.mjs';
import {loadTsModule} from '../tests/ui/load-ts.mjs';
const {EmptyParagraphFont} = await loadTsModule('./ui/word-empty-paragraph-font.ts');
const [input, output, mode = 'existing'] = process.argv.slice(2);
assert(input && output && ['existing', 'expose-original-empty'].includes(mode), 'Expected input output [existing|expose-original-empty]');
const source = JSON.parse(readFileSync(input, 'utf8')), errors = [], edits = [];
let editor;
try {
  editor = new Editor({element: null, extensions: [...interactionExtensions({onError: message => errors.push(message)}), EmptyParagraphFont.configure({onError: message => errors.push(message)})], content: source});
  const plugins = editor.extensionManager.plugins;
  for (const plugin of plugins) editor.registerPlugin(plugin);
  const view = editor.view;
  view.dispatch = editor.extensionManager.dispatchTransaction(view.dispatch.bind(view));
  Object.defineProperty(editor, 'view', {get: () => view});
  editor.view.dispatch(editor.state.tr.setMeta('addToHistory', false));
  const invoke = (key, modifiers = {}) => editor.state.plugins.some(plugin => plugin.props.handleKeyDown?.(view,
    {key, keyCode: key === 'Backspace' ? 8 : key.charCodeAt(0), ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...modifiers}));
  if (mode === 'expose-original-empty') {
    assert.equal(editor.state.doc.child(1).content.size, 0);
    edits.push({action: 'delete first body paragraph in working copy to expose existing second empty paragraph', deletedText: editor.state.doc.firstChild.textContent});
    editor.view.dispatch(closeHistory(editor.state.tr.delete(0, editor.state.doc.firstChild.nodeSize)));
    editor.commands.setTextSelection(1);
    assert(invoke('2', {ctrlKey: true, altKey: true}));
    edits.push({action: 'registered Ctrl-Alt-2 makes exposed original empty paragraph a heading'});
  }
  editor.commands.setTextSelection(1);
  assert.equal(editor.state.doc.firstChild.type.name, 'heading'); assert.equal(editor.state.doc.firstChild.content.size, 0);
  assert.equal(editor.commands.undoInputRule(), false);
  const before = editor.state.doc, beforeSnapshot = snapshot(before), depth = undoDepth(editor.state), selection = editor.state.selection.toJSON();
  assert(invoke('Backspace')); assert.equal(errors.length, 0);
  const after = editor.state.doc;
  assert.equal(after.firstChild.type.name, 'paragraph'); assert.equal(after.firstChild.content.size, 0);
  const {level: _, ...attrs} = before.firstChild.attrs;
  assert.deepEqual({...after.firstChild.attrs}, attrs); assert.deepEqual(editor.state.selection.toJSON(), selection);
  assert.equal(after.childCount, before.childCount);
  for (let index = 1; index < before.childCount; index++) assert(after.child(index).eq(before.child(index)));
  assert.equal(undoDepth(editor.state), depth + 1);
  assert(editor.commands.undo()); assert(editor.state.doc.eq(before));
  assert(editor.commands.redo()); assert(editor.state.doc.eq(after));
  writeFileSync(output, JSON.stringify(snapshot(after), null, 2) + '\n', {flag: 'wx'});
  console.log(JSON.stringify({mode, edits, before: beforeSnapshot, fullManagerPluginCount: plugins.length,
    actualRegisteredPluginCallback: true, realDispatchMiddleware: true, undoRedoExact: true, historyIncrement: 1,
    browserOrNativeDispatch: false, noFakeContent: true}));
} finally {editor?.destroy();}
