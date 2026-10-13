// Browser-free actual registered keymap/input-rule plugin verification driver.
// No DOM event delivery, native WebView, IME, or application save UI is claimed.
// Usage: node scripts/convert-word-block-interactions.mjs input.json output.json shortcut|toggle|input 0|1|2|3
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
import {Editor} from '@tiptap/core';
import {Fragment, Slice} from '@tiptap/pm/model';
import {TextSelection} from '@tiptap/pm/state';
import {closeHistory, undoDepth} from '@tiptap/pm/history';
import {interactionExtensions, snapshot, blocks} from '../tests/ui/word-block-type-harness.mjs';

const [input, output, mode, value] = process.argv.slice(2);
assert(input && output && ['shortcut', 'toggle', 'input'].includes(mode) && /^[0-3]$/.test(value ?? ''), 'Expected input output shortcut|toggle|input 0|1|2|3');
const level = Number(value);
assert(mode === 'shortcut' || level > 0, 'Input and toggle require a heading level');
const errors = [], records = [];
let editor;
try {
  editor = new Editor({element: null, extensions: interactionExtensions({onError: value => errors.push(value)}), content: JSON.parse(readFileSync(input, 'utf8'))});
  // All real extension-manager plugins, including the app history boundary and
  // ordinary trailingNode normalization. Only DOM event delivery is simulated.
  const registered = editor.extensionManager.plugins;
  const keymaps = registered.filter(plugin => plugin.props.handleKeyDown && !plugin.spec.isInputRules);
  const inputRules = registered.filter(plugin => plugin.spec.isInputRules);
  for (const plugin of registered) editor.registerPlugin(plugin);
  assert(registered.some(plugin => plugin.key.startsWith('trailingNode$')), 'Missing actual trailingNode plugin');
  const source = editor.state.doc, sourceBlocks = blocks(source);
  // Normalization during the first transaction is recorded separately, outside
  // conversion history. The source-only range excludes an auto-appended tail.
  editor.view.dispatch(editor.state.tr.setMeta('addToHistory', false));
  const original = editor.state.doc;
  const invokeShortcut = target => {
    const event = {key: String(target), keyCode: 48 + target, code: `Digit${target}`, ctrlKey: true, altKey: true, shiftKey: false, metaKey: false};
    const handled = keymaps.some(plugin => plugin.props.handleKeyDown(editor.view, event));
    assert(handled, 'Actual registered Ctrl/Mod-Alt callback was not consumed');
    return handled;
  };
  if (mode !== 'input') {
    const first = sourceBlocks[0], last = sourceBlocks.at(-1);
    const selected = TextSelection.create(editor.state.doc, first.pos + 1, last.pos + last.node.nodeSize - 1);
    editor.view.dispatch(editor.state.tr.setSelection(selected));
    const rounds = mode === 'toggle' ? 2 : 1;
    for (let index = 0; index < rounds; index++) {
      const before = editor.state.doc, depth = undoDepth(editor.state);
      invokeShortcut(level);
      const changed = !editor.state.doc.eq(before);
      records.push({entry: `Ctrl/Mod-Alt-${level}`, changed, historyIncrement: undoDepth(editor.state) - depth});
      assert.deepEqual(editor.state.selection.toJSON(), selected.toJSON());
      assert.equal(undoDepth(editor.state) - depth, Number(changed));
      if (errors.length) break;
    }
    if (errors.length) {
      assert(editor.state.doc.eq(original), 'Rejected shortcut changed document');
      assert.equal(undoDepth(editor.state), 0, 'Rejected shortcut changed history');
      throw new Error(errors.join('\n'));
    }
    const converted = editor.state.doc, changed = records.filter(record => record.changed).length;
    for (let index = 0; index < changed; index++) assert(editor.commands.undo(), 'Undo failed');
    assert(editor.state.doc.eq(original), 'Full shortcut undo did not restore source');
    for (let index = 0; index < changed; index++) assert(editor.commands.redo(), 'Redo failed');
    assert(editor.state.doc.eq(converted), 'Full shortcut redo differs');
  } else {
    // Type a prefix at each imported paragraph start; remove it only through the
    // actual input-rule plugin. Existing tail text/marks remain the source oracle.
    const count = sourceBlocks.length;
    for (let index = 0; index < count; index++) {
      const before = editor.state.doc, {pos} = blocks(before)[index], from = pos + 1;
      editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(before, from)));
      editor.view.dispatch(closeHistory(editor.state.tr.insertText('#'.repeat(level), from)));
      editor.view.dispatch(closeHistory(editor.state.tr));
      const hashed = editor.state.doc, cursor = from + level;
      const handled = inputRules.some(plugin => plugin.props.handleTextInput(editor.view, cursor, cursor, ' '));
      if (!handled) {
        assert(editor.state.doc.eq(hashed), 'Rejected input rule changed document or consumed trigger');
        assert(errors.length && errors.some(value => value.includes('列表首段须保留为正文')), 'Input refusal lacked precise list diagnostic');
        throw new Error(errors.join('\n'));
      }
      assert.equal(errors.length, 0);
      const converted = editor.state.doc, convertedSnapshot = snapshot(converted);
      // Backspace semantics uses the plugin's real metadata, not hand-built meta.
      assert(editor.commands.undoInputRule(), 'Actual plugin failed to retain undoInputRule metadata');
      const reverted = editor.state.doc, expected = hashed.type.schema.nodeFromJSON(hashed.toJSON());
      const expectedLiteral = expected.replace(cursor, cursor, new Slice(
        Fragment.from(expected.type.schema.text(' ', hashed.resolve(cursor).marks())), 0, 0));
      assert(reverted.eq(expectedLiteral), 'undoInputRule did not restore literal prefix and space');
      assert(editor.commands.undo(), 'Undoing undoInputRule failed');
      assert.deepEqual(snapshot(editor.state.doc), convertedSnapshot, 'Undo of Backspace-like restoration changed conversion');
      assert(editor.state.doc.eq(converted));
      records.push({entry: `${'#'.repeat(level)} space`, paragraphIndex: index, pluginHandled: true, undoInputRuleRestoredLiteral: true, undoRestorationExact: true});
    }
  }
  writeFileSync(output, `${JSON.stringify(snapshot(editor.state.doc), null, 2)}\n`, {flag: 'wx'});
  console.log(JSON.stringify({mode, level, records, undoDepth: undoDepth(editor.state), keymapPlugins: keymaps.length, inputRulePlugins: inputRules.length,
    fullManagerPluginCount: registered.length, initialAppendedParagraphs: blocks(original).length - sourceBlocks.length,
    finalAppendedParagraphs: blocks(editor.state.doc).length - sourceBlocks.length,
    registeredPluginDispatch: true, trailingNodeIncluded: true, browserOrNativeDispatch: false}));
} catch (error) {
  console.error(error instanceof Error ? error.stack : String(error));
  process.exitCode = 1;
} finally {editor?.destroy();}
